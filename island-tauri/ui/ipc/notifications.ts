// Subscribe before bootstrap; the pure core bounds replay, active rows and tombstones.
import { useSyncExternalStore } from "react";
import { onNotificationReceived, onNotificationClosed, onNotificationSnapshot, type IslandNotification } from "./client";
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
