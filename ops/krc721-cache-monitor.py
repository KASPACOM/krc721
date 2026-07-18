#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


TARGETS = {
    "dev": {
        "status": "https://krc721-cache-dev.kaspa.com/status",
        "metadata": "https://krc721-cache-dev.kaspa.com/krc721/testnet-10/metadata/ZTEST/1",
        "thumbnail": "https://krc721-cache-dev.kaspa.com/krc721/testnet-10/thumbnail/ZTEST/1",
    },
    "prod": {
        "status": "https://krc721-cache.kaspa.com/status",
        "metadata": "https://krc721-cache.kaspa.com/krc721/mainnet/metadata/YONATOSHI/6186",
        "thumbnail": "https://krc721-cache.kaspa.com/krc721/mainnet/thumbnail/YONATOSHI/6186",
    },
}

STATE_PATH = Path("/var/lib/krc721-cache-monitor/state.json")
ENV_PATHS = (Path("/root/.openclaw/.env"), Path("/root/.hermes/.env"))
DEFAULT_ALERT_CHAT_ID = "2090199766"
DEGRADED_ALERT_RUNS = 5
CRITICAL_ALERT_RUNS = 2
RECOVERY_RUNS = 2
REPEAT_ALERT_SECONDS = 3 * 60 * 60


def utc_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def load_env_file(path):
    try:
        lines = path.read_text().splitlines()
    except FileNotFoundError:
        return
    for line in lines:
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


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


def request(url, timeout=8, max_bytes=1_048_576):
    req = urllib.request.Request(
        url,
        headers={"User-Agent": "KRC721-cache-monitor/2.0", "Range": "bytes=0-1048575"},
    )
    started = time.monotonic()
    with urllib.request.urlopen(req, timeout=timeout) as response:
        body = response.read(max_bytes)
        return {
            "code": response.status,
            "content_type": response.headers.get_content_type(),
            "bytes_read": len(body),
            "elapsed_ms": round((time.monotonic() - started) * 1000),
            "body": body,
        }


def safe_request(url, timeout=8):
    return retry_call(lambda: request(url, timeout=timeout))


def add_degradation(degradations, key, message):
    degradations.append({"key": key, "message": message})


def check_public(report, failures, degradations):
    public = {}
    for name, urls in TARGETS.items():
        checks = {}

        status_result = safe_request(urls["status"])
        if not status_result["ok"]:
            add_degradation(
                degradations,
                f"{name}_public_transport",
                f"{name}: public cache failed after retries: {status_result['error']}",
            )
            checks["status"] = status_result
            checks["metadata"] = {"skipped": "status transport unavailable"}
            checks["thumbnail"] = {"skipped": "status transport unavailable"}
            public[name] = checks
            continue
        else:
            status = status_result["value"]
            checks["status"] = {
                key: value for key, value in status.items() if key != "body"
            }
            checks["status"]["attempts"] = status_result["attempts"]
            try:
                payload = json.loads(status["body"])
                if not isinstance(payload.get("metrics"), dict):
                    raise ValueError("missing metrics object")
            except Exception as exc:
                failures.append(
                    f"{name}: /status invalid JSON: {type(exc).__name__}: {exc}"
                )

        metadata_result = safe_request(urls["metadata"])
        if not metadata_result["ok"]:
            add_degradation(
                degradations,
                f"{name}_metadata_transport",
                f"{name}: metadata probe failed after retries: {metadata_result['error']}",
            )
            checks["metadata"] = metadata_result
        else:
            metadata = metadata_result["value"]
            checks["metadata"] = {
                key: value for key, value in metadata.items() if key != "body"
            }
            checks["metadata"]["attempts"] = metadata_result["attempts"]
            try:
                payload = json.loads(metadata["body"])
                if not isinstance(payload.get("name"), str) or not isinstance(
                    payload.get("image"), str
                ):
                    raise ValueError("missing name or image")
            except Exception as exc:
                failures.append(
                    f"{name}: metadata probe invalid: {type(exc).__name__}: {exc}"
                )

        thumbnail_result = safe_request(urls["thumbnail"], timeout=10)
        if not thumbnail_result["ok"]:
            add_degradation(
                degradations,
                f"{name}_thumbnail_transport",
                f"{name}: thumbnail probe failed after retries: {thumbnail_result['error']}",
            )
            checks["thumbnail"] = thumbnail_result
        else:
            thumbnail = thumbnail_result["value"]
            checks["thumbnail"] = {
                key: value for key, value in thumbnail.items() if key != "body"
            }
            checks["thumbnail"]["attempts"] = thumbnail_result["attempts"]
            if not thumbnail.get("content_type", "").startswith("image/"):
                failures.append(
                    f"{name}: thumbnail content-type={thumbnail.get('content_type')}"
                )
            elif not thumbnail["body"].startswith(
                (b"\x89PNG\r\n\x1a\n", b"\xff\xd8\xff", b"RIFF")
            ):
                failures.append(f"{name}: thumbnail has invalid image signature")

        public[name] = checks
    report["public"] = public


