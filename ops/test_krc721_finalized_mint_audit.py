import importlib.util
import pathlib
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).with_name("krc721-finalized-mint-audit.py")
SPEC = importlib.util.spec_from_file_location("mint_audit", SCRIPT)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class FinalizedMintAuditTests(unittest.TestCase):
    def setUp(self):
        self.mint = {
            "op": "mint",
            "tick": "HASH",
            "opScore": "120496282288001",
            "txIdRev": "10a11cbd",
            "opData": {"tokenId": "1564"},
        }

    def test_canonical_manifest_is_stable(self):
        records = AUDIT.canonical_mints(
            [
                {**self.mint, "txIdRev": "later", "opScore": "120496282288002"},
                self.mint,
                {**self.mint, "op": "transfer", "txIdRev": "ignored"},
            ]
        )
        self.assertEqual([record["txId"] for record in records], ["10a11cbd", "later"])
        self.assertEqual(
            AUDIT.manifest_sha256(records),
            AUDIT.manifest_sha256(list(reversed(records))),
        )

    def test_compare_detects_token_id_rewrite(self):
        expected = [AUDIT.canonical_mint(self.mint)]
        actual = [
            AUDIT.canonical_mint(
                {**self.mint, "opData": {"tokenId": "2097"}}
            )
        ]
        report = AUDIT.compare_records(expected, actual)
        self.assertFalse(report["ok"])
        self.assertEqual(report["mismatched"][0]["txId"], "10a11cbd")

    def test_manifest_round_trip(self):
        records = [AUDIT.canonical_mint(self.mint)]
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "manifest.jsonl"
            AUDIT.write_manifest(path, records)
            self.assertEqual(AUDIT.read_manifest(path), records)


if __name__ == "__main__":
    unittest.main()
