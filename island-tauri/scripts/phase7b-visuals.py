#!/usr/bin/env python3
"""Review actual native captures; checkpoint cannot masquerade as complete visuals."""
import argparse
import json
from pathlib import Path
from PIL import Image, ImageDraw
ROOT=Path(__file__).resolve().parents[1]
EVID=ROOT/'evidence/phase7b'
SCENES=['launcher','terminal','monitor','ai','settings','notifications','clipboard','music','timer','calendar','scratchpad','downloads','controls','mixer','brightness','agents','updates','quickInfo']
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--checkpoint',action='store_true');args=parser.parse_args()
    scenes=SCENES if args.checkpoint else SCENES+['edit','layout']
    paths=[EVID/f'{theme}-{scene}.png' for theme in ['skin','neutral'] for scene in scenes]
    assert all(p.is_file() for p in paths),'Missing settled captures: '+', '.join(str(p) for p in paths if not p.is_file())
    concepts=sorted((ROOT.parent/'concept-pics').glob('*.png'));assert len(concepts)==4
    sheets=[]
    for theme in ['skin','neutral']:
        rows=(len(scenes)+4)//5
        sheet=Image.new('RGB',(2000,600+rows*500),'#202023');draw=ImageDraw.Draw(sheet)
        for i,p in enumerate(concepts):
            image=Image.open(p).convert('RGB');image.thumbnail((470,550));sheet.paste(image,(i*500+10,30));draw.text((i*500+10,10),p.name,fill='white')
        for i,scene in enumerate(scenes):
            path=EVID/f'{theme}-{scene}.png';rects=json.loads(path.with_suffix('.json').read_text());assert rects
            x=min(r['x'] for r in rects);y=min(r['y'] for r in rects);right=max(r['x']+r['width'] for r in rects);bottom=max(r['y']+r['height'] for r in rects)
            image=Image.open(path).convert('RGBA').crop((int(x),int(y),int(right+1),int(bottom+1)))
            canvas=Image.new('RGBA',image.size,'#202023');canvas.alpha_composite(image);image=canvas.convert('RGB');image.thumbnail((380,465))
            left=(i%5)*400+10;top=600+(i//5)*500
            sheet.paste(image,(left,top+25));draw.text((left,top),f'{theme}-{scene} (native WebKit)',fill='white')
        output=EVID/f'review-{theme}.png';sheet.save(output);sheets.append(str(output))
    manifest={'status':'PARTIAL' if args.checkpoint else 'CAPTURED_REQUIRES_REVIEW','engine':'Tauri/Wry/WebKitGTK private nested KWin','images':[str(p) for p in paths],'sheets':sheets,'missingCheckpointScenes':['edit','layout'] if args.checkpoint else []}
    (EVID/'screenshots-checkpoint.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(f'{manifest["status"]}: {len(paths)} actual captures, {len(sheets)} review sheets; human/agent viewing is separate')
if __name__=='__main__':main()
