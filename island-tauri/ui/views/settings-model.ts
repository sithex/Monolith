// Settings page model (Nov 5): every row once, with the setting keys it edits. Search,
// per-card Reset and visibility all read this table, so a row's label, the keys a Reset
// restores and the rows a search can find never drift apart. Pure: no React, no IPC.
import type { Appearance, Settings, SettingsPatch } from '../ipc/client';

export type Tab = 'general' | 'appearance' | 'layout';
export type SettingKey = keyof Settings;
export type RowSpec = {
  id: string; label: string; hint?: string; keys: readonly SettingKey[];
  /** Shown only when this holds (e.g. glass strength needs translucency). */
  when?(settings: Partial<Settings>): boolean;
};
export type CardSpec = {
  id: string; title: string; tab: Tab; rows: readonly RowSpec[];
  /** false: the card's own controls own its state (no Reset link). */
  reset?: boolean;
  /** Reset also returns the colour scheme to the default (`skin`). */
  resetsAppearance?: boolean;
};

export const DEFAULT_APPEARANCE: Appearance = 'skin';
const on = (key: SettingKey) => (s: Partial<Settings>) => s[key] !== false;

export const SETTINGS_CARDS: readonly CardSpec[] = [
  { id: 'behavior', title: 'Behavior', tab: 'general', rows: [
    { id: 'swipeNavigation', label: 'Swipe between views', hint: 'Horizontal drag or two-finger swipe changes app', keys: ['swipeNavigation'] },
    { id: 'stealth', label: 'Auto-hide when idle', keys: ['stealth'] },
    { id: 'clickSounds', label: 'Click sounds', keys: ['clickSounds'] },
  ] },
  { id: 'shortcuts', title: 'Shortcuts', tab: 'general', rows: [
    { id: 'openShortcut', label: 'Open shortcut', hint: 'KDE global shortcut that opens the island; not set = none', keys: ['openShortcut'] },
    { id: 'paletteShortcut', label: 'Command palette shortcut', hint: 'KDE global shortcut that opens the command palette; not set = none', keys: ['paletteShortcut'] },
  ] },
  { id: 'gameMode', title: 'Game mode', tab: 'general', rows: [
    { id: 'gameModeAuto', label: 'Game mode automatically', hint: 'Quiets the island while a fullscreen game has focus', keys: ['gameModeAuto'] },
    { id: 'gameMode', label: 'Game mode now', hint: 'Quiet the island until you turn it off', keys: ['gameMode'] },
    { id: 'gameModeDnd', label: 'Do Not Disturb while gaming', hint: 'Notifications wait quietly in history during game mode', keys: ['gameModeDnd'] },
    { id: 'gameModeFreezeCompanion', label: 'Freeze the companion', hint: 'The notch companion stops animating during game mode', keys: ['gameModeFreezeCompanion'] },
    { id: 'gameModePausePollers', label: 'Pause background updates', hint: 'Weather, calendar, stats and update checks wait until game mode ends', keys: ['gameModePausePollers'] },
  ] },
  { id: 'terminalCommand', title: 'Terminal launch', tab: 'general', reset: false, rows: [
    { id: 'terminalCommand', label: 'Hermes launch command', hint: 'Command the terminal runs (hermes --cli, --tui)', keys: ['terminalCommand'] },
  ] },
  { id: 'notifications', title: 'Notifications', tab: 'general', rows: [
    { id: 'notificationsEnabled', label: 'Hermes notifications', hint: 'Agent and system alerts on the island', keys: ['notificationsEnabled'] },
    { id: 'dnd', label: 'Do Not Disturb', hint: 'Keep arrivals quietly, no toast', keys: ['dnd'] },
    { id: 'systemNotificationMirror', label: 'Mirror desktop notifications', hint: 'Show other apps\' notifications too', keys: ['systemNotificationMirror'] },
    { id: 'silenceKdePopups', label: 'Silence KDE popups', hint: 'Monolith shows them; KDE keeps history', keys: ['silenceKdePopups'] },
    { id: 'notificationSound', label: 'Notification sound', hint: 'Play a sound for new notifications (per-app rules can mute)', keys: ['notificationSound'] },
    { id: 'notificationSoundId', label: 'Sound', hint: 'Which sound plays; per-app rules can pick their own', keys: ['notificationSoundId'] },
    { id: 'approvalsInNotch', label: 'Approvals in the notch', hint: 'Approve or deny Hermes requests from the notch hover', keys: ['approvalsInNotch'] },
    { id: 'redactBodies', label: 'Hide message text', hint: 'Only titles are kept', keys: ['redactBodies'] },
    { id: 'deniedApps', label: 'Blocked apps', hint: 'Apps that never notify', keys: ['notificationDeniedApps'] },
    { id: 'appRules', label: 'Per-app rules', hint: 'Popup, sound and urgency for each app', keys: ['notificationAppRules'] },
  ] },
  { id: 'clock', title: 'Clock', tab: 'general', rows: [
    { id: 'showDate', label: 'Show date under time', keys: ['showDate'] },
  ] },
  { id: 'display', title: 'Display', tab: 'general', rows: [
    { id: 'displayBackend', label: 'Display server', hint: 'Wayland or X11 (XWayland); applies when the island restarts', keys: ['displayBackend'] },
  ] },
  { id: 'apps', title: 'Apps', tab: 'general', rows: [
    { id: 'hardwareMonitor', label: 'Hardware monitor', keys: ['hardwareMonitor'] },
    { id: 'arrangeApps', label: 'Arrange or hide apps', hint: 'Hold any app in Apps, then drag it', keys: [] },
  ] },
  { id: 'backup', title: 'Backup', tab: 'general', reset: false, rows: [
    { id: 'exportSettings', label: 'Export settings', hint: 'Copies your settings as JSON to the clipboard', keys: [] },
    { id: 'importSettings', label: 'Import settings', hint: 'Paste exported JSON; applied all at once or not at all', keys: [] },
  ] },
  { id: 'scheme', title: 'Colour scheme', tab: 'appearance', resetsAppearance: true, rows: [
    { id: 'scheme', label: 'Colour scheme', hint: 'Hermes skin, presets, album artwork or custom colours', keys: ['customAccent', 'customBackground', 'customText'] },
    { id: 'hermesThemeSync', label: 'Use for Hermes too', hint: 'Applies this scheme as your Hermes skin; off restores your previous skin', keys: ['hermesThemeSync'] },
    { id: 'overrides', label: 'Colour overrides', hint: 'Accent and background come from the scheme unless overridden', keys: ['accent', 'background', 'borderColor'] },
  ] },
  { id: 'surface', title: 'Surface', tab: 'appearance', rows: [
    { id: 'translucent', label: 'Translucent background', hint: 'Frosted glass over the desktop (KWin blur)', keys: ['translucent'] },
    { id: 'glassStrength', label: 'Glass strength', hint: 'How much of the desktop shows through', keys: ['glassStrength'], when: on('translucent') },
    { id: 'borderEnabled', label: 'Outline', keys: ['borderEnabled'] },
  ] },
  { id: 'motion', title: 'Motion', tab: 'appearance', rows: [
    { id: 'reduceMotion', label: 'Reduce motion', hint: 'Short fades instead of springs and morphs (also follows the system setting)', keys: ['reduceMotion'] },
    { id: 'animationSpeed', label: 'Animation speed', hint: 'Faster or slower island animations', keys: ['animationSpeed'], when: s => s.reduceMotion !== true },
  ] },
  { id: 'notch', title: 'Notch', tab: 'appearance', rows: [
    { id: 'everyMonitor', label: 'Show notch on every monitor', hint: 'Displays without a top panel get a small notch panel', keys: ['showNotchOnEveryMonitor'] },
    { id: 'notchMonitors', label: 'Monitors', hint: 'Which displays get a notch (none ticked = all)', keys: ['notchMonitors'], when: s => s.showNotchOnEveryMonitor === true },
    { id: 'waitingPulse', label: 'Pulse while waiting', hint: 'The notch breathes while Hermes waits for you', keys: ['notchWaitingPulse'] },
    { id: 'hoverPeek', label: 'Hover peek', hint: 'Hovering the notch previews the island', keys: ['notchHoverPeek'] },
    { id: 'companion', label: 'Companion', hint: 'A small face in the notch whose eyes follow your pointer', keys: ['companionEnabled'] },
    { id: 'companionCharacter', label: 'Character', hint: 'Companion character', keys: ['companionCharacter'], when: on('companionEnabled') },
    { id: 'companionReactive', label: 'Reactive companion', hint: 'The companion reacts to Hermes working, waiting and finishing', keys: ['companionReactive'], when: on('companionEnabled') },
    { id: 'companionMonitor', label: 'Companion monitor', hint: 'Which notch shows the companion', keys: ['companionMonitor'], when: on('companionEnabled') },
    { id: 'notchColor', label: 'Notch color', keys: ['panelClockColor'] },
    { id: 'notchWidth', label: 'Width', hint: 'Notch width', keys: ['notchWidth'] },
    { id: 'notchBorder', label: 'Notch border', keys: ['notchBorderEnabled'] },
    { id: 'notchBorderColor', label: 'Notch border color', keys: ['notchBorderColor'] },
    { id: 'notchBorderOpacity', label: 'Border opacity', keys: ['notchBorderOpacity'] },
    { id: 'notchBorderWidth', label: 'Border thickness', keys: ['notchBorderWidth'] },
  ] },
  { id: 'text', title: 'Text', tab: 'appearance', rows: [
    { id: 'uiFont', label: 'UI font', hint: 'System or OpenRunde', keys: ['uiFont'] },
    { id: 'terminalFontSize', label: 'Terminal font size', keys: ['terminalFontSize'] },
    { id: 'terminalFontFamily', label: 'Terminal font', hint: 'Installed fonts; Default = DejaVu Sans Mono', keys: ['terminalFontFamily'] },
    { id: 'terminalScrollback', label: 'Terminal scrollback', hint: 'Lines kept above the screen', keys: ['terminalScrollback'] },
    { id: 'terminalCursorStyle', label: 'Terminal cursor', hint: 'Block, bar or underline', keys: ['terminalCursorStyle'] },
    { id: 'terminalCursorBlink', label: 'Blinking cursor', keys: ['terminalCursorBlink'] },
  ] },
  { id: 'layoutEditor', title: 'Layout', tab: 'layout', reset: false, rows: [
    { id: 'sideButtons', label: 'Side buttons', hint: 'Buttons around the open island', keys: [] },
    { id: 'islandSize', label: 'Island size', hint: 'Width and maximum height', keys: [] },
  ] },
  { id: 'position', title: 'Position', tab: 'layout', rows: [
    { id: 'offsetX', label: 'Horizontal offset', keys: ['offsetX'] },
    { id: 'offsetY', label: 'Vertical offset', keys: ['offsetY'] },
  ] },
];

