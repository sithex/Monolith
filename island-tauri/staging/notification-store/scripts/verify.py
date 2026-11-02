#!/usr/bin/env python3
"""Crate-local, reproducible gates and compiling behavior mutations.

All writes stay below this crate. No process killing, sibling build, or live IO.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "target" / "verification"


def run(args):
    result = subprocess.run(args, cwd=ROOT, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            check=False)
    return result.returncode, result.stdout


def gates():
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    transcript = ""
    for command in [
        ["nice", "-n", "10", "cargo", "fmt"],
        ["nice", "-n", "10", "cargo", "fmt", "--check"],
        ["nice", "-n", "10", "cargo", "clippy", "--all-targets", "--locked", "--", "-D", "warnings"],
        ["nice", "-n", "10", "cargo", "test", "--locked"],
    ]:
        code, output = run(command)
        transcript += "$ " + " ".join(command) + "\n" + output + f"exit={code}\n\n"
        print(transcript.split("$ ")[-1], end="", flush=True)
        (EVIDENCE / "gates.log").write_text(transcript)
        if code:
            raise SystemExit(code)


MUTATIONS = [
    ("M1", "store.rs", "self.entries.pop_back()", "self.entries.pop_front()",
     "Evict newest instead of oldest", "ring_evicts_oldest_and_reports_closed_before_received"),
    ("M2", "store.rs", "self.entries.push_front(Entry {", "self.entries.push_back(Entry {",
     "Append inserts and dedupe updates at the back", "dedupe_moves_to_front_keeps_id_updates_fields_and_refreshes_ttl"),
    ("M3", "store.rs", 'input.app_name == "Hermes Island" || ', "",
     "Remove own-app loop guard", "loop_guard_and_deny_list_drop_without_deltas_or_id_consumption"),
    ("M4", "store.rs", "toast: !self.policy.dnd &&", "toast: true &&",
     "Ignore DND for toast decisions", "dnd_stores_and_suppresses_even_critical_toasts_low_is_always_silent"),
    ("M5", "store.rs", "now_ms >= deadline", "now_ms > deadline",
     "Miss exact TTL expiry boundary", "ttl_boundaries_zero_resident_and_critical_transient_policy"),
    ("M6", "dbus.rs", "self.pending.pop_back()", "self.pending.pop_front()",
     "Evict most-recent correlation instead of LRU", "lru_evicts_least_recent_serial_and_touch_refreshes"),
    ("M7", "ingest.rs", "payload.len() > MAX_LINE_BYTES", "payload.len() >= MAX_LINE_BYTES",
     "Reject exactly 16 KiB payload", "line_cap_exact_boundary_counts_bytes_excluding_lf_or_crlf"),
    ("M8", "ingest.rs", "/ 1000.0", "/ 2000.0",
     "Refill bucket at half the required rate", "rate_limiter_refills_fractionally_at_twenty_per_second_and_caps_burst"),
    ("M9", "store.rs", 'Some("Hidden".into())', 'Some("Visible".into())',
     "Break incoming-body redaction", "redaction_is_applied_before_storage_and_emission"),
    ("M10", "dbus.rs", "p.sender == destination && p.serial == reply_serial", "p.serial == reply_serial",
     "Correlate serials without caller identity", "correlation_is_per_sender_and_unmatched_returns_never_create_items"),
]


def mutations():
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    pristine = EVIDENCE / "pristine"
    if pristine.exists():
        shutil.rmtree(pristine)  # only this helper's private, ignored snapshot
    shutil.copytree(ROOT, pristine, ignore=shutil.ignore_patterns("target"))
    rows = []
    for ident, filename, old, new, description, required in MUTATIONS:
        path = ROOT / "src" / filename
        original = path.read_bytes()
        source = original.decode()
        # M9 intentionally changes both redaction sites; all others must be unique.
        count = source.count(old)
        if count < 1 or (ident != "M9" and count != 1):
            raise RuntimeError(f"{ident}: expected mutation anchor, found {count}")
        try:
            path.write_text(source.replace(old, new))
            os.utime(path, None)
            code, output = run(["nice", "-n", "10", "cargo", "test", "--locked", "--no-fail-fast"])
            (EVIDENCE / f"{ident}.log").write_text(output)
            failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", output, re.M)))
            valid = code == 101 and required in failed and "could not compile" not in output
        finally:
            path.write_bytes(original)
            os.utime(path, None)  # force Cargo to rebuild pristine source
            diff_code, diff_output = run(["diff", "-r", "--exclude=target", str(pristine), str(ROOT)])
            (EVIDENCE / f"{ident}.diff.log").write_text(diff_output)
        row = dict(id=ident, description=description, exit=code, failed_tests=failed,
                   required_test=required, runtime_killed=valid,
                   restored_diff_exit=diff_code, restored_diff=diff_output)
        rows.append(row)
        (EVIDENCE / "mutations.json").write_text(json.dumps(rows, indent=2) + "\n")
        print(json.dumps(row), flush=True)
        if not valid or diff_code != 0 or diff_output:
            raise SystemExit(f"{ident}: mutation did not meet acceptance")
    print(f"{len(rows)} compiling mutations killed; every restoration diff empty.")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in {"gates", "mutations"}:
        raise SystemExit("usage: python3 scripts/verify.py gates|mutations")
    {"gates": gates, "mutations": mutations}[sys.argv[1]]()
