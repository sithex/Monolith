// Font picker model (pure): filtering, the "Default" entry and the rendered window.
import type { FontFamily } from '../../ipc/bindings';
import { normalize } from '../settings-model';

export const DEFAULT_FONT_LABEL = 'Default (DejaVu Sans Mono)';
export const FONT_ROW_PX = 32;
/** Rows rendered beyond the visible ones on each side (keeps fast scrolling filled). */
export const FONT_OVERSCAN = 4;
/** Hard cap on rendered rows, whatever the viewport (each row loads its own font). */
export const FONT_MAX_RENDERED = 50;

export type FontEntry = { value: string; label: string; monospace: boolean };

/**
 * The picker's entries: the Default entry ('' = DejaVu Sans Mono) first, then families
 * matching every query word (case/accent-insensitive), monospace only when asked. The saved
 * value stays listed even if it is not installed or not monospace, so the current choice is
 * always visible.
 */
export function fontEntries(fonts: readonly FontFamily[], query: string, monoOnly: boolean, current: string): FontEntry[] {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  const matches = (name: string) => words.every(word => normalize(name).includes(word));
  const out: FontEntry[] = [];
  if (matches(DEFAULT_FONT_LABEL)) out.push({ value: '', label: DEFAULT_FONT_LABEL, monospace: true });
  const known = fonts.some(f => f.family === current);
  if (current && !known && matches(current)) out.push({ value: current, label: current, monospace: true });
  for (const font of fonts) {
    if (monoOnly && !font.monospace && font.family !== current) continue;
    if (matches(font.family)) out.push({ value: font.family, label: font.family, monospace: font.monospace });
  }
  return out;
}

/** Index range [start, end) of rows to render for a scroll position (virtualized list). */
export function fontWindow(scrollTop: number, viewport: number, total: number): { start: number; end: number } {
  const first = Math.max(0, Math.floor(scrollTop / FONT_ROW_PX) - FONT_OVERSCAN);
  const count = Math.min(FONT_MAX_RENDERED, Math.ceil(viewport / FONT_ROW_PX) + 2 * FONT_OVERSCAN);
  const end = Math.min(total, first + count);
  return { start: Math.max(0, Math.min(first, end - count)), end };
}

/** scrollTop that brings row `index` into view (minimal movement), for keyboard navigation. */
export function scrollToRow(index: number, scrollTop: number, viewport: number): number {
  const top = index * FONT_ROW_PX, bottom = top + FONT_ROW_PX;
  if (top < scrollTop) return top;
  if (bottom > scrollTop + viewport) return bottom - viewport;
  return scrollTop;
}

/** CSS family for a preview row: the family itself (already CSS-safe from Rust), then a generic fallback. */
export const previewFamily = (entry: FontEntry) => entry.value ? `${entry.value}, ${entry.monospace ? 'monospace' : 'sans-serif'}` : '"DejaVu Sans Mono", monospace';