const ROWS = new Map(SETTINGS_CARDS.flatMap(card => card.rows.map(row => [row.id, row] as const)));
export const rowSpec = (id: string): RowSpec => {
  const row = ROWS.get(id);
  if (!row) throw new Error(`Unknown settings row ${id}`);
  return row;
};
export const cardSpec = (id: string): CardSpec => {
  const card = SETTINGS_CARDS.find(c => c.id === id);
  if (!card) throw new Error(`Unknown settings card ${id}`);
  return card;
};

/** Every setting key a card's Reset restores (union of its rows' keys, schema order kept). */
export function cardKeys(cardId: string): SettingKey[] {
  return [...new Set(cardSpec(cardId).rows.flatMap(row => row.keys))];
}
/** The patch that resets one card to the Rust defaults (`settings_defaults`). */
export function resetPatch(cardId: string, defaults: Settings): SettingsPatch {
  const patch: Record<string, unknown> = {};
  for (const key of cardKeys(cardId)) patch[key] = defaults[key];
  return patch as SettingsPatch;
}

const visible = (row: RowSpec, settings: Partial<Settings>) => row.when ? row.when(settings) : true;
export const normalize = (text: string) => text.toLowerCase().normalize('NFKD').replace(/\p{M}/gu, '').trim();

/**
 * Rows matching `query` across all tabs: every whitespace-separated word must occur in the
 * row's label + hint (case- and accent-insensitive). Rows hidden by `when` never match.
 * Grouped by card, in page order; empty for a blank query.
 */
