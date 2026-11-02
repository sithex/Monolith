import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const a=load('ui/views/app-adapters.ts');
const { createClipboardHistory }=load('ui/stores/clipboard-history.ts');
const { createEpochTopic }=load('ui/stores/epoch-topic.ts');
const { enqueueEpochCommand }=load('ui/views/app-lifecycle.ts');
const editor=load('ui/views/calendar-editor.ts');
const { ControlsApp }=load('ui/island/components/ControlsApp.tsx');
const { ClipboardView }=load('ui/island/components/ClipboardView.tsx');
const tick=()=>new Promise(resolve=>setImmediate(resolve));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
const clip=(patch={})=>({rev:1,id:'c-1',kind:'text',preview:'Fixture only',bytes:12,pinned:false,copiedAt:1000,sourceApp:null,...patch});
const status=(patch={})=>({available:true,enabled:true,paused:false,detail:'ready',watcherRunning:true,connectionsOpen:1,connectionAttempts:1,dropped:0,captured:1,sensitiveSkipped:0,denied:0,echoes:0,...patch});
function historyHarness(options={}) {
  const reads=[],added=[],changed=[],unlistened=[];
  const owner=createClipboardHistory({
    list:(query,limit)=>{const d=deferred();reads.push({query,limit,...d});return d.promise;},
    status: options.status??(async()=>status()),
    added:async cb=>{added.push(cb);return ()=>unlistened.push('added');},
    changed:async cb=>{changed.push(cb);return ()=>unlistened.push('status');},
  });
  return {owner,reads,added,changed,unlistened};
}
const render=(C,props)=>renderToStaticMarkup(createElement(C,props));
const noop=new Proxy({},{get:()=>()=>{}});
const toggles={rev:1,available:true,detail:'ready',powerAvailable:true,powerDetail:'ready',powerProfile:'balanced',powerChoices:['balanced','power-saver'],degraded:null,heldBy:[],micAvailable:true,micDetail:'ready',micMuted:false,dnd:false,powerRequest:null,micRequest:null};
const updates={rev:1,available:true,detail:'ready',count:null,repoCount:null,aurCount:null,needsReboot:false,stale:false,error:null,items:[],ageSecs:null,nextCheckInSecs:60};
const track={player:'Fixture',title:'Fixture title',artist:'Fixture artist',album:'',playing:true,positionMs:null,durationMs:null,artUrl:null,canControl:true,busName:'org.mpris.MediaPlayer2.fixture',status:'playing',rate:1,sampledAtMs:1000,trackId:'/fixture/track',canSeek:false,canGoNext:true,canGoPrevious:false,volumePercent:null,volumeWritable:false,pid:null};
const draft={uid:'fixture-event',summary:'Lunch, tea; \\ work\nNext line',location:'Fixture room',start:'2026-11-02T09:30:00',end:'2026-11-02T10:30:00',allDay:false};
const row={uid:'fixture-event',summary:'Fixture title',location:null,startS:1793611800,endS:1793615400,allDay:false,source:'local',readOnly:false};

