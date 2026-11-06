import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import ts from 'typescript';
import * as React from 'react';
import { load } from './phase7b-loader.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));
const native=createRequire(import.meta.url),tick=()=>new Promise(resolve=>setImmediate(resolve));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
// A deterministic hook/effect driver for production handlers; not a browser/GUI claim.
function driver(overrides={}, transform=(_path, source)=>source) {
  const slots=[],effects=[],cache=new Map();let index=0;
  const hooks={...React,
    useState(initial){const i=index++;slots[i]??={value:typeof initial==='function'?initial():initial};return [slots[i].value,next=>{slots[i].value=typeof next==='function'?next(slots[i].value):next;}];},
    useRef(initial){const i=index++;return slots[i]??=( {current:initial} );},
    useSyncExternalStore(_subscribe,get){index++;return get();},
    useEffect(effect,deps){const i=index++,previous=slots[i];if(!previous||deps.some((v,n)=>v!==previous.deps[n])){slots[i]={deps};effects.push(()=>{previous?.cleanup?.();slots[i].cleanup=effect();});}},
    useId(){index++;return 'fixture-id';},
    useMemo(factory){index++;return factory();},
  };
  hooks.useLayoutEffect=hooks.useEffect;
  function compile(path){const file=resolve(root,path),relative=file.slice(root.length);if(relative in overrides)return overrides[relative];if(cache.has(file))return cache.get(file).exports;
    const source=transform(relative,readFileSync(file,'utf8')),out=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}}).outputText;
    const module={exports:{}};cache.set(file,module);
    const require=name=>{if(name==='react')return hooks;if(name in overrides)return overrides[name];if(!name.startsWith('.'))return native(name);const base=resolve(dirname(file),name),p=['','.ts','.tsx','.js','/index.ts'].map(ext=>base+ext).find(p=>existsSync(p)&&statSync(p).isFile());assert.ok(p,`Unresolved ${name}`);return compile(p);};
    vm.runInThisContext(`(function(require,module,exports){${out}\n})`,{filename:file})(require,module,module.exports);return module.exports;
  }
  return {load:compile,render(C,props){index=0;const tree=C(props);while(effects.length)effects.shift()();return tree;},dispose(){for(const slot of slots)slot?.cleanup?.();}};
}
function nodes(tree){if(!tree||typeof tree!=='object')return [];if(Array.isArray(tree))return tree.flatMap(nodes);return [tree,...nodes(tree.props?.children)];}
// All globals below are private DOM fixtures, restored after each synchronous test.
function globals(values, run) {
  const before=Object.fromEntries(Object.keys(values).map(k=>[k,Object.getOwnPropertyDescriptor(globalThis,k)]));
  try {for(const [k,v] of Object.entries(values))Object.defineProperty(globalThis,k,{value:v,writable:true,configurable:true});return run();}
  finally {for(const [k,v] of Object.entries(before))if(v)Object.defineProperty(globalThis,k,v);else delete globalThis[k];}
}
const component=(tree,name)=>nodes(tree).find(n=>n.type?.name===name);
const labeled=(tree,label)=>nodes(tree).find(n=>n.props?.['aria-label']===label);
const textButton=(tree,text)=>nodes(tree).find(n=>n.type==='button'&&n.props.children===text);
test('compact notice dismiss is revision-scoped, resume opens existing terminal, agent notice opens agents',()=>{
  const calls=[];let state={mode:'hover',rev:9,terminalStarted:true,hoverCause:{type:'panelPill'}};let items=[];
  const h=driver({'ui/island/preview-notice.css':{},'ui/ipc/island-state.ts':{useIslandState:()=>state,dispatch:async intent=>calls.push(intent)},'ui/ipc/notifications.ts':{useNotifications:()=>items}});
  const {PreviewNotice}=h.load('ui/island/PreviewNotice.tsx');const fail=e=>{throw e;};
  let tree=h.render(PreviewNotice,{fail});assert.ok(nodes(tree).some(n=>n.type==='strong'&&n.props.children==='Resume terminal'));
  labeled(tree,'Dismiss preview').props.onClick({stopPropagation(){}});assert.deepEqual(calls.pop(),{type:'dismissPreview',rev:9});
  nodes(tree).find(n=>n.props?.['data-testid']==='open').props.onClick();assert.deepEqual(calls.pop(),{type:'selectView',view:'terminal'});
  items=[{id:'new-run',title:'Agent run started',appName:'Agents'}];state={...state,rev:10,hoverCause:{type:'notification',id:'new-run'}};
  tree=h.render(PreviewNotice,{fail});assert.ok(nodes(tree).some(n=>n.type==='strong'&&n.props.children==='Agent run started'));
  state={...state,rev:11,hoverCause:{type:'pointer'}};tree=h.render(PreviewNotice,{fail}); // crossing retains event, not generic resume
  nodes(tree).find(n=>n.props?.['data-testid']==='open').props.onClick();assert.deepEqual(calls.pop(),{type:'selectView',view:'agents'});
  h.dispose();
});
const clip={rev:1,id:'c-1',kind:'text',preview:'Fixture',bytes:7,pinned:false,copiedAt:1000,sourceApp:null};
const row={uid:'fixture-event',summary:'Fixture title',location:'Fixture room',startS:1793611800,endS:1793615400,allDay:false,source:'local',readOnly:false};
const calendar={rev:1,available:true,detail:'ready',configured:false,stale:false,ageMs:null,updatedAt:null,agenda:[row]};
const timer={rev:1,completedKind:null,available:true,detail:'ready',sampledAtMs:100,countdownStatus:'idle',remainingMs:0,pomodoroStatus:'paused',phase:'shortBreak',phaseRemainingMs:300000,round:1,stopwatchStatus:'idle',elapsedMs:0,laps:[]};
function appHarness(){
  const {createEpochTopic}=load('ui/stores/epoch-topic.ts');const appStores=Object.fromEntries(['timer','scratchpad','downloads','calendar','mixer','brightness','toggles','tiles','updates','agents'].map(k=>[k,createEpochTopic()]));
  const sent=[],queries=[],reads=[];const clipboard={items:[clip],status:{available:true,enabled:true,paused:false},query:'',error:null};let clipEpoch=1;
  const history={setVisible:on=>sent.push(['visible',on]),setQuery:q=>{clipboard.query=q;},epoch:()=>clipEpoch,isCurrent:g=>g===clipEpoch,invalidate:()=>sent.push(['invalidate']),refreshStatus:async()=>sent.push(['status'])};
  const api={calendar:q=>{const pending=deferred();queries.push({q,...pending});return pending.promise;},calendarCommand:async op=>sent.push(['calendar',op]),calendarRefresh:async()=>sent.push(['refresh']),timer:()=>{const pending=deferred();reads.push(pending);return pending.promise;},timerCommand:async op=>{sent.push(['timer',op]);return {...timer,rev:2};},togglesCommand:async op=>{sent.push(['toggle',op]);return {...appStores.toggles.getSnapshot(),rev:2};}};
  const d=driver({
    'ui/ipc/client.ts':{appApi:api,clipboardCopy:async id=>sent.push(['copy',id]),clipboardDelete:async id=>sent.push(['delete',id]),clipboardPin:async(id,pinned)=>sent.push(['pin',id,pinned]),clipboardSetPaused:async paused=>sent.push(['pause',paused]),mediaSetVisible:async on=>sent.push(['mediaVisible',on]),mediaControl:async op=>sent.push(['media',op])},
    'ui/stores/index.ts':{useSettings:()=>({dnd:false,redactBodies:false,clipboardHistory:true})},
    'ui/ipc/notifications.ts':{useNotifications:()=>[]},
    'ui/stores/apps.ts':{appStores,clipboardHistory:history,useClipboardHistory:()=>clipboard,useApp:key=>appStores[key].getSnapshot(),useMedia:()=>null,useLyrics:()=>null,useEqualizer:()=>null,useWeather:()=>null,usePower:()=>null,useBluetooth:()=>null,serviceStatus:()=>null},
  });
  const {AppsView}=d.load('ui/views/AppsView.tsx');return {...d,AppsView,appStores,sent,queries,reads,api,clipboard,resetClip:()=>clipEpoch++,renderApp(props){return d.render(AppsView,{active:true,view:'calendar',...props});}};
}

