// Colour scheme presets (Nov 2). Rust persists only the choice (`Appearance`) and the
// Custom seed colours (`customAccent`/`customBackground`/`customText` settings);
// the palettes live here so the island, its CSS variables, xterm and the optional
// Hermes theme sync all follow one source.
import type { Appearance, Palette } from '../../ipc/client';

export type SchemeRoles = Omit<Palette, 'name'>;
export type Scheme = { id: Appearance; name: string; note: string; dots: [string, string, string]; palette: SchemeRoles | null };
export type CustomSeeds = { customAccent?: string; customBackground?: string; customText?: string };

const roles = (background: string, surface: string, border: string, dim: string, text: string, primary: string, accent: string, error = '#ff8b8b'): SchemeRoles =>
  ({ background, surface, border, dim, text, primary, accent, error });

/** Nick's Necron-green Hermes look, frozen (the `necron` skin's island roles). */
export const GREEN: SchemeRoles = roles('#080d0a', '#121c15', '#3d6243', '#8a9c8d', '#d8e2d7', '#e7eee4', '#80ff36', '#ff7272');
const NEUTRAL: SchemeRoles = roles('#000000', '#111111', '#303034', '#a0a0a5', '#eeeeef', '#ffffff', '#479dff');

const HEX = /^#[0-9a-f]{6}$/i;
const rgb = (hex: string) => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16));
/** sRGB mix: `amount` of `b` into `a`, as #rrggbb. */
export function mixHex(a: string, b: string, amount: number): string {
  const x = rgb(a), y = rgb(b);
  return `#${x.map((v, i) => Math.round(v + (y[i]! - v) * amount).toString(16).padStart(2, '0')).join('')}`;
}
const luma = (hex: string) => { const [r, g, b] = rgb(hex); return (0.2126 * r! + 0.7152 * g! + 0.0722 * b!) / 255; };

/**
 * The Custom scheme from three seeds; every other role is derived so any pick stays
 * coherent. Empty/invalid seeds fall back to the green look.
 */
export function customPalette(seeds: CustomSeeds): SchemeRoles {
  const pick = (v: string | undefined, fallback: string) => v && HEX.test(v) ? v.toLowerCase() : fallback;
  const background = pick(seeds.customBackground, GREEN.background);
  const accent = pick(seeds.customAccent, GREEN.accent);
  const light = luma(background) > .55;
  // Default text: the green look's own text on its own background, otherwise a neutral
  // near-white (or near-black on light backgrounds) faintly tinted by the accent.
  const text = pick(seeds.customText, light ? mixHex('#16181a', accent, .06)
    : background === GREEN.background && accent === GREEN.accent ? GREEN.text : mixHex('#eceef1', accent, .06));
  return {
    background,
    surface: mixHex(mixHex(background, text, light ? .05 : .065), accent, .035),
    border: mixHex(mixHex(background, text, .16), accent, .14),
    dim: mixHex(text, background, .38),
    text,
    primary: mixHex(text, light ? '#000000' : '#ffffff', .35),
    accent,
    error: light ? '#c62828' : '#ff7272',
  };
}

export const SCHEMES: Scheme[] = [
  { id: 'skin', name: 'Hermes skin', note: 'follows your Hermes skin', dots: ['var(--skin-surface)', 'var(--skin-accent)', 'var(--skin-text)'], palette: null },
  { id: 'green', name: 'Green', note: 'accent #80ff36 · gauss green', dots: ['#1c3a20', '#80ff36', '#d8e2d7'], palette: GREEN },
  { id: 'neutral', name: 'Neutral black', note: 'accent #479dff', dots: ['#1c1c1e', '#479dff', '#ffffff'], palette: NEUTRAL },
  { id: 'ocean', name: 'Ocean', note: 'accent #4aa8ff · deep navy', dots: ['#123a6b', '#2f86ff', '#d8ecff'],
    palette: roles('#050d18', '#0c1a2c', '#1b3150', '#8299b4', '#e3eefb', '#bfe0ff', '#4aa8ff') },
  { id: 'dune', name: 'Dune', note: 'accent #f0a24a · warm sand', dots: ['#6b3410', '#f0883e', '#ffe2b8'],
    palette: roles('#120b06', '#20150c', '#3a2817', '#b09678', '#f6e8d6', '#ffd7a3', '#f0a24a') },
  { id: 'dusk', name: 'Dusk', note: 'accent #a77bff · violet hour', dots: ['#3a1f7a', '#b06bff', '#efe4ff'],
    palette: roles('#0c0918', '#18122c', '#2d2450', '#a097bd', '#eee8ff', '#dcc8ff', '#a77bff') },
  { id: 'silk', name: 'Silk', note: 'monochrome · soft white', dots: ['#2a2a2d', '#9a9aa0', '#ffffff'],
    palette: roles('#0b0b0c', '#171719', '#2b2b2f', '#9a9aa0', '#f2f2f3', '#ffffff', '#e9e9ec') },
  { id: 'aurora', name: 'Aurora', note: 'accent #3ddc97 · polar green', dots: ['#0b4a39', '#22c98a', '#c6ffe6'],
    palette: roles('#03110d', '#0a2019', '#153d31', '#84b2a0', '#e1f7ee', '#a8f5d2', '#3ddc97') },
  { id: 'ember', name: 'Ember', note: 'accent #ff6a3d · glowing coal', dots: ['#5a1408', '#ff5a2e', '#ffd2bf'],
    palette: roles('#130605', '#21100c', '#3d1d16', '#b58f86', '#f9e6e1', '#ffc9b8', '#ff6a3d') },
  { id: 'rose', name: 'Rose', note: 'accent #ff6fa8 · night bloom', dots: ['#5c1037', '#ff4f97', '#ffd6e8'],
    palette: roles('#13060d', '#221019', '#3f1d30', '#b98fa6', '#fbe6f0', '#ffcde2', '#ff6fa8') },
  { id: 'glacier', name: 'Glacier', note: 'accent #7fd8f0 · ice blue', dots: ['#164356', '#56c6e6', '#e6faff'],
    palette: roles('#061015', '#0d1d25', '#1b3643', '#88a9b5', '#e4f4f8', '#c2f0fb', '#7fd8f0') },
  { id: 'custom', name: 'Custom', note: 'your own colours', dots: ['#2a2a2d', '#9a9aa0', '#ffffff'], palette: null },
  // Nov 5: derived at runtime from the playing artwork (island/album-scheme.ts); Skin when none.
  { id: 'album', name: 'Album', note: 'from the playing artwork', dots: ['var(--skin-surface)', 'var(--skin-accent)', 'var(--skin-text)'], palette: null },
];

export const schemeById = (id: Appearance) => SCHEMES.find(s => s.id === id) ?? SCHEMES[0]!;

/** The palette a scheme imposes on every role, or null to follow the Hermes skin. */
export function schemePalette(id: Appearance, seeds: CustomSeeds): SchemeRoles | null {
  return id === 'custom' ? customPalette(seeds) : schemeById(id).palette;
}
