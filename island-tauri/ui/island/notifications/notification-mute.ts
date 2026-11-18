import { useEffect, useState } from 'react';
import type { NotificationAppRule, Settings, SettingsPatch } from '../../ipc/bindings';
import { muteApp, restoreAppRule } from '../../ipc/notification-rules';
import { saveSettings } from '../../stores';
import { changePopup } from '../../ipc/notifications';

type Save = (update: (current: Settings) => SettingsPatch) => Promise<void>;

/** Compute against the latest settings at write time; Undo only touches this app. */
export async function muteFromNotification(app: string, save: Save) {
  let previous: NotificationAppRule | undefined;
  await save(current => {
    previous = current.notificationAppRules.find(rule => rule.app === app);
    return muteApp(current.notificationAppRules, app);
  });
  return () => save(current => restoreAppRule(current.notificationAppRules, app, previous));
}

export function useNotificationMute(fail: (error: unknown) => void) {
  const [confirmation, setConfirmation] = useState<{ app: string; undo(): Promise<void> } | null>(null);
  useEffect(() => {
    if (!confirmation) return;
    const timer = setTimeout(() => setConfirmation(null), 6500);
    return () => clearTimeout(timer);
  }, [confirmation]);
  const mute = async (app: string, id: string) => {
    try {
      const undo = await muteFromNotification(app, saveSettings);
      setConfirmation({ app, undo });
      await changePopup({ type: 'dismiss', id });
    } catch (error) { fail(error); }
  };
  const undo = () => {
    if (!confirmation) return;
    void confirmation.undo().then(() => setConfirmation(value => value === confirmation ? null : value)).catch(fail);
  };
  return { mute, confirmation: confirmation?.app, undo };
}
