#!/usr/bin/env python3
"""Create and verify an immutable KRC721 mint transaction-to-token manifest."""

import argparse
import hashlib
import json
import os
import sys
import tempfile
import urllib.parse
import urllib.request


PAGE_SIZE = 50


def fetch_json(url, timeout=30):
    request = urllib.request.Request(url, headers={"User-Agent": "krc721-mint-audit/1"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def fetch_ops_back(base, min_op_score):
    operations = {}
    offset = None
    while True:
        query = {"direction": "back", "limit": str(PAGE_SIZE)}
        if offset is not None:
            query["offset"] = str(offset)
        payload = fetch_json(f"{base.rstrip('/')}/ops?{urllib.parse.urlencode(query)}")
        page = payload.get("result") or []
        if not page:
            break
        reached_boundary = False
        for operation in page:
            score = int(operation["opScore"])
            if score < min_op_score:
                reached_boundary = True
                continue
            operations[operation["txIdRev"]] = operation
        if reached_boundary:
            break
        next_offset = payload.get("next")
        if next_offset is None or next_offset == offset:
            break
        offset = next_offset
    return list(operations.values())


def canonical_mint(operation):
    if operation.get("op") != "mint":
        return None
    return {
        "txId": operation["txIdRev"],
        "opScore": str(operation["opScore"]),
        "tick": operation["tick"],
        "tokenId": str((operation.get("opData") or {}).get("tokenId")),
    }


def canonical_mints(operations, max_op_score=None):
    records = []
    for operation in operations:
        if max_op_score is not None and int(operation["opScore"]) > max_op_score:
            continue
        record = canonical_mint(operation)
        if record is not None:
            records.append(record)
    return sorted(records, key=lambda item: (int(item["opScore"]), item["txId"]))


def manifest_bytes(records):
    records = sorted(records, key=lambda item: (int(item["opScore"]), item["txId"]))
    return b"".join(
        (
            json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        for record in records
    )


def manifest_sha256(records):
    return hashlib.sha256(manifest_bytes(records)).hexdigest()


def write_manifest(path, records):
    destination = os.path.abspath(path)
    os.makedirs(os.path.dirname(destination), exist_ok=True)
    fd, temporary = tempfile.mkstemp(
        prefix=".krc721-mint-manifest-", dir=os.path.dirname(destination)
    )
    try:
        with os.fdopen(fd, "wb") as output:
            output.write(manifest_bytes(records))
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, destination)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def read_manifest(path):
    records = []
    with open(path, encoding="utf-8") as manifest:
        for line_number, line in enumerate(manifest, 1):
            if not line.strip():
                continue
            record = json.loads(line)
            required = {"txId", "opScore", "tick", "tokenId"}
            if set(record) != required:
                raise ValueError(
                    f"{path}:{line_number}: expected fields {sorted(required)}"
                )
            records.append({key: str(record[key]) for key in required})
    return sorted(records, key=lambda item: (int(item["opScore"]), item["txId"]))


def compare_records(expected, actual):
    expected_by_tx = {record["txId"]: record for record in expected}
    actual_by_tx = {record["txId"]: record for record in actual}
    missing = sorted(set(expected_by_tx) - set(actual_by_tx))
    extra = sorted(set(actual_by_tx) - set(expected_by_tx))
    mismatched = sorted(
        tx_id
        for tx_id in set(expected_by_tx) & set(actual_by_tx)
        if expected_by_tx[tx_id] != actual_by_tx[tx_id]
    )
    return {
        "ok": not missing and not extra and not mismatched,
        "expected": len(expected),
        "actual": len(actual),
        "missing": missing,
        "extra": extra,
        "mismatched": [
            {
                "txId": tx_id,
                "expected": expected_by_tx[tx_id],
                "actual": actual_by_tx[tx_id],
            }
            for tx_id in mismatched
        ],
    }


def parse_target(value):
    if "=" not in value:
        raise argparse.ArgumentTypeError("target must be NAME=URL")
    name, url = value.split("=", 1)
    if not name or not url:
        raise argparse.ArgumentTypeError("target must be NAME=URL")
    return name, url


def generate(args):
    records = canonical_mints(fetch_ops_back(args.base, args.min_op_score))
    write_manifest(args.output, records)
    print(
        json.dumps(
            {
                "ok": True,
                "output": os.path.abspath(args.output),
                "mints": len(records),
                "minOpScore": str(args.min_op_score),
                "maxOpScore": records[-1]["opScore"] if records else None,
                "sha256": manifest_sha256(records),
            },
            sort_keys=True,
        )
    )
    return 0


def verify(args):
    expected = read_manifest(args.manifest)
    if not expected:
        raise ValueError("manifest contains no mint records")
    min_score = min(int(record["opScore"]) for record in expected)
    max_score = max(int(record["opScore"]) for record in expected)
    report = {
        "manifest": os.path.abspath(args.manifest),
        "manifestSha256": manifest_sha256(expected),
        "minOpScore": str(min_score),
        "maxOpScore": str(max_score),
        "targets": {},
    }
    for name, base in args.target:
        actual = canonical_mints(fetch_ops_back(base, min_score), max_score)
        report["targets"][name] = compare_records(expected, actual)
    report["ok"] = all(target["ok"] for target in report["targets"].values())
    print(json.dumps(report, sort_keys=True))
    return 0 if report["ok"] else 1


def build_parser():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    generate_parser = subparsers.add_parser("generate")
    generate_parser.add_argument("--base", required=True)
    generate_parser.add_argument("--min-op-score", required=True, type=int)
    generate_parser.add_argument("--output", required=True)
    generate_parser.set_defaults(handler=generate)

    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--manifest", required=True)
    verify_parser.add_argument(
        "--target", required=True, action="append", type=parse_target, metavar="NAME=URL"
    )
    verify_parser.set_defaults(handler=verify)
    return parser


def main():
    try:
        args = build_parser().parse_args()
        return args.handler(args)
    except Exception as error:
        print(json.dumps({"ok": False, "error": str(error)}, sort_keys=True))
        return 2


if __name__ == "__main__":
    sys.exit(main())