test('runtime Timer view never polls while another app is shown or island is idle',async()=>{
  const h=appHarness();
  try {h.renderApp({view:'music'});assert.equal(h.reads.length,0);h.renderApp({view:'timer',active:false});assert.equal(h.reads.length,0);
    h.renderApp({view:'timer',active:true});assert.equal(h.reads.length,1);h.renderApp({view:'terminal'});h.reads[0].resolve(timer);await tick();assert.equal(h.appStores.timer.getSnapshot(),null);
  } finally {h.dispose();}
});
test('runtime Timer wires authoritative pomodoro completion even after phase changes',()=>{
  const h=appHarness();h.appStores.timer.apply({...timer,completedKind:'pomodoro',phase:'longBreak',pomodoroStatus:'running'});
  try {const tree=h.renderApp({view:'timer'});assert.equal(component(tree,'TimerApp').props.completedKind,'pomodoro');} finally {h.dispose();}
});
test('runtime AppsView Calendar creates selects edits deletes and refreshes through actual UI intent handlers',async()=>{
  const h=appHarness();h.appStores.calendar.apply(calendar);h.renderApp();assert.equal(h.queries.length,1);h.queries[0].resolve(calendar);await tick();
  let tree=h.renderApp();let cal=component(tree,'CalendarApp');assert.ok(cal);cal.props.intents.calendar({type:'create'});tree=h.renderApp();let edit=component(tree,'CalendarEventEditor');assert.ok(edit);assert.equal(edit.props.row,null);
  const doc='BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n';await edit.props.command({type:'create',ics:doc});assert.deepEqual(h.sent.find(c=>c[0]==='calendar'),['calendar',{type:'create',ics:doc}]);
  edit.props.close();tree=h.renderApp();h.queries.at(-1).resolve(calendar);await tick();tree=h.renderApp();cal=component(tree,'CalendarApp');
  cal.props.intents.calendar({type:'select',instanceKey:cal.props.model.agenda[0].instanceKey});tree=h.renderApp();edit=component(tree,'CalendarEventEditor');assert.equal(edit.props.row.uid,row.uid);
  await edit.props.command({type:'edit',source:'local',uid:row.uid,ics:doc});await edit.props.command({type:'delete',source:'local',uid:row.uid});assert.deepEqual(h.sent.filter(c=>c[0]==='calendar').map(c=>c[1].type),['create','edit','delete']);
  h.dispose();
});
test('runtime AppsView Calendar one-flight month queries fence late same-revision replies and lifecycle resets',async()=>{
  const h=appHarness();h.appStores.calendar.apply(calendar);h.renderApp();h.queries[0].resolve(calendar);await tick();let tree=h.renderApp();
  component(tree,'CalendarApp').props.intents.calendar({type:'next'});tree=h.renderApp();assert.equal(h.queries.length,2);
  component(tree,'CalendarApp')?.props.intents.calendar({type:'next'}); // Old content is cleared while the request is pending.
  h.appStores.calendar.reset();h.renderApp();h.queries[1].resolve({...calendar,agenda:[{...row,summary:'Stale private event'}]});await tick();assert.equal(h.queries.length,3);
  h.queries[2].resolve({...calendar,rev:0,agenda:[]});await tick();tree=h.renderApp();assert.equal(component(tree,'CalendarApp').props.model.agenda.length,0);h.dispose();
});
test('runtime AppsView Clipboard actual handlers search pause pin delete copy and fence queued commands',async()=>{
  const h=appHarness();let tree=h.renderApp({view:'clipboard'}),clip=component(tree,'ClipboardView');assert.equal(clip.props.model.status.paused,false);
  clip.props.intents.clipboard({type:'search',query:'fixture'});assert.equal(h.clipboard.query,'fixture');
  for(const op of [{type:'pause',paused:true},{type:'pin',id:'c-1',pinned:true},{type:'delete',id:'c-1'},{type:'copy',id:'c-1'}])clip.props.intents.clipboard(op);
  await tick();assert.deepEqual(h.sent.filter(c=>['pause','pin','delete','copy'].includes(c[0])),[['pause',true],['pin','c-1',true],['delete','c-1'],['copy','c-1']]);assert.equal(h.clipboard.status.paused,false,'No optimistic paused state');
  clip.props.intents.clipboard({type:'copy',id:'c-1'});h.resetClip();await tick();assert.equal(h.sent.filter(c=>c[0]==='copy').length,1);h.dispose();
});
test('runtime CalendarEventEditor actual form handlers save retain errors confirm deletion and forbid subscription mutations',async()=>{
  const commands=[],d=driver(),{CalendarEventEditor}=d.load('ui/views/CalendarEventEditor.tsx');let closed=0;
  const props={row,date:new Date('2026-11-02T00:00:00'),command:async op=>commands.push(op),close:()=>closed++};
  let tree=d.render(CalendarEventEditor,props);labeled(tree,'Event title').props.onChange({target:{value:'Edited fixture title'}});tree=d.render(CalendarEventEditor,props);
  nodes(tree).find(n=>n.type==='form').props.onSubmit({preventDefault(){}});await tick();assert.equal(commands[0].type,'edit');assert.equal(commands[0].uid,row.uid);assert.match(commands[0].ics,/SUMMARY:Edited fixture title/);assert.equal(closed,1);
  tree=d.render(CalendarEventEditor,props);textButton(tree,'Delete event').props.onClick();tree=d.render(CalendarEventEditor,props);textButton(tree,'Confirm delete event').props.onClick();await tick();assert.deepEqual(commands[1],{type:'delete',source:'local',uid:row.uid});
  const readOnly={...props,row:{...row,source:'subscription',readOnly:true}},rd=driver(),ReadOnly=rd.load('ui/views/CalendarEventEditor.tsx').CalendarEventEditor;
  tree=rd.render(ReadOnly,readOnly);assert.equal(nodes(tree).some(n=>n.type==='form'),false);assert.equal(textButton(tree,'Delete event'),undefined);
  const fd=driver(),Fail=fd.load('ui/views/CalendarEventEditor.tsx').CalendarEventEditor,bad={...props,command:async()=>{throw Error('Fixture write failed');}};
  tree=fd.render(Fail,bad);nodes(tree).find(n=>n.type==='form').props.onSubmit({preventDefault(){}});await tick();tree=fd.render(Fail,bad);assert.equal(nodes(tree).find(n=>n.props?.role==='alert').props.children,'Fixture write failed');
  assert.equal(labeled(tree,'Event title').props.value,row.summary);d.dispose();rd.dispose();fd.dispose();
});
test('runtime system-topic listeners fence replaced lifecycle callbacks and validate before revision acceptance',async()=>{
  const systems=[],apps=[];
  const clipStatus={available:false,enabled:false,paused:false,detail:'disabled',watcherRunning:false,connectionsOpen:0,connectionAttempts:0,dropped:0,captured:0,sensitiveSkipped:0,denied:0,echoes:0};
  const empty={timer:null,scratchpad:null,downloads:null,calendar:null,brightness:null,agents:null,mixer:null,toggles:null,updates:null,tiles:null};
  const settings={clipboardHistory:false,clipboardHistorySize:100,clipboardPersist:false,moduleRegistry:{version:1,modules:[]}};
  const service=name=>({rev:0,service:name,available:false,detail:'Fixture unavailable'});
  const boot={settings,apps:empty,mediaRev:0,media:null,lyrics:null,weather:null,power:null,bluetoothRev:0,bluetooth:[],capabilities:{clipboardStatus:clipStatus,powerStatus:service('power'),weatherStatus:service('weather'),bluetoothStatus:service('bluetooth'),mediaStatus:service('media')}};
  const client={bootstrap:async()=>boot,launcherGet:async()=>({rev:0,items:[],editing:false}),layoutGet:async()=>({rev:0}),onAppTopics:async h=>{apps.push(h);return ()=>{};},onSystemTopics:async h=>{systems.push(h);return ()=>{};},onLauncherChanged:async()=>()=>{},onLayoutChanged:async()=>()=>{},clipboardList:async()=>[],onClipboardAdded:async()=>()=>{},onClipboardStatus:async()=>()=>{}};
  const d=driver({'ui/ipc/client.ts':client}),stores=d.load('ui/stores/apps.ts');await stores.listenExtended();
  systems[0].media({rev:50,nowPlaying:null});assert.equal(stores.mediaStore.getSnapshot().rev,50);
  const originalError=console.error;console.error=()=>{};
  try {systems[0].media({rev:999,nowPlaying:{playing:'invalid'}});}finally{console.error=originalError;}
  systems[0].media({rev:51,nowPlaying:null});assert.equal(stores.mediaStore.getSnapshot().rev,51,'Malformed payload never poisons the high-water rev');
  await stores.configureAppTopics(settings);systems[0].media({rev:52,nowPlaying:null});assert.equal(stores.mediaStore.getSnapshot().rev,0);
  systems.at(-1).media({rev:1,nowPlaying:null});assert.equal(stores.mediaStore.getSnapshot().rev,1);
  apps[0].timer({...timer,rev:999});assert.equal(stores.appStores.timer.getSnapshot(),null);apps.at(-1).timer({...timer,rev:0});assert.equal(stores.appStores.timer.getSnapshot().rev,0);
  stores.clipboardHistory.dispose();d.dispose();
});

