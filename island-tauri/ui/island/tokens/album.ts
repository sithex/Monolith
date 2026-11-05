// Album scheme (Nov 5): accent + background tint taken from the playing track's artwork.
// Pure colour maths only; the sampling (one 24x24 canvas downsample per artwork change)
// lives in ../album-scheme.ts. No artwork, or artwork with no usable colour, means the
// caller falls back to the Hermes skin palette.
import { customPalette, mixHex, type SchemeRoles } from './schemes';

/** Edge length of the downsampled artwork (24x24 = 576 pixels). */
export const ALBUM_SAMPLE = 24;
/** WCAG non-text contrast the accent must keep against the island surface. */
export const ALBUM_MIN_CONTRAST = 3;
/** Crossfade between two tracks' colours (CSS @property transition). */
export const ALBUM_FADE_MS = 600;

const HEX = /^#[0-9a-f]{6}$/i;
const channel = (v: number) => Math.max(0, Math.min(255, Math.round(v)));
export const rgbHex = (r: number, g: number, b: number) => `#${[r, g, b].map(v => channel(v).toString(16).padStart(2, '0')).join('')}`;
const rgb = (hex: string): [number, number, number] => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];

/** WCAG 2 relative luminance of `#rrggbb`. */
export function relativeLuminance(hex: string): number {
  const [r, g, b] = rgb(hex).map(v => { const c = v / 255; return c <= .03928 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4; }) as [number, number, number];
  return .2126 * r + .7152 * g + .0722 * b;
}
/** WCAG 2 contrast ratio (1..21). */
export function contrastRatio(a: string, b: string): number {
  const x = relativeLuminance(a), y = relativeLuminance(b);
  return (Math.max(x, y) + .05) / (Math.min(x, y) + .05);
}

export function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), l = (max + min) / 2, d = max - min;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s, l];
}
export function hslToHex(h: number, s: number, l: number): string {
  const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs((h / 60) % 2 - 1)), m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return rgbHex((r + m) * 255, (g + m) * 255, (b + m) * 255);
}

/**
 * The vivid dominant colour of RGBA pixel data (a 24x24 downsample): pixels are binned by
 * hue (12 bins of 30°), each weighted by saturation and by how far it is from black/white,
 * so a small saturated area beats a large grey one. Returns the mean colour of the heaviest
 * bin, or null when the artwork is effectively greyscale/blank (caller falls back to Skin).
 */
export function pickAlbumColour(data: ArrayLike<number>): string | null {
  const bins = Array.from({ length: 12 }, () => ({ w: 0, r: 0, g: 0, b: 0 }));
  let total = 0, pixels = 0;
  for (let i = 0; i + 3 < data.length; i += 4) {
    if (data[i + 3]! < 128) continue;
    pixels++;
    const r = data[i]!, g = data[i + 1]!, b = data[i + 2]!;
    const [h, s, l] = rgbToHsl(r, g, b);
    if (s < .18 || l < .08 || l > .94) continue;
    const w = s * s * (1 - Math.abs(l - .5) * 1.6);
    if (w <= 0) continue;
    const bin = bins[Math.floor(h / 30) % 12]!;
    bin.w += w; bin.r += r * w; bin.g += g * w; bin.b += b * w; total += w;
  }
  if (!pixels || total < pixels * .004) return null;
  const best = bins.reduce((a, b) => b.w > a.w ? b : a);
  return rgbHex(best.r / best.w, best.g / best.w, best.b / best.w);
}

/**
 * `accent` with its HSL lightness moved (away from `surface`) until the WCAG contrast is at
 * least `min`; saturation is lifted to stay "vivid". Already-compliant colours are returned unchanged.
 */
export function ensureContrast(accent: string, surface: string, min = ALBUM_MIN_CONTRAST): string {
  if (!HEX.test(accent) || !HEX.test(surface)) return accent;
  if (contrastRatio(accent, surface) >= min) return accent.toLowerCase();
  const [h, s0, l0] = rgbToHsl(...rgb(accent));
  const s = Math.max(s0, .55), lighter = relativeLuminance(surface) < .4;
  for (let step = 1; step <= 50; step++) {
    const l = Math.max(0, Math.min(1, l0 + (lighter ? 1 : -1) * step * .02));
    const hex = hslToHex(h, s, l);
    if (contrastRatio(hex, surface) >= min) return hex;
    if (l === 0 || l === 1) break;
  }
  return lighter ? '#ffffff' : '#000000';
}

/** Full island palette for an artwork accent: dark background faintly tinted by the art. */
export function albumPalette(art: string): SchemeRoles {
  const seed = HEX.test(art) ? art.toLowerCase() : '#888888';
  const background = mixHex('#08090a', seed, .1);
  const base = customPalette({ customBackground: background, customAccent: seed });
  const tuned = customPalette({ customBackground: background, customAccent: ensureContrast(seed, base.surface) });
  // The surface carries a 3.5% accent tint, so re-check against the final surface.
  return { ...tuned, accent: ensureContrast(tuned.accent, tuned.surface) };
}
