# KRC721 production monitors

The parity monitor compares prod1 and prod2 symmetrically, validates operation and fee counters, checks known incident transactions, verifies host services, and hashes the executable behind each service's live `/proc/<MainPID>/exe`. It does not trust a checkout or an inactive binary path as deployment evidence.

Install the script and units on the monitoring host, write the approved release values to `/etc/krc721-indexer-parity-monitor.env`, run `systemctl daemon-reload`, and enable `krc721-indexer-parity-monitor.timer`.

```ini
EXPECTED_COMMIT=<approved-short-commit>
EXPECTED_BINARY_SHA256=<approved-release-sha256>
```

Update both values atomically whenever an approved binary is deployed. This pins parity to the reviewed artifact instead of merely proving that both hosts run the same artifact.

Critical data, service, node, and artifact failures alert after two consecutive
runs. Moving-tip `isIndexerSynced=false` observations and retried transport
errors are degradations; they alert only after five consecutive runs. Recovery
requires two consecutive fully healthy runs, and an unresolved incident repeats
at most every three hours.

The cache monitor follows the same degradation and recovery policy. Its source
and units are:

```text
ops/krc721-cache-monitor.py
ops/krc721-cache-monitor.service
ops/krc721-cache-monitor.timer
```

Run the unit tests with:

```bash
python3 -m unittest ops/test_krc721_indexer_parity_monitor.py
```

Run a non-alerting live check with:

```bash
/usr/local/bin/krc721-indexer-parity-monitor.py --ops-window 500 --blue-lag-tolerance 360 --no-alert
python3 ops/krc721-cache-monitor.py --no-alert
```

Peer parity cannot detect a future defect that both indexers reproduce identically. The known transaction and NFT probes cover the incidents recorded here, but a general common-mode guarantee requires a separately implemented chain-derived KRC721 verifier.

## Finalized mint manifest

After an isolated canonical replay has been verified, freeze its finalized
mint transaction-to-token assignments in a manifest:

```bash
python3 ops/krc721-finalized-mint-audit.py generate \
  --base=http://127.0.0.1:18800/api/v1/krc721/mainnet \
  --min-op-score=<rebuild-boundary-op-score> \
  --output=/var/lib/krc721-indexer-monitor/canonical-mints.jsonl
```

Verify both public indexers against that immutable baseline:

```bash
python3 ops/krc721-finalized-mint-audit.py verify \
  --manifest=/var/lib/krc721-indexer-monitor/canonical-mints.jsonl \
  --target=prod1=https://krc721-indexer.kaspa.com/api/v1/krc721/mainnet \
  --target=prod2=https://krc721-indexer-2.kaspa.com/api/v1/krc721/mainnet
```

The command exits non-zero for missing, additional, or rewritten mint
assignments. Generate the baseline only from the independently replayed
canary, never from a production peer merely because both peers agree.

Recovery commands must always point at a copied database explicitly:

```bash
krc721d --mainnet \
  --data-dir=/root/krc721-rebuild-<timestamp>/.krc721 \
  --rewind-blue-score=<score> --dry-run
```

Never run a production recovery rewind without `--data-dir`.