// Regression coverage for the completed home/layout/backend/notch/size fixes.
// Execute production handlers with deterministic hooks, never the live IPC or desktop.
test('top-left Hermes and clock navigate home without collapsing, pinning before navigation when closed',async()=>{
  for(const [mode,view] of [['large','settings'],['hover','timer'],['still','music'],['large','terminal']]) {
    const calls=[],errors=[],pin=deferred();
    const d=driver({
      'ui/ipc/island-state.ts':{useIslandState:()=>({mode,view}),dispatch:op=>{calls.push(op);return op.type==='pin'?pin.promise:Promise.resolve();}},
      'ui/stores/index.ts':{useSettings:()=>({showDate:true,hourFormat:'h24'})},
      'ui/island/geometry.ts':{nativePill:()=>null},
    });
    try {
      const C=d.load('ui/island/Topline.tsx').Topline,tree=d.render(C,{fail:e=>errors.push(e)});
      const home=labeled(tree,mode==='still'?'Open Hermes terminal':'Go to the Hermes terminal');assert.ok(home);
      home.props.onClick();
      // React bubbles the same click to the topline. A button must not trigger collapse.
      tree.props.onClick({target:{closest:selector=>selector==='button'?{}:null}});
      if(mode!=='large'){assert.deepEqual(calls,[{type:'pin'}]);pin.resolve();await tick();}
      const expected=[...(mode==='large'?[]:[{type:'pin'}]),...(view==='terminal'?[]:[{type:'selectView',view:'terminal'}])];
      assert.deepEqual(calls,expected);assert.deepEqual(errors,[]);
      if(mode==='large') { tree.props.onClick({target:{closest:()=>null}});assert.deepEqual(calls.at(-1),{type:'collapse'}); }
    } finally {d.dispose();}
  }
});
test('home navigation forwards pin failure and does not select the terminal after a failed pin',async()=>{
  const calls=[],errors=[],error=Error('fixture pin rejected');
  const d=driver({'ui/ipc/island-state.ts':{useIslandState:()=>({mode:'hover',view:'settings'}),dispatch:op=>{calls.push(op);return Promise.reject(error);}},'ui/stores/index.ts':{useSettings:()=>({})},'ui/island/geometry.ts':{nativePill:()=>null}});
  try {const tree=d.render(d.load('ui/island/Topline.tsx').Topline,{fail:e=>errors.push(e)});labeled(tree,'Go to the Hermes terminal').props.onClick();await tick();assert.deepEqual(calls,[{type:'pin'}]);assert.deepEqual(errors,[error]);}finally{d.dispose();}
});

