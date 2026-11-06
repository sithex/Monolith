// Key press → KGlobalAccel / QKeySequence portable text (`Meta+\``, `Ctrl+Alt+Space`),
// exactly the spellings `crate::keyseq::parse` accepts (src-tauri/src/keyseq.rs). Pure.
//
// KDE conventions kept: modifier order Meta, Ctrl, Alt, Shift; named keys `Space`, `Return`,
// `Esc`, `PgUp`…; a shifted symbol is written as the symbol without Shift (`Alt+~`, `Meta+!`,
// as in kglobalshortcutsrc), while Shift stays for letters, digits-free named keys and F-keys.

export type KeyLike = {
  key: string; code?: string;
  metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean;
  /** Some engines report Super as `OS`; `getModifierState('OS'|'Super'|'Hyper')` folds those in. */
  getModifierState?(name: string): boolean;
};
export type Chord =
  | { kind: 'modifier'; held: string }        // only modifiers so far: keep waiting, show `held`
  | { kind: 'cancel' }                        // Esc alone: stop recording, keep the old value
  | { kind: 'clear' }                         // Backspace/Delete alone: unbind ('')
  | { kind: 'invalid'; reason: string; held: string }
  | { kind: 'chord'; text: string };

const MODIFIER_KEYS = new Set(['Meta', 'OS', 'Super', 'Hyper', 'Control', 'Alt', 'AltGraph', 'Shift', 'CapsLock', 'NumLock', 'ScrollLock', 'Fn', 'FnLock']);

/** DOM `key` names → the names keyseq.rs knows. */
const NAMED: Record<string, string> = {
  ' ': 'Space', Spacebar: 'Space', Enter: 'Return', Tab: 'Tab', Escape: 'Esc', Esc: 'Esc', Backspace: 'Backspace',
  Delete: 'Del', Insert: 'Ins', Home: 'Home', End: 'End', PageUp: 'PgUp', PageDown: 'PgDown',
  ArrowLeft: 'Left', ArrowUp: 'Up', ArrowRight: 'Right', ArrowDown: 'Down', Left: 'Left', Up: 'Up', Right: 'Right', Down: 'Down',
  Pause: 'Pause', PrintScreen: 'Print', ContextMenu: 'Menu', Apps: 'Menu',
};
/** Keys that may be bound without a modifier (anything else alone would eat normal typing). */
const LONE_OK = new Set(['Print', 'Pause', 'Menu', 'Ins']);

/** Unshifted US-layout character per physical key: fallback when `key` is dead/unidentified. */
const CODE_CHAR: Record<string, string> = {
  Backquote: '`', Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\',
  Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Slash: '/', Space: ' ',
};

const sup = (e: KeyLike) => e.metaKey || !!e.getModifierState?.('OS') || !!e.getModifierState?.('Super') || !!e.getModifierState?.('Hyper');
function mods(e: KeyLike, shift: boolean): string[] {
  const out: string[] = [];
  if (sup(e)) out.push('Meta');
  if (e.ctrlKey) out.push('Ctrl');
  if (e.altKey) out.push('Alt');
  if (shift) out.push('Shift');
  return out;
}
const join = (m: string[], key: string) => [...m, key].join('+');

/** Modifiers currently held, as a live preview (`Meta+Ctrl+…`); '' when none. */
export function heldText(e: KeyLike): string {
  const m = mods(e, e.shiftKey);
  return m.length ? `${m.join('+')}+…` : '';
}

/** The base key name for keyseq.rs, or null when it cannot be written. */
function keyName(e: KeyLike): string | null {
  if (NAMED[e.key]) return NAMED[e.key]!;
  const f = /^F([1-9]|[12]\d|3[0-5])$/.exec(e.key);
  if (f) return e.key;
  let ch = e.key;
  if (ch.length !== 1 || !/^[\x20-\x7e]$/.test(ch)) {
    // Dead keys (`´`, `^`), unknown keys and non-Latin layouts (`ф`): fall back to the
    // physical key's US character, as KDE binds to the Latin layout's keysym.
    const code = e.code ?? '';
    if (/^Key[A-Z]$/.test(code)) ch = code.slice(3);
    else if (/^(Digit|Numpad)\d$/.test(code)) ch = code.slice(-1);
    else if (CODE_CHAR[code]) ch = CODE_CHAR[code]!;
    else return null;
  }
  if (ch === ' ') return 'Space';
  // keyseq.rs accepts one printable ASCII character (Qt keeps punctuation's ASCII code).
  if (!/^[\x21-\x7e]$/.test(ch)) return null;
  return /[a-z]/.test(ch) ? ch.toUpperCase() : ch;
}

/** Classify one keydown while recording. */
export function chordFromKey(e: KeyLike): Chord {
  if (MODIFIER_KEYS.has(e.key)) return { kind: 'modifier', held: heldText(e) };
  const anyMod = sup(e) || e.ctrlKey || e.altKey || e.shiftKey;
  if (!anyMod && (e.key === 'Escape' || e.key === 'Esc')) return { kind: 'cancel' };
  if (!anyMod && (e.key === 'Backspace' || e.key === 'Delete')) return { kind: 'clear' };
  const name = keyName(e);
  const held = heldText(e);
  if (name === null) return { kind: 'invalid', reason: 'That key can\'t be used in a KDE shortcut', held };
  if (name === ',') return { kind: 'invalid', reason: 'Comma can\'t be bound (KDE uses it to separate shortcuts)', held };
  // A printable symbol that Shift produced is written without Shift (KDE: `Alt+~`, `Meta+!`).
  const symbol = name.length === 1 && !/[A-Z0-9]/.test(name);
  const shift = e.shiftKey && !symbol;
  const m = mods(e, shift);
  const plain = !sup(e) && !e.ctrlKey && !e.altKey;
  if (plain && !/^F\d+$/.test(name) && !LONE_OK.has(name)) {
    return { kind: 'invalid', reason: 'Add Meta, Ctrl or Alt (only F-keys, Print and Pause work alone)', held };
  }
  return { kind: 'chord', text: join(m, name) };
}
