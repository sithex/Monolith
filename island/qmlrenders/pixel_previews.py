#!/usr/bin/env python3
"""Print picker-preview SVGs (TSX) traced pixel-for-pixel from rendered companion frames.

Usage: pixel_previews.py FRAMES_DIR id [id ...]   (FRAMES_DIR from render_pixel_companions.py)
Pixels equal to the render background #131313 are transparent.
"""
import sys
from PIL import Image

def svg(path):
    im = Image.open(path).convert("RGB")
    parts = {}
    for y in range(im.height):
        x = 0
        while x < im.width:
            c = im.getpixel((x, y)); e = x + 1
            while e < im.width and im.getpixel((e, y)) == c: e += 1
            if c != (0x13, 0x13, 0x13):
                parts.setdefault("#%02x%02x%02x" % c, []).append(f"M{x} {y}h{e - x}v1h-{e - x}z")
            x = e
    paths = "".join(f'<path d="{"".join(v)}" fill="{k}"/>' for k, v in parts.items())
    return f'<svg viewBox="0 0 20 20" width="28" height="28" shapeRendering="crispEdges" aria-hidden="true">{paths}</svg>'

for id_ in sys.argv[2:]:
    print(f"const {id_} = () => {svg(f'{sys.argv[1]}/{id_}-look-center.png')};")
