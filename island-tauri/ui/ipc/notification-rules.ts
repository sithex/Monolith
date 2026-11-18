import type { NotificationAppRule, SettingsPatch } from './bindings';

export const isMutedApp = (rule: NotificationAppRule) => !rule.popup && !rule.sound;

export function restoreAppRule(rules: readonly NotificationAppRule[], app: string, previous?: NotificationAppRule): SettingsPatch {
  return { notificationAppRules: previous
    ? rules.some(r => r.app === app) ? rules.map(r => r.app === app ? previous : r) : [...rules, previous]
    : rules.filter(r => r.app !== app) };
}

export function muteApp(rules: readonly NotificationAppRule[], app: string): SettingsPatch {
  const previous = rules.find(r => r.app === app);
  return restoreAppRule(rules, app, { urgency: 'app', soundId: '', ...previous, app, popup: false, sound: false });
}

export function unmuteApp(rules: readonly NotificationAppRule[], app: string): SettingsPatch {
  const rule = rules.find(r => r.app === app);
  if (!rule || !isMutedApp(rule)) return {};
  return restoreAppRule(rules, app, rule.urgency === 'app' && !rule.soundId ? undefined : { ...rule, popup: true, sound: true });
}
