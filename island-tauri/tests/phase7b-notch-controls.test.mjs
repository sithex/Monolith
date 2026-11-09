import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
const root=process.env.ISLAND_UI_ROOT ?? new URL('../ui',import.meta.url).pathname;
const { NotchControls }=load(`${root}/island/NotchControls.tsx`);
const { DraftRange }=load(`${root}/island/components/AppPrimitives.tsx`);
const children=e=>[e,...[e?.props?.children].flat(Infinity).filter(Boolean).flatMap(c=>typeof c==='object'?children(c):[])];
test('every notch and companion row has its stable ID target even when disabled', () => {
 const { SETTINGS_CARDS } = load(`${root}/views/settings-model.ts`);
 const expected = SETTINGS_CARDS.filter(card => ['notch', 'companion'].includes(card.id)).flatMap(card => card.rows.map(row => row.id));
 for (const settings of [{}, { showNotchOnEveryMonitor: false, companionEnabled: false, notchBorderEnabled: false }]) {
  const tree = NotchControls({ settings, background: '#000000', accent: '#00ff00', patch() {} });
  const targets = children(tree).filter(node => node.props?.['data-setting-id']);
  assert.deepEqual(targets.map(node => node.props['data-setting-id']), expected);
  assert.ok(targets.every(node => node.props['data-setting-label'] && node.props.tabIndex === -1));
 }
});
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
 // Saved character stays editable while the companion is off; it must not enable it.
 picker({companionEnabled:false}).props.onPick('frog');
 assert.deepEqual(calls.pop(),{companionCharacter:'frog'});
 const p=picker({});
 assert.equal(p.props.value,'blob');
 // The picker is stateful now: render it with hook stubs; grouped display order is mapped back to list order.
 const order=pickerHarness(p.props).mod.COMPANION_CHARACTERS.map(c=>c.name);
 const pickedTiles=props=>pickerHarness(props).tiles().sort((a,b)=>order.indexOf(a.props['aria-label'])-order.indexOf(b.props['aria-label']));
 const tiles=pickedTiles(p.props);
 assert.deepEqual(tiles.map(e=>e.props['aria-label']),['Blob','Isaac','Null','Companion Cube · Portal','Nurgling','Hollow Knight','Frog','Necron Warrior','Necron Overlord','Canoptek Scarab','Eye of Sauron','Mushroom','Sonichu','Salamander']);
 assert.deepEqual(tiles.map(e=>e.props['aria-checked']),[true,false,false,false,false,false,false,false,false,false,false,false,false,false]);
 tiles[1].props.onClick();
 assert.deepEqual(calls,[{companionCharacter:'isaac'}]);
 const ids=['blob','isaac','notchmimic','companioncube','nurgling','hollowknight','frog','necronwarrior','necronoverlord','canoptekscarab','eyeofsauron','mushroom','sonichu','salamander'];
 calls.length=0;
 tiles.forEach(t=>t.props.onClick());
 assert.deepEqual(calls,ids.map(companionCharacter=>({companionCharacter})));
 ids.forEach(companionCharacter=>{
  const picked=picker({companionCharacter});
  const radios=pickedTiles(picked.props);
  assert.deepEqual(radios.map(e=>e.props['aria-checked']),ids.map(id=>id===companionCharacter));
 });
 const chosen=picker({companionCharacter:'isaac'});
 assert.equal(chosen.props.value,'isaac');
});

