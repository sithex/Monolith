#!/usr/bin/env python3
"""Build review contact sheets from actual PNGs; no display/browser startup."""
import json
import pathlib
from PIL import Image, ImageDraw
ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / 'evidence/round2/review'
SHOTS = ROOT / 'evidence/round2/screenshots'
APPS = ['calendar', 'scratchpad', 'downloads', 'mixer', 'brightness', 'notifications', 'clipboard', 'music', 'quickinfo', 'modules']
def panel(canvas, path, x, y, w, h, crop=False):
    image = Image.open(path).convert('RGB')
    if crop:
        image = image.crop((170, 150, 930, 1100))
    image.thumbnail((w, h))
    canvas.paste(image, (x, y))
def main():
    OUT.mkdir(parents=True, exist_ok=True)
    concepts = sorted((ROOT.parents[1] / 'concept-pics').glob('*.png'))
    assert len(concepts) == 4
    for theme in ['skin', 'neutral']:
        sheet = Image.new('RGB', (1800, 1760), '#252528'); draw = ImageDraw.Draw(sheet)
        draw.text((10, 8), 'Original concepts (unchanged source) / actual Round 2 component crops below', fill='white')
        for n, path in enumerate(concepts):
            panel(sheet, path, n * 450 + 10, 40, 430, 470)
        for n, story in enumerate(APPS):
            x, y = n % 5 * 360, 540 + n // 5 * 600
            draw.text((x + 8, y), f'{theme} / {story}', fill='white')
            panel(sheet, SHOTS / f'{theme}-{story}.png', x + 8, y + 25, 344, 560, crop=True)
        path = OUT / f'concepts-and-{theme}.png'; sheet.save(path); print(path)
    report = json.loads((SHOTS / 'status.json').read_text())
    for theme in ['skin', 'neutral']:
        paths = [ROOT / p for p in report['paths'] if pathlib.Path(p).name.startswith(theme + '-')]
        for page in range((len(paths) + 7) // 8):
            sheet = Image.new('RGB', (1600, 1640), '#252528'); draw = ImageDraw.Draw(sheet)
            for n, path in enumerate(paths[page * 8:page * 8 + 8]):
                x, y = n % 4 * 400, n // 4 * 820
                draw.text((x + 8, y + 8), path.stem, fill='white')
                panel(sheet, path, x + 8, y + 35, 384, 760, crop=True)
            path = OUT / f'{theme}-states-{page + 1}.png'; sheet.save(path); print(path)
if __name__ == '__main__':
    main()
