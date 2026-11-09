import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';

const { ArtworkCrossfade, LyricsStage } = load('ui/island/components/NowPlaying.tsx');
const { DUR, EASE_OUT, setMotionSpeed } = load('ui/island/motion-profile.ts');
const mediaCss = readFileSync(new URL('../ui/island/media.css', import.meta.url), 'utf8');
const motionCss = readFileSync(new URL('../ui/island/motion-media.css', import.meta.url), 'utf8');
const track = { busName: 'org.mpris.MediaPlayer2.fixture', trackId: '/track/one', title: 'Track', playing: true };
const lines = Array.from({ length: 15 }, (_, i) => ({ timeMs: i * 1000, text: `Line ${i}` }));
const props = (activeIndex = 7, overrides = {}) => ({
  model: { lyrics: { status: 'synced', lines, plain: null, activeIndex } },
  track, reduced: false, enabled: true, seek() {}, ...overrides,
});
function capture(Component, input) {
  let tree;
  function Capture() { tree = Component(input); return tree; }
  renderToStaticMarkup(createElement(Capture));
  return tree;
}
function nodes(tree) {
  if (!tree || typeof tree !== 'object') return [];
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  return [tree, ...nodes(tree.props?.children)];
}
const content = input => nodes(capture(LyricsStage, input)).find(node => node.props?.className === 'np-lyrics-content');
function withKeyboard(run) {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'document');
  Object.defineProperty(globalThis, 'document', { configurable: true, value: { documentElement: { dataset: { navVia: 'key' } } } });
  try { run(); } finally {
    if (previous) Object.defineProperty(globalThis, 'document', previous);
    else delete globalThis.document;
  }
}

test('lyrics animate blur only within the inclusive ±4 window and keep the original blur distances', () => {
  for (const active of [0, 7, 14, null]) {
    const items = nodes(capture(LyricsStage, props(active))).filter(node => node.type === 'li');
    assert.equal(items.length, lines.length);
    items.forEach((item, index) => {
      const delta = active === null ? null : Math.abs(index - active);
      assert.equal(item.props['data-near'], delta !== null && delta <= 4);
      assert.equal(item.props.style['--d'], delta === null ? 2 : Math.min(4, delta));
      assert.equal(item.props['aria-current'], index === active ? 'true' : undefined);
    });
  }
  const base = mediaCss.match(/\.island \.now-playing \.am-lyrics button \{([^}]+)\}/)[1];
  assert.match(base, /filter: blur\(calc\(var\(--d, 2\) \* \.5px\)\)/);
  assert.doesNotMatch(base.match(/transition: ([^;]+)/)[1], /filter/);
  assert.match(mediaCss, /li\[data-near='true'\] button \{[^}]*transition-property: opacity, transform, background-color, filter/);
});