const layoutFixture={preset:{type:'custom',width:1800,maxHeight:1280},buttonDiameter:40,gap:10,buttons:[{id:'apps',side:'left',slot:0,action:{type:'appGrid'},label:null}]};
function layoutHarness(transform) {
  const calls=[],listeners=new Map(),snapshot={rev:1,unchangedSizing:false,layout:layoutFixture};
  const geometry={buttons:[{id:'apps',rect:{x:0,y:0,width:40,height:40}}],emptySlots:[]};
  const d=driver({'ui/stores/apps.ts':{useLayout:()=>snapshot,layoutStore:{apply(){}}},'ui/ipc/client.ts':{layoutGeometry:async()=>geometry,layoutCommand:async op=>{calls.push(op);return snapshot;}}},transform);
  const C=d.load('ui/island/LayoutEditor.tsx').LayoutEditor;
  return {...d,calls,listeners,renderEditor:()=>d.render(C)};
}
function layoutGlobals(run) {return globals({innerWidth:1920,innerHeight:1440,window:{addEventListener(){},removeEventListener(){}}},run);}
test('Layout selected action panel clips during animation and releases dropdown overflow only after settling',async()=>{
  const h=layoutHarness();
  try {
    layoutGlobals(()=>h.renderEditor());await tick();
    layoutGlobals(()=>{
      let tree=h.renderEditor();const events=new Map();
      // Select through the real click-vs-drag pointer handler, rather than seeding hook slots.
      globals({window:{addEventListener:(k,v)=>events.set(k,v),removeEventListener:k=>events.delete(k)}},()=>{
        labeled(tree,'Edit Apps').props.onPointerDown({button:0,pointerId:1,clientX:0,clientY:0,currentTarget:{setPointerCapture(){}}});
        events.get('pointerup')({clientX:0,clientY:0});
      });
      const panel=()=>nodes(h.renderEditor()).find(n=>n.props?.className==='le-selected');
      assert.equal(panel().props['data-settled'],false);
      panel().props.onAnimationComplete();assert.equal(panel().props['data-settled'],true);
      panel().props.onAnimationStart();assert.equal(panel().props['data-settled'],false);
      panel().props.onAnimationComplete();tree=h.renderEditor();
      const dropdown=component(tree,'Dropdown');assert.ok(dropdown.props.options.length>1);
      dropdown.props.onChange('open:timer');
      assert.deepEqual(h.calls.at(-1),{type:'edit',id:'apps',action:{type:'timer'},label:null});
      // CSS is not executed by this hook harness: pin the paired selector separately.
      const css=readFileSync(resolve(root,'ui/island/polish.css'),'utf8');
      assert.match(css,/\.le-selected\s*\{[^}]*overflow:\s*hidden/);
      assert.match(css,/\.le-selected\[data-settled=['"]true['"]\]\s*\{[^}]*overflow:\s*visible/);
    });
  } finally {h.dispose();}
});
test('Layout custom renderer sizing advertises the full valid bounds and commits only on release',()=>{
  const h=layoutHarness();try {layoutGlobals(()=>{
    const tree=h.renderEditor(),width=labeled(tree,'Custom width'),height=labeled(tree,'Custom maximum height');
    assert.deepEqual([width.props.min,width.props.max,width.props.step],[300,1800,10]);
    assert.deepEqual([height.props.min,height.props.max,height.props.step],[240,1280,10]);
    width.props.onCommit(300,false);height.props.onCommit(240,false);assert.deepEqual(h.calls,[]);
    width.props.onCommit(300,true);height.props.onCommit(240,true);
    assert.deepEqual(h.calls,[{type:'preset',preset:{type:'custom',width:300,maxHeight:1280}},{type:'preset',preset:{type:'custom',width:1800,maxHeight:240}}]);
    width.props.onCommit(1800,true);height.props.onCommit(1280,true);assert.equal(h.calls.length,2,'unchanged limits do not issue redundant saves');
  });}finally{h.dispose();}
});

test('Settings exposes working display backend controls only for desktop Linux and reports the running backend',()=>{
  for(const ua of ['Mozilla/5.0 (X11; Linux x86_64)','Mozilla/5.0 (Windows NT 10.0)','Mozilla/5.0 (Macintosh; Intel Mac OS X)','Mozilla/5.0 (Linux; Android 14)']) {
    for(const running of ['wayland','x11']) {
      const calls=[],settings={displayBackend:running==='wayland'?'x11':'wayland'};
      const d=driver({'ui/stores/index.ts':{useSettings:()=>settings,useAppearance:()=> 'green',usePalette:()=>({}),saveSettings:async p=>calls.push(p)}});
      try {globals({navigator:{userAgent:ua},document:{documentElement:{dataset:{displayBackend:running}}}},()=>{
        const tree=d.render(d.load('ui/views/SettingsView.tsx').SettingsView),group=labeled(tree,'Display server');
        if(ua.includes('X11; Linux')) {
          assert.ok(group);const wayland=textButton(group,'Wayland'),x11=textButton(group,'X11');
          assert.equal(wayland.props['aria-pressed'],settings.displayBackend==='wayland');assert.equal(x11.props['aria-pressed'],settings.displayBackend==='x11');
          wayland.props.onClick();x11.props.onClick();assert.deepEqual(calls,[{displayBackend:'wayland'},{displayBackend:'x11'}]);
          const row=nodes(tree).find(n=>n.props?.label==='Display server');assert.match(row.props.hint,/applies when the island restarts/);assert.ok(row.props.hint.includes(running==='wayland'?'native Wayland':'X11 (XWayland)'));
        } else {assert.equal(group,undefined);assert.deepEqual(calls,[]);}
      });}finally{d.dispose();}
    }
  }
});
test('Notch width handler preserves the 110–320 range and writes only final commits with no island size changes',()=>{
  const {NotchControls}=load('ui/island/NotchControls.tsx');
  for(const width of [undefined,110,144,320]) {
    const calls=[],tree=NotchControls({settings:{notchWidth:width},background:'#000000',accent:'#ffffff',patch:p=>calls.push(p)}),slider=labeled(tree,'Notch width');
    assert.deepEqual([slider.props.min,slider.props.max,slider.props.step,slider.props.value],[110,320,2,width??144]);
    for(const value of [110,144,320]) {slider.props.onCommit(value,false);assert.equal(calls.length,0);}
    slider.props.onCommit(110,true);slider.props.onCommit(320,true);assert.deepEqual(calls,[{notchWidth:110},{notchWidth:320}]);
  }
});
function surfaceHarness({unchangedSizing=false,maxHeight=1190}={}) {
  const state={rev:1,mode:'large',view:'terminal'},starts=[],animation={stop(){},start:async target=>{starts.push(target);}};
  const d=driver({
    'framer-motion':{motion:{section:'section'},useReducedMotion:()=>true,useAnimationControls:()=>animation},
    'ui/ipc/island-state.ts':{useIslandState:()=>state},
    'ui/ipc/client.ts':{},
    'ui/stores/index.ts':{useSettings:()=>({offsetX:0,offsetY:0}),usePalette:()=>({})},
    'ui/stores/apps.ts':{useLayout:()=>({rev:1,unchangedSizing,layout:{...layoutFixture,preset:{type:'custom',width:820,maxHeight},buttons:[]}}),useLauncher:()=>({rev:1,items:[]})},
    'ui/island/geometry.ts':{...load('ui/island/geometry.ts'),nativePill:()=>null},
    'ui/island/components/IslandShell.tsx':{UI_SPRING:{}},
  });
  // Animation is a hook-free recording boundary; all sizing and content measurement
  // below still execute the actual IslandSurface component.
  const C=d.load('ui/island/IslandSurface.tsx').IslandSurface;
  return {...d,state,starts,renderSurface:(props={})=>d.render(C,{children:null,fail:e=>{throw e;},...props})};
}

test('explicit custom preset terminal height can exceed the old 672px per-view default',()=>{
  globals({innerWidth:1920,innerHeight:1440,addEventListener(){},removeEventListener(){}},()=>{
    for(const maxHeight of [1190,1280]) {
      const h=surfaceHarness({maxHeight});try {const tree=h.renderSurface();assert.equal(tree.props.initial.height,maxHeight);assert.equal(h.starts.at(-1).height,maxHeight);assert.equal(tree.props.style['--open-content-height'],`${maxHeight-44}px`);}finally{h.dispose();}
    }
  });
});
test('unchanged sizing retains the original terminal height even with a larger stored custom preset',()=>{
  globals({innerWidth:1920,innerHeight:1440,addEventListener(){},removeEventListener(){}},()=>{
    const h=surfaceHarness({unchangedSizing:true});try {assert.equal(h.renderSurface().props.initial.height,672);}finally{h.dispose();}
  });
});

test('content-fitted Settings honors the explicit height cap without forcing empty expansion',()=>{
  const frames=[];
  class Observer {observe(){} unobserve(){} disconnect(){}}
  globals({innerWidth:1920,innerHeight:1440,addEventListener(){},removeEventListener(){},ResizeObserver:Observer,MutationObserver:Observer,
    requestAnimationFrame:cb=>{frames.push(cb);return frames.length;},cancelAnimationFrame(){},getComputedStyle:()=>({paddingTop:'0',paddingBottom:'0'})},()=>{
    for(const [childHeight,expected] of [[300,344],[900,944],[2000,1190]]) {
      const h=surfaceHarness();try {
        const tree=h.renderSurface(),child={offsetHeight:childHeight},pane={firstElementChild:child,offsetHeight:500};
        const root={getBoundingClientRect:()=>({width:820,height:1190}),style:{getPropertyValue:()=>'',setProperty(){}},querySelector:selector=>selector.includes('.app-pane')?pane:selector==='.topline'?{offsetHeight:42}:selector==='.terminal-wrap'?{offsetHeight:500}:{}};
        tree.props.ref.current=root;h.state.view='settings';h.state.rev++;
        h.renderSurface();while(frames.length)frames.shift()();
        const fitted=h.renderSurface();
        assert.equal(fitted.props.initial.height,expected);
        assert.equal(h.starts.at(-1).height,expected,'animation target follows content, not the old 640px Settings ceiling');
        assert.equal(fitted.props.style['--open-content-height'],`${expected-44}px`);
        const palette=h.renderSurface({paletteOpen:true});
        assert.equal(palette.props.initial.height,Math.min(1190,Math.max(560,expected)),'palette expands short pages without violating the saved ceiling');
        assert.equal(h.renderSurface({paletteOpen:false}).props.initial.height,expected,'closing restores content-fit size');
      }finally{h.dispose();frames.length=0;}
    }
  });
});

test('renderer fit bounds retain large custom preferences and fit the full side-button union on small viewports',()=>{
  const {fitSurface,sideRects,effectiveLayout,bounds,clampUnion}=load('ui/island/geometry.ts');
  const layout={...layoutFixture,buttons:['left','right','bottom'].flatMap(side=>[0,3,7].map(slot=>({id:`${side}-${slot}`,side,slot,action:{type:'timer'}})))};
  const original=JSON.stringify(layout);
  for(const viewport of [{width:2560,height:1600},{width:800,height:600},{width:320,height:240},{width:120,height:90}]) {
    const fit=fitSurface(1800,1280,viewport,layout);assert.ok(fit.width>0&&fit.height>0);assert.ok(fit.buttonScale>0&&fit.buttonScale<=1);
    if(viewport.width===2560)assert.deepEqual([fit.width,fit.height],[1800,1280]);
    const rect={x:0,y:0,width:fit.width,height:fit.height},rects=[rect,...sideRects(effectiveLayout(layout,fit.buttonScale),rect).map(b=>b.rect)];
    const box=bounds(rects);assert.ok(box.width<=viewport.width+1e-8);assert.ok(box.height<=viewport.height+1e-8);
    const offset=clampUnion(rects,{x:9999,y:-9999},viewport);
    for(const r of rects){assert.ok(r.x+offset.x>=-1e-8);assert.ok(r.y+offset.y>=-1e-8);assert.ok(r.x+offset.x+r.width<=viewport.width+1e-8);assert.ok(r.y+offset.y+r.height<=viewport.height+1e-8);}
  }
  assert.equal(JSON.stringify(layout),original,'viewport fitting must not rewrite saved preferences');
});

test('Settings wires the terminal launch preference without restarting or submitting a chat',async()=>{
  const saved=[];
  const d=driver({'ui/stores/index.ts':{useSettings:()=>({terminalCommand:'hermes --cli'}),useAppearance:()=> 'skin',usePalette:()=>({}),saveSettings:async patch=>saved.push(patch)}});
  try {globals({navigator:{userAgent:'Linux'},document:{documentElement:{dataset:{displayBackend:'wayland'}}}},()=>{
    const tree=d.render(d.load('ui/views/SettingsView.tsx').SettingsView),editor=component(tree,'TerminalCommandSettings');
    assert.ok(editor,'General settings must expose the terminal launch command');
    assert.equal(editor.props.value,'hermes --cli');
    editor.props.onSave('hermes --tui');
    assert.deepEqual(saved,[{terminalCommand:'hermes --tui'}]);
  });}finally{d.dispose();}
});

test('Settings search shows matching rows from every tab, empty state, Esc clears; card Reset patches Rust defaults after inline confirm',async()=>{
  const saved=[],appearances=[];
  const settings={translucent:true,companionEnabled:true,reduceMotion:false,animationSpeed:150,terminalCursorStyle:'bar'};
  const defaults={reduceMotion:false,animationSpeed:100,swipeNavigation:true,stealth:false,clickSounds:true,gameModeAuto:true,gameMode:false,openShortcut:'Meta+`',
    customAccent:'',customBackground:'',customText:'',hermesThemeSync:false,accent:'',background:'',borderColor:''};
  const d=driver({'ui/stores/index.ts':{useSettings:()=>settings,useAppearance:()=> 'album',usePalette:()=>({}),saveSettings:async p=>saved.push(p),saveAppearance:async a=>appearances.push(a)},
    'ui/ipc/client.ts':{settingsDefaults:async()=>defaults,displayOutputs:async()=>['DP-1'],terminalCopy:async()=>{}}});
  // globals() restores synchronously; this async body keeps its own fixture for the awaits.
  const before={navigator:Object.getOwnPropertyDescriptor(globalThis,'navigator'),document:Object.getOwnPropertyDescriptor(globalThis,'document')};
  for(const [k,v] of Object.entries({navigator:{userAgent:'Linux'},document:{documentElement:{dataset:{displayBackend:'wayland'}}}}))Object.defineProperty(globalThis,k,{value:v,writable:true,configurable:true});
  try {await (async()=>{
    const View=d.load('ui/views/SettingsView.tsx').SettingsView;
    let tree=d.render(View);
    const rows=t=>nodes(t).filter(n=>typeof n.props?.label==='string'&&n.type?.name==='Row').map(n=>n.props.label);
    assert.ok(rows(tree).includes('Open shortcut')&&!rows(tree).includes('Terminal cursor'),'General tab only');
    labeled(tree,'Search settings').props.onChange({target:{value:'CURSOR'}});tree=d.render(View);
    assert.deepEqual(rows(tree),['Terminal cursor','Blinking cursor'],'Appearance rows found from the General tab');
    assert.equal(nodes(tree).find(n=>n.props?.role==='tablist'),undefined,'tabs hide while searching');
    labeled(tree,'Search settings').props.onChange({target:{value:'zzz'}});tree=d.render(View);
    assert.match(nodes(tree).find(n=>n.props?.className==='settings-empty').props.children.join(''),/No settings match/);
    let prevented=0;labeled(tree,'Search settings').props.onKeyDown({key:'Escape',preventDefault(){prevented++;},stopPropagation(){}});tree=d.render(View);
    assert.equal(prevented,1);assert.equal(labeled(tree,'Search settings').props.value,'');
    labeled(tree,'Search settings').props.onChange({target:{value:'animation'}});tree=d.render(View);
    const motion=nodes(tree).find(n=>n.type?.name==='ResetHeader'&&n.props.title==='Motion');
    assert.ok(motion?.props.onReset,'Motion card has a Reset');
    await motion.props.onReset();
    assert.deepEqual(saved,[{reduceMotion:false,animationSpeed:100}]);
    labeled(tree,'Search settings').props.onChange({target:{value:'colour scheme'}});tree=d.render(View);
    await nodes(tree).find(n=>n.type?.name==='ResetHeader'&&n.props.title==='Colour scheme').props.onReset();
    assert.deepEqual(saved[1],{customAccent:'',customBackground:'',customText:'',hermesThemeSync:false,accent:'',background:'',borderColor:''});
    assert.deepEqual(appearances,['skin'],'scheme reset returns to the default scheme');
    assert.equal(nodes(tree).find(n=>n.type?.name==='ResetHeader'&&n.props.title==='Backup'),undefined);
  })();}finally{d.dispose();for(const [k,v] of Object.entries(before))if(v)Object.defineProperty(globalThis,k,v);else delete globalThis[k];}
});
test('ResetHeader asks inline before resetting',async()=>{
  const d=driver(),calls=[];
  const H=d.load('ui/views/SettingsView.tsx').ResetHeader,props={title:'Motion',onReset:async()=>calls.push('reset')};
  let tree=d.render(H,props);
  labeled(tree,'Reset Motion').props.onClick();assert.deepEqual(calls,[],'first tap only arms');
  tree=d.render(H,props);assert.ok(nodes(tree).some(n=>n.type==='small'&&n.props.children==='Reset?'));
  labeled(tree,'Confirm reset Motion').props.onClick();await tick();assert.deepEqual(calls,['reset']);
  d.dispose();
});

function commandHarness(value='',onSave=async()=>{}) {
  const d=driver(),C=d.load('ui/island/TerminalCommandSettings.tsx').TerminalCommandSettings;
  const props={value,onSave};return {...d,props,renderEditor:()=>d.render(C,props)};
}
test('terminal command editing and presets preserve literal input and save only on submit',async()=>{
  const calls=[],h=commandHarness('',async value=>calls.push(value));
  try {
    let tree=h.renderEditor();assert.equal(labeled(tree,'Hermes launch command').props.value,'');
    assert.equal(textButton(tree,'Save command').props.disabled,true);
    textButton(tree,'hermes --cli').props.onClick();tree=h.renderEditor();
    assert.equal(labeled(tree,'Hermes launch command').props.value,'hermes --cli');assert.deepEqual(calls,[]);
    const literal='"/path with spaces/hermes" --profile "work desk" --CLI';
    labeled(tree,'Hermes launch command').props.onChange({target:{value:literal}});tree=h.renderEditor();
    nodes(tree).find(n=>n.type==='form').props.onSubmit({preventDefault(){}});await tick();
    assert.deepEqual(calls,[literal],'never normalize or execute command text in the renderer');
    assert.match(nodes(h.renderEditor()).find(n=>n.props?.role==='status').props.children,/next terminal session/);
  } finally {h.dispose();}
});
test('terminal command failure retains the draft and allows reset to the built-in launcher',async()=>{
  const calls=[],h=commandHarness('hermes',async value=>{calls.push(value);if(value)throw Error('Invalid launch command');});
  try {
    let tree=h.renderEditor();labeled(tree,'Hermes launch command').props.onChange({target:{value:'hermes "'}});tree=h.renderEditor();
    nodes(tree).find(n=>n.type==='form').props.onSubmit({preventDefault(){}});await tick();tree=h.renderEditor();
    assert.equal(nodes(tree).find(n=>n.props?.role==='alert').props.children,'Invalid launch command');
    assert.equal(labeled(tree,'Hermes launch command').props.value,'hermes "');
    textButton(tree,'Reset to built-in').props.onClick();await tick();tree=h.renderEditor();
    assert.deepEqual(calls,['hermes "','']);assert.equal(labeled(tree,'Hermes launch command').props.value,'');
    assert.equal(nodes(tree).some(n=>n.props?.role==='alert'),false);
  }finally{h.dispose();}
});
test('terminal command serializes saves and preserves edits made during an in-flight save',async()=>{
  const pending=deferred(),calls=[],h=commandHarness('hermes',value=>{calls.push(value);return pending.promise;});
  try {
    let tree=h.renderEditor();labeled(tree,'Hermes launch command').props.onChange({target:{value:'hermes --cli'}});tree=h.renderEditor();
    const submit=nodes(tree).find(n=>n.type==='form').props.onSubmit;
    submit({preventDefault(){}});submit({preventDefault(){}});assert.deepEqual(calls,['hermes --cli']);
    tree=h.renderEditor();labeled(tree,'Hermes launch command').props.onChange({target:{value:'hermes --tui'}});
    h.props.value='hermes --cli';pending.resolve();await tick();h.renderEditor();tree=h.renderEditor();
    assert.equal(labeled(tree,'Hermes launch command').props.value,'hermes --tui');
    assert.equal(textButton(tree,'Save command').props.disabled,false);
  }finally{h.dispose();}
});

