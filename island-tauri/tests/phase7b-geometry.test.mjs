import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
const source = readFileSync(new URL('../ui/island/geometry.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
const { sideRects, bounds, fitSurface, clampUnion, effectiveLayout, visibleLayout, nativePill, createGeometryReporter } = await import(`data:text/javascript;base64,${Buffer.from(compiled.outputText).toString('base64')}`);
const button = (id, side, slot, action = {type:'appGrid'}) => ({id,side,slot,action,label:null});
const layout = (d = 36, g = 10, full = false) => ({version:1,preset:{type:'compact'},buttonDiameter:d,gap:g,buttons:full?
  ['left','right','bottom'].flatMap(side => Array.from({length:side==='bottom'?5:3},(_,i)=>button(`${side}-${i}`,side,i))):
  [button('apps','left',0),button('timer','left',1,{type:'timer'}),button('settings','right',0,{type:'settings'}),button('volume','right',1,{type:'volume'}),button('music','bottom',0,{type:'nowPlaying'})]});
const inside = (r, v) => { assert.ok(r.x >= -1e-8 && r.y >= -1e-8, JSON.stringify(r)); assert.ok(r.x+r.width <= v.width+1e-8 && r.y+r.height <= v.height+1e-8, JSON.stringify({r,v})); };
test('ratified terminal and other-view sizes and +24/+12 offsets still fit the default complete union',()=>{
  for(const [width,height] of [[1040,672],[640,740],[640,500]]) {
    const v={width:1180,height:820},model=layout(),f=fitSurface(width,height,v,model);
    assert.deepEqual(f,{width,height,buttonScale:1});
    const outer={x:(v.width-width)/2,y:8,width,height},all=[outer,...sideRects(model,outer).map(b=>b.rect)];
    assert.deepEqual(clampUnion(all,{x:24,y:12},v),{x:24,y:12});
  }
});
test('max diameter/gap and all eleven circles fit small viewports and both extreme offsets without filling gaps',()=>{
  for(const width of [80,200,400,800,1180]) for(const height of [100,240,400,820]) for(const d of [28,56]) for(const g of [4,24]) {
    const v={width,height},model=layout(d,g,true),fit=fitSurface(1040,740,v,model),live=effectiveLayout(model,fit.buttonScale);
    const shell={x:(width-fit.width)/2,y:8,width:fit.width,height:fit.height};
    const all=[shell,...sideRects(live,shell).map(b=>b.rect)];
    assert.equal(all.length,12);
    assert.ok(bounds(all).width<=width+1e-8 && bounds(all).height<=height+1e-8);
    for(const desired of [{x:-400,y:-200},{x:400,y:200}]) {
      const move=clampUnion(all,desired,v);
      all.forEach(r=>inside({...r,x:r.x+move.x,y:r.y+move.y},v));
    }
    for(let i=1;i<all.length;i++) for(let j=i+1;j<all.length;j++) {
      const a=all[i],b=all[j];
      assert.ok(a.x+a.width<=b.x+1e-8 || b.x+b.width<=a.x+1e-8 || a.y+a.height<=b.y+1e-8 || b.y+b.height<=a.y+1e-8);
    }
  }
});
test('circles keep their requested diameter whenever they fit, including asymmetric bottom rows',()=>{
  for(const removed of ['none','left','right']) {
    const model=layout(56,24,true);model.buttons=model.buttons.filter(b=>b.side!==removed);
    const v={width:400,height:400},fit=fitSurface(1040,740,v,model);
    assert.equal(fit.buttonScale,1,'400px can contain every maximum-size circle without scaling');
    const shell={x:0,y:0,width:fit.width,height:fit.height};
    const all=[shell,...sideRects(model,shell).map(b=>b.rect)];
    assert.ok(bounds(all).width<=400+1e-8);assert.ok(bounds(all).height<=400+1e-8);
    const move=clampUnion(all,{x:0,y:0},v);all.forEach(r=>inside({...r,x:r.x+move.x,y:r.y+move.y},v));
  }
});
test('short-island bottom row clears the longest side column plus the exact configured gap',()=>{
  const model=layout(56,24,true),shell={x:200,y:10,width:300,height:20},all=sideRects(model,shell);
  const bottom=all.find(b=>b.id==='bottom-0').rect;
  const last=all.find(b=>b.id==='left-2').rect;
  assert.equal(bottom.y,last.y+last.height+24);
  assert.equal(bottom.x,200+150-(5*56+4*24)/2);
});
test('disabled module actions vanish from both projection and union before reporting',()=>{
  const model=layout();model.buttons.push(button('quick','bottom',1,{type:'openApp',view:'quickInfo'}));
  const active=visibleLayout(model,['terminal','settings','timer']);
  assert.deepEqual(active.buttons.map(b=>b.id),['apps','timer','settings','quick']);
  assert.equal(sideRects(active,{x:100,y:8,width:640,height:640}).length,4);
});

test('native pill CSS geometry is exact, and absent geometry never guesses an anchor',()=>{
  const saved=globalThis.document; globalThis.document={documentElement:{}};
  globalThis.getComputedStyle=()=>({getPropertyValue:key=>({'--native-pill-x':'517.25px','--native-pill-y':'4.5px','--native-pill-width':'146px','--native-pill-height':'32px','--native-pill-radius':'16px'})[key]??''});
  assert.deepEqual(nativePill(),{x:517.25,y:4.5,width:146,height:32,radius:16});
  globalThis.getComputedStyle=()=>({getPropertyValue:()=>''});assert.equal(nativePill(),null);globalThis.document=saved;
});
test('single flush moves side circles with outer clamp, reports current rev, and settles idle without feedback',async()=>{
  const queue=[],sent=[],props={};let current={mode:'large',rev:4},offset={x:400,y:200};
  globalThis.innerWidth=1180;globalThis.innerHeight=820;globalThis.devicePixelRatio=1;
  globalThis.window={addEventListener(){},removeEventListener(){},dispatchEvent(){}};
  globalThis.ResizeObserver=class{observe(){}disconnect(){}};
  globalThis.requestAnimationFrame=cb=>{queue.push(cb);return queue.length;};globalThis.cancelAnimationFrame=()=>{};
  const buttons=layout().buttons.map(b=>({dataset:{sideButton:b.id},style:{},getClientRects:()=>[{}],
    get offsetLeft(){return parseFloat(this.style.left);},get offsetTop(){return parseFloat(this.style.top);},
    get offsetWidth(){return parseFloat(this.style.width);},get offsetHeight(){return parseFloat(this.style.height);},
    getBoundingClientRect(){assert.fail('animated button bounds must not drive ShapeInput');}}));
  const outer={style:{setProperty(k,v){props[k]=parseFloat(v);}},ownerDocument:{querySelectorAll:()=>buttons},getBoundingClientRect:()=>({x:70+(props['--popup-x']??0),y:8+(props['--popup-y']??0),width:1040,height:672})};
  globalThis.DOMMatrixReadOnly=class{constructor(t){const v=t.match(/matrix\((.*)\)/)[1].split(',').map(Number);this.m41=v[4];this.m42=v[5];}};
  globalThis.getComputedStyle=e=>({visibility:'visible',borderRadius:e===outer?'24px':e.style.borderRadius,transform:e===outer?`matrix(1,0,0,1,${props['--popup-x']??0},${props['--popup-y']??0})`:'none'});
  const owner=createGeometryReporter(outer,()=>current,()=>offset,async r=>sent.push(r),assert.fail,()=>layout());
  current={mode:'large',rev:5};owner.schedule(true);queue.shift()();await new Promise(resolve=>setImmediate(resolve));
  assert.equal(sent[0].stateRev,5);assert.equal(sent[0].rects.length,6);sent[0].rects.forEach(r=>inside(r,{width:1180,height:820}));
  assert.equal(sent[0].rects[0].x,94);assert.equal(queue.length,0,'no idle or IPC geometry loop');
  owner.schedule(true);queue.shift()();await new Promise(resolve=>setImmediate(resolve));assert.equal(sent.length,1,'translation does not accumulate');
  offset={x:-400,y:-200};owner.schedule(true);queue.shift()();await new Promise(resolve=>setImmediate(resolve));sent.at(-1).rects.forEach(r=>inside(r,{width:1180,height:820}));owner.dispose();
});
test('side layout follows the drawn shell during an offset glide, not its future destination', async()=>{
  const queue=[],props={},root={dataset:{}},style={};let offset={x:0,y:0};
  globalThis.innerWidth=1180;globalThis.innerHeight=820;globalThis.devicePixelRatio=1;
  globalThis.window={addEventListener(){},removeEventListener(){},dispatchEvent(){}};
  globalThis.ResizeObserver=class{observe(){}disconnect(){}};
  globalThis.requestAnimationFrame=cb=>{queue.push(cb);return queue.length;};globalThis.cancelAnimationFrame=()=>{};
  const button={dataset:{sideButton:'apps'},style,getClientRects:()=>[{}],
    get offsetLeft(){return parseFloat(style.left);},get offsetTop(){return parseFloat(style.top);},
    get offsetWidth(){return parseFloat(style.width);},get offsetHeight(){return parseFloat(style.height);}};
  const outer={style:{setProperty(k,v){props[k]=parseFloat(v);}},ownerDocument:{documentElement:root,querySelectorAll:()=>[button]},
    getBoundingClientRect:()=>({x:270,y:11,width:640,height:640})};
  globalThis.getComputedStyle=e=>({visibility:'visible',borderRadius:'18px',transform:'none'});
  const model={...layout(),buttons:[layout().buttons[0]]};
  const owner=createGeometryReporter(outer,()=>({mode:'large',rev:1}),()=>offset,async()=>{},assert.fail,()=>model);
  queue.shift()();await new Promise(r=>setImmediate(r));
  offset={x:0,y:100};owner.schedule();queue.shift()();await new Promise(r=>setImmediate(r));
  assert.equal(props['--popup-y'],100);
  assert.equal(parseFloat(style.top),11,'the button must not jump to y=111 while the shell is still at y=11');
  owner.dispose();
});

