// Colour scheme presets (Nov 2; community themes Nov 6). Rust persists only the choice (`Appearance`) and the
// Custom seed colours (`customAccent`/`customBackground`/`customText` settings);
// the palettes live here so the island, its CSS variables, xterm and the optional
// Hermes theme sync all follow one source.
import type { Appearance, Palette } from '../../ipc/client';

export type SchemeRoles = Omit<Palette, 'name'>;
/**
 * `family` groups variants into one picker row (UI only; the Rust `Appearance` enum stays
 * flat, one id per variant). `ansi16` is the theme's official terminal palette (black, red,
 * green, yellow, blue, magenta, cyan, white, then the bright eight); schemes without it keep
 * xterm's built-in ANSI colours.
 */
export type Scheme = { id: Appearance; family: string; name: string; note: string; dots: [string, string, string];
  palette: SchemeRoles | null; ansi16?: readonly string[] };
export type Family = { id: string; name: string; group: 'monolith' | 'community'; schemes: Scheme[] };
/** Display names of multi-variant or renamed families; a single-variant family uses its scheme's name. */
const FAMILY_NAMES: Record<string, string> = { monolith: 'Monolith', catppuccin: 'Catppuccin', gruvbox: 'Gruvbox', tokyonight: 'Tokyo Night', rosepine: 'Rosé Pine', solarized: 'Solarized' };
const COMMUNITY = new Set(['catppuccin', 'dracula', 'nord', 'gruvbox', 'tokyonight', 'rosepine', 'onedark', 'solarized']);
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
  { id: 'skin', family: 'skin', name: 'Hermes skin', note: 'follows your Hermes skin', dots: ['var(--skin-surface)', 'var(--skin-accent)', 'var(--skin-text)'], palette: null },
  { id: 'green', family: 'monolith', name: 'Green', note: 'accent #80ff36 · gauss green', dots: ['#1c3a20', '#80ff36', '#d8e2d7'], palette: GREEN },
  { id: 'neutral', family: 'monolith', name: 'Neutral black', note: 'accent #479dff', dots: ['#1c1c1e', '#479dff', '#ffffff'], palette: NEUTRAL },
  { id: 'ocean', family: 'monolith', name: 'Ocean', note: 'accent #4aa8ff · deep navy', dots: ['#123a6b', '#2f86ff', '#d8ecff'],
    palette: roles('#050d18', '#0c1a2c', '#1b3150', '#8299b4', '#e3eefb', '#bfe0ff', '#4aa8ff') },
  { id: 'dune', family: 'monolith', name: 'Dune', note: 'accent #f0a24a · warm sand', dots: ['#6b3410', '#f0883e', '#ffe2b8'],
    palette: roles('#120b06', '#20150c', '#3a2817', '#b09678', '#f6e8d6', '#ffd7a3', '#f0a24a') },
  { id: 'dusk', family: 'monolith', name: 'Dusk', note: 'accent #a77bff · violet hour', dots: ['#3a1f7a', '#b06bff', '#efe4ff'],
    palette: roles('#0c0918', '#18122c', '#2d2450', '#a097bd', '#eee8ff', '#dcc8ff', '#a77bff') },
  { id: 'silk', family: 'monolith', name: 'Silk', note: 'monochrome · soft white', dots: ['#2a2a2d', '#9a9aa0', '#ffffff'],
    palette: roles('#0b0b0c', '#171719', '#2b2b2f', '#9a9aa0', '#f2f2f3', '#ffffff', '#e9e9ec') },
  { id: 'aurora', family: 'monolith', name: 'Aurora', note: 'accent #3ddc97 · polar green', dots: ['#0b4a39', '#22c98a', '#c6ffe6'],
    palette: roles('#03110d', '#0a2019', '#153d31', '#84b2a0', '#e1f7ee', '#a8f5d2', '#3ddc97') },
  { id: 'ember', family: 'monolith', name: 'Ember', note: 'accent #ff6a3d · glowing coal', dots: ['#5a1408', '#ff5a2e', '#ffd2bf'],
    palette: roles('#130605', '#21100c', '#3d1d16', '#b58f86', '#f9e6e1', '#ffc9b8', '#ff6a3d') },
  { id: 'rose', family: 'monolith', name: 'Rose', note: 'accent #ff6fa8 · night bloom', dots: ['#5c1037', '#ff4f97', '#ffd6e8'],
    palette: roles('#13060d', '#221019', '#3f1d30', '#b98fa6', '#fbe6f0', '#ffcde2', '#ff6fa8') },
  { id: 'glacier', family: 'monolith', name: 'Glacier', note: 'accent #7fd8f0 · ice blue', dots: ['#164356', '#56c6e6', '#e6faff'],
    palette: roles('#061015', '#0d1d25', '#1b3643', '#88a9b5', '#e4f4f8', '#c2f0fb', '#7fd8f0') },
  { id: 'custom', family: 'custom', name: 'Custom', note: 'your own colours', dots: ['#2a2a2d', '#9a9aa0', '#ffffff'], palette: null },
  // Nov 5: derived at runtime from the playing artwork (island/album-scheme.ts); Skin when none.
  { id: 'album', family: 'album', name: 'Album', note: 'from the playing artwork', dots: ['var(--skin-surface)', 'var(--skin-accent)', 'var(--skin-text)'], palette: null },
  // ---- Community schemes (Nov 6). Every hex is copied from the project's own palette
  //      source below (fetched, not from memory); role mapping: background = base,
  //      surface/border = the theme's surface/overlay steps, accent = its signature colour,
  //      error = its red. Light schemes also need a `[data-theme=…]` block in themes.css.
  // Catppuccin palette v1.8.0 — https://github.com/catppuccin/palette/blob/main/palette.json (colors + ansiColors) · MIT
  { id: 'catppuccin-mocha', family: 'catppuccin', name: 'Mocha', note: 'accent #cba6f7 · mauve', dots: ['#313244', '#cba6f7', '#cdd6f4'],
    palette: roles('#1e1e2e', '#313244', '#45475a', '#a6adc8', '#cdd6f4', '#b4befe', '#cba6f7', '#f38ba8'),
    ansi16: ['#45475a', '#f38ba8', '#a6e3a1', '#f9e2af', '#89b4fa', '#f5c2e7', '#94e2d5', '#a6adc8', '#585b70', '#f37799', '#89d88b', '#ebd391', '#74a8fc', '#f2aede', '#6bd7ca', '#bac2de'] },
  { id: 'catppuccin-macchiato', family: 'catppuccin', name: 'Macchiato', note: 'accent #c6a0f6 · mauve', dots: ['#363a4f', '#c6a0f6', '#cad3f5'],
    palette: roles('#24273a', '#363a4f', '#494d64', '#a5adcb', '#cad3f5', '#b7bdf8', '#c6a0f6', '#ed8796'),
    ansi16: ['#494d64', '#ed8796', '#a6da95', '#eed49f', '#8aadf4', '#f5bde6', '#8bd5ca', '#a5adcb', '#5b6078', '#ec7486', '#8ccf7f', '#e1c682', '#78a1f6', '#f2a9dd', '#63cbc0', '#b8c0e0'] },
  { id: 'catppuccin-frappe', family: 'catppuccin', name: 'Frappé', note: 'accent #ca9ee6 · mauve', dots: ['#414559', '#ca9ee6', '#c6d0f5'],
    palette: roles('#303446', '#414559', '#51576d', '#a5adce', '#c6d0f5', '#babbf1', '#ca9ee6', '#e78284'),
    ansi16: ['#51576d', '#e78284', '#a6d189', '#e5c890', '#8caaee', '#f4b8e4', '#81c8be', '#a5adce', '#626880', '#e67172', '#8ec772', '#d9ba73', '#7b9ef0', '#f2a4db', '#5abfb5', '#b5bfe2'] },
  { id: 'catppuccin-latte', family: 'catppuccin', name: 'Latte', note: 'accent #8839ef · mauve · light', dots: ['#ccd0da', '#8839ef', '#4c4f69'],
    palette: roles('#eff1f5', '#ccd0da', '#bcc0cc', '#6c6f85', '#4c4f69', '#1e66f5', '#8839ef', '#d20f39'),
    ansi16: ['#5c5f77', '#d20f39', '#40a02b', '#df8e1d', '#1e66f5', '#ea76cb', '#179299', '#acb0be', '#6c6f85', '#de293e', '#49af3d', '#eea02d', '#456eff', '#fe85d8', '#2d9fa8', '#bcc0cc'] },
  // Dracula spec — https://spec.draculatheme.com (Standard + ANSI) · MIT (github.com/dracula/dracula-theme)
  { id: 'dracula', family: 'dracula', name: 'Dracula', note: 'accent #bd93f9 · purple', dots: ['#21222c', '#bd93f9', '#f8f8f2'],
    palette: roles('#282a36', '#21222c', '#44475a', '#6272a4', '#f8f8f2', '#ff79c6', '#bd93f9', '#ff5555'),
    ansi16: ['#21222c', '#ff5555', '#50fa7b', '#f1fa8c', '#bd93f9', '#ff79c6', '#8be9fd', '#f8f8f2', '#6272a4', '#ff6e6e', '#69ff94', '#ffffa5', '#d6acff', '#ff92df', '#a4ffff', '#ffffff'] },
  // Nord — https://www.nordtheme.com/docs/colors-and-palettes; ANSI + dim from the official ports github.com/nordtheme/konsole src/nord.colorscheme and nordtheme/alacritty src/nord.yaml · MIT
  { id: 'nord', family: 'nord', name: 'Nord', note: 'accent #88c0d0 · frost', dots: ['#3b4252', '#88c0d0', '#d8dee9'],
    palette: roles('#2e3440', '#3b4252', '#4c566a', '#a5abb6', '#d8dee9', '#eceff4', '#88c0d0', '#bf616a'),
    ansi16: ['#3b4252', '#bf616a', '#a3be8c', '#ebcb8b', '#81a1c1', '#b48ead', '#88c0d0', '#e5e9f0', '#4c566a', '#bf616a', '#a3be8c', '#ebcb8b', '#81a1c1', '#b48ead', '#8fbcbb', '#eceff4'] },
  // Gruvbox (dark, medium contrast) — https://github.com/morhetz/gruvbox colors/gruvbox.vim (palette + g:terminal_color_*) · MIT/X11 (README)
  { id: 'gruvbox-dark', family: 'gruvbox', name: 'Dark', note: 'accent #fabd2f · yellow', dots: ['#3c3836', '#fabd2f', '#ebdbb2'],
    palette: roles('#282828', '#3c3836', '#504945', '#a89984', '#ebdbb2', '#fbf1c7', '#fabd2f', '#fb4934'),
    ansi16: ['#282828', '#cc241d', '#98971a', '#d79921', '#458588', '#b16286', '#689d6a', '#a89984', '#928374', '#fb4934', '#b8bb26', '#fabd2f', '#83a598', '#d3869b', '#8ec07c', '#ebdbb2'] },
  // Gruvbox light (medium contrast): the same file with &background=light (light0/light1/light2 as base/surface/border, dark1 text, faded_* accents; terminal_color_* mapping as in dark).
  { id: 'gruvbox-light', family: 'gruvbox', name: 'Light', note: 'accent #b57614 · yellow · light', dots: ['#ebdbb2', '#b57614', '#3c3836'],
    palette: roles('#fbf1c7', '#ebdbb2', '#d5c4a1', '#7c6f64', '#3c3836', '#282828', '#b57614', '#9d0006'),
    ansi16: ['#fbf1c7', '#cc241d', '#98971a', '#d79921', '#458588', '#b16286', '#689d6a', '#7c6f64', '#928374', '#9d0006', '#79740e', '#b57614', '#076678', '#8f3f71', '#427b58', '#3c3836'] },
  // Tokyo Night (night, storm) — https://github.com/folke/tokyonight.nvim lua/tokyonight/colors/{storm,night}.lua + extras/kitty/tokyonight_{night,storm}.conf (ANSI) · Apache-2.0
  { id: 'tokyo-night', family: 'tokyonight', name: 'Night', note: 'accent #7aa2f7 · blue', dots: ['#292e42', '#7aa2f7', '#c0caf5'],
    palette: roles('#1a1b26', '#292e42', '#3b4261', '#737aa2', '#c0caf5', '#7dcfff', '#7aa2f7', '#f7768e'),
    ansi16: ['#15161e', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#a9b1d6', '#414868', '#ff899d', '#9fe044', '#faba4a', '#8db0ff', '#c7a9ff', '#a4daff', '#c0caf5'] },
  { id: 'tokyo-storm', family: 'tokyonight', name: 'Storm', note: 'accent #7aa2f7 · blue', dots: ['#292e42', '#7aa2f7', '#c0caf5'],
    palette: roles('#24283b', '#292e42', '#3b4261', '#737aa2', '#c0caf5', '#7dcfff', '#7aa2f7', '#f7768e'),
    ansi16: ['#1d202f', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#a9b1d6', '#414868', '#ff899d', '#9fe044', '#faba4a', '#8db0ff', '#c7a9ff', '#a4daff', '#c0caf5'] },
  // Rosé Pine (main, moon, dawn) — https://rosepinetheme.com/palette (= github.com/rose-pine/palette palette.json); borders = its highlight med (high on dawn); ANSI from the official github.com/rose-pine/alacritty dist/rose-pine{,-moon,-dawn}.toml (that port's dawn white is the older #575279; the palette's dawn text is #464261) · MIT
  { id: 'rose-pine', family: 'rosepine', name: 'Main', note: 'accent #ebbcba · rose', dots: ['#1f1d2e', '#ebbcba', '#e0def4'],
    palette: roles('#191724', '#1f1d2e', '#403d52', '#908caa', '#e0def4', '#c4a7e7', '#ebbcba', '#eb6f92'),
    ansi16: ['#26233a', '#eb6f92', '#31748f', '#f6c177', '#9ccfd8', '#c4a7e7', '#ebbcba', '#e0def4', '#6e6a86', '#eb6f92', '#31748f', '#f6c177', '#9ccfd8', '#c4a7e7', '#ebbcba', '#e0def4'] },
  { id: 'rose-pine-moon', family: 'rosepine', name: 'Moon', note: 'accent #ea9a97 · rose', dots: ['#2a273f', '#ea9a97', '#e0def4'],
    palette: roles('#232136', '#2a273f', '#44415a', '#908caa', '#e0def4', '#c4a7e7', '#ea9a97', '#eb6f92'),
    ansi16: ['#393552', '#eb6f92', '#3e8fb0', '#f6c177', '#9ccfd8', '#c4a7e7', '#ea9a97', '#e0def4', '#6e6a86', '#eb6f92', '#3e8fb0', '#f6c177', '#9ccfd8', '#c4a7e7', '#ea9a97', '#e0def4'] },
  { id: 'rose-pine-dawn', family: 'rosepine', name: 'Dawn', note: 'accent #b4637a · love · light', dots: ['#f2e9e1', '#b4637a', '#464261'],
    palette: roles('#faf4ed', '#f2e9e1', '#cecacd', '#797593', '#464261', '#286983', '#b4637a', '#b4637a'),
    ansi16: ['#f2e9e1', '#b4637a', '#286983', '#ea9d34', '#56949f', '#907aa9', '#d7827e', '#575279', '#9893a5', '#b4637a', '#286983', '#ea9d34', '#56949f', '#907aa9', '#d7827e', '#575279'] },
  // One Dark — https://github.com/joshdick/onedark.vim autoload/onedark.vim + colors/onedark.vim (terminal_ansi_colors) · MIT; dim = @mono-2 hsl(220, 9%, 55%) from atom/atom packages/one-dark-syntax/styles/colors.less · MIT
  { id: 'one-dark', family: 'onedark', name: 'One Dark', note: 'accent #61afef · blue', dots: ['#2c323c', '#61afef', '#abb2bf'],
    palette: roles('#282c34', '#2c323c', '#3e4452', '#828997', '#abb2bf', '#c678dd', '#61afef', '#e06c75'),
    ansi16: ['#282c34', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#abb2bf', '#3e4452', '#be5046', '#98c379', '#d19a66', '#61afef', '#c678dd', '#56b6c2', '#5c6370'] },
  // Solarized — https://ethanschoonover.com/solarized (github.com/altercation/solarized README 'The Values', 16-colour mapping from its TERMCOL column) · MIT
  { id: 'solarized-dark', family: 'solarized', name: 'Dark', note: 'accent #268bd2 · blue', dots: ['#073642', '#268bd2', '#93a1a1'],
    palette: roles('#002b36', '#073642', '#586e75', '#839496', '#93a1a1', '#eee8d5', '#268bd2', '#dc322f'),
    ansi16: ['#073642', '#dc322f', '#859900', '#b58900', '#268bd2', '#d33682', '#2aa198', '#eee8d5', '#002b36', '#cb4b16', '#586e75', '#657b83', '#839496', '#6c71c4', '#93a1a1', '#fdf6e3'] },
  // Solarized light: same accents and the same 16-colour mapping (Solarized's ANSI palette is identical in both modes); base3 background, base2 surface; text base02 / primary base03 / dim base01, because the canonical body text base00 is only 4.1:1 on base3 (base01 4.4:1 on base2) and this theme must stay readable (≥ 4.5).
  { id: 'solarized-light', family: 'solarized', name: 'Light', note: 'accent #268bd2 · blue · light', dots: ['#eee8d5', '#268bd2', '#073642'],
    palette: roles('#fdf6e3', '#eee8d5', '#93a1a1', '#586e75', '#073642', '#002b36', '#268bd2', '#dc322f'),
    ansi16: ['#073642', '#dc322f', '#859900', '#b58900', '#268bd2', '#d33682', '#2aa198', '#eee8d5', '#002b36', '#cb4b16', '#586e75', '#657b83', '#839496', '#6c71c4', '#93a1a1', '#fdf6e3'] },
];

export const schemeById = (id: Appearance) => SCHEMES.find(s => s.id === id) ?? SCHEMES[0]!;

/** Picker rows: one per family, in first-appearance order, variants in SCHEMES order. */
export function schemeFamilies(schemes: readonly Scheme[] = SCHEMES): Family[] {
  const out = new Map<string, Family>();
  for (const s of schemes) {
    let f = out.get(s.family);
    if (!f) out.set(s.family, f = { id: s.family, name: FAMILY_NAMES[s.family] ?? s.name, group: COMMUNITY.has(s.family) ? 'community' : 'monolith', schemes: [] });
    f.schemes.push(s);
  }
  return [...out.values()];
}
/** The family the picker opens on: the active scheme's, if it has variants to show. */
export function initialOpenFamily(value: Appearance, families: readonly Family[] = schemeFamilies()): string | null {
  const f = families.find(f => f.schemes.some(s => s.id === value));
  return f && f.schemes.length > 1 ? f.id : null;
}
/** True when a palette's background is light: the island then swaps its white overlays for ink ones (tokens/light-tone.css). */
export const isLightPalette = (roles: Pick<SchemeRoles, 'background'>) => HEX.test(roles.background) && luma(roles.background) > .55;

/** The palette a scheme imposes on every role, or null to follow the Hermes skin. */
export function schemePalette(id: Appearance, seeds: CustomSeeds): SchemeRoles | null {
  return id === 'custom' ? customPalette(seeds) : schemeById(id).palette;
}

/**
 * Arrow-key focus movement over the visible picker rows (family headers + scheme radios).
 * Returns the index to focus, or null when the key is not a navigation key.
 */
export function navTarget(key: string, index: number, count: number): number | null {
  if (count === 0) return null;
  if (key === 'ArrowDown') return Math.min(count - 1, index + 1);
  if (key === 'ArrowUp') return Math.max(0, index - 1);
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  return null;
}
