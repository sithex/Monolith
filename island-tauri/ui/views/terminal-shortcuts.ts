import type { Terminal } from '@xterm/xterm';

type Clipboard = {
  selection(): string;
  copy(text: string): Promise<unknown>;
  fail(error: unknown): void;
  /** True while the Hermes TUI itself (mouse-tracking mode) holds a drag selection. */
  appSelection?(): boolean;
  /** Called once the app selection has been handed to the TUI for copying. */
  appCopied?(): void;
};

// Copy xterm's selection explicitly: WebKit's native Copy accelerator is not
// reliable in the overlay. Keep native paste for xterm's bracketed-paste handling.
// When the TUI owns the mouse, xterm's selection is empty and the highlighted text lives
// in the TUI; Ctrl+C then forwards ETX so the TUI copies it (it answers via OSC 52).
export function terminalShortcut(event: KeyboardEvent, input: (data: string) => void, clipboard?: Clipboard): boolean {
  if (!event.ctrlKey || event.altKey || event.metaKey) return true;
  const key = event.key.toLowerCase();
  if (event.shiftKey && key === 'c') {
    event.preventDefault();
    if (event.type === 'keydown' && !event.repeat) { input('\x03'); clipboard?.appCopied?.(); }
    return false;
  }
  if (!event.shiftKey && key === 'c') {
    event.preventDefault();
    if (event.type === 'keydown' && !event.repeat && clipboard) {
      const text = clipboard.selection();
      if (text) void clipboard.copy(text).catch(clipboard.fail);
      else if (clipboard.appSelection?.()) { input('\x03'); clipboard.appCopied?.(); }
    }
    return false;
  }
  if (!event.shiftKey && key === 'v') return false;
  return true;
}

/** Decode an OSC 52 payload (`<targets>;<base64>`). Queries and malformed data yield null. */
export function decodeOsc52(data: string): string | null {
  const split = data.indexOf(';');
  if (split < 0) return null;
  const targets = data.slice(0, split), payload = data.slice(split + 1).trim();
  if (!/^[cpqs0-7]*$/.test(targets) || !payload || payload === '?') return null;
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(payload)) return null;
  try {
    const bytes = Uint8Array.from(atob(payload), ch => ch.charCodeAt(0));
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch { return null; }
}

/**
 * The Hermes TUI copies its own (mouse) selection by emitting OSC 52, which kitty
 * honours and xterm.js ignores by default. Write those requests to the system clipboard.
 * Clipboard read queries are swallowed unanswered so programs cannot read the clipboard.
 */
export function keepTerminalClipboard(terminal: Pick<Terminal, 'parser'>, copy: (text: string) => Promise<unknown>, fail: (error: unknown) => void) {
  return terminal.parser.registerOscHandler(52, data => {
    const text = decodeOsc52(data);
    if (text) void copy(text).catch(fail);
    return true;
  });
}

/**
 * Tracks whether the user has drag-selected text inside the TUI while it captures the
 * mouse. A click, a keystroke or a copy clears it, mirroring the TUI's own selection.
 */
export function trackAppSelection(terminal: Pick<Terminal, 'modes'>, element: HTMLElement) {
  let armed = false, start: { x: number; y: number } | null = null;
  const tracking = () => terminal.modes.mouseTrackingMode !== 'none';
  const down = (event: MouseEvent) => { armed = false; start = event.button === 0 && !event.shiftKey ? { x: event.clientX, y: event.clientY } : null; };
  const up = (event: MouseEvent) => {
    if (start && tracking() && Math.hypot(event.clientX - start.x, event.clientY - start.y) >= 4) armed = true;
    start = null;
  };
  const double = () => { if (tracking()) armed = true; };
  const key = (event: KeyboardEvent) => {
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(event.key)) return;
    if (event.ctrlKey && event.key.toLowerCase() === 'c') return;
    armed = false;
  };
  element.addEventListener('mousedown', down, true);
  window.addEventListener('mouseup', up, true);
  element.addEventListener('dblclick', double, true);
  element.addEventListener('keydown', key, true);
  return {
    active: () => armed && tracking(),
    clear: () => { armed = false; },
    dispose: () => {
      element.removeEventListener('mousedown', down, true);
      window.removeEventListener('mouseup', up, true);
      element.removeEventListener('dblclick', double, true);
      element.removeEventListener('keydown', key, true);
    },
  };
}
