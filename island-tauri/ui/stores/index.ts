import { useSyncExternalStore } from 'react';
import { createTopic } from './topic';
import { bootstrap, visualGet, onVisualChanged, settingsPatch, appearanceSet,
  type Settings, type SettingsPatch, type Palette, type Appearance, type VisualChanged,
  type BootstrapSnapshot, type SystemStats, type AiUsage } from '../ipc/client';
import { listenIslandState, seedIslandState } from '../ipc/island-state';
import { listenNotifications, seedNotifications } from '../ipc/notifications';
import { listenExtended, loadWorkspace, seedExtended } from './apps';

// One useSyncExternalStore store per Phase 7a topic. Actor and notifications reuse
// the accepted stores, also used by Toast; there is no second IslandState authority.
export const settingsStore = createTopic<Settings>();
export const paletteStore = createTopic<Palette>();
export const appearanceStore = createTopic<Appearance>();
export const monitorStore = createTopic<SystemStats | null>();
export const aiStore = createTopic<AiUsage | null>();
export const runtimeStore = createTopic<BootstrapSnapshot['runtime']>();
export const acceptVisual = (snapshot: VisualChanged) => {
  settingsStore.apply(snapshot.settings, snapshot.rev);
  paletteStore.apply(snapshot.palette, snapshot.rev);
  appearanceStore.apply(snapshot.appearance, snapshot.rev);
};
export const useSettings = () => useSyncExternalStore(settingsStore.subscribe, settingsStore.getSnapshot);
export const usePalette = () => useSyncExternalStore(paletteStore.subscribe, paletteStore.getSnapshot);
export const useAppearance = () => useSyncExternalStore(appearanceStore.subscribe, appearanceStore.getSnapshot);
export const useMonitor = () => useSyncExternalStore(monitorStore.subscribe, monitorStore.getSnapshot);
export const useAi = () => useSyncExternalStore(aiStore.subscribe, aiStore.getSnapshot);
export async function loadStores() {
  await listenIslandState();
  await listenNotifications();
  await listenExtended();
  const off = await onVisualChanged(acceptVisual);
  try { return { boot: await bootstrap(), visual: await visualGet(), workspace: await loadWorkspace() }; }
  catch (error) { off(); throw error; }
}
export function seedStores({ boot, visual, workspace }: Awaited<ReturnType<typeof loadStores>>) {
  seedExtended(boot, workspace);
  seedIslandState(boot.state);
  seedNotifications(boot.notifications);
  settingsStore.seed(visual.settings, visual.rev);
  paletteStore.seed(visual.palette, visual.rev);
  appearanceStore.seed(visual.appearance, visual.rev);
  runtimeStore.seed(boot.runtime, 0);
  monitorStore.seed(null, 0);
  aiStore.seed(null, 0);
}
// No optimistic appearance/settings state: successful command + authoritative revision.
let settingsTail: Promise<unknown> = Promise.resolve();
export function saveSettings(patch: SettingsPatch): Promise<void> {
  const result = settingsTail.catch(() => undefined).then(async () => {
    await settingsPatch(patch);
    acceptVisual(await visualGet());
  });
  settingsTail = result;
  return result;
}
export async function saveAppearance(appearance: Appearance) {
  acceptVisual(await appearanceSet(appearance));
}
