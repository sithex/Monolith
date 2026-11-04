#!/usr/bin/env python3
"""Assemble real offscreen captures and verify pixel changes/settling. Run from any cwd."""
from pathlib import Path
import json
from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
import argparse
parser = argparse.ArgumentParser()
parser.add_argument("--output", type=Path, default=ROOT / "evidence/companion-pack")
OUT = parser.parse_args().output
IDS = ["notchmimic", "companioncube", "nurgling", "hollowknight"]
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
    native = Image.new("RGB", (300, len(IDS)*40+25), "#121215")
    nd = ImageDraw.Draw(native)
    for row, id_ in enumerate(IDS):
        nd.text((0,30+row*40), id_, fill="white")
        for col,state in enumerate(STATES):
            im=Image.open(OUT / f"renders/{id_}-{factor}x-{state}.png")
            # True 20px inspection, plus separate 4x renderer in main sheet.
            if factor == 4: im=im.resize((20,20),Image.Resampling.NEAREST)
            native.paste(im,(120+col*32,25+row*40))
    native.save(OUT / f"native-{factor}x.png")
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
# This grid is driven only by real brain pointer input, never art property writes.
tracking = {}
for factor in (1,4):
    states = ["left","center","right","up","down","settled"]
    frames = {s: Image.open(OUT / f"renders/cube-track-{factor}x-{s}.png").convert("RGB") for s in states}
    def diff(a,b):
        return sum(isinstance(p, tuple) and max(p)>1 for p in ImageChops.difference(frames[a],frames[b]).get_flattened_data())
    result = {s: diff("center",s) for s in states if s != "center"}
    assert all(result[s]>0 for s in ["left","right","up","down"]), result
    assert result["settled"] == 0, result
    assert diff("left","right")>0 and diff("up","down")>0
    tracking[f"{factor}x"] = result
    for native in [True,False]:
        tile = 20 if native else 80
        sheet = Image.new("RGB", (len(states)*100, tile+30), "#121215")
        draw = ImageDraw.Draw(sheet)
        for col,s in enumerate(states):
            draw.text((col*100,0),s,fill="white")
            sheet.paste(frames[s].resize((tile,tile),Image.Resampling.NEAREST),(col*100,25))
        sheet.save(OUT / f"cube-tracking-{factor}x-{'native' if native else 'enlarged'}.png")
(OUT / "tracking-diffs.json").write_text(json.dumps(tracking,indent=2)+"\n")
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
