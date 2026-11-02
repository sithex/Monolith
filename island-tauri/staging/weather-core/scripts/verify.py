#!/usr/bin/env python3
"""Reproduce nice-priority gates and compiling, reverted mutation checks.

All written evidence and pristine copies stay below this crate's target/.
This developer verification helper is not part of the pure Rust library.
"""
from pathlib import Path
import json
import os
import re
import shlex
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "target" / "verification"


def run(command):
    result = subprocess.run(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, check=False,
        env={**os.environ, "CARGO_TERM_COLOR": "never"},
    )
    text = f"$ {shlex.join(command)}\n{result.stdout}exit={result.returncode}\n"
    return result.returncode, text, result.stdout


def gates():
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    transcript = []
    commands = [
        ["nice", "-n", "10", "cargo", "fmt"],
        ["nice", "-n", "10", "cargo", "fmt", "--check"],
        ["nice", "-n", "10", "cargo", "clippy", "--all-targets", "--locked", "--", "-D", "warnings"],
        ["nice", "-n", "10", "cargo", "test", "--locked"],
    ]
    for command in commands:
        status, text, _ = run(command)
        transcript.append(text)
        print(text, flush=True)
        (EVIDENCE / "gates.log").write_text("\n".join(transcript))
        if status:
            raise SystemExit(status)


def mutations():
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    pristine = EVIDENCE / "pristine"
    # Reusing stale snapshots is forbidden; retain prior runs rather than delete.
    if pristine.exists():
        index = 1
        while (EVIDENCE / f"pristine-previous-{index}").exists():
            index += 1
        pristine.rename(EVIDENCE / f"pristine-previous-{index}")
    shutil.copytree(ROOT, pristine, ignore=shutil.ignore_patterns("target"))
    source = ROOT / "src" / "lib.rs"
    original = source.read_bytes()
    text = original.decode()
    cases = [
        ("M1", "Remove body-size cap before JSON decoding",
         "if body.len() > MAX_BODY_BYTES {", "if false {",
         "parsers_check_body_cap_before_json_including_exact_boundary"),
        ("M2", "Accept Fahrenheit as Celsius",
         'if unit.as_str() != Some("°C") {',
         'if unit.as_str() != Some("°C") && unit.as_str() != Some("°F") {',
         "forecast_requires_celsius_and_rejects_fahrenheit_or_any_other_unit"),
        ("M3", "Return same instant at exact refresh boundary",
         "900 - now_unix % 900", "(900 - now_unix % 900) % 900",
         "next_refresh_exact_boundary_is_strictly_next_quarter_hour"),
        ("M4", "Leave ampersand unescaped in query values",
         'b"-._~".contains(&byte)', 'b"-._~&".contains(&byte)',
         "geocode_percent_encoding_roundtrips_reserved_characters_and_utf8"),
        ("M5", "Drop last-good measurements on failure",
         "..last.clone()", "..{ let _ = last; WeatherState::default() }",
         "failure_keeps_entire_last_good_snapshot_stale_without_mutating_input"),
        ("M6", "Accept temperatures above 70 Celsius",
         "(-100.0..=70.0).contains(&temp_c)", "(-100.0..=700.0).contains(&temp_c)",
         "forecast_temperature_range_is_inclusive_and_rejects_outliers"),
        ("M7", "Accept is_day=2",
         "Some(1) => true", "Some(1) | Some(2) => true",
         "forecast_daylight_is_only_integer_zero_or_one"),
        ("M8", "Accept latitude just beyond north pole",
         "(-90.0..=90.0).contains(&latitude)", "(-90.0..=91.0).contains(&latitude)",
         "geocode_drops_invalid_coordinates_and_blank_names_but_keeps_boundaries"),
        ("M9", "Retain eleven geocoding results",
         ".take(10).collect()", ".take(11).collect()",
         "geocode_retains_first_ten_valid_results_in_original_order"),
        ("M10", "Mis-map thunderstorm to fog",
         "95 => Condition::Thunderstorm", "95 => Condition::Fog",
         "wmo_mapping_pins_every_requested_code_condition_and_stable_icon_pair"),
        ("M11", "Keep 201 API-reason characters",
         "reason.chars().take(200)", "reason.chars().take(201)",
         "api_reasons_truncate_at_two_hundred_unicode_characters_not_bytes"),
        ("M12", "Remove heavy-thunderstorm code 97 mapping",
         "97 => Condition::HeavyThunderstorm,",
         "// M12: heavy-thunderstorm arm removed",
         "wmo_mapping_pins_every_requested_code_condition_and_stable_icon_pair"),
        ("M13", "Reject undocumented in-range weather codes again",
         ".filter(|&n| n <= 99)",
         ".filter(|&n| n <= 99 && wmo_condition(n).is_some())",
         "forecast_codes_are_in_range_unsigned_integers_or_rejected"),
    ]
    results = []
    for mutation_id, description, before, after, expected in cases:
        if text.count(before) != 1:
            raise RuntimeError(f"{mutation_id}: mutation target is not unique")
        try:
            source.write_text(text.replace(before, after, 1))
            os.utime(source, None)
            status, log, output = run([
                "nice", "-n", "10", "cargo", "test", "--locked", "--no-fail-fast",
            ])
            (EVIDENCE / f"{mutation_id}.log").write_text(log)
            failed = re.findall(r"^test (\S+) \.\.\. FAILED$", output, re.MULTILINE)
            compiled = "Running tests/weather.rs" in output and "error[E" not in output
        finally:
            source.write_bytes(original)
            os.utime(source, None)  # Never preserve a stale mutated executable's mtime.
            diff_status, diff_log, diff_output = run([
                "diff", "-r", "--exclude=target", str(pristine), str(ROOT),
            ])
            (EVIDENCE / f"{mutation_id}.diff.log").write_text(diff_log)
        result = {
            "id": mutation_id, "description": description, "test_exit": status,
            "compiled": compiled, "failed_tests": failed,
            "expected_test": expected, "diff_exit": diff_status,
            "diff_empty": diff_output == "",
        }
        results.append(result)
        (EVIDENCE / "mutations.json").write_text(json.dumps(results, indent=2) + "\n")
        print(json.dumps(result), flush=True)
        if not (status == 101 and compiled and expected in failed and diff_status == 0 and not diff_output):
            raise SystemExit(f"{mutation_id}: not a compiling named-test kill with identical restoration")
    print(f"{len(results)} compiling mutations killed; every restoration diff empty.")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in {"gates", "mutations"}:
        raise SystemExit("usage: python3 scripts/verify.py gates|mutations")
    if not (ROOT / "Cargo.lock").is_file():
        raise SystemExit("Generate Cargo.lock first with nice -n 10 cargo generate-lockfile")
    {"gates": gates, "mutations": mutations}[sys.argv[1]]()
