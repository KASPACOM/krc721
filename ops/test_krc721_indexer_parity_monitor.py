import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("krc721-indexer-parity-monitor.py")
SPEC = importlib.util.spec_from_file_location("krc721_parity_monitor", SCRIPT)
monitor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(monitor)


def success(result):
    return {"ok": True, "data": {"message": "success", "result": result}}


class ParityMonitorTests(unittest.TestCase):
    def test_expected_binary_sha_is_enforced_per_host(self):
        snapshots = {
            "prod1": {
                "ssh_ok": True,
                "service": "active",
                "node_service": "active",
                "nginx_route_http": "200",
                "exec_start": "ws://127.0.0.1:17110",
                "node_exec_start": "/home/krc721-pr11-prod1/repo/target/release/krc721d",
                "commit": "4a0cc53",
                "version": "v2.0.0-4a0cc53",
                "binary_sha256": "unexpected",
            },
            "prod2": {
                "ssh_ok": True,
                "service": "active",
                "node_service": "active",
                "proxy_service": "active",
                "exec_start": "ws://127.0.0.1:17110",
                "commit": "4a0cc53",
                "version": "v2.0.0-4a0cc53",
                "binary_sha256": "unexpected",
                "forbidden_process_count": "0",
            },
        }
        failures = []
        degradations = []
        with mock.patch.object(
            monitor, "host_snapshot", side_effect=lambda name, _info: snapshots[name]
        ):
            monitor.check_hosts({}, failures, degradations, "4a0cc53", "approved")

        self.assertEqual(sum("binary SHA-256" in failure for failure in failures), 2)
        self.assertEqual(degradations, [])

    def test_host_snapshot_uses_running_process_artifact(self):
        output = "\n".join(
            [
                "service=active",
                "exec_start=/release/krc721d --mainnet --node-rpc=ws://127.0.0.1:17110",
                "main_pid=42",
                "running_exe=/release/krc721d",
                "running_version=v2.0.0-f00bf59",
                "running_binary_sha256=approved",
                "node_service=active",
                "node_exec_start=/node/krc721d --daemon",
                "block_not_found=0",
                "sync_task_errors=0",
                "stale_sync_forced=0",
                "forced_score_cleanup=0",
            ]
        )
        info = {
            "ssh": "host",
            "service": "indexer.service",
            "node_service": "node.service",
            "expected_node_binary": "/node/krc721d",
        }
        with mock.patch.object(
            monitor,
            "run_ssh",
            return_value={"ok": True, "stdout": output, "stderr": ""},
        ):
            snapshot = monitor.host_snapshot("prod", info)

        self.assertEqual(snapshot["commit"], "f00bf59")
        self.assertEqual(snapshot["version"], "v2.0.0-f00bf59")
        self.assertEqual(snapshot["binary_sha256"], "approved")
        self.assertEqual(snapshot["running_exe"], "/release/krc721d")

    def test_moving_tip_counter_drift_escalates_only_when_persistent(self):
        prod1 = {
            "isNodeConnected": True,
            "isNodeSynced": True,
            "isIndexerSynced": True,
            "blueScore": 1_000,
            "currentOpScore": 248_000_000,
            "lastKnownBlockHash": "a",
            "tokenDeploymentsTotal": 1,
            "tokenMintsTotal": 2,
            "tokenTransfersTotal": 10,
            "tokenListingsTotal": 3,
            "tokenSendsTotal": 4,
            "powFeesTotal": 5,
            "royaltyFeesTotal": 6,
        }
        prod2 = dict(prod1)
        prod2.update(
            {
                "blueScore": 999,
                "currentOpScore": 247_752_000,
                "lastKnownBlockHash": "b",
                "tokenTransfersTotal": 9,
            }
        )

        def fetch(url):
            if "indexer-2" in url:
                return success(prod2)
            if "indexer.kaspa.com" in url:
                return success(prod1)
            raise AssertionError(url)

        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            report = {}
            failures = []
            degradations = []
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor, "safe_fetch_json", side_effect=fetch),
            ):
                monitor.check_statuses(report, failures, degradations, 360)
                monitor.escalate_persistent_counter_drift(report, failures)
                self.assertEqual(failures, [])
                state_path.write_text(
                    json.dumps(
                        {
                            "counter_drift_signature": report["counter_drift_signature"],
                            "counter_drift_streak": 1,
                        }
                    )
                )
                monitor.escalate_persistent_counter_drift(report, failures)

        self.assertEqual(len(failures), 1)
        self.assertEqual(degradations, [])
        self.assertIn("counter drift persisted", failures[0])

        prod2["blueScore"] = prod1["blueScore"]
        prod2["currentOpScore"] = prod1["currentOpScore"]
        prod2["lastKnownBlockHash"] = prod1["lastKnownBlockHash"]
        same_tip_failures = []
        same_tip_degradations = []
        with mock.patch.object(monitor, "safe_fetch_json", side_effect=fetch):
            monitor.check_statuses({}, same_tip_failures, same_tip_degradations, 360)
        self.assertEqual(len(same_tip_failures), 1)
        self.assertIn("identical indexed tip", same_tip_failures[0])

    def test_unsynced_indexer_is_degraded_before_five_runs(self):
        status = {
            "isNodeConnected": True,
            "isNodeSynced": True,
            "isIndexerSynced": False,
            "blueScore": 1_000,
            "currentOpScore": 248_000_000,
            "lastKnownBlockHash": "a",
            "tokenDeploymentsTotal": 1,
            "tokenMintsTotal": 2,
            "tokenTransfersTotal": 3,
            "tokenListingsTotal": 4,
            "tokenSendsTotal": 5,
            "powFeesTotal": 6,
            "royaltyFeesTotal": 7,
        }

        with mock.patch.object(monitor, "safe_fetch_json", return_value=success(status)):
            failures = []
            degradations = []
            report = {}
            monitor.check_statuses(report, failures, degradations, 360)
            with tempfile.TemporaryDirectory() as directory:
                with mock.patch.object(
                    monitor, "STATE_PATH", Path(directory) / "state.json"
                ):
                    monitor.escalate_degradations(report, failures, degradations)

        self.assertEqual(failures, [])
        self.assertEqual(
            {item["key"] for item in degradations},
            {"prod1_indexer_unsynced", "prod2_indexer_unsynced"},
        )

    def test_unsynced_indexer_escalates_on_fifth_run(self):
        degradations = [
            {
                "key": "prod1_indexer_unsynced",
                "message": "prod1: indexer not synced at moving tip",
            }
        ]
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(
                json.dumps({"degradation_streaks": {"prod1_indexer_unsynced": 4}})
            )
            report = {}
            failures = []
            with mock.patch.object(monitor, "STATE_PATH", state_path):
                monitor.escalate_degradations(report, failures, degradations)

        self.assertEqual(len(failures), 1)
        self.assertIn("persisted for 5 monitor runs", failures[0])
        self.assertEqual(report["degradation_streaks"]["prod1_indexer_unsynced"], 5)

    def test_retry_call_recovers_without_reporting_failure(self):
        call = mock.Mock(side_effect=[OSError("dns"), OSError("tls"), {"ok": True}])
        with mock.patch.object(monitor.time, "sleep"):
            result = monitor.retry_call(call)

        self.assertTrue(result["ok"])
        self.assertEqual(result["attempts"], 3)
        self.assertEqual(call.call_count, 3)

    def test_symmetric_feed_detects_an_op_missing_from_prod1(self):
        common = {
            "op": "send",
            "tick": "HASH",
            "opData": {"tokenId": "2016"},
            "opScore": "120379885224001",
            "feeRev": "173000",
            "txIdRev": "83f17d",
        }
        failures = []
        summary = monitor.compare_prod_feeds(
            [{**common, "txIdRev": "shared"}],
            [{**common, "txIdRev": "shared"}, common],
            failures,
        )

        self.assertEqual(summary["only_prod2"], 1)
        self.assertEqual(summary["only_prod1"], 0)
        self.assertEqual(len(failures), 1)

    def test_second_critical_failure_alerts(self):
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(
                json.dumps(
                    {
                        "status": "failing",
                        "consecutive_failures": 1,
                        "last_alert_at": 1_000,
                        "alert_active": False,
                    }
                )
            )
            report = {"ts": "2026-07-16T00:00:00Z"}
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor.time, "time", return_value=1_100),
                mock.patch.object(monitor, "telegram_send", return_value={"sent": True}) as send,
            ):
                monitor.maybe_alert(report, ["still broken"], [])

            send.assert_called_once()
            self.assertTrue(report["state"]["alert_active"])
            self.assertEqual(report["state"]["consecutive_failures"], 2)

    def test_escalated_degradation_alerts_on_fifth_run(self):
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(json.dumps({"status": "degraded"}))
            report = {
                "ts": "2026-07-16T00:00:00Z",
                "escalated_degradations": [{"key": "prod1_indexer_unsynced"}],
            }
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor.time, "time", return_value=1_100),
                mock.patch.object(
                    monitor, "telegram_send", return_value={"sent": True}
                ) as send,
            ):
                monitor.maybe_alert(
                    report,
                    ["prod1: indexer not synced persisted for 5 monitor runs"],
                    [{"key": "prod1_indexer_unsynced", "message": "unsynced"}],
                )

            send.assert_called_once()
            self.assertTrue(report["state"]["alert_active"])

    def test_four_tip_flickers_do_not_send_and_fifth_sustained_run_alerts(self):
        degradation = {
            "key": "prod1_indexer_unsynced",
            "message": "prod1: indexer not synced at moving tip",
        }
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor.time, "time", return_value=1_100),
                mock.patch.object(
                    monitor, "telegram_send", return_value={"sent": True}
                ) as send,
            ):
                for run in range(1, 6):
                    report = {"ts": f"2026-07-16T00:0{run}:00Z"}
                    failures = []
                    monitor.escalate_degradations(
                        report, failures, [degradation]
                    )
                    monitor.maybe_alert(report, failures, [degradation])
                    if run < 5:
                        send.assert_not_called()

            send.assert_called_once()
            self.assertTrue(report["state"]["alert_active"])
            self.assertIn("5 monitor runs", failures[0])

    def test_recovery_requires_two_healthy_runs(self):
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(
                json.dumps(
                    {
                        "status": "failing",
                        "alert_active": True,
                        "healthy_streak": 0,
                        "last_alert_at": 1_000,
                    }
                )
            )
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor.time, "time", return_value=1_100),
                mock.patch.object(
                    monitor, "telegram_send", return_value={"sent": True}
                ) as send,
            ):
                first = {"ts": "2026-07-16T00:00:00Z"}
                monitor.maybe_alert(first, [], [])
                send.assert_not_called()
                second = {"ts": "2026-07-16T00:02:00Z"}
                monitor.maybe_alert(second, [], [])
                send.assert_called_once()

            self.assertFalse(second["state"]["alert_active"])
            self.assertEqual(second["state"]["healthy_streak"], 2)

    def test_no_alert_does_not_write_state(self):
        with tempfile.TemporaryDirectory() as directory:
            state_path = Path(directory) / "state.json"
            state_path.write_text(json.dumps({"status": "ok", "healthy_streak": 3}))
            before = state_path.read_text()
            report = {"ts": "2026-07-16T00:00:00Z"}
            with mock.patch.object(monitor, "STATE_PATH", state_path):
                monitor.maybe_alert(
                    report,
                    [],
                    [{"key": "prod1_indexer_unsynced", "message": "unsynced"}],
                    no_alert=True,
                )

            self.assertEqual(state_path.read_text(), before)
            self.assertTrue(report["state"]["dry_run"])


if __name__ == "__main__":
    unittest.main()