export function searchRows(query: string, settings: Partial<Settings>, cards: readonly CardSpec[] = SETTINGS_CARDS): { card: CardSpec; rows: RowSpec[] }[] {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  return cards.map(card => ({ card, rows: card.rows.filter(row => {
    const text = normalize(`${row.label} ${row.hint ?? ''}`);
    return visible(row, settings) && words.every(word => text.includes(word));
  }) })).filter(group => group.rows.length > 0);
}

/** Row ids to render: the tab's visible rows, or the search hits across all tabs. */
export function shownRows(tab: Tab, query: string, settings: Partial<Settings>): Set<string> {
  if (normalize(query)) return new Set(searchRows(query, settings).flatMap(group => group.rows.map(row => row.id)));
  return new Set(SETTINGS_CARDS.filter(card => card.tab === tab).flatMap(card => card.rows.filter(row => visible(row, settings)).map(row => row.id)));
}

// ── Shortcuts ────────────────────────────────────────────────────────────────────────────
// Mirrors Rust `check_shortcut` → `crate::keyseq::parse` (src-tauri/src/keyseq.rs).
const MODIFIERS = new Set(['meta', 'super', 'win', 'ctrl', 'control', 'alt', 'shift']);
const NAMED_KEYS = new Set(['esc', 'escape', 'tab', 'backspace', 'return', 'enter', 'ins', 'insert', 'del', 'delete', 'pause', 'print',
  'home', 'end', 'left', 'up', 'right', 'down', 'pgup', 'pageup', 'pgdown', 'pagedown', 'space', 'menu']);
