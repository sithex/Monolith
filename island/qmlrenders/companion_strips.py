#!/usr/bin/env python3
"""Assemble real offscreen captures and verify pixel changes/settling. Run from any cwd."""
from pathlib import Path
import json
from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "evidence/companion-pack"
IDS = ["notchmimic", "obelisk", "companioncube", "nurgling", "hollowknight"]
STATES = ["normal", "hover", "sleep", "habit", "settled"]
summary = {}
for factor in (1, 4):
    tile = 100
    sheet = Image.new("RGB", (5 * tile + 120, 5 * 110 + 30), "#121215")
    draw = ImageDraw.Draw(sheet)
    for col, state in enumerate(STATES):
        draw.text((120 + col * tile, 5), state, fill="#f1ede4")
    for row, id_ in enumerate(IDS):
        draw.text((5, 40 + row * 110), id_, fill="#f1ede4")
        frames = {}
        for col, state in enumerate(STATES):
            im = Image.open(OUT / f"renders/{id_}-{factor}x-{state}.png").convert("RGB")
            assert im.size == (20 * factor, 20 * factor), im.size
            frames[state] = im
            im = im.resize((80, 80), Image.Resampling.NEAREST)
            sheet.paste(im, (120 + col * tile, 30 + row * 110))
        def changed(a, b):
            diff = ImageChops.difference(frames[a], frames[b])
            return sum(isinstance(p, tuple) and max(p) > 1 for p in diff.get_flattened_data())
        # +/- one channel value is software antialias cache rounding, not motion.
        for state in ["timed-before", "timed-after"] + [f"timed-{i}" for i in range(1,7)]:
            frames[state] = Image.open(OUT / f"renders/{id_}-{factor}x-{state}.png").convert("RGB")
        timed_changes = [changed("timed-before", f"timed-{i}") for i in range(1,7)]
        result = {"timed_changed_pixels": timed_changes,
                  "timed_settled_changed_pixels": changed("timed-before", "timed-after"),
                  "habit_changed_pixels": changed("normal", "habit"),
                  "settled_changed_pixels": changed("normal", "settled"),
                  "hover_changed_pixels": changed("normal", "hover"),
                  "sleep_changed_pixels": changed("normal", "sleep")}
        assert max(timed_changes) > 0, (id_, result)
        assert result["timed_settled_changed_pixels"] == 0, (id_, result)
        assert result["habit_changed_pixels"] > 0, (id_, result)
        assert result["settled_changed_pixels"] == 0, (id_, result)
        assert result["hover_changed_pixels"] > 0, (id_, result)
        summary[f"{id_}-{factor}x"] = result
    sheet.save(OUT / f"strips-{factor}x.png")
    timeline = ["timed-before"] + [f"timed-{i}" for i in range(1,7)] + ["timed-after"]
    sheet = Image.new("RGB", (len(timeline) * 90 + 120, 580), "#121215")
    draw = ImageDraw.Draw(sheet)
    for col, state in enumerate(timeline):
        draw.text((120 + col * 90, 5), state, fill="#f1ede4")
    for row, id_ in enumerate(IDS):
        draw.text((5, 40 + row * 110), id_, fill="#f1ede4")
        for col, state in enumerate(timeline):
            im = Image.open(OUT / f"renders/{id_}-{factor}x-{state}.png").convert("RGB")
            sheet.paste(im.resize((80,80), Image.Resampling.NEAREST), (120 + col * 90, 30 + row * 110))
    sheet.save(OUT / f"timelines-{factor}x.png")
(OUT / "image-diffs.json").write_text(json.dumps(summary, indent=2) + "\n")
# Rasterize the actual authored picker SVG fragments for dark/light inspection.
import re
import subprocess
source = (ROOT / "island-tauri/ui/island/companionCharacters.tsx").read_text()
picker = OUT / "picker"
picker.mkdir(exist_ok=True)
sheet = Image.new("RGB", (600, 280), "#17171b")
draw = ImageDraw.Draw(sheet)
draw.rectangle((0, 140, 600, 280), fill="#eeeae3")
for index, id_ in enumerate(IDS):
    match = re.search(r"const " + id_ + r" = \(\) => (<svg.*?</svg>);", source)
    assert match, id_
    svg = match.group(1).replace('<svg ', '<svg xmlns="http://www.w3.org/2000/svg" ')
    svg = svg.replace("strokeWidth", "stroke-width")
    (picker / f"{id_}.svg").write_text(svg)
    subprocess.run(["rsvg-convert", "-w", "112", "-h", "112", str(picker / f"{id_}.svg"),
                    "-o", str(picker / f"{id_}.png")], check=True)
    im = Image.open(picker / f"{id_}.png")
    for row in range(2):
        sheet.paste(im, (index * 120 + 4, row * 140 + 24), im)
    draw.text((index * 120 + 4, 5), id_, fill="white")
sheet.save(picker / "previews.png")
print(json.dumps(summary, indent=2))
