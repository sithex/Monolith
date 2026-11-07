#!/usr/bin/env python3
"""Run the real QML capture harness into an explicit isolated evidence directory.

Qt Test rejects custom options, so set the output property in a disposable copy;
keep the checked-in harness's default and previous evidence untouched.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

import qt6_tools

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
out = args.output.resolve()
(out / "renders").mkdir(parents=True, exist_ok=True)
source = (ROOT / "island/qmlrenders/tst_companion_render.qml").read_text()
source = source.replace('"../plasma-anchor/contents/ui"', json.dumps((ROOT / "island/plasma-anchor/contents/ui").as_uri()))
old = 'readonly property string outputDirectory: Qt.resolvedUrl("../../evidence/companion-pack").toString().replace("file://", "")'
assert old in source
source = source.replace(old, 'readonly property string outputDirectory: ' + json.dumps(str(out)))
with tempfile.TemporaryDirectory(prefix="companion-render-", dir=os.environ["TMPDIR"]) as tmp:
    harness = Path(tmp) / "tst_companion_render.qml"
    harness.write_text(source)
    subprocess.run([qt6_tools.require("qmltestrunner"), "-input", str(harness), "-o", str(out / "render-log.txt") + ",txt"],
                   env={**os.environ, "QT_QPA_PLATFORM":"offscreen", "QT_QUICK_BACKEND":"software"}, check=True)
