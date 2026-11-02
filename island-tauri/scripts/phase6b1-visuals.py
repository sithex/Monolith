#!/usr/bin/env python3
"""Baseline/current settled composited view pairs; no synthetic UI or image correction."""
from pathlib import Path
from PIL import Image,ImageDraw
root=Path(__file__).resolve().parents[1]
out=root/'evidence/phase6b1'
for view in ['terminal','monitor','ai','settings']:
    name=f'overlay-{view}-KWIN-COMPOSITED.png'
    a=Image.open(root/'evidence/phase3/phase2-baseline'/name).convert('RGB')
    b=Image.open(root/'evidence/phase2'/name).convert('RGB')
    assert a.size==b.size
    box=(250,0,1350,680)
    a,b=a.crop(box),b.crop(box)
    pair=Image.new('RGB',(2200,710),'#202020')
    draw=ImageDraw.Draw(pair)
    draw.text((12,8),f'BASELINE — {view}',fill='white')
    draw.text((1112,8),f'CURRENT PHASE 6b1 — {view}',fill='white')
    pair.paste(a,(0,30));pair.paste(b,(1100,30))
    path=out/f'final-{view}-comparison.png';pair.save(path)
    print(path)