// ---- Companion picker harness: real production module, hook stubs, re-render on state change ----
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import ts from 'typescript';
const React=createRequire(import.meta.url)('react');
function pickerHarness(props){
 const slots=[];let index=0,current;
 const hooks={...React,
  useState(initial){const i=index++;slots[i]??={value:initial};const slot=slots[i];return [slot.value,v=>{slot.value=typeof v==='function'?v(slot.value):v;}];},
  useRef(initial){const i=index++;return slots[i]??={current:initial};}};
 const source=readFileSync(`${root}/island/companionCharacters.tsx`,'utf8');
 const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
 const module={exports:{}};
 const require=name=>name==='react'?hooks:name==='react/jsx-runtime'?createRequire(import.meta.url)('react/jsx-runtime'):(()=>{throw Error(name);})();
 vm.runInThisContext(`(function(require,module,exports){${js}\n})`,{filename:'companionCharacters.tsx'})(require,module,module.exports);
 const h={mod:module.exports,props:{...props},
  render(){index=0;current=module.exports.CompanionPicker(h.props);return current;},
  nodes:()=>children(current),
  tiles:()=>h.nodes().filter(e=>e.props?.role==='radio'),
  ids:()=>h.tiles().map(e=>e.props['aria-label']),
  find:pred=>h.nodes().find(pred),
  type(text){h.find(e=>e.props?.['aria-label']==='Search companions').props.onChange({target:{value:text}});h.render();},
  chip(name){h.find(e=>e.props?.className==='companion-chip'&&e.props.children[0]===name).props.onClick();h.render();},
  chips:()=>h.nodes().filter(e=>e.props?.className==='companion-chip')};
 h.render();return h;
}
const ALL_IDS=['blob','isaac','notchmimic','companioncube','nurgling','hollowknight','frog','necronwarrior','necronoverlord','canoptekscarab','eyeofsauron','mushroom','sonichu','salamander'];
const NAMES=['Blob','Null','Frog','Salamander','Isaac','Companion Cube · Portal','Hollow Knight','Mushroom','Sonichu','Nurgling','Necron Warrior','Necron Overlord','Canoptek Scarab','Eye of Sauron'];
test('every companion appears once, in a known group, with grouped headings',()=>{
 const { COMPANION_CHARACTERS, COMPANION_GROUPS }=pickerHarness({value:'blob',onPick(){}}).mod;
 assert.deepEqual(COMPANION_CHARACTERS.map(c=>c.id),ALL_IDS);
 assert.equal(new Set(COMPANION_CHARACTERS.map(c=>c.id)).size,COMPANION_CHARACTERS.length);
 assert.deepEqual([...COMPANION_GROUPS],['Originals','Games','Warhammer 40K','Fantasy']);
 assert.ok(COMPANION_CHARACTERS.every(c=>COMPANION_GROUPS.includes(c.group)&&typeof c.preview==='function'));
 const h=pickerHarness({value:'blob',onPick(){}});
 assert.deepEqual(h.ids(),NAMES);
 assert.deepEqual(h.nodes().filter(e=>e.props?.className==='companion-group-title').map(e=>e.props.children),['Originals','Games','Warhammer 40K','Fantasy']);
 assert.deepEqual(h.chips().map(e=>e.props.children[0]+' '+e.props.children[1].props.children),['All 14','Originals 4','Games 5','Warhammer 40K 4','Fantasy 1']);
});
test('companion previews scale to a crisp integer multiple of any viewBox',()=>{
 const h=pickerHarness({value:'salamander',onPick(){}});
 for(const c of h.mod.COMPANION_CHARACTERS){
  const side=Number(c.preview().props.viewBox.split(' ')[2]);
  const svg=h.find(e=>e.props?.['aria-label']===c.name).props.children[0].props.children;
  assert.equal(svg.props.width%side,0,c.id); assert.equal(svg.props.width,svg.props.height); assert.ok(svg.props.width<=56&&svg.props.width>=side,c.id);
  if(c.preview().props.shapeRendering) assert.equal(svg.props.shapeRendering,'crispEdges');
 }
 const big=h.find(e=>e.props?.className==='companion-preview companion-preview-large').props.children;
 const selSide=Number(h.mod.COMPANION_CHARACTERS.find(c=>c.id==='salamander').preview().props.viewBox.split(' ')[2]);
 assert.ok(big.props.width%selSide===0&&big.props.width>=2*selSide,'large preview is an integer multiple of the selected viewBox');
});
test('companion search matches name, group and franchise case-insensitively',()=>{
 const h=pickerHarness({value:'blob',onPick(){}});
 h.type('FROG'); assert.deepEqual(h.ids(),['Frog']);
 h.type('warhammer'); assert.deepEqual(h.ids(),['Nurgling','Necron Warrior','Necron Overlord','Canoptek Scarab']);
 h.type('  lord of the RINGS '); assert.deepEqual(h.ids(),['Eye of Sauron']);
 h.type('portal'); assert.deepEqual(h.ids(),['Companion Cube · Portal']);
 assert.equal(h.find(e=>e.props?.className==='companion-group-title'),undefined,'no headings while searching');
 h.type('zzz'); assert.deepEqual(h.ids(),[]);
 assert.equal(h.find(e=>e.props?.role==='status').props.children,'No companions match');
 h.type(''); assert.equal(h.ids().length,14);
 assert.equal(h.find(e=>e.props?.role==='status'),undefined);
});
test('companion group chips filter, combine with search and keep the selection',()=>{
 const picks=[];const h=pickerHarness({value:'frog',onPick:id=>picks.push(id)});
 h.chip('Games'); assert.deepEqual(h.ids(),['Isaac','Companion Cube · Portal','Hollow Knight','Mushroom','Sonichu']);
 assert.deepEqual(h.chips().map(e=>e.props['aria-pressed']),[false,false,true,false,false]);
 assert.equal(h.find(e=>e.props?.className==='companion-group-title'),undefined);
 // Frog is filtered out but still the saved value: header shows it, nothing was picked.
 assert.equal(h.props.value,'frog'); assert.deepEqual(picks,[]);
 assert.equal(h.find(e=>e.props?.className==='companion-current-text').props.children[0].props.children,'Frog');
 assert.ok(h.tiles().every(t=>t.props['aria-checked']===false));
 h.type('sonic'); assert.deepEqual(h.ids(),['Sonichu']);
 h.chip('Fantasy'); assert.deepEqual(h.ids(),[]);
 h.type(''); assert.deepEqual(h.ids(),['Eye of Sauron']);
 h.chip('All'); assert.equal(h.ids().length,14);
 assert.equal(h.tiles().find(t=>t.props['aria-label']==='Frog').props['aria-checked'],true);
 assert.deepEqual(picks,[]);
});
test('companion picker keyboard: roving tabindex, arrows, Home/End and picking',()=>{
 const picks=[];const h=pickerHarness({value:'isaac',onPick:id=>picks.push(id)});
 const tab=()=>h.tiles().map(t=>t.props.tabIndex);
 assert.deepEqual(tab(),NAMES.map(n=>n==='Isaac'?0:-1));
 // Fake 4-column layout: DOM order = visible order.
 const focused=[];
 const radios=()=>NAMES.map((name,i)=>({offsetTop:Math.floor(i/4)*100,offsetLeft:(i%4)*100,focus(){focused.push(name);}}));
 h.find(e=>e.props?.role==='radiogroup').props.ref.current={querySelectorAll:()=>radios()};
 const key=(from,k)=>{let prevented=false;h.find(e=>e.props?.['aria-label']===from).props.onKeyDown({key:k,preventDefault(){prevented=true;}});h.render();return prevented;};
 assert.ok(key('Isaac','ArrowRight')); assert.equal(focused.pop(),'Companion Cube · Portal');
 assert.equal(tab().indexOf(0),5);
 key('Companion Cube · Portal','ArrowDown'); assert.equal(focused.pop(),'Nurgling');
 key('Nurgling','ArrowUp'); assert.equal(focused.pop(),'Companion Cube · Portal');
 key('Companion Cube · Portal','ArrowLeft'); assert.equal(focused.pop(),'Isaac');
 key('Isaac','Home'); assert.equal(focused.pop(),'Blob'); assert.equal(tab().indexOf(0),0);
 key('Blob','End'); assert.equal(focused.pop(),'Eye of Sauron'); assert.equal(tab().indexOf(0),13);
 key('Eye of Sauron','ArrowRight'); assert.equal(focused.pop(),'Eye of Sauron');
 assert.equal(key('Blob','Tab'),false); assert.deepEqual(focused,[]);
 h.tiles()[0].props.onClick(); assert.deepEqual(picks,['blob']);
 // Pure navigation helper on a ragged grid (groups end mid-row).
 const { companionFocusTarget }=h.mod, r=[{top:0,left:0},{top:0,left:1},{top:0,left:2},{top:1,left:0},{top:2,left:0},{top:2,left:1}];
 assert.equal(companionFocusTarget('ArrowDown',2,r),3);
 assert.equal(companionFocusTarget('ArrowDown',3,r),4);
 assert.equal(companionFocusTarget('ArrowUp',5,r),3);
 assert.equal(companionFocusTarget('ArrowUp',1,r),1);
 assert.equal(companionFocusTarget('a',1,r),null);
});
test('companion picker tab stop falls back to the first visible tile while the selection is filtered out',()=>{
 const h=pickerHarness({value:'frog',onPick(){}});
 h.chip('Fantasy'); assert.deepEqual(h.tiles().map(t=>t.props.tabIndex),[0]);
 h.chip('All'); assert.equal(h.tiles().find(t=>t.props.tabIndex===0).props['aria-label'],'Frog');
});
test('disabled companion picker disables every control and ignores picks',()=>{
 const h=pickerHarness({value:'blob',disabled:true,onPick(){throw Error('picked');}});
 assert.ok(h.tiles().every(t=>t.props.disabled===true));
 assert.ok(h.chips().every(t=>t.props.disabled===true));
 assert.equal(h.find(e=>e.props?.['aria-label']==='Search companions').props.disabled,true);
 assert.equal(h.find(e=>e.props?.role==='radiogroup').props['aria-label'],'Companion character');
});
