#!/usr/bin/env python3
"""Render the pixel companions through the real brain and assemble 1x/8x contact sheets.

Usage: render_pixel_companions.py --output DIR [--ids a,b]
Writes DIR/frames/*.png (true-size captures: each character's own 20-26 px square slot), DIR/sheet-1x.png and DIR/sheet-8x.png.
"""
import argparse, json, os, subprocess, tempfile
from pathlib import Path
from PIL import Image, ImageDraw
import qt6_tools

ROOT = Path(__file__).resolve().parents[2]
ap = argparse.ArgumentParser()
ap.add_argument("--output", type=Path, required=True)
ap.add_argument("--ids", default="companioncube,nurgling,hollowknight,frog")
a = ap.parse_args()
out = a.output.resolve(); frames = out / "frames"; frames.mkdir(parents=True, exist_ok=True)
ids = a.ids.split(",")
src = (ROOT / "island/qmlrenders/tst_companion_pixel.qml").read_text()
src = src.replace('"../plasma-anchor/contents/ui"', json.dumps((ROOT / "island/plasma-anchor/contents/ui").as_uri()))
src = src.replace('outputDirectory: "/tmp/companion-pixel"', "outputDirectory: " + json.dumps(str(frames)))
src = src.replace('characterIds: ["companioncube", "nurgling", "hollowknight", "frog"]', "characterIds: " + json.dumps(ids))
with tempfile.TemporaryDirectory(prefix="companion-pixel-", dir=os.environ.get("TMPDIR")) as tmp:
    h = Path(tmp) / "tst_companion_pixel.qml"; h.write_text(src)
    r = subprocess.run([qt6_tools.require("qmltestrunner"), "-input", str(h)], capture_output=True, text=True,
                       env={**os.environ, "QT_QPA_PLATFORM": "offscreen", "QT_QUICK_BACKEND": "software"})
    (out / "render-log.txt").write_text(r.stdout + r.stderr)
    if r.returncode: print(r.stdout[-3000:], r.stderr[-3000:]); raise SystemExit(r.returncode)
looks = ["look-center", "look-left", "look-right", "look-up", "look-down", "look-near-left", "look-near-right", "look-downright", "blink", "hover", "sleep"]
for scale in (1, 8):
    rows = []
    for id_ in ids:
        hab = sorted([p.stem[len(id_) + 1:] for p in frames.glob(id_ + "-habit-*.png")], key=lambda s: (s.split("-")[1], int(s.split("-")[2])))
        rows.append((id_, looks, 26))
        if id_ == "isaac":
            rows.append((id_, [f"flow-{f}" for f in range(8)], 26))
        rows.append((id_, hab, 40 if id_ == "isaac" else 26))
    gap = 4 if scale == 1 else 8
    sheet_width = max(len(names) * (width * scale + gap) for _, names, width in rows)
    top = 0 if scale == 1 else 14
    row_height = 26 * scale + gap + top
    sheet = Image.new("RGB", (sheet_width + 4, len(rows) * row_height + 4), "#0b0b0c")
    d = ImageDraw.Draw(sheet)
    for ri, (id_, names, width) in enumerate(rows):
        cell = width * scale + gap
        for ci, n in enumerate(names):
            f = frames / f"{id_}-{n}.png"
            if not f.exists(): continue
            im = Image.open(f).convert("RGB")
            if id_ == "isaac" and n.startswith("habit-"):
                assert im.size == (40, 26), (f, im.size)
            else:
                assert im.width == im.height and 20 <= im.width <= 26, (f, im.size)
            x, y = 4 + ci * cell, 4 + ri * row_height
            if scale > 1: d.text((x, y), n.replace("look-", "")[:12], fill="#bbbbbb")
            sheet.paste(im.resize((im.width * scale, im.height * scale), Image.NEAREST), (x, y + top))
    sheet.save(out / f"sheet-{scale}x.png")
print(out / "sheet-8x.png")
