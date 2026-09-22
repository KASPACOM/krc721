#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


BASES = {
    "legacy": "https://mainnet.krc721.stream/api/v1/krc721/mainnet",
    "prod1": "https://krc721-indexer.kaspa.com/api/v1/krc721/mainnet",
    "prod2": "https://krc721-indexer-2.kaspa.com/api/v1/krc721/mainnet",
}

INDEXER_TARGETS = ("prod1", "prod2")

HOSTS = {
    "prod1": {
        "ssh": "krc721-mainnet",
        "label": "prod1-pr11",
        "service": "krc721-pr11-prod1.service",
        "node_service": "kaspa-mainnet-node-pr11.service",
        "expected_node_binary": "/home/krc721-pr11-prod1/repo/target/release/krc721d",
        "expected_node_rpc": "ws://127.0.0.1:17110",
        "nginx_host": "krc721-indexer.kaspa.com",
    },
    "prod2": {
        "ssh": "krc721-mainnet-2",
        "service": "krc721.service",
        "node_service": "kaspa-mainnet-node.service",
        "expected_node_binary": "/home/krc721-pr11-canary/repo/target/release/krc721d",
        "proxy_service": "krc721-prod2-node-prod1-proxy.service",
        "forbidden_process_pattern": "--http-listen=127.0.0.1:8801",
    },
}

KNOWN_TXS = {
    "prod1_hash_2016_send": {
        "host": "prod1",
        "txid": "83f17d118f13728c5b89eba6f7acc03d4d761bbe68650e5e201f7038cc682397",
        "op": "send",
        "tick": "HASH",
        "token_id": "2016",
        "fee": "173000",
    },
    "prod2_hash_2016_send": {
        "host": "prod2",
        "txid": "83f17d118f13728c5b89eba6f7acc03d4d761bbe68650e5e201f7038cc682397",
        "op": "send",
        "tick": "HASH",
        "token_id": "2016",
        "fee": "173000",
    },
    "prod1_wolfpack_474_transfer": {
        "host": "prod1",
        "txid": "c6f79aaaba1a5f652e71cd599084f177b0da4f66e6f9a2c29bc3cb25c48896a4",
        "op": "transfer",
        "tick": "WOLFPACK",
        "token_id": "474",
        "fee": "292900",
    },
    "prod2_wolfpack_474_transfer": {
        "host": "prod2",
        "txid": "c6f79aaaba1a5f652e71cd599084f177b0da4f66e6f9a2c29bc3cb25c48896a4",
        "op": "transfer",
        "tick": "WOLFPACK",
        "token_id": "474",
        "fee": "292900",
    },
    "prod1_yonatoshi_7291": {
        "host": "prod1",
        "txid": "a2c87fa011fa1de2644fca683fe3fba3704eb6fbe3c6224a9aa14e29e279a22a",
        "tick": "YONATOSHI",
        "token_id": "7291",
    },
    "prod2_chronicv3_11": {
        "host": "prod2",
        "txid": "615132eeb22a61c431e6c72db642ce7bbfaf1129a5115c2567c1f3a168911141",
        "tick": "CHRONICV3",
        "token_id": "11",
    },
    "prod1_wscout26_125": {
        "host": "prod1",
        "txid": "b2dbb6ce11088b1453d3fb6cf6095c423da0bb2cc3a08b69c4929ba986c0d8ef",
        "tick": "WSCOUT26",
        "token_id": "125",
    },
    "prod1_freenacho_4649_list": {
        "host": "prod1",
        "txid": "2e2e0b3cb5eb083d7e3b5ab0640487e881ec865688737b21f137944f75cb913f",
        "tick": "FREENACHO",
        "token_id": "4649",
    },
}

STATE_PATH = Path("/var/lib/krc721-indexer-monitor/state.json")
DEFAULT_ALERT_CHAT_ID = "2090199766"
ENV_PATHS = ["/root/.openclaw/.env", "/root/.hermes/.env"]
DEGRADED_ALERT_RUNS = 5
CRITICAL_ALERT_RUNS = 2
RECOVERY_RUNS = 2
REPEAT_ALERT_SECONDS = 3 * 60 * 60


