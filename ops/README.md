# KRC721 production parity monitor

The monitor compares prod1 and prod2 symmetrically, validates all operation and fee counters, checks known incident transactions, verifies host services, and requires the deployed binary SHA-256 to match on both hosts.

Install the script and units on the monitoring host, write the approved release values to `/etc/krc721-indexer-parity-monitor.env`, run `systemctl daemon-reload`, and enable `krc721-indexer-parity-monitor.timer`. The monitor alerts after two consecutive failures and repeats an active alert at most hourly.

```ini
EXPECTED_COMMIT=<approved-short-commit>
EXPECTED_BINARY_SHA256=<approved-release-sha256>
```

Update both values atomically whenever an approved binary is deployed. This pins parity to the reviewed artifact instead of merely proving that both hosts run the same artifact.

Run the unit tests with:

```bash
python3 -m unittest ops/test_krc721_indexer_parity_monitor.py
```

Run a non-alerting live check with:

```bash
/usr/local/bin/krc721-indexer-parity-monitor.py --ops-window 500 --blue-lag-tolerance 360 --no-alert
```

Peer parity cannot detect a future defect that both indexers reproduce identically. The known transaction and NFT probes cover the incidents recorded here, but a general common-mode guarantee requires a separately implemented chain-derived KRC721 verifier.
