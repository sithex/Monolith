// Subscribe before bootstrap; the pure core bounds replay, active rows and tombstones.
import { useSyncExternalStore } from "react";
import { onNotificationReceived, onNotificationClosed, onNotificationSnapshot, onNotificationPopups, notificationPopup, type NotificationPopups, type PopupOp, type IslandNotification } from "./client";
import { createNotificationStore } from "./notifications-core";

const store = createNotificationStore();
let listening: Promise<void> | null = null;
export function listenNotifications(): Promise<void> {
  if (listening) return listening;
  listening = (async () => {
    // Close first: a received event must never race a not-yet-registered close listener.
    const offSnapshot = await onNotificationSnapshot((snapshot) => store.reconcile(snapshot));
    const offClosed = await onNotificationClosed((closed) => store.apply({ closed }));
    try {
      await onNotificationReceived((received) => store.apply({ received }));
    } catch (error) {
      offClosed();
      offSnapshot();
      throw error;
    }
  })().catch((error: unknown) => { listening = null; throw error; });
  return listening;
}
export function seedNotifications(initial: IslandNotification[]) {
  store.seed(initial);
  const { droppedBootstrap } = store.diagnostics();
  if (droppedBootstrap) console.warn(`Notification bootstrap overflow: ${droppedBootstrap} deltas dropped (stale content suppressed)`);
}
export function useNotifications() {
  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}

let popups: NotificationPopups = { rev: 0, items: [] };
const popupListeners = new Set<() => void>();
let popupListening: Promise<void> | null = null;
function acceptPopups(next: NotificationPopups) {
  if (((next.rev - popups.rev) | 0) < 0) return;
  popups = next; popupListeners.forEach(fn => fn());
}
export function listenPopups() {
  return popupListening ??= (async () => {
    await onNotificationPopups(acceptPopups);
    acceptPopups(await notificationPopup({ type: 'list' }));
  })().catch(error => { popupListening = null; throw error; });
}
// Serialise hover enter/leave so a late enter reply cannot leave a card paused forever.
let popupOperations = Promise.resolve();
export function changePopup(op: PopupOp): Promise<void> {
  const next = popupOperations.then(async () => { acceptPopups(await notificationPopup(op)); });
  popupOperations = next.catch(() => {});
  return next;
}
export function usePopups() {
  return useSyncExternalStore(fn => { popupListeners.add(fn); return () => popupListeners.delete(fn); }, () => popups);
}