test('transport and lyrics use the shared press scale with independent short lyric press timing', () => {
  assert.doesNotMatch(mediaCss, /button:active|scale\(\.9\)|scale\(\.95\)|scale\(1\.04\)/);
  assert.match(mediaCss, /\.am-lyrics li \{[^}]*transition: transform \.7s/);
  assert.match(mediaCss, /transition: opacity \.55s ease, transform var\(--dur-press\) var\(--ease-out\)/);
  assert.match(mediaCss, /transition-duration: \.55s, var\(--dur-press\), var\(--dur-base\), \.55s/);
  assert.match(mediaCss, /@media \(hover: hover\) and \(pointer: fine\) \{\s*\.island \.now-playing \.transport/);
});

test('artwork retains keyed outgoing content and fades opacity without layout or scale motion', () => {
  const src = 'data:image/jpeg;base64,AAAA';
  const tree = ArtworkCrossfade({ src, alt: 'Artwork', reduced: false });
  assert.equal(tree.props.initial, false, 'the first artwork does not blink in');
  const layer = tree.props.children;
  assert.equal(layer.key, src);
  assert.deepEqual(layer.props.initial, { opacity: 0 });
  assert.deepEqual(layer.props.animate, { opacity: 1 });
  assert.deepEqual(layer.props.exit, { opacity: 0 });
  assert.equal(layer.props.transition.duration, DUR.base);
  assert.deepEqual(layer.props.transition.ease, EASE_OUT);
  assert.notEqual(ArtworkCrossfade({ src: null, alt: '', reduced: false }).props.children.key, layer.key);
  assert.match(motionCss, /\.np-artwork-layer \{ position: absolute; inset: 0; \}/);
  for (const invalid of ['https://example.invalid/art.jpg', 'file:///art.jpg', 'data:image/svg+xml;base64,AAAA']) {
    assert.doesNotMatch(renderToStaticMarkup(createElement(ArtworkCrossfade, { src: invalid, alt: 'Artwork', reduced: false })), /<img/);
  }
});

test('lyrics fade on load or track replacement, not on every active-line update', () => {
  const loading = content(props(7, { model: { lyrics: { status: 'loading', lines: [], plain: null, activeIndex: null } } }));
  assert.equal(loading.props.initial, false);
  const loaded = content(props());
  assert.deepEqual(loaded.props.initial, { opacity: 0 });
  assert.deepEqual(loaded.props.animate, { opacity: 1 });
  assert.equal(loaded.props.transition.duration, DUR.slow);
  assert.deepEqual(loaded.props.transition.ease, EASE_OUT);
  assert.notEqual(loading.key, loaded.key);
  assert.equal(content(props(8)).key, loaded.key);
  assert.notEqual(content(props(7, { track: { ...track, trackId: '/track/two' } })).key, loaded.key);
  const plain = content(props(null, { model: { lyrics: { status: 'plain', lines: [], plain: 'Plain lyrics', activeIndex: null } } }));
  assert.deepEqual(plain.props.initial, { opacity: 0 });
});

test('reduced motion keeps short opacity fades and removes lyric scale and animated filters', () => {
  const layer = ArtworkCrossfade({ src: null, alt: '', reduced: true }).props.children;
  assert.equal(layer.props.transition.duration, DUR.press);
  assert.deepEqual(layer.props.initial, { opacity: 0 });
  const stage = content(props(7, { reduced: true }));
  assert.deepEqual(stage.props.initial, { opacity: 0 });
  assert.equal(stage.props.transition.duration, DUR.press);
  assert.match(motionCss, /\[data-reduced='true'\] \.am-lyrics li \{ transform: none; transition: none; \}/);
  assert.match(motionCss, /\[data-reduced='true'\] \.am-lyrics li button \{\s*filter: none;\s*transition: opacity var\(--dur-press\)/);
  assert.match(readFileSync(new URL('../ui/island/components/NowPlaying.tsx', import.meta.url), 'utf8'), /behavior: reduced \? 'instant' : 'smooth'/);
});

// Controller decision (Nov 9): artwork/lyrics follow playback (ambient), so the last input
// modality must not switch their fades off.
test('artwork and lyric fades are ambient: unaffected by keyboard modality', () => {
  withKeyboard(() => {
    assert.equal(ArtworkCrossfade({ src: null, alt: '', reduced: false }).props.children.props.transition.duration, DUR.base);
    const stage = content(props());
    assert.equal(stage.props.transition.duration, DUR.slow);
  });
});

test('artwork and lyric fades follow the shared animation speed', () => {
  try {
    setMotionSpeed(200);
    assert.equal(ArtworkCrossfade({ src: null, alt: '', reduced: false }).props.children.props.transition.duration, DUR.base / 2);
    assert.equal(content(props()).props.transition.duration, DUR.slow / 2);
  } finally { setMotionSpeed(100); }
});

test('track times and volume use stable tabular numerals', () => {
  assert.match(motionCss, /\.track-progress \.app-footer, \.now-playing \.player-volume small \{ font-variant-numeric: tabular-nums; \}/);
});
