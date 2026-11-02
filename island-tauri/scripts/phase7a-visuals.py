#!/usr/bin/env python3
"""Assemble actual settled Phase7a captures beside unchanged concept references."""
from pathlib import Path
import json
from PIL import Image, ImageDraw
ROOT = Path(__file__).resolve().parents[1]
EVID = ROOT/'evidence/phase7a'
concepts = sorted((ROOT.parent/'concept-pics').glob('*.png'))
assert len(concepts) == 4
sheet = Image.new('RGB', (1760, 1800), '#202023')
draw = ImageDraw.Draw(sheet)
for index, path in enumerate(concepts):
    image = Image.open(path).convert('RGB'); image.thumbnail((420, 580))
    x = 10 + 440*index
    sheet.paste(image,(x,35)); draw.text((x,12),path.name,fill='white')
paths = []
for row, theme in enumerate(['skin','neutral']):
    for column, view in enumerate(['terminal','monitor','ai','settings']):
        name=f'{theme}-{view}'
        path=EVID/f'{name}.png'; meta=json.loads((EVID/f'{name}.json').read_text())
        image=Image.open(path).convert('RGBA')
        x,y,w,h=(meta[key] for key in ['x','y','width','height'])
        image=image.crop((int(x),int(y),int(x+w+1),int(y+h+1)))
        canvas=Image.new('RGBA',image.size,'#202023');canvas.alpha_composite(image)
        image=canvas.convert('RGB');image.thumbnail((420,535))
        left=10+column*440;top=650+row*575
        sheet.paste(image,(left,top+25));draw.text((left,top),f'{name} (real WebKit window pixels)',fill='white')
        paths.append(str(path))
sheet.save(EVID/'concept-review.png')
(EVID/'screenshots.json').write_text(json.dumps({'engine':'Tauri Wry / WebKitGTK on private nested KWin','settle':'two identical consecutive window ARGB captures, >=250ms apart, bounded 8s','images':paths,'review':str(EVID/'concept-review.png')},indent=2)+'\n')
print('PASS: eight actual screenshots and concept-review.png assembled; visual review is an agent step, not inferred here')