def run_ssh(command, timeout=8):
    def invoke():
        proc = subprocess.run(
            [
                "ssh",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "krc721-cache",
                command,
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"exit={proc.returncode}: {proc.stderr.strip()[:200]}")
        return proc

    return retry_call(invoke)


def check_host(report, failures, degradations):
    command = """
printf 'service=%s\\n' "$(systemctl is-active krc721-cache-prod.service || true)"
printf 'local_status=%s\\n' "$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' http://127.0.0.1:8282/status || true)"
printf 'disk_percent=%s\\n' "$(df --output=pcent / | tail -1 | tr -dc '0-9')"
printf 'restarts=%s\\n' "$(systemctl show krc721-cache-prod.service -p NRestarts --value 2>/dev/null || true)"
"""
    result = run_ssh(command)
    if not result["ok"]:
        add_degradation(
            degradations,
            "cache_host_ssh",
            f"host: SSH check failed after retries: {result['error']}",
        )
        report["host"] = {
            "ssh_ok": False,
            "error": result["error"],
            "attempts": result["attempts"],
        }
        return

    proc = result["value"]
    host = {"ssh_ok": True, "attempts": result["attempts"]}
    for line in proc.stdout.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            host[key] = value
    if host.get("service") != "active":
        failures.append(f"host: cache service={host.get('service')}")
    if host.get("local_status") != "200":
        failures.append(f"host: local /status HTTP {host.get('local_status')}")
    try:
        disk_percent = int(host.get("disk_percent", "100"))
    except ValueError:
        disk_percent = 100
    if disk_percent >= 90:
        failures.append(f"host: root disk usage={disk_percent}%")
    report["host"] = host


def read_state():
    try:
        return json.loads(STATE_PATH.read_text())
    except Exception:
        return {}


def write_state(state):
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    STATE_PATH.write_text(json.dumps(state, sort_keys=True) + "\n")


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


def telegram_send(text):
    for path in ENV_PATHS:
        load_env_file(path)
    token = os.environ.get("KRC721_TELEGRAM_BOT_TOKEN") or os.environ.get(
        "TELEGRAM_BOT_TOKEN"
    )
    chat_id = (
        os.environ.get("KRC721_ALERT_CHAT_ID")
        or os.environ.get("TELEGRAM_ALERT_CHAT_ID")
        or DEFAULT_ALERT_CHAT_ID
    )
    if not token or not chat_id:
        return {"sent": False, "reason": "missing token/chat_id"}
    data = urllib.parse.urlencode(
        {"chat_id": chat_id, "text": text, "disable_web_page_preview": "true"}
    ).encode()
    req = urllib.request.Request(
        f"https://api.telegram.org/bot{token}/sendMessage",
        data=data,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as response:
            payload = json.load(response)
        return {"sent": bool(payload.get("ok")), "telegram_ok": payload.get("ok")}
    except Exception as exc:
        return {"sent": False, "reason": f"{type(exc).__name__}: {exc}"}


def incident_fingerprint(failures):
    canonical = json.dumps(sorted(failures), separators=(",", ":"))
    return hashlib.sha256(canonical.encode()).hexdigest()[:16] if failures else ""


def update_alert_state(report, failures, degradations, no_alert=False):
    state = read_state()
    now = int(time.time())
    consecutive = int(state.get("consecutive_failures", 0))
    healthy_streak = int(state.get("healthy_streak", 0))
    alert_active = bool(state.get("alert_active", False))
    last_alert_at = int(state.get("last_alert_at", 0))

    if failures:
        consecutive += 1
        healthy_streak = 0
        status = "failing"
    elif degradations:
        consecutive = 0
        healthy_streak = 0
        status = "degraded"
    else:
        consecutive = 0
        healthy_streak += 1
        status = "ok"

    if no_alert:
        alert = {"sent": False, "reason": "no_alert"}
    else:
        escalated = bool(report.get("escalated_degradations"))
        new_alert = (
            failures
            and (consecutive >= CRITICAL_ALERT_RUNS or escalated)
            and not alert_active
        )
        repeat_alert = (
            failures
            and alert_active
            and now - last_alert_at >= REPEAT_ALERT_SECONDS
        )
        if new_alert or repeat_alert:
            text = (
                "KRC721 cache monitor ALERT\n"
                f"time: {report['ts']}\n"
                + "\n".join(f"- {failure}" for failure in failures[:8])
                + "\n\nRun: systemctl status krc721-cache-monitor.service"
            )
            alert = telegram_send(text)
            if alert.get("sent"):
                alert_active = True
                last_alert_at = now
                state["active_incident_fingerprint"] = incident_fingerprint(failures)
        elif (
            not failures
            and not degradations
            and alert_active
            and healthy_streak >= RECOVERY_RUNS
        ):
            alert = telegram_send(
                "KRC721 cache monitor RECOVERED\n"
                f"time: {report['ts']}\n"
                "Dev and prod cache checks passed twice."
            )
            if alert.get("sent"):
                alert_active = False
                last_alert_at = now
                state["active_incident_fingerprint"] = ""
        else:
            alert = {"sent": False, "reason": "threshold_not_met"}

    if not no_alert:
        state.update(
            {
                "status": status,
                "consecutive_failures": consecutive,
                "healthy_streak": healthy_streak,
                "alert_active": alert_active,
                "last_alert_at": last_alert_at,
                "last_run_at": now,
                "last_failures": failures[:20],
                "degradation_streaks": report.get("degradation_streaks", {}),
            }
        )
        write_state(state)
    report["alert"] = alert
    report["state"] = {
        "status": status,
        "consecutive_failures": consecutive,
        "healthy_streak": healthy_streak,
        "alert_active": alert_active,
        "last_alert_at": last_alert_at,
        "dry_run": no_alert,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-alert", action="store_true")
    parser.add_argument("--test-alert", action="store_true")
    args = parser.parse_args()

    if args.test_alert:
        result = telegram_send(
            "KRC721 cache monitor ENABLED\n"
            "Checks dev and prod every 2 minutes with retry and degradation debounce."
        )
        print(json.dumps(result, sort_keys=True))
        return 0 if result.get("sent") else 1

    report = {"ts": utc_now()}
    failures = []
    degradations = []
    check_public(report, failures, degradations)
    check_host(report, failures, degradations)
    escalate_degradations(report, failures, degradations)
    report["ok"] = not failures
    report["failures"] = failures
    update_alert_state(report, failures, degradations, no_alert=args.no_alert)
    print(json.dumps(report, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
