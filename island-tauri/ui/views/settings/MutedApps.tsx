import type { NotificationAppRule, SettingsPatch } from '../../ipc/bindings';
import { isMutedApp, unmuteApp } from '../../ipc/notification-rules';

export function MutedApps({ rules, onSave }: {
  rules: readonly NotificationAppRule[];
  onSave(update: (current: readonly NotificationAppRule[]) => SettingsPatch): void;
}) {
  const muted = rules.filter(isMutedApp);
  if (muted.length === 0) return <small className="muted">No muted apps</small>;
  return <ul className="app-rule-list" aria-label="Muted apps">
    {muted.map(({ app }) => <li key={app} className="app-rule">
      <span className="app-rule-name" title={app}>{app}</span>
      <button type="button" className="link" aria-label={`Unmute ${app}`} onClick={() => onSave(current => unmuteApp(current, app))}>Unmute</button>
    </li>)}
  </ul>;
}
