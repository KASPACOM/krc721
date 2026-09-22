import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("krc721-cache-monitor.py")
SPEC = importlib.util.spec_from_file_location("krc721_cache_monitor", SCRIPT)
monitor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(monitor)


def response(body, content_type="application/json"):
    return {
        "code": 200,
        "content_type": content_type,
        "bytes_read": len(body),
        "elapsed_ms": 1,
        "body": body,
    }


class CacheMonitorTests(unittest.TestCase):
    def test_retry_call_recovers_on_third_attempt(self):
        call = mock.Mock(side_effect=[OSError("dns"), OSError("tls"), "ok"])
        with mock.patch.object(monitor.time, "sleep"):
            result = monitor.retry_call(call)

        self.assertTrue(result["ok"])
        self.assertEqual(result["value"], "ok")
        self.assertEqual(result["attempts"], 3)

    def test_transport_failure_is_degraded(self):
        failed = {"ok": False, "error": "OSError: dns", "attempts": 3}
        with mock.patch.object(monitor, "safe_request", return_value=failed):
            report = {}
            failures = []
            degradations = []
            monitor.check_public(report, failures, degradations)

        self.assertEqual(failures, [])
        self.assertEqual(len(degradations), 2)
        self.assertEqual(
            {item["key"] for item in degradations},
            {"dev_public_transport", "prod_public_transport"},
        )

    def test_invalid_metadata_is_critical(self):
        status = {"ok": True, "value": response(b'{"metrics": {}}'), "attempts": 1}
        metadata = {"ok": True, "value": response(b'{"name": "missing-image"}'), "attempts": 1}
        thumbnail = {
            "ok": True,
            "value": response(b"\x89PNG\r\n\x1a\nrest", "image/png"),
            "attempts": 1,
        }
        with mock.patch.object(
            monitor, "safe_request", side_effect=[status, metadata, thumbnail] * 2
        ):
            failures = []
            monitor.check_public({}, failures, [])

        self.assertEqual(len(failures), 2)
        self.assertTrue(all("metadata probe invalid" in item for item in failures))

    def test_degradation_escalates_on_fifth_run(self):
        item = {"key": "prod_status_transport", "message": "prod status failed"}
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(
                json.dumps({"degradation_streaks": {"prod_status_transport": 4}})
            )
            report = {}
            failures = []
            with mock.patch.object(monitor, "STATE_PATH", state_path):
                monitor.escalate_degradations(report, failures, [item])

        self.assertEqual(len(failures), 1)
        self.assertIn("5 monitor runs", failures[0])

    def test_second_critical_failure_alerts(self):
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(
                json.dumps(
                    {
                        "status": "failing",
                        "consecutive_failures": 1,
                        "alert_active": False,
                    }
                )
            )
            report = {"ts": "2026-07-18T00:00:00Z"}
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor.time, "time", return_value=1_000),
                mock.patch.object(
                    monitor, "telegram_send", return_value={"sent": True}
                ) as send,
            ):
                monitor.update_alert_state(report, ["cache service inactive"], [])

        send.assert_called_once()
        self.assertTrue(report["state"]["alert_active"])

    def test_recovery_requires_two_healthy_runs(self):
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(
                json.dumps({"alert_active": True, "healthy_streak": 0})
            )
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor.time, "time", return_value=1_000),
                mock.patch.object(
                    monitor, "telegram_send", return_value={"sent": True}
                ) as send,
            ):
                first = {"ts": "2026-07-18T00:00:00Z"}
                monitor.update_alert_state(first, [], [])
                send.assert_not_called()
                second = {"ts": "2026-07-18T00:02:00Z"}
                monitor.update_alert_state(second, [], [])

        send.assert_called_once()
        self.assertFalse(second["state"]["alert_active"])

    def test_no_alert_preserves_state(self):
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(json.dumps({"status": "ok", "healthy_streak": 2}))
            before = state_path.read_text()
            with mock.patch.object(monitor, "STATE_PATH", state_path):
                report = {"ts": "2026-07-18T00:00:00Z"}
                monitor.update_alert_state(
                    report,
                    [],
                    [{"key": "network", "message": "network"}],
                    no_alert=True,
                )

            self.assertEqual(state_path.read_text(), before)
            self.assertTrue(report["state"]["dry_run"])


if __name__ == "__main__":
    unittest.main()
