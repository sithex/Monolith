#!/usr/bin/env python3
"""Crate-local gates and compiling mutation checks. Never touches siblings."""
from pathlib import Path
import json
import os
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target" / "verification"
SOURCE = ROOT / "src" / "lib.rs"
CARGO = ["nice", "-n", "10", "cargo"]


def run(command):
    result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, check=False)
    return result.returncode, result.stdout


def gates():
    OUT.mkdir(parents=True, exist_ok=True)
    sections = []
    commands = [
        CARGO + ["generate-lockfile"],
        CARGO + ["fmt"],
        CARGO + ["fmt", "--check"],
        CARGO + ["clippy", "--all-targets", "--locked", "--", "-D", "warnings"],
        CARGO + ["test", "--locked"],
    ]
    for command in commands:
        status, output = run(command)
        section = "$ " + " ".join(command) + "\n" + output + f"exit={status}\n"
        print(section, end="", flush=True)
        sections.append(section)
        (OUT / "gates.log").write_text("\n".join(sections))
        if status:
            raise SystemExit(status)


# Each needle must match exactly once. Mutation results must include the pinned
# test below, not merely a build failure or some unrelated failing test.
MUTATIONS = [
    ("M1", "Import clears current state before parsing",
     "        let (next, report) = Self::from_json(json)?;",
     "        self.modules.clear();\n        let (next, report) = Self::from_json(json)?;",
     "import_atomic_on_syntax_semantic_version_and_size_errors"),
    ("M2", "Reorder bypasses permutation check",
     "if ids.len() != self.modules.len() || wanted.len() != ids.len() || wanted != present {",
     "if false && (ids.len() != self.modules.len() || wanted.len() != ids.len() || wanted != present) {",
     "reorder_rejects_every_non_permutation_atomically"),
    ("M3", "Current-version load drops unknown modules",
     "CURRENT_VERSION => Registry::new(raw.version, raw.modules),",
     "CURRENT_VERSION => Registry::new(raw.version, raw.modules.into_iter().filter(|m| m.id.is_known()).collect()),",
     "unknown_modules_preserved_verbatim_and_inert"),
    ("M4", "Version-0 migration enables unknown modules",
     "                    module.enabled = false;",
     "                    module.enabled = true;",
     "upgrade_never_enables_unknown_modules"),
    ("M5", "Remove enum rename_all_fields",
     '    rename_all_fields = "camelCase",\n', "",
     "json_shape_snapshot_all_modules_camel_case"),
    ("M6", "Accept nonfinite numbers in opaque config",
     "Self::Number(n) => n.is_finite(),",
     "Self::Number(_n) => true,",
     "unknown_nonfinite_payload_rejected_recursively"),
    ("M7", "Treat unknown stored enabled bit as active",
     "self.id.is_known() && self.enabled", "self.enabled",
     "unknown_modules_preserved_verbatim_and_inert"),
    ("M8", "Clipboard persistence legacy key is never mapped",
     '("clipboardPersist", ModuleConfig::Clipboard { persist, .. }) => {\n            assign_bool(persist, value)\n        }',
     '("clipboardPersist", ModuleConfig::Clipboard { persist: _persist, .. }) => false,',
     "realistic_flat_fixture_migrates_all_corresponding_keys_without_loss"),
]


def compare(snapshot):
    status, output = run(["diff", "-r", "--exclude=target", "--exclude=__pycache__",
                          str(snapshot), str(ROOT)])
    if status:
        raise RuntimeError(f"Restoration differs (exit={status}):\n{output}")
    return output


def mutations():
    OUT.mkdir(parents=True, exist_ok=True)
    snapshot = OUT / "pristine"
    if snapshot.exists():
        shutil.rmtree(snapshot)  # only this script's owned crate-local snapshot
    shutil.copytree(ROOT, snapshot, ignore=shutil.ignore_patterns("target", "__pycache__"))
    original = SOURCE.read_bytes()
    text = original.decode()
    rows = []
    for label, description, needle, replacement, required in MUTATIONS:
        compare(snapshot)
        if text.count(needle) != 1:
            raise RuntimeError(f"{label}: needle must match once, got {text.count(needle)}")
        try:
            SOURCE.write_text(text.replace(needle, replacement, 1))
            os.utime(SOURCE, None)
            status, output = run(CARGO + ["test", "--locked", "--no-fail-fast"])
            (OUT / f"{label}.log").write_text(output + f"\nexit={status}\n")
            failed = re.findall(r"^test (\w+) \.\.\. FAILED$", output, re.MULTILINE)
            if not status or required not in failed:
                raise RuntimeError(f"{label}: no pinned runtime failure; exit={status}\n{output}")
        finally:
            SOURCE.write_bytes(original)
            os.utime(SOURCE, None)  # do not let cargo reuse a mutated binary
            compare(snapshot)
        row = {"id": label, "breakage": description, "exit": status,
               "failed_tests": failed, "restored_diff_exit": 0,
               "restored_diff_output": ""}
        rows.append(row)
        (OUT / "mutations.json").write_text(json.dumps(rows, indent=2) + "\n")
        print(f"{label}: exit={status}; failed={', '.join(failed)}; restored diff exit=0, empty", flush=True)
    compare(snapshot)
    print("All mutations compiled, failed pinned tests, restored/touched and diffed identical.")


if __name__ == "__main__":
    if sys.argv[1:] == ["gates"]:
        gates()
    elif sys.argv[1:] == ["mutations"]:
        mutations()
    else:
        raise SystemExit("usage: python3 scripts/verify.py {gates|mutations}")
