import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const { ClipboardView } = load('ui/island/components/ClipboardView.tsx');
const { NowPlaying, playerStream } = load('ui/island/components/NowPlaying.tsx');
const render = (component, props) => renderToStaticMarkup(createElement(component, props));
const clip = (id, copiedAt) => ({ id, kind:'text', preview:`Preview ${id}`, bytes:20, pinned:false, copiedAt, sourceApp:null, ageLabel:'Now' });
test('ClipboardView renders newest first including actionable entry identities without mutating the input', () => {
  const items = [clip('middle', 2000), clip('oldest', 1000), clip('newest', 3000)];
  const html = render(ClipboardView, { model:{ paused:false, items }, intents:{ clipboard() {} } });
  const ids = [...html.matchAll(/aria-label="Copy entry ([^"]+)"/g)].map(match => match[1]);
  assert.deepEqual(ids, ['newest','middle','oldest']);
  assert.deepEqual(items.map(item => item.id), ['middle','oldest','newest']);
});
const media = positionMs => ({ nowPlaying:{ player:'Fixture', title:'Track', artist:'Artist', album:'Album', playing:true,
  positionMs, durationMs:180000, artUrl:null, canControl:true, busName:'org.mpris.MediaPlayer2.fixture',
  status:'playing', rate:1, sampledAtMs:1000, trackId:'/fixture/track', canSeek:true, canGoNext:true, canGoPrevious:true },
  lyrics:{ status:'off', lines:[], plain:null, activeIndex:null }, equalizer:{ active:false, bands:[] } });
test('NowPlaying clamps negative position before formatting the elapsed label never displaying negative time', () => {
  for (const position of [-1, -1000, -61000]) {
    const html = render(NowPlaying, { model:media(position), intents:{ media() {} } });
    assert.match(html, /class="app-footer muted"><span>0:00<\/span>/);
    assert.match(html, /aria-label="Track progress" max="180000" value="0"/);
    assert.doesNotMatch(html, /<span>-\d/);
  }
  assert.match(render(NowPlaying, { model:media(null), intents:{ media() {} } }), /<span>Unknown<\/span>/);
});

function capture(Component, props) { let tree; function Capture() { tree = Component(props); return tree; } renderToStaticMarkup(createElement(Capture)); return tree; }
function nodes(tree) { if (!tree || typeof tree !== 'object') return []; if (Array.isArray(tree)) return tree.flatMap(nodes); return [tree, ...nodes(tree.props?.children)]; }
const stream = (index,pid,appName='spotify') => ({index,pid,appName,binary:'spotify',sinkIndex:1,mediaName:null,iconName:null,muted:false,corked:false,volumePercent:65});
test('NowPlaying chooses writable MPRIS volume first and stream fallback never changes system output', () => {
  const model = media(1000); Object.assign(model.nowPlaying,{volumePercent:52,volumeWritable:true,pid:42});
  const calls=[]; const props={model,intents:{media:op=>calls.push(op)},mixer:{available:true,streams:[stream(8,42)]},onStreamVolume:(index,percent)=>calls.push({index,percent})};
  let tree=capture(NowPlaying,props); let slider=nodes(tree).find(n=>n.props?.['aria-label']==='Player volume');
  assert.equal(slider.props.value,52); slider.props.onCommit(31,true); assert.deepEqual(calls.pop(),{type:'volume',percent:31});
  model.nowPlaying.volumeWritable=false;
  tree=capture(NowPlaying,props);slider=nodes(tree).find(n=>n.props?.['aria-label']==='Player volume');
  assert.equal(slider.props.value,65);slider.props.onCommit(20,true);assert.deepEqual(calls.pop(),{index:8,percent:20});
  assert.equal(playerStream(model.nowPlaying,[stream(8,42),stream(9,42)]),null,'ambiguous streams are unavailable, not guessed');
  assert.equal(playerStream({...model.nowPlaying,pid:99,player:'spotify'},[stream(8,42)]).index,8,'exact application-name fallback');
});
test('NowPlaying displays JPEG artwork, opt-in lyrics and a capture-free playing indicator', () => {
  const model=media(1000);model.nowPlaying.artUrl='data:image/jpeg;base64,AAAA';
  let html=render(NowPlaying,{model,intents:{media(){}},lyricsEnabled:false});
  assert.match(html,/src="data:image\/jpeg;base64,AAAA"/); assert.match(html,/Enable lyrics from LRCLIB/); assert.match(html,/Title, artist, album and duration are shared/); assert.match(html,/Playing indicator/);
  model.nowPlaying.playing=false;model.lyrics={status:'synced',lines:[{timeMs:0,text:'First'},{timeMs:1000,text:'Current'}],plain:'First\nCurrent',activeIndex:1};
  html=render(NowPlaying,{model,intents:{media(){}},lyricsEnabled:true});
  assert.match(html,/Paused indicator/);assert.match(html,/aria-current="true">Current/);
  const toggles=[]; const tree=capture(NowPlaying,{model,intents:{media(){}},lyricsEnabled:true,onLyricsEnabled:enabled=>toggles.push(enabled)});
  nodes(tree).find(n=>n.props?.['aria-label']==='Enable lyrics from LRCLIB').props.onChange({target:{checked:false}});assert.deepEqual(toggles,[false]);
});
