// Terminal appearance settings (Nov 5) → xterm options. Applied to the LIVE xterm through its
// option setters: recreating the Terminal would drop the PTY view (TerminalView.tsx).
import type { TerminalCursorStyle } from '../ipc/bindings';

export const DEFAULT_TERMINAL_FONT = '"DejaVu Sans Mono", monospace';
export type TerminalLook = { fontFamily: string; scrollback: number; cursorStyle: TerminalCursorStyle; cursorBlink: boolean };

/** `terminalFontFamily` ('' = DejaVu Sans Mono) as an xterm/CSS family list with a monospace fallback. */
export function terminalFontFamily(value: string): string {
  const family = value.trim();
  // Rust already refuses quotes and CSS syntax characters; this is defence in depth.
  if (!family || /["';{}<>\\]/.test(family)) return DEFAULT_TERMINAL_FONT;
  return /(^|,)\s*monospace\s*$/i.test(family) ? family : `${family}, monospace`;
}

export function terminalOptions(look: TerminalLook) {
  return {
    fontFamily: terminalFontFamily(look.fontFamily),
    scrollback: Math.round(Math.min(100000, Math.max(500, Number.isFinite(look.scrollback) ? look.scrollback : 5000))),
    cursorStyle: look.cursorStyle === 'bar' || look.cursorStyle === 'underline' ? look.cursorStyle : 'block' as TerminalCursorStyle,
    cursorBlink: look.cursorBlink !== false,
  };
}

/** Set only the options that changed (each xterm setter may reflow or re-measure). Returns whether the cell size can change. */
export function applyTerminalOptions(options: Record<string, unknown>, look: TerminalLook): boolean {
  const next = terminalOptions(look);
  let remeasure = false;
  for (const [key, value] of Object.entries(next)) {
    if (options[key] === value) continue;
    options[key] = value;
    if (key === 'fontFamily') remeasure = true;
  }
  return remeasure;
}
