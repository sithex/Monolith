import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './phase7b-loader.mjs';
import { readFileSync } from 'node:fs';
test('island shadow includes an ambient halo, not just a bottom-heavy bar', () => {
  const root=process.env.ISLAND_UI_ROOT ?? new URL('../ui',import.meta.url).pathname;
  const css=readFileSync(`${root}/island/vorssaint.css`,'utf8');
  assert.match(css, /\.island\[data-state\]\s*\{\s*box-shadow:\s*0 0 24px/);
});
const { notchMorphPath } = load(`${process.env.ISLAND_UI_ROOT ?? new URL('../ui', import.meta.url).pathname}/island/notch-profile.ts`);
test('withdrawal ends at the notch crown, shoulders and bottom instead of a pill', () => {
  const d=notchMorphPath(144,26,0,1,0.8);
  assert.ok(d.startsWith('M 0.5 0.5 H 143.5 C 138 0.5 138 3 138 6'));
  assert.ok(d.endsWith('V 6 C 6 3 6 0.5 0.5 0.5 Z'));
  assert.ok(d.includes('H 16 '));
  assert.notEqual(d,notchMorphPath(144,26,13,0,0.8));
});
test('notch outline remains inside its unchanged taskbar box for all border widths', () => {
  for(const width of [0,.8,1,2,4]) {
    const d=notchMorphPath(144,26,0,1,width);
    for (const [,command,values] of d.matchAll(/([MHVC])\s+([\d.\s]+)/g)) {
      const numbers=values.trim().split(/\s+/).map(Number);
      assert.ok(numbers.every(Number.isFinite));
      for(let i=0;i<numbers.length;i++) {
        const limit=command==='H'?144:command==='V'?26:i%2===0?144:26;
        assert.ok(numbers[i]>=width/2&&numbers[i]<=limit-width/2);
      }
    }
  }
});