def utc_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def load_env_file(path):
    try:
        for line in Path(path).read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            key = key.strip()
            value = value.strip().strip('"').strip("'")
            os.environ.setdefault(key, value)
    except FileNotFoundError:
        pass


def fetch_json(url, timeout=8):
    request = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def retry_call(call, attempts=3, delays=(1, 2)):
    errors = []
    for attempt in range(1, attempts + 1):
        try:
            return {"ok": True, "value": call(), "attempts": attempt}
        except Exception as exc:
            errors.append(f"{type(exc).__name__}: {exc}")
            if attempt < attempts:
                time.sleep(delays[min(attempt - 1, len(delays) - 1)])
    return {"ok": False, "error": errors[-1], "errors": errors, "attempts": attempts}


def safe_fetch_json(url):
    result = retry_call(lambda: fetch_json(url))
    if result["ok"]:
        return {"ok": True, "data": result["value"], "attempts": result["attempts"]}
    return result


def result_or_none(payload):
    if payload.get("ok") and payload.get("data", {}).get("message") == "success":
        return payload["data"].get("result")
    return None


def fetch_ops(base, n):
    out = []
    offset = None
    while len(out) < n:
        query = {"direction": "back", "limit": "50"}
        if offset:
            query["offset"] = str(offset)
        payload = fetch_json(f"{base}/ops?{urllib.parse.urlencode(query)}")
        page = payload.get("result") or []
        out.extend(page)
        offset = payload.get("next")
        if not offset or not page:
            break
    return out[:n]


def safe_fetch_ops(base, n):
    return retry_call(lambda: fetch_ops(base, n))


def op_by_tx(base, txid):
    payload = safe_fetch_json(f"{base}/ops/txid/{txid}")
    return result_or_none(payload), payload


def op_sig(op):
    if not op:
        return None
    data = op.get("opData") or {}
    return (
        op.get("op"),
        op.get("tick"),
        str(data.get("tokenId")),
        str(op.get("opScore")),
        str(op.get("feeRev")),
    )


