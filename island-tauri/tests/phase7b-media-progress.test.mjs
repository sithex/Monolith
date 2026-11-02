import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const { mediaProgress } = load('ui/island/media-progress.ts');
const { NowPlaying } = load('ui/island/components/NowPlaying.tsx');
const { parseMedia } = load('ui/views/app-adapters.ts');
const sample = overrides => ({ player:'spotify', title:'Track', artist:'Artist', album:'Album', playing:true,
  positionMs:10000, durationMs:180000, sampledAtMs:1000, rate:1, status:'playing',
  artUrl:null, canControl:true, busName:'org.mpris.MediaPlayer2.spotify', trackId:'/track/one', canSeek:true, canGoNext:true, canGoPrevious:true, ...overrides });
test('media progress advances at 1x without new host events and never compounds prior projections', () => {
  const track = sample();
  for (const now of [1000, 1250, 1500, 2000, 51000]) assert.equal(mediaProgress(track, now).position, 10000 + now - 1000);
  assert.equal(track.positionMs,10000);
});
test('media progress freezes on pause and resumes from the replacement host sample', () => {
  const paused = sample({playing:false,status:'paused',positionMs:12000,sampledAtMs:3000});
  assert.equal(mediaProgress(paused,5000).position,12000);
  assert.equal(mediaProgress(paused,50000).position,12000);
  assert.equal(mediaProgress({...paused,playing:true,status:'playing',sampledAtMs:50000},51000).position,13000);
  assert.equal(mediaProgress(sample({status:'stopped',playing:false}),51000).position,0);
});
test('media progress snaps forward and backward on seek without carrying the previous anchor', () => {
  assert.equal(mediaProgress(sample(),5000).position,14000);
  assert.equal(mediaProgress(sample({positionMs:90000,sampledAtMs:5000}),5250).position,90250);
  assert.equal(mediaProgress(sample({positionMs:2000,sampledAtMs:5250}),5500).position,2250);
});
test('media progress replaces anchors on track and preferred-player changes including missing track IDs', () => {
  assert.equal(mediaProgress(sample({trackId:'/track/two',positionMs:0,sampledAtMs:8000}),8250).position,250);
  const browser = sample({busName:'org.mpris.MediaPlayer2.plasma_browser_integration',trackId:null,title:'Browser',positionMs:34000,sampledAtMs:8250});
  assert.equal(mediaProgress(browser,8500).position,34250);
  assert.equal(mediaProgress({...browser,title:'Next',positionMs:null,sampledAtMs:8500},9000).position,null);
});
test('media progress keeps unknown position and duration honest instead of inventing zero', () => {
  assert.deepEqual(mediaProgress(null,9000),{position:null,length:null});
  assert.deepEqual(mediaProgress(sample({positionMs:null}),9000),{position:null,length:180000});
  for (const durationMs of [null,0,-1,NaN,Infinity]) assert.deepEqual(mediaProgress(sample({durationMs}),2000),{position:11000,length:null});
  for (const positionMs of [NaN,Infinity]) assert.equal(mediaProgress(sample({positionMs}),9000).position,null);
});
test('media progress clamps bounds, future samples and invalid clocks and honors playback rate', () => {
  assert.equal(mediaProgress(sample({positionMs:-100}),1000).position,0);
  assert.equal(mediaProgress(sample({positionMs:179900}),2000).position,180000);
  assert.equal(mediaProgress(sample(),500).position,10000);
  assert.equal(mediaProgress(sample(),null).position,10000);
  assert.equal(mediaProgress(sample(),NaN).position,10000);
  assert.equal(mediaProgress(sample({sampledAtMs:NaN}),2000).position,10000);
  assert.equal(mediaProgress(sample({rate:2}),2000).position,12000);
  for (const rate of [0,-1,NaN,Infinity]) assert.equal(mediaProgress(sample({rate}),2000).position,11000);
});
test('parseMedia retains host position sample, epoch and rate, including unknown positions after changes', () => {
  const track = sample({rate:1.5,sampledAtMs:1793611800123});
  const parsed = parseMedia({rev:4,nowPlaying:track}).nowPlaying;
  assert.equal(parsed.sampledAtMs,track.sampledAtMs);
  assert.equal(parsed.rate,1.5);
  assert.equal(mediaProgress(parsed,track.sampledAtMs+1000).position,11500);
  assert.equal(parseMedia({rev:5,nowPlaying:{...track,positionMs:null}}).nowPlaying.positionMs,null);
});
test('NowPlaying uses projected progress bounds for both label and bar and keeps unknown bar empty', () => {
  const render = track => renderToStaticMarkup(createElement(NowPlaying,{model:{nowPlaying:track,lyrics:{status:'off',lines:[],plain:null,activeIndex:null},equalizer:{active:false,bands:[]}},intents:{media(){}}}));
  const stopped = render(sample({playing:false,status:'stopped'}));
  assert.match(stopped,/max="180000" value="0"/);
  assert.match(stopped,/class="app-footer muted"><span>0:00<\/span>/);
  assert.match(render(sample({playing:false,positionMs:200000})),/class="app-footer muted"><span>3:00<\/span>/);
  for (const track of [sample({positionMs:null}),sample({durationMs:null})]) {
    const html = render(track);
    assert.doesNotMatch(html,/<progress/);
    assert.match(html,/Unknown/);
  }
});
