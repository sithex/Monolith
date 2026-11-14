import { useSyncExternalStore } from 'react';
import { createEpochTopic } from './epoch-topic';
import { createTopic } from './topic';
import { createClipboardHistory } from './clipboard-history';
import { parseMedia, parseLyrics, parseEqualizer } from '../views/app-adapters';
import { bootstrap, launcherGet, layoutGet, onLauncherChanged, onLayoutChanged, onAppTopics, onSystemTopics,
  type AppsSnapshot, type BootstrapSnapshot, type Settings, type LauncherChanged, type LayoutChanged,
  clipboardList, onClipboardAdded, onClipboardStatus,
  type MediaChanged, type LyricsChanged, type EqualizerChanged, type WeatherState, type PowerState, type BtDevice, type ServiceStatus } from '../ipc/client';
export const clipboardHistory = createClipboardHistory({ list: clipboardList, status: async () => (await bootstrap()).capabilities.clipboardStatus, added: onClipboardAdded, changed: onClipboardStatus });
export const useClipboardHistory = () => useSyncExternalStore(clipboardHistory.subscribe, clipboardHistory.getSnapshot);
const clipboardKey = (s: Settings) => JSON.stringify([s.clipboardHistory, s.clipboardHistorySize, s.clipboardPersist, s.moduleRegistry?.modules.find(m => m.id === 'clipboard')]);
export const appStores = {
  timer: createEpochTopic<NonNullable<AppsSnapshot['timer']>>(), scratchpad: createEpochTopic<NonNullable<AppsSnapshot['scratchpad']>>(),
  downloads: createEpochTopic<NonNullable<AppsSnapshot['downloads']>>(), calendar: createEpochTopic<NonNullable<AppsSnapshot['calendar']>>(),
  brightness: createEpochTopic<NonNullable<AppsSnapshot['brightness']>>(), agents: createEpochTopic<NonNullable<AppsSnapshot['agents']>>(),
  mixer: createEpochTopic<NonNullable<AppsSnapshot['mixer']>>(), toggles: createEpochTopic<NonNullable<AppsSnapshot['toggles']>>(),
  updates: createEpochTopic<NonNullable<AppsSnapshot['updates']>>(), tiles: createEpochTopic<NonNullable<AppsSnapshot['tiles']>>(),
  transfers: createEpochTopic<NonNullable<AppsSnapshot['transfers']>>(),
};
export const launcherStore = createTopic<LauncherChanged>();
export const layoutStore = createTopic<LayoutChanged>();
export const mediaStore = createEpochTopic<MediaChanged>(), lyricsStore = createEpochTopic<LyricsChanged>(), equalizerStore = createEpochTopic<EqualizerChanged>();
export const weatherStore = createEpochTopic<WeatherState>(), powerStore = createEpochTopic<PowerState>();
export const bluetoothStore = createEpochTopic<{ rev: number; devices: BtDevice[] }>();
const statuses = new Map<string, ReturnType<typeof createEpochTopic<ServiceStatus>>>();
export function serviceStatus(name: string) { return statuses.get(name)?.getSnapshot() ?? null; }
export const useApp = <K extends keyof AppsSnapshot>(key: K) => useSyncExternalStore<AppsSnapshot[keyof AppsSnapshot]>(appStores[key].subscribe, appStores[key].getSnapshot) as AppsSnapshot[K];
export const useLauncher = () => useSyncExternalStore(launcherStore.subscribe, launcherStore.getSnapshot);
export const useLayout = () => useSyncExternalStore(layoutStore.subscribe, layoutStore.getSnapshot);
export const useMedia = () => useSyncExternalStore(mediaStore.subscribe, mediaStore.getSnapshot);
export const useLyrics = () => useSyncExternalStore(lyricsStore.subscribe, lyricsStore.getSnapshot);
export const useEqualizer = () => useSyncExternalStore(equalizerStore.subscribe, equalizerStore.getSnapshot);
export const useWeather = () => useSyncExternalStore(weatherStore.subscribe, weatherStore.getSnapshot);
export const usePower = () => useSyncExternalStore(powerStore.subscribe, powerStore.getSnapshot);
export const useBluetooth = () => useSyncExternalStore(bluetoothStore.subscribe, bluetoothStore.getSnapshot);
let offApps: (() => void) | null = null, offSystem: (() => void) | null = null, signature = '', lifecycle = 0;
function validated<T extends { rev: number }>(store: ReturnType<typeof createEpochTopic<T>>, parse: (raw: unknown) => T, raw: unknown, epoch = store.epoch()) {
  try { store.apply(parse(raw), epoch); } catch { console.error('Rejected malformed media snapshot'); }
}
async function subscribeSystem() {
  const m = mediaStore.epoch(), l = lyricsStore.epoch(), e = equalizerStore.epoch(), w = weatherStore.epoch(), p = powerStore.epoch(), b = bluetoothStore.epoch(), generation = lifecycle;
  return onSystemTopics({ media: v => { validated(mediaStore,parseMedia,v,m); }, lyrics: v => { validated(lyricsStore,parseLyrics,v,l); }, equalizer: v => { validated(equalizerStore,parseEqualizer,v,e); },
    weather: v => { weatherStore.apply(v,w); }, power: v => { powerStore.apply(v,p); }, bluetooth: v => { bluetoothStore.apply(v,b); },
    status: v => { if (generation !== lifecycle) return; if (!statuses.has(v.service)) statuses.set(v.service, createEpochTopic<ServiceStatus>()); statuses.get(v.service)?.apply(v); } });
}
async function subscribeApps() {
  const g = Object.fromEntries(Object.entries(appStores).map(([k, s]) => [k, s.epoch()]));
  return onAppTopics({ timer: v => { appStores.timer.apply(v, g.timer); }, scratchpad: v => { appStores.scratchpad.apply(v, g.scratchpad); },
    downloads: v => { appStores.downloads.apply(v, g.downloads); }, calendar: v => { appStores.calendar.apply(v, g.calendar); },
    brightness: v => { appStores.brightness.apply(v, g.brightness); }, agents: v => { appStores.agents.apply(v, g.agents); },
    mixer: v => { appStores.mixer.apply(v, g.mixer); }, toggles: v => { appStores.toggles.apply(v, g.toggles); },
    updates: v => { appStores.updates.apply(v, g.updates); }, tiles: v => { appStores.tiles.apply(v, g.tiles); },
    transfers: v => { appStores.transfers.apply(v, g.transfers); } });
}
export async function listenExtended() {
  offApps = await subscribeApps();
  await onLauncherChanged(v => launcherStore.apply(v, v.rev));
  await onLayoutChanged(v => layoutStore.apply(v, v.rev));
  offSystem = await subscribeSystem();
}
export const loadWorkspace = async () => ({ launcher: await launcherGet(), layout: await layoutGet() });
function seedApps(boot: BootstrapSnapshot) {
  if (boot.apps.timer) appStores.timer.apply(boot.apps.timer);
  if (boot.apps.scratchpad) appStores.scratchpad.apply(boot.apps.scratchpad);
  if (boot.apps.downloads) appStores.downloads.apply(boot.apps.downloads);
  if (boot.apps.calendar) appStores.calendar.apply(boot.apps.calendar);
  if (boot.apps.brightness) appStores.brightness.apply(boot.apps.brightness);
  if (boot.apps.agents) appStores.agents.apply(boot.apps.agents);
  if (boot.apps.mixer) appStores.mixer.apply(boot.apps.mixer);
  if (boot.apps.toggles) appStores.toggles.apply(boot.apps.toggles);
  if (boot.apps.tiles) appStores.tiles.apply(boot.apps.tiles);
  if (boot.apps.updates) appStores.updates.apply(boot.apps.updates);
  if (boot.apps.transfers) appStores.transfers.apply(boot.apps.transfers);
}
export function seedExtended(boot: BootstrapSnapshot, workspace: Awaited<ReturnType<typeof loadWorkspace>>) {
  signature = JSON.stringify(boot.settings.moduleRegistry);
  launcherStore.seed(workspace.launcher, workspace.launcher.rev); layoutStore.seed(workspace.layout, workspace.layout.rev);
  seedApps(boot); seedSystem(boot);
  void clipboardHistory.configure(boot.settings.clipboardHistory, clipboardKey(boot.settings));
}
function seedSystem(boot: BootstrapSnapshot) {
  validated(mediaStore,parseMedia,{ rev: boot.mediaRev, nowPlaying: boot.media });
  if (boot.lyrics) validated(lyricsStore,parseLyrics,boot.lyrics);
  if (boot.weather) weatherStore.apply(boot.weather); if (boot.power) powerStore.apply(boot.power);
  bluetoothStore.apply({ rev: boot.bluetoothRev, devices: boot.bluetooth });
  for (const v of [boot.capabilities.powerStatus, boot.capabilities.weatherStatus, boot.capabilities.bluetoothStatus, boot.capabilities.mediaStatus]) {
    if (!statuses.has(v.service)) statuses.set(v.service, createEpochTopic<ServiceStatus>()); statuses.get(v.service)?.apply(v);
  }
}
export async function configureAppTopics(settings: Settings) {
  void clipboardHistory.configure(settings.clipboardHistory, clipboardKey(settings));
  const next = JSON.stringify(settings.moduleRegistry); if (signature === next) return;
  signature = next; const generation = ++lifecycle; offApps?.(); offApps = null; offSystem?.(); offSystem = null;
  for (const store of Object.values(appStores)) store.reset();
  for (const store of [mediaStore,lyricsStore,equalizerStore,weatherStore,powerStore,bluetoothStore]) store.reset();
  for (const store of statuses.values()) store.reset();
  const off = await subscribeApps(); if (generation !== lifecycle) { off(); return; } offApps = off;
  const systemOff = await subscribeSystem(); if (generation !== lifecycle) { systemOff(); return; } offSystem = systemOff;
  const boot = await bootstrap(); if (generation !== lifecycle) return; seedApps(boot); seedSystem(boot);
  const launcher = await launcherGet(); if (generation === lifecycle) launcherStore.apply(launcher, launcher.rev);
}
