import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
const root=process.env.ISLAND_UI_ROOT ?? new URL('../ui',import.meta.url).pathname;
const { NotchControls }=load(`${root}/island/NotchControls.tsx`);
const { DraftRange }=load(`${root}/island/components/AppPrimitives.tsx`);
const children=e=>[e,...[e?.props?.children].flat(Infinity).filter(Boolean).flatMap(c=>typeof c==='object'?children(c):[])];
test('notch appearance controls emit independent color and border settings',()=>{
 const calls=[];
 const controls=children(NotchControls({settings:{panelClockColor:'#123456',notchBorderEnabled:true,notchBorderColor:'#abcdef',notchBorderOpacity:.6,notchBorderWidth:2.5},background:'#000000',accent:'#00ff00',patch:p=>calls.push(p)}));
 const input=label=>controls.find(e=>e.props?.['aria-label']===label||e.props?.label===label);
 // Colours use the in-island picker swatch (the native GTK chooser opens behind the island on Wayland).
 assert.equal(input('Notch color').props.value,'#123456');
 input('Notch color').props.onPick('#112233');
 input('Notch border').props.onChange({target:{checked:false}});
 input('Notch border color').props.onPick('#fedcba');
 input('Notch border opacity').props.onCommit(30,true);
 input('Notch border thickness').props.onCommit(1.5,true);
 assert.deepEqual(calls,[{panelClockColor:'#112233'},{notchBorderEnabled:false},{notchBorderColor:'#fedcba'},{notchBorderOpacity:.3},{notchBorderWidth:1.5}]);
 assert.equal(input('Notch border opacity').type,DraftRange);
 assert.equal(input('Notch border opacity').props.value,60);
 assert.equal(input('Notch border thickness').props.value,2.5);
});

test('notch companion switch is on by default and patches companionEnabled',()=>{
 const calls=[];
 const find=s=>children(NotchControls({settings:s,background:'#000000',accent:'#00ff00',patch:p=>calls.push(p)})).find(e=>e.props?.['aria-label']==='Notch companion');
 assert.equal(find({}).props.checked,true);
 assert.equal(find({companionEnabled:false}).props.checked,false);
 find({}).props.onChange({target:{checked:false}});
 assert.deepEqual(calls,[{companionEnabled:false}]);
});

test('companion character picker lists every character and patches the choice',()=>{
 const calls=[];
 const all=s=>children(NotchControls({settings:s,background:'#000000',accent:'#00ff00',patch:p=>calls.push(p)}));
 const picker=s=>all(s).find(e=>e.props?.value!==undefined&&typeof e.props?.onPick==='function'&&e.props?.value!==e.props?.label&&!('resetLabel' in e.props));
 // Hidden while the companion is off.
 assert.equal(picker({companionEnabled:false}),undefined);
 const p=picker({});
 assert.equal(p.props.value,'blob');
 const tiles=children(p.type(p.props)).filter(e=>e.props?.role==='radio');
 assert.deepEqual(tiles.map(e=>e.props['aria-label']),['Blob','Isaac','Null','Little Obelisk','Companion Cube · Portal','Nurgling','Hollow Knight']);
 assert.deepEqual(tiles.map(e=>e.props['aria-checked']),[true,false,false,false,false,false,false]);
 tiles[1].props.onClick();
 assert.deepEqual(calls,[{companionCharacter:'isaac'}]);
 const ids=['blob','isaac','notchmimic','obelisk','companioncube','nurgling','hollowknight'];
 calls.length=0;
 tiles.forEach(t=>t.props.onClick());
 assert.deepEqual(calls,ids.map(companionCharacter=>({companionCharacter})));
 ids.forEach(companionCharacter=>{
  const picked=picker({companionCharacter});
  const radios=children(picked.type(picked.props)).filter(e=>e.props?.role==='radio');
  assert.deepEqual(radios.map(e=>e.props['aria-checked']),ids.map(id=>id===companionCharacter));
 });
 const chosen=picker({companionCharacter:'isaac'});
 assert.equal(chosen.props.value,'isaac');
});