test('runtime Controls renders working toggles when tiles and updates are independently absent',()=>{
  const model=a.adaptIndependentControls(toggles,null,null);
  const html=render(ControlsApp,{model,intents:noop});
  assert.match(html,/aria-label="Mic mute"/);assert.match(html,/aria-label="Cycle power profile"/);assert.match(html,/Do Not Disturb/);
  assert.match(html,/System tiles disabled/);assert.match(html,/Package updates disabled/);assert.doesNotMatch(html,/0 repo|0 AUR|update-badge/);
  const onlyUpdates=render(ControlsApp,{model:a.adaptIndependentControls(null,null,updates),intents:noop});
  assert.match(onlyUpdates,/Unknown · no successful check/);assert.match(onlyUpdates,/Check updates/);assert.doesNotMatch(onlyUpdates,/aria-label="Mic mute"/);
});
test('runtime clipboard push invalidates one authoritative list and coalesces bursts without speculative append',async()=>{
  const h=historyHarness();await h.owner.configure(true,'first');h.owner.setVisible(true);
  assert.equal(h.reads.length,1);assert.equal(h.reads[0].limit,500);
  for(let rev=2;rev<=100;rev++)h.added[0](clip({rev}));
  assert.equal(h.reads.length,1);assert.deepEqual(h.owner.getSnapshot().items,[]);
  h.reads[0].resolve([clip()]);await tick();assert.equal(h.reads.length,2);assert.deepEqual(h.owner.getSnapshot().items,[]);
  h.reads[1].resolve([clip({rev:100,id:'c-100'})]);await tick();assert.equal(h.owner.getSnapshot().items[0].id,'c-100');
  h.added[0](clip({rev:99}));h.added[0](clip({rev:100}));assert.equal(h.reads.length,2);
  h.owner.dispose();
});
test('runtime clipboard search and empty results fence stale in-flight query replies',async()=>{
  const h=historyHarness();await h.owner.configure(true,'first');h.owner.setVisible(true);
  h.owner.setQuery('old');h.owner.setQuery('latest');
  h.reads[0].resolve([clip()]);await tick();assert.equal(h.reads.length,2);assert.equal(h.reads[1].query,'latest');assert.deepEqual(h.owner.getSnapshot().items,[]);
  h.reads[1].resolve([]);await tick();assert.equal(h.owner.getSnapshot().query,'latest');assert.deepEqual(h.owner.getSnapshot().items,[]);
  assert.throws(()=>h.owner.setQuery('é'.repeat(2049)),/Invalid clipboard search/);h.owner.dispose();
});
test('runtime clipboard hide/reopen and lifecycle reset keep one flight and reject old callbacks and errors',async()=>{
  const h=historyHarness();await h.owner.configure(true,'first');h.owner.setVisible(true);const epoch=h.owner.epoch();
  h.owner.setVisible(false);h.owner.setVisible(true);await h.owner.configure(false,'off');await h.owner.configure(true,'new');
  h.added[0](clip({rev:900}));h.changed[0](status({paused:true}));
  assert.equal(h.reads.length,1);assert.equal(h.owner.getSnapshot().status.paused,false);assert.equal(h.owner.isCurrent(epoch),false);
  h.reads[0].reject(Error('old lifecycle error'));await tick();assert.equal(h.reads.length,2);assert.equal(h.owner.getSnapshot().error,null);
  h.reads[1].resolve([clip({rev:0})]);await tick();assert.equal(h.owner.getSnapshot().items[0].rev,0);assert.deepEqual(h.unlistened,['added','status','added','status']);h.owner.dispose();
});
test('runtime clipboard push status wins bootstrap race and pause is never guessed',async()=>{
  const pending=deferred();const h=historyHarness({status:()=>pending.promise});
  const setup=h.owner.configure(true,'first');await tick();h.changed[0](status({paused:true}));pending.resolve(status({paused:false}));await setup;
  assert.equal(h.owner.getSnapshot().status.paused,true);assert.equal(h.reads.length,0);
  const html=render(ClipboardView,{model:{items:[],paused:true,status:h.owner.getSnapshot().status,query:'',error:null},intents:noop});
  assert.match(html,/Resume capture/);assert.match(html,/Capture paused/);h.owner.dispose();
});
test('runtime clipboard status/bootstrap refreshes are serialized and latest lifecycle is fetched',async()=>{
  const pending=[];let active=0,max=0;const h=historyHarness({status:()=>{active++;max=Math.max(max,active);const d=deferred();pending.push(d);return d.promise.finally(()=>active--);}});
  const first=h.owner.configure(true,'one');await tick();const second=h.owner.configure(false,'two');await tick();const third=h.owner.configure(true,'three');await tick();
  assert.equal(pending.length,1);pending[0].resolve(status());await tick();assert.equal(pending.length,2);pending[1].resolve(status());await Promise.all([first,second,third]);assert.equal(max,1);h.owner.dispose();
});
test('runtime clipboard malformed push or list cannot poison revision or expose concealed rows',async()=>{
  const h=historyHarness();await h.owner.configure(true,'first');h.owner.setVisible(true);
  h.added[0](clip({rev:4000,concealed:true}));assert.match(h.owner.getSnapshot().error,/Malformed/);
  h.reads[0].resolve([clip({rev:2,preview:'Safe fixture'})]);await tick();assert.equal(h.owner.getSnapshot().items[0].preview,'Safe fixture');
  h.added[0](clip({rev:3}));h.reads[1].resolve([clip({rev:3}),clip({rev:3})]);await tick();assert.match(h.owner.getSnapshot().error,/Malformed/);assert.equal(h.owner.getSnapshot().items.length,1);h.owner.dispose();
});
test('runtime clipboard adapters validate identities bounds nulls kinds counters and UTF-8',()=>{
  assert.deepEqual(a.parseClip(clip()),clip());
  for(const patch of [{id:'../c-1'},{id:'c-01'},{kind:'html'},{bytes:1048577},{copiedAt:NaN},{rev:-1},{pinned:0},{preview:'😀'.repeat(281)},{sourceApp:undefined},{sensitive:true}])assert.throws(()=>a.parseClip(clip(patch)),/Malformed/);
  assert.equal(a.parseClip(clip({preview:'😀'.repeat(280)})).preview.length,560);
  assert.throws(()=>a.parseClips([clip(),clip({id:'c-2',rev:2})]),/Malformed/);
  for(const patch of [{enabled:'yes'},{paused:null},{connectionsOpen:-1},{captured:0.5},{detail:'\0'}])assert.throws(()=>a.parseClipboardStatus(status(patch)),/Malformed/);
});
test('runtime media adapters validate full DTOs and retain null position duration and optional track identity',()=>{
  assert.deepEqual(a.parseMedia({rev:2,nowPlaying:track}),{rev:2,nowPlaying:track});assert.deepEqual(a.parseMedia({rev:3,nowPlaying:null}),{rev:3,nowPlaying:null});
  for(const patch of [{busName:'org.fake.player'},{trackId:'bad/id'},{positionMs:-1},{durationMs:Infinity},{sampledAtMs:null},{playing:1},{status:'buffering'},{rate:NaN},{artUrl:'file:///private'},{title:'x'.repeat(16385)},{canControl:undefined}])assert.throws(()=>a.parseMedia({rev:1,nowPlaying:{...track,...patch}}),/Malformed/);
  assert.throws(()=>a.parseMedia({rev:0.5,nowPlaying:null}),/Malformed/);
  for (const artUrl of ['https://i.scdn.co/image/test','data:image/svg+xml;base64,AAAA','data:image/jpeg;base64,'+'A'.repeat(1400000)]) assert.throws(()=>a.parseMedia({rev:1,nowPlaying:{...track,artUrl}}),/Malformed/);
  for (const mime of ['png','jpeg','webp']) assert.equal(a.parseMedia({rev:1,nowPlaying:{...track,artUrl:`data:image/${mime};base64,AAAA`}}).nowPlaying.artUrl,`data:image/${mime};base64,AAAA`);
  for (const patch of [{volumePercent:NaN},{volumePercent:-1},{volumeWritable:1},{pid:-1}]) assert.throws(()=>a.parseMedia({rev:1,nowPlaying:{...track,...patch}}),/Malformed/);
});
test('runtime lyric and equalizer adapters reject unordered lines invalid active index enum and band ranges',()=>{
  const lyric={rev:1,status:'synced',trackKey:'0123456789abcdef',lines:[{timeMs:1,text:'One'},{timeMs:2,text:'Two'}],plain:null,activeIndex:1};
  assert.deepEqual(a.parseLyrics(lyric),lyric);
  for(const patch of [{status:'ready'},{trackKey:'wrong'},{activeIndex:2},{activeIndex:-1},{lines:[{timeMs:2,text:'Two'},{timeMs:1,text:'One'}]},{plain:'é'.repeat(524289)}])assert.throws(()=>a.parseLyrics({...lyric,...patch}),/Malformed/);
  assert.deepEqual(a.parseEqualizer({rev:1,active:false,bands:[]}),{rev:1,active:false,bands:[]});
  for(const raw of [{rev:1,active:true,bands:[]},{rev:1,active:true,bands:Array(33).fill(0)},{rev:1,active:true,bands:Array(8).fill(1.1)},{rev:1,active:true,bands:Array(8).fill(NaN)}])assert.throws(()=>a.parseEqualizer(raw),/Malformed/);
});
test('runtime Calendar creates escaped bounded single-event ICS and edits/deletes exact local UID',()=>{
  const created=editor.saveEvent(draft,null);assert.equal(created.type,'create');assert.match(created.ics,/SUMMARY:Lunch\\, tea\\; \\\\ work\\nNext line\r\n/);assert.equal((created.ics.match(/BEGIN:VEVENT/g)??[]).length,1);
  assert.deepEqual(editor.saveEvent(draft,row),{type:'edit',source:'local',uid:row.uid,ics:created.ics});assert.deepEqual(editor.deleteEvent(row),{type:'delete',source:'local',uid:row.uid});
  const long=editor.eventDocument({...draft,summary:'😀'.repeat(100)});for(const line of long.split('\r\n'))assert.ok(Buffer.byteLength(line)<=75);
  assert.equal(long.replace(/\r\n /g,'').includes('😀'.repeat(100)),true);
});
test('runtime Calendar refuses read-only mutations identity changes invalid dates and injection',()=>{
  for(const r of [{...row,readOnly:true},{...row,source:'subscription'}]){assert.throws(()=>editor.saveEvent(draft,r),/read-only/);assert.throws(()=>editor.deleteEvent(r),/read-only/);}
  assert.throws(()=>editor.saveEvent({...draft,uid:'another'},row),/identity/);
  for(const patch of [{summary:''},{uid:'a\r\nBEGIN:VEVENT'},{start:'2026-02-31T09:00:00'},{end:'2026-11-02T08:00:00'},{location:'\0'},{summary:'é'.repeat(4097)}])assert.throws(()=>editor.eventDocument({...draft,...patch}));
  const doc=editor.eventDocument({...draft,summary:'Meeting\r\nEND:VEVENT\r\nBEGIN:VEVENT'});assert.equal(doc.split('\r\n').filter(l=>l==='BEGIN:VEVENT').length,1);
});
test('runtime Calendar all-day uses date values and exclusive end without timezone shifting',()=>{
  const doc=editor.eventDocument({...draft,allDay:true,start:'2026-11-02',end:'2026-11-04'});assert.match(doc,/DTSTART;VALUE=DATE:20261102\r\nDTEND;VALUE=DATE:20261104/);assert.doesNotMatch(doc,/T0900|TZID/);
  assert.throws(()=>editor.eventDocument({...draft,allDay:true,start:'2026-11-02',end:'2026-11-02'}),/after/);
});
test('runtime queued app commands capture lifecycle before queueing and fence in-flight replies',async()=>{
  const store=createEpochTopic(),queue=[],sent=[];const send=async()=>{sent.push(1);return {rev:1};};
  enqueueEpochCommand(store,fn=>queue.push(fn),send);store.reset();await queue.shift()();assert.deepEqual(sent,[]);
  const pending=deferred();enqueueEpochCommand(store,fn=>queue.push(fn),()=>pending.promise);const running=queue.shift()();store.reset();pending.resolve({rev:99});await running;assert.equal(store.getSnapshot(),null);
  enqueueEpochCommand(store,fn=>queue.push(fn),send);await queue.shift()();assert.equal(store.getSnapshot().rev,1);
});
