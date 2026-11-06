// Per-app notification rules editor model (pure). Mirrors Rust `check_app_rules`: exact,
// trimmed, unique, non-empty app names (≤256 bytes, no control chars), at most 128 rules.
import type { NotificationAppRule, RuleUrgency } from '../../ipc/bindings';
import { normalize } from '../settings-model';

export const MAX_APP_RULES = 128;
export const MAX_APP_BYTES = 256;
export const URGENCY_OPTIONS: { value: RuleUrgency; label: string }[] = [
  { value: 'app', label: 'As the app sends' }, { value: 'low', label: 'Low' },
  { value: 'normal', label: 'Normal' }, { value: 'critical', label: 'Critical' },
];

/** Why `app` can't be added (null = ok). */
export function appNameError(app: string, rules: readonly NotificationAppRule[]): string | null {
  const name = app.trim();
  if (!name) return 'Enter an app name';
  if (new TextEncoder().encode(name).length > MAX_APP_BYTES) return `At most ${MAX_APP_BYTES} bytes`;
  if (/\p{Cc}/u.test(name)) return 'No control characters';
  if (rules.some(r => r.app === name)) return `${name} already has a rule`;
  if (rules.length >= MAX_APP_RULES) return `At most ${MAX_APP_RULES} rules`;
  return null;
}
/** New rules start as "no change" (popup + sound on, the app's urgency). */
export function addRule(rules: readonly NotificationAppRule[], app: string): NotificationAppRule[] {
  return [...rules, { app: app.trim(), popup: true, sound: true, urgency: 'app' }];
}
export function updateRule(rules: readonly NotificationAppRule[], app: string, change: Partial<Omit<NotificationAppRule, 'app'>>): NotificationAppRule[] {
  return rules.map(r => r.app === app ? { ...r, ...change } : r);
}
export function removeRule(rules: readonly NotificationAppRule[], app: string): NotificationAppRule[] {
  return rules.filter(r => r.app !== app);
}
/**
 * Combobox suggestions: app names seen in the notification list (newest first) plus blocked
 * apps, without the ones that already have a rule, filtered by `query`, at most `limit`.
 */
export function appSuggestions(seen: readonly string[], rules: readonly NotificationAppRule[], query: string, limit = 8): string[] {
  const taken = new Set(rules.map(r => r.app)), words = normalize(query).split(/\s+/).filter(Boolean), out: string[] = [];
  for (const raw of seen) {
    const app = raw.trim();
    if (!app || taken.has(app) || out.includes(app) || /\p{Cc}/u.test(app)) continue;
    if (!words.every(w => normalize(app).includes(w))) continue;
    out.push(app);
    if (out.length >= limit) break;
  }
  return out;
}
/** Short summary for the row hint. */
export function rulesSummary(rules: readonly NotificationAppRule[]): string {
  if (!rules.length) return 'Popup, sound and urgency per app';
  const quiet = rules.filter(r => !r.popup).length, muted = rules.filter(r => !r.sound).length;
  const parts = [`${rules.length} app${rules.length === 1 ? '' : 's'}`];
  if (quiet) parts.push(`${quiet} without popups`);
  if (muted) parts.push(`${muted} muted`);
  return parts.join(' · ');
}
