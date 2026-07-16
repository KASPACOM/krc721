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
    def test_moving_tip_counter_drift_escalates_only_when_persistent(self):
        prod1 = {
            "isNodeConnected": True,
            "isNodeSynced": True,
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
            with (
                mock.patch.object(monitor, "STATE_PATH", state_path),
                mock.patch.object(monitor, "safe_fetch_json", side_effect=fetch),
            ):
                monitor.check_statuses(report, failures, 360)
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
        self.assertIn("counter drift persisted", failures[0])

        prod2["blueScore"] = prod1["blueScore"]
        prod2["currentOpScore"] = prod1["currentOpScore"]
        prod2["lastKnownBlockHash"] = prod1["lastKnownBlockHash"]
        same_tip_failures = []
        with mock.patch.object(monitor, "safe_fetch_json", side_effect=fetch):
            monitor.check_statuses({}, same_tip_failures, 360)
        self.assertEqual(len(same_tip_failures), 1)
        self.assertIn("identical indexed tip", same_tip_failures[0])

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

    def test_second_failure_alerts_even_within_hour_of_recovery(self):
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
                monitor.maybe_alert(report, ["still broken"])

            send.assert_called_once()
            self.assertTrue(report["state"]["alert_active"])
            self.assertEqual(report["state"]["consecutive_failures"], 2)


if __name__ == "__main__":
    unittest.main()
