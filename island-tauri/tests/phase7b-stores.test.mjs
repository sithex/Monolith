import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
const { createEpochTopic } = load('ui/stores/epoch-topic.ts');
const { geometryReport } = load('ui/island/geometry.ts');
const { createMediaVisibility } = load('ui/views/media-visibility.ts');
test('music hiding and collapse send false without waiting for unrelated app work', async () => {
  const sent=[]; const owner=createMediaVisibility(async value=>{sent.push(value);},assert.fail);
  owner.set(true);await new Promise(resolve=>setImmediate(resolve));
  owner.set(false);await new Promise(resolve=>setImmediate(resolve));
  assert.deepEqual(sent,[true,false]);
});
test('a rapid music reopen cannot coalesce away a pending privacy hide', async () => {
  const sent=[],replies=[]; const owner=createMediaVisibility(value=>{sent.push(value);return new Promise(resolve=>replies.push(resolve));},assert.fail);
  owner.set(true);owner.set(false);owner.set(true);assert.deepEqual(sent,[true]);
  replies.shift()();await new Promise(resolve=>setImmediate(resolve));assert.deepEqual(sent,[true,false]);
  replies.shift()();await new Promise(resolve=>setImmediate(resolve));assert.deepEqual(sent,[true,false,true]);
  replies.shift()();await new Promise(resolve=>setImmediate(resolve));
});
test('new app lifecycle drops old callbacks and accepts restarted zero revision', () => {
  const store=createEpochTopic(); const old=store.epoch();
  assert.equal(store.apply({rev:50,detail:'old'},old),true);
  const current=store.reset();assert.equal(store.getSnapshot(),null);
  assert.equal(store.apply({rev:51,detail:'late previous lifecycle'},old),false);
  assert.equal(store.apply({rev:0,detail:'new lifecycle'},current),true);
  assert.equal(store.getSnapshot().detail,'new lifecycle');
});
test('older revision cannot overwrite a newly integrated app store', () => {
  const store=createEpochTopic();store.apply({rev:8,detail:'new'});
  assert.equal(store.apply({rev:7,detail:'old'}),false);
  assert.equal(store.apply({rev:8,detail:'equal'}),false);
  assert.equal(store.getSnapshot().detail,'new');
});
test('geometry includes every visible side circle but not a gap filling bounding box', () => {
  const button={offsetLeft:20,offsetTop:10,offsetWidth:36,offsetHeight:36,
    getClientRects:()=>[{}],getBoundingClientRect:()=>({x:23,y:13,width:30,height:30})};
  const hidden={getClientRects:()=>[],getBoundingClientRect:()=>assert.fail('hidden surface measured')};
  const outer={ownerDocument:{querySelectorAll:()=>[button,hidden]},getBoundingClientRect:()=>({x:70,y:8,width:640,height:640})};
  globalThis.getComputedStyle=e=>({visibility:'visible',borderRadius:e===button?'18px':'24px'});
  const r=geometryReport(outer,{rev:4,mode:'large'},true,1);
  assert.deepEqual(r.rects,[{x:70,y:8,width:640,height:640,radius:24},{x:20,y:10,width:36,height:36,radius:18}]);
  assert.equal(r.stateRev,4);assert.equal(r.rects.some(r=>r.x===20&&r.width===690),false);
  assert.equal(geometryReport(outer,{rev:5,mode:'still'},true,1).rects.length,1);
});