/** keyseq.rs `key_code`: a named key, F1–F35, or one printable ASCII character. */
export const isShortcutKey = (key: string) => NAMED_KEYS.has(key.toLowerCase()) || /^f([1-9]|[12]\d|3[0-5])$/i.test(key) || /^[\x21-\x7e]$/.test(key);
/** KDE shortcut syntax check (`Meta+\``, `Ctrl+Alt+Space`); null when valid. Empty = no shortcut. */
export function shortcutError(value: string): string | null {
  const text = value.trim();
  if (!text) return null;
  if (text.length > 64) return 'At most 64 characters';
  if (/[\p{Cc},]/u.test(text) || !/^[\x20-\x7e]*$/.test(text)) return 'No commas, control or non-ASCII characters';
  // A trailing "++" means the key itself is "+".
  const parts = text.endsWith('++') ? [...text.slice(0, -2).split('+'), '+'] : text.split('+');
  if (parts.some(p => p.trim() === '')) return 'Use Modifier+Key, e.g. Meta+` or Ctrl+Alt+Space';
  const key = parts[parts.length - 1]!.trim(), mods = parts.slice(0, -1).map(p => p.trim().toLowerCase());
  const bad = mods.find(m => !MODIFIERS.has(m));
  if (bad) return `Unknown modifier "${bad}" (use Meta, Ctrl, Alt or Shift)`;
  const bits = mods.map(m => m === 'super' || m === 'win' ? 'meta' : m === 'control' ? 'ctrl' : m);
  if (new Set(bits).size !== bits.length) return 'A modifier is repeated';
  if (MODIFIERS.has(key.toLowerCase())) return 'End with a key, not a modifier';
  if (!isShortcutKey(key)) return `Unknown key "${key}"`;
  if (!mods.length && key.length === 1) return 'Add a modifier, e.g. Meta+' + key;
  return null;
}

/** Same characters Rust refuses in `terminalFontFamily`; null when valid. */
export function fontFamilyError(value: string): string | null {
  if (value.length > 128) return 'At most 128 characters';
  return /["';{}<>\\\p{Cc}]/u.test(value) ? 'Quotes and ; { } < > \\ are not allowed' : null;
}

// ── Notch monitors ───────────────────────────────────────────────────────────────────────
/**
 * Toggle one output in `notchMonitors` (empty = every monitor). Unticking from "all" keeps
 * the other connected outputs; ticking the last missing one collapses back to [] (all).
 */
export function toggleMonitor(current: readonly string[], outputs: readonly string[], name: string, checked: boolean): string[] {
  const base = current.length ? [...current] : [...outputs];
  const next = checked ? (base.includes(name) ? base : [...base, name]) : base.filter(n => n !== name);
  if (outputs.length && outputs.every(o => next.includes(o)) && next.every(n => outputs.includes(n))) return [];
  return next;
}
/** Checklist rows: connected outputs plus saved names that are not connected now. */
export function monitorChoices(current: readonly string[], outputs: readonly string[]): { name: string; checked: boolean; connected: boolean }[] {
  const names = [...outputs, ...current.filter(n => !outputs.includes(n))];
  return names.map(name => ({ name, checked: current.length === 0 || current.includes(name), connected: outputs.includes(name) }));
}

// ── Export / import ──────────────────────────────────────────────────────────────────────
/** Machine-local or app-owned keys that never travel in an export (moduleRegistry = installed apps + their config). */
export const EXPORT_EXCLUDED: readonly SettingKey[] = ['moduleRegistry'];
export const EXPORT_FORMAT = 'monolith-settings';

export function exportSettings(settings: Settings, appearance: Appearance): string {
  const copy: Record<string, unknown> = { ...settings };
  for (const key of EXPORT_EXCLUDED) delete copy[key];
  return JSON.stringify({ format: EXPORT_FORMAT, version: 1, appearance, settings: copy }, null, 2);
}

/**
 * Parse exported JSON (or a bare settings object) into one `settings_patch` plus the scheme.
 * Shape errors are reported here; value validation is Rust's (the patch is atomic there).
 */
export function parseImport(text: string, appearances: readonly string[]): { patch: SettingsPatch; appearance: Appearance | null } {
  let value: unknown;
  try { value = JSON.parse(text); } catch { throw new Error('Not valid JSON'); }
  const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
  if (!isObject(value)) throw new Error('Expected a JSON object');
  const wrapped = value.format === EXPORT_FORMAT || 'settings' in value;
  const settings = wrapped ? value.settings : value;
  if (!isObject(settings)) throw new Error('"settings" must be an object');
  const patch: Record<string, unknown> = { ...settings };
  for (const key of EXPORT_EXCLUDED) delete patch[key];
  if (!Object.keys(patch).length) throw new Error('No settings to import');
  let appearance: Appearance | null = null;
  if (wrapped && value.appearance !== undefined) {
    if (typeof value.appearance !== 'string' || !appearances.includes(value.appearance)) throw new Error(`Unknown colour scheme ${JSON.stringify(value.appearance)}`);
    appearance = value.appearance as Appearance;
  }
  return { patch: patch as SettingsPatch, appearance };
}