def run_ssh(host, command, timeout=8):
    def invoke():
        proc = subprocess.run(
            ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", host, command],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"exit={proc.returncode}: {proc.stderr.strip()[:200]}")
        return proc

    result = retry_call(invoke)
    if not result["ok"]:
        return result
    proc = result["value"]
    return {
        "ok": True,
        "returncode": proc.returncode,
        "stdout": proc.stdout.strip(),
        "stderr": proc.stderr.strip(),
        "attempts": result["attempts"],
    }


def host_snapshot(name, info):
    service = info.get("service")
    process_pattern = info.get("process_pattern")
    nginx_host = info.get("nginx_host")
    if service:
        service_lines = f"""
printf 'service=%s\\n' "$(systemctl is-active {service} || true)"
printf 'exec_start=%s\\n' "$(systemctl show {service} -p ExecStart --value 2>/dev/null || true)"
main_pid="$(systemctl show {service} -p MainPID --value 2>/dev/null || true)"
printf 'main_pid=%s\\n' "$main_pid"
printf 'running_exe=%s\\n' "$(readlink -f "/proc/$main_pid/exe" 2>/dev/null || true)"
printf 'running_version=%s\\n' "$("/proc/$main_pid/exe" --version 2>/dev/null || true)"
printf 'running_binary_sha256=%s\\n' "$(sha256sum "/proc/$main_pid/exe" 2>/dev/null | awk '{{print $1}}' || true)"
printf 'block_not_found=%s\\n' "$(journalctl -u {service} --since '5 minutes ago' --no-pager 2>/dev/null | grep -Ec 'Block .*not found in chain state|Block .*not found in blockhash_to_score' || true)"
printf 'sync_task_errors=%s\\n' "$(journalctl -u {service} --since '5 minutes ago' --no-pager 2>/dev/null | grep -Ec 'Nexus sync task error|ERROR|panic|thread.*panicked' || true)"
printf 'stale_sync_forced=%s\\n' "$(journalctl -u {service} --since '5 minutes ago' --no-pager 2>/dev/null | grep -Ec 'historical response did not remove stale sync point' || true)"
printf 'forced_score_cleanup=%s\\n' "$(journalctl -u {service} --since '5 minutes ago' --no-pager 2>/dev/null | grep -Ec 'Using forced rollback blue score' || true)"
"""
    else:
        service_lines = f"""
printf 'service=%s\\n' "$(pgrep -af -- '{process_pattern}' >/dev/null && printf active || printf inactive)"
printf 'process_count=%s\\n' "$(pgrep -af -- '{process_pattern}' | grep -vc pgrep || true)"
printf 'exec_start=%s\\n' "$(pgrep -af -- '{process_pattern}' | grep -v pgrep | head -1 || true)"
printf 'block_not_found=0\\n'
printf 'sync_task_errors=0\\n'
printf 'stale_sync_forced=0\\n'
printf 'forced_score_cleanup=0\\n'
"""
    nginx_lines = ""
    if nginx_host:
        nginx_lines = f"""
printf 'nginx_route_http=%s\\n' "$(curl -sS -o /dev/null -w '%{{http_code}}' -H 'Host: {nginx_host}' http://127.0.0.1/api/v1/krc721/mainnet/status || true)"
"""
    command = f"""
set -o pipefail
printf 'label=%s\\n' "{info.get('label', name)}"
{service_lines}
printf 'node_service=%s\\n' "$(systemctl is-active {info['node_service']} || true)"
printf 'node_exec_start=%s\\n' "$(systemctl show {info['node_service']} -p ExecStart --value 2>/dev/null || true)"
{f'''printf 'proxy_service=%s\\n' "$(systemctl is-active {info['proxy_service']} || true)"''' if info.get('proxy_service') else ''}
{f'''printf 'forbidden_process_count=%s\\n' "$(pgrep -af -- '{info['forbidden_process_pattern']}' | grep -vc pgrep || true)"''' if info.get('forbidden_process_pattern') else ''}
{nginx_lines}
"""
    raw = run_ssh(info["ssh"], command)
    parsed = {"ssh_ok": raw.get("ok", False), "raw_error": raw.get("error") or raw.get("stderr")}
    if raw.get("stdout"):
        for line in raw["stdout"].splitlines():
            if "=" in line:
                key, value = line.split("=", 1)
                parsed[key] = value
    version = parsed.get("running_version", "")
    match = re.search(r"-([0-9a-f]{7,40})$", version)
    parsed["commit"] = match.group(1) if match else ""
    parsed["version"] = version
    parsed["binary_sha256"] = parsed.get("running_binary_sha256", "")
    return parsed


def add_degradation(degradations, key, message):
    degradations.append({"key": key, "message": message})


def check_statuses(report, failures, degradations, blue_lag_tolerance, check_legacy=False):
    statuses = {}
    warnings = report.setdefault("warnings", [])
    counter_drifts = report.setdefault("counter_drifts", [])
    parity_fields = (
        "tokenDeploymentsTotal",
        "tokenMintsTotal",
        "tokenTransfersTotal",
        "tokenListingsTotal",
        "tokenSendsTotal",
        "powFeesTotal",
        "royaltyFeesTotal",
    )
    for name, base in BASES.items():
        if name == "legacy" and not check_legacy:
            statuses[name] = "disabled"
            continue
        payload = safe_fetch_json(f"{base}/status")
        status = result_or_none(payload)
        statuses[name] = status
        if not status:
            message = f"{name}: status fetch failed: {payload.get('error') or payload.get('data')}"
            if name == "legacy":
                warnings.append(message)
            else:
                add_degradation(degradations, f"{name}_status_fetch", message)
            continue
        if name not in INDEXER_TARGETS:
            continue
        if status.get("isNodeConnected") is not True:
            failures.append(f"{name}: node not connected")
        if status.get("isNodeSynced") is not True:
            failures.append(f"{name}: node not synced")
        if status.get("isIndexerSynced") is not True:
            add_degradation(
                degradations,
                f"{name}_indexer_unsynced",
                f"{name}: indexer not synced at moving tip",
            )

    prod2 = statuses.get("prod2")
    prod1 = statuses.get("prod1")
    if prod1 and prod2:
        blue_delta = int(prod1.get("blueScore", 0)) - int(prod2.get("blueScore", 0))
        op_delta = int(prod1.get("currentOpScore", 0)) - int(prod2.get("currentOpScore", 0))
        moving_tip = abs(blue_delta) <= blue_lag_tolerance and abs(op_delta) <= (
            blue_lag_tolerance * 248_000
        )
        same_tip = (
            prod1.get("lastKnownBlockHash")
            and prod1.get("lastKnownBlockHash") == prod2.get("lastKnownBlockHash")
        )
        for field in parity_fields:
            if prod1.get(field) != prod2.get(field):
                msg = f"prod1/prod2: {field} prod1={prod1.get(field)} prod2={prod2.get(field)}"
                if same_tip:
                    failures.append(f"{msg} at identical indexed tip")
                elif moving_tip:
                    warnings.append(f"{msg} while tips differ by blue_delta={blue_delta}")
                    counter_drifts.append(
                        {
                            "field": field,
                            "prod1": prod1.get(field),
                            "prod2": prod2.get(field),
                        }
                    )
                else:
                    failures.append(msg)

    legacy = statuses.get("legacy")
    if isinstance(legacy, dict):
        for name in INDEXER_TARGETS:
            status = statuses.get(name)
            if not status:
                continue
            blue_lag = int(legacy.get("blueScore", 0)) - int(status.get("blueScore", 0))
            op_lag = int(legacy.get("currentOpScore", 0)) - int(status.get("currentOpScore", 0))
            moving_tip = abs(blue_lag) <= blue_lag_tolerance and abs(op_lag) <= (
                blue_lag_tolerance * 248_000
            )
            for field in parity_fields:
                if status.get(field) != legacy.get(field):
                    msg = f"{name}: {field}={status.get(field)} legacy={legacy.get(field)}"
                    suffix = f"while tip differs by blue_lag={blue_lag}" if moving_tip else "legacy drift"
                    warnings.append(f"{msg} {suffix}")
            if blue_lag > blue_lag_tolerance:
                warnings.append(f"{name}: legacy blueScore lag {blue_lag} > {blue_lag_tolerance}")

    report["statuses"] = statuses


def escalate_degradations(report, failures, degradations, threshold=DEGRADED_ALERT_RUNS):
    state = read_state()
    previous = state.get("degradation_streaks") or {}
    current = {}
    messages = {}
    for item in degradations:
        key = item["key"]
        current[key] = int(previous.get(key, 0)) + 1
        messages[key] = item["message"]
    report["degradations"] = degradations
    report["degradation_streaks"] = current
    escalated = []
    for key, streak in current.items():
        if streak >= threshold:
            message = f"{messages[key]} persisted for {streak} monitor runs"
            failures.append(message)
            escalated.append({"key": key, "streak": streak, "message": message})
    report["escalated_degradations"] = escalated


def escalate_persistent_counter_drift(report, failures):
    current = report.get("counter_drifts") or []
    signature = json.dumps(current, sort_keys=True, separators=(",", ":")) if current else ""
    state = read_state()
    previous = state.get("counter_drift_signature", "")
    previous_streak = int(state.get("counter_drift_streak", 0))
    streak = previous_streak + 1 if signature and signature == previous else (1 if signature else 0)
    report["counter_drift_signature"] = signature
    report["counter_drift_streak"] = streak
    if streak >= 2:
        failures.append(
            f"prod1/prod2: counter drift persisted for {streak} monitor runs: {current}"
        )


def compare_recent_ops(reference_name, reference_ops, target_names, failures, warnings, warning_only=False):
    summary = {
        "reference": reference_name,
        "checked": len(reference_ops),
        "reference_newest": op_sig(reference_ops[0]) if reference_ops else None,
        "reference_oldest": op_sig(reference_ops[-1]) if reference_ops else None,
    }
    for name in target_names:
        exact = missing = mismatch = 0
        samples = []
        for reference_op in reference_ops:
            txid = reference_op["txIdRev"]
            op, payload = op_by_tx(BASES[name], txid)
            if not op:
                missing += 1
                if len(samples) < 5:
                    samples.append(
                        {"type": "missing", "txid": txid, reference_name: op_sig(reference_op)}
                    )
            elif op_sig(op) != op_sig(reference_op):
                mismatch += 1
                if len(samples) < 5:
                    samples.append(
                        {
                            "type": "mismatch",
                            "txid": txid,
                            reference_name: op_sig(reference_op),
                            "actual": op_sig(op),
                        }
                    )
            else:
                exact += 1
        summary[name] = {
            "exact": exact,
            "missing": missing,
            "mismatch": mismatch,
            "samples": samples,
        }
        if missing or mismatch:
            message = f"{name}: recent ops vs {reference_name} exact={exact} missing={missing} mismatch={mismatch}"
            if warning_only:
                warnings.append(message)
            else:
                failures.append(message)
    return summary


def compare_prod_feeds(prod1_ops, prod2_ops, failures):
    summary = {
        "reference": "symmetric_prod1_prod2",
        "prod1_fetched": len(prod1_ops),
        "prod2_fetched": len(prod2_ops),
    }
    if not prod1_ops or not prod2_ops:
        failures.append("prod1/prod2: one recent operation feed is empty")
        return summary

    prod1_scores = [int(op["opScore"]) for op in prod1_ops]
    prod2_scores = [int(op["opScore"]) for op in prod2_ops]
    overlap_oldest = max(min(prod1_scores), min(prod2_scores))
    overlap_newest = min(max(prod1_scores), max(prod2_scores))
    summary["overlap_oldest"] = str(overlap_oldest)
    summary["overlap_newest"] = str(overlap_newest)
    if overlap_oldest > overlap_newest:
        failures.append("prod1/prod2: recent operation feeds have no score overlap")
        return summary

    feeds = {}
    for name, ops in (("prod1", prod1_ops), ("prod2", prod2_ops)):
        feeds[name] = {
            op["txIdRev"]: op
            for op in ops
            if overlap_oldest <= int(op["opScore"]) <= overlap_newest
        }

    exact = mismatch = 0
    only_prod1 = []
    only_prod2 = []
    mismatch_samples = []
    for txid in sorted(set(feeds["prod1"]) | set(feeds["prod2"])):
        prod1_op = feeds["prod1"].get(txid)
        prod2_op = feeds["prod2"].get(txid)
        if prod1_op is None:
            if len(only_prod2) < 5:
                only_prod2.append({"txid": txid, "sig": op_sig(prod2_op)})
        elif prod2_op is None:
            if len(only_prod1) < 5:
                only_prod1.append({"txid": txid, "sig": op_sig(prod1_op)})
        elif op_sig(prod1_op) != op_sig(prod2_op):
            mismatch += 1
            if len(mismatch_samples) < 5:
                mismatch_samples.append(
                    {"txid": txid, "prod1": op_sig(prod1_op), "prod2": op_sig(prod2_op)}
                )
        else:
            exact += 1

    prod1_only_count = len(set(feeds["prod1"]) - set(feeds["prod2"]))
    prod2_only_count = len(set(feeds["prod2"]) - set(feeds["prod1"]))
    summary.update(
        {
            "exact": exact,
            "mismatch": mismatch,
            "only_prod1": prod1_only_count,
            "only_prod2": prod2_only_count,
            "samples": {
                "only_prod1": only_prod1,
                "only_prod2": only_prod2,
                "mismatch": mismatch_samples,
            },
        }
    )
    if prod1_only_count or prod2_only_count or mismatch:
        failures.append(
            "prod1/prod2: symmetric recent ops "
            f"exact={exact} only_prod1={prod1_only_count} "
            f"only_prod2={prod2_only_count} mismatch={mismatch}"
        )
    return summary


def check_recent_ops(report, failures, degradations, ops_window, check_legacy=False):
    warnings = report.setdefault("warnings", [])
    statuses = report.get("statuses") or {}
    unavailable = [
        name for name in INDEXER_TARGETS if not isinstance(statuses.get(name), dict)
    ]
    if unavailable:
        report["recent_ops"] = {
            "reference": "symmetric_prod1_prod2",
            "checked": 0,
            "skipped": f"status unavailable for {','.join(unavailable)}",
        }
        return
    prod1_result = safe_fetch_ops(BASES["prod1"], ops_window)
    if not prod1_result["ok"]:
        add_degradation(
            degradations,
            "prod1_recent_ops_fetch",
            f"prod1: recent ops fetch failed after retries: {prod1_result['error']}",
        )
        report["recent_ops"] = {"reference": "prod1", "checked": 0}
        return
    prod1_ops = prod1_result["value"]

    prod2_result = safe_fetch_ops(BASES["prod2"], ops_window)
    if not prod2_result["ok"]:
        add_degradation(
            degradations,
            "prod2_recent_ops_fetch",
            f"prod2: recent ops fetch failed after retries: {prod2_result['error']}",
        )
        report["recent_ops"] = {"reference": "symmetric_prod1_prod2", "checked": 0}
        return
    prod2_ops = prod2_result["value"]

    summary = compare_prod_feeds(prod1_ops, prod2_ops, failures)

    if not check_legacy:
        report["recent_ops"] = summary
        return

    try:
        legacy_ops = fetch_ops(BASES["legacy"], ops_window)
    except Exception as exc:
        warnings.append(f"legacy: recent ops fetch failed: {type(exc).__name__}: {exc}")
    else:
        summary["legacy_optional"] = compare_recent_ops(
            "legacy", legacy_ops, INDEXER_TARGETS, failures, warnings, warning_only=True
        )
    report["recent_ops"] = summary


def check_known_txs(report, failures, degradations):
    known = {}
    statuses = report.get("statuses") or {}
    for label, expected in KNOWN_TXS.items():
        if not isinstance(statuses.get(expected["host"]), dict):
            known[label] = {"found": False, "skipped": "host status unavailable"}
            continue
        op, payload = op_by_tx(BASES[expected["host"]], expected["txid"])
        actual = {
            "found": bool(op),
            "op": op.get("op") if op else None,
            "tick": op.get("tick") if op else None,
            "tokenId": str((op.get("opData") or {}).get("tokenId")) if op else None,
            "fee": str(op.get("feeRev")) if op else None,
        }
        known[label] = actual
        if not op:
            if not payload.get("ok"):
                add_degradation(
                    degradations,
                    f"{label}_probe",
                    f"{label}: tx probe failed after retries: {payload.get('error')}",
                )
            else:
                failures.append(f"{label}: tx missing on {expected['host']}")
        elif (
            (expected.get("op") and actual["op"] != expected["op"])
            or actual["tick"] != expected["tick"]
            or actual["tokenId"] != expected["token_id"]
            or (expected.get("fee") and actual["fee"] != expected["fee"])
        ):
            failures.append(f"{label}: expected {expected['tick']}#{expected['token_id']} got {actual}")
    report["known_txs"] = known


def check_hosts(
    report, failures, degradations, expected_commit=None, expected_binary_sha256=None
):
    hosts = {}
    warnings = report.setdefault("warnings", [])
    for name, info in HOSTS.items():
        snapshot = host_snapshot(name, info)
        hosts[name] = snapshot
        if not snapshot.get("ssh_ok"):
            add_degradation(
                degradations,
                f"{name}_ssh",
                f"{name}: SSH check failed after retries: {snapshot.get('raw_error')}",
            )
            continue
        if snapshot.get("service") != "active":
            failures.append(f"{name}: indexer service {snapshot.get('service')}")
        if info.get("process_pattern"):
            try:
                process_count = int(snapshot.get("process_count", "0"))
            except ValueError:
                process_count = 0
            if process_count != 1:
                failures.append(f"{name}: process_count={process_count}")
        if info.get("nginx_host") and snapshot.get("nginx_route_http") != "200":
            failures.append(f"{name}: nginx route http={snapshot.get('nginx_route_http')}")
        expected_node_rpc = info.get("expected_node_rpc")
        if expected_node_rpc and expected_node_rpc not in snapshot.get("exec_start", ""):
            failures.append(f"{name}: node-rpc does not include {expected_node_rpc}")
        if snapshot.get("node_service") != "active":
            if info.get("node_service_optional") and expected_node_rpc in snapshot.get("exec_start", ""):
                pass
            else:
                failures.append(f"{name}: node/tunnel service {snapshot.get('node_service')}")
        expected_node_binary = info.get("expected_node_binary")
        if expected_node_binary and expected_node_binary not in snapshot.get("node_exec_start", ""):
            failures.append(f"{name}: node binary does not include {expected_node_binary}")
        if info.get("forbidden_process_pattern"):
            try:
                forbidden_process_count = int(snapshot.get("forbidden_process_count", "0"))
            except ValueError:
                forbidden_process_count = 0
            if forbidden_process_count > 0:
                failures.append(f"{name}: forbidden_process_count={forbidden_process_count}")
        if info.get("proxy_service") and snapshot.get("proxy_service") != "active":
            failures.append(f"{name}: proxy service {snapshot.get('proxy_service')}")
        if expected_commit and snapshot.get("commit") != expected_commit:
            failures.append(f"{name}: commit {snapshot.get('commit')} expected {expected_commit}")
        if expected_binary_sha256 and snapshot.get("binary_sha256") != expected_binary_sha256:
            failures.append(
                f"{name}: binary SHA-256 {snapshot.get('binary_sha256')} "
                f"expected {expected_binary_sha256}"
            )
        try:
            sync_errors = int(snapshot.get("sync_task_errors", "0"))
        except ValueError:
            sync_errors = 0
        if sync_errors > 0:
            warnings.append(f"{name}: recent sync_task_errors={sync_errors}")

        try:
            block_not_found = int(snapshot.get("block_not_found", "0"))
        except ValueError:
            block_not_found = 0
        try:
            forced_score_cleanup = int(snapshot.get("forced_score_cleanup", "0"))
        except ValueError:
            forced_score_cleanup = 0
        if block_not_found > forced_score_cleanup:
            failures.append(
                f"{name}: block_not_found={block_not_found} forced_score_cleanup={forced_score_cleanup}"
            )
    if all(name in hosts for name in INDEXER_TARGETS):
        prod1 = hosts["prod1"]
        prod2 = hosts["prod2"]
        if prod1.get("commit") != prod2.get("commit"):
            failures.append(
                f"prod1/prod2: deployed commits differ prod1={prod1.get('commit')} prod2={prod2.get('commit')}"
            )
        if prod1.get("version") != prod2.get("version"):
            failures.append(
                f"prod1/prod2: deployed versions differ prod1={prod1.get('version')} prod2={prod2.get('version')}"
            )
        if not prod1.get("binary_sha256") or prod1.get("binary_sha256") != prod2.get("binary_sha256"):
            failures.append(
                "prod1/prod2: binary SHA-256 differs "
                f"prod1={prod1.get('binary_sha256')} prod2={prod2.get('binary_sha256')}"
            )
    report["hosts"] = hosts


def read_state():
    try:
        return json.loads(STATE_PATH.read_text())
    except Exception:
        return {}


def write_state(state):
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    STATE_PATH.write_text(json.dumps(state, sort_keys=True) + "\n")


def telegram_send(text):
    for path in ENV_PATHS:
        load_env_file(path)
    token = os.environ.get("KRC721_TELEGRAM_BOT_TOKEN") or os.environ.get("TELEGRAM_BOT_TOKEN")
    chat_id = (
        os.environ.get("KRC721_ALERT_CHAT_ID")
        or os.environ.get("TELEGRAM_ALERT_CHAT_ID")
        or DEFAULT_ALERT_CHAT_ID
    )
    if not token or not chat_id:
        return {"sent": False, "reason": "missing token/chat_id"}
    data = urllib.parse.urlencode(
        {
            "chat_id": chat_id,
            "text": text,
            "disable_web_page_preview": "true",
        }
    ).encode()
    request = urllib.request.Request(
        f"https://api.telegram.org/bot{token}/sendMessage",
        data=data,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            payload = json.load(response)
        return {"sent": bool(payload.get("ok")), "telegram_ok": payload.get("ok")}
    except Exception as exc:
        return {"sent": False, "reason": f"{type(exc).__name__}: {exc}"}


def incident_fingerprint(failures):
    canonical = json.dumps(sorted(failures), separators=(",", ":"))
    return hashlib.sha256(canonical.encode()).hexdigest()[:16] if failures else ""


def maybe_alert(report, failures, degradations, no_alert=False):
    if no_alert:
        state = read_state()
        report["alert"] = {"sent": False, "reason": "no_alert"}
        report["state"] = {
            "status": state.get("status", "ok"),
            "consecutive_failures": int(state.get("consecutive_failures", 0)),
            "healthy_streak": int(state.get("healthy_streak", 0)),
            "last_alert_at": int(state.get("last_alert_at", 0)),
            "dry_run": True,
        }
        return

    state = read_state()
    now = int(time.time())
    consecutive_failures = int(state.get("consecutive_failures", 0))
    healthy_streak = int(state.get("healthy_streak", 0))
    last_alert_at = int(state.get("last_alert_at", 0))

    if failures:
        consecutive_failures += 1
        healthy_streak = 0
        status = "failing"
    elif degradations:
        consecutive_failures = 0
        healthy_streak = 0
        status = "degraded"
    else:
        consecutive_failures = 0
        healthy_streak += 1
        status = "ok"

    alert = None
    alert_active = bool(state.get("alert_active", False))

    escalated_degradation = bool(report.get("escalated_degradations"))
    new_alert = (
        status == "failing"
        and (consecutive_failures >= CRITICAL_ALERT_RUNS or escalated_degradation)
        and not alert_active
    )
    repeat_alert = (
        status == "failing"
        and alert_active
        and now - last_alert_at >= REPEAT_ALERT_SECONDS
    )
    if new_alert or repeat_alert:
        text = (
            "KRC721 indexer monitor ALERT\n"
            f"time: {report['ts']}\n"
            f"failures: {len(failures)}\n"
            + "\n".join(f"- {item}" for item in failures[:8])
            + "\n\nRun: systemctl status krc721-indexer-parity-monitor.service"
        )
        alert = telegram_send(text)
        if alert.get("sent"):
            last_alert_at = now
            alert_active = True
            state["active_incident_fingerprint"] = incident_fingerprint(failures)
    elif status == "ok" and alert_active and healthy_streak >= RECOVERY_RUNS:
        text = (
            "KRC721 indexer monitor RECOVERED\n"
            f"time: {report['ts']}\n"
            "prod1/prod2 health checks passed."
        )
        alert = telegram_send(text)
        if alert.get("sent"):
            last_alert_at = now
            alert_active = False
            state["active_incident_fingerprint"] = ""
    else:
        alert = {"sent": False, "reason": "threshold_not_met"}

    state.update(
        {
            "status": status,
            "consecutive_failures": consecutive_failures,
            "healthy_streak": healthy_streak,
            "last_alert_at": last_alert_at,
            "alert_active": alert_active,
            "last_run_at": now,
            "last_failures": failures[:20],
            "counter_drift_signature": report.get("counter_drift_signature", ""),
            "counter_drift_streak": int(report.get("counter_drift_streak", 0)),
            "degradation_streaks": report.get("degradation_streaks", {}),
        }
    )
    write_state(state)
    report["alert"] = alert
    report["state"] = {
        "status": status,
        "consecutive_failures": consecutive_failures,
        "healthy_streak": healthy_streak,
        "last_alert_at": last_alert_at,
        "alert_active": alert_active,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ops-window", type=int, default=100)
    parser.add_argument("--blue-lag-tolerance", type=int, default=360)
    parser.add_argument("--expected-commit", default="")
    parser.add_argument("--expected-binary-sha256", default="")
    parser.add_argument("--check-legacy", action="store_true")
    parser.add_argument("--no-alert", action="store_true")
    args = parser.parse_args()

    report = {"ts": utc_now()}
    failures = []
    degradations = []
    check_statuses(
        report,
        failures,
        degradations,
        args.blue_lag_tolerance,
        check_legacy=args.check_legacy,
    )
    escalate_persistent_counter_drift(report, failures)
    check_recent_ops(
        report,
        failures,
        degradations,
        args.ops_window,
        check_legacy=args.check_legacy,
    )
    check_known_txs(report, failures, degradations)
    check_hosts(
        report,
        failures,
        degradations,
        args.expected_commit,
        args.expected_binary_sha256,
    )
    escalate_degradations(report, failures, degradations)
    report["ok"] = not failures
    report["failures"] = failures
    maybe_alert(report, failures, degradations, no_alert=args.no_alert)
    print(json.dumps(report, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
