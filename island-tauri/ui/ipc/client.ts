// The only module that imports the generated bindings (SPEC §5.6): replacing the
// generator later touches this file alone.
import { Channel } from "@tauri-apps/api/core";
import {
  commands,
  events,
  type AiConnectResult,
  type AiProvider,
  type AiUsage,
  type AnchorKind,
  type BootstrapSnapshot,
  type ClipSummary,
  type ClipContent,
  type ClipboardStatus,
  type GeometryAck,
  type GeometryReport,
  type IslandError,
  type IslandState,
  type IslandNotification,
  type NotificationClosed,
  type NotificationSnapshot,
  type Palette,
  type Settings,
  type SettingsPatch,
  type SystemStats,
  type TerminalEvent,
  type TerminalSnapshot,
  type UiIntent,
  type VoiceOp,
  type VoiceStatus,
  type VoicePanel,
  type VoicePanelOp,
} from "./bindings";

export type {
  AiConnectResult,
  AiProvider,
  AiUsage,
  AnchorKind,
  BootstrapSnapshot,
  ClipSummary,
  ClipContent,
  ClipboardStatus,
  GeometryAck,
  GeometryReport,
  IslandState,
  IslandNotification,
  NotificationClosed,
  NotificationSnapshot,
  Palette,
  Settings,
  SettingsPatch,
  SystemStats,
  TerminalEvent,
  TerminalSnapshot,
  UiIntent,
  VoiceOp,
  VoiceStatus,
  VoicePanel,
  VoicePanelOp,
};

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: IslandError };

/** Unwraps tauri-specta's `Result` so callers get a value or a thrown, typed-message Error. */
async function unwrap<T>(pending: Promise<Result<T>>): Promise<T> {
  const result = await pending;
  if (result.status === "error") {
    throw new Error(`${result.error.code}: ${result.error.message}`);
  }
  return result.data;
}

export const reportGeometry = (report: GeometryReport): Promise<GeometryAck> =>
  unwrap(commands.islandReportGeometry(report));
export const bootstrap = (): Promise<BootstrapSnapshot> => unwrap(commands.islandBootstrap());
export const notificationsList = (): Promise<IslandNotification[]> => unwrap(commands.notificationsList());
export const notificationsDismiss = (id: string) => unwrap(commands.notificationsDismiss(id));
export const notificationsClear = () => unwrap(commands.notificationsClear());
export const clipboardList = (query: string | null = null, limit = 100): Promise<ClipSummary[]> => unwrap(commands.clipboardList(query, limit));
export const clipboardGet = (id: string): Promise<ClipContent> => unwrap(commands.clipboardGet(id));
export const clipboardCopy = (id: string) => unwrap(commands.clipboardCopy(id));
export const terminalCopy = (text: string) => unwrap(commands.terminalCopy(text));
export const clipboardPin = (id: string, pinned: boolean) => unwrap(commands.clipboardPin(id, pinned));
export const clipboardDelete = (id: string) => unwrap(commands.clipboardDelete(id));
export const clipboardClear = () => unwrap(commands.clipboardClear());
export const clipboardSetPaused = (paused: boolean) => unwrap(commands.clipboardSetPaused(paused));
export const onClipboardAdded = (cb: (item: ClipSummary) => void) => events.clipboardAdded.listen((event) => cb(event.payload));
export const onClipboardStatus = (cb: (status: ClipboardStatus) => void) => {
  let last: number | null = null;
  return events.clipboardStatusChanged.listen((event) => {
    const { rev, status } = event.payload;
    if (last !== null && ((rev - last) >>> 0) >= 0x80000000) return;
    if (last === rev) return;
    last = rev; cb(status);
  });
};
export const onNotificationReceived = (cb: (n: IslandNotification) => void) => events.notificationReceived.listen((event) => cb(event.payload));
export const onNotificationClosed = (cb: (n: NotificationClosed) => void) => events.notificationClosed.listen((event) => cb(event.payload));
export const onNotificationSnapshot = (cb: (n: NotificationSnapshot) => void) => events.notificationSnapshot.listen((event) => cb(event.payload));
/** Typed intent → state actor. Resolves with the actor's state after the intent (informational). */
export const islandDispatch = (intent: UiIntent): Promise<IslandState> =>
  unwrap(commands.islandDispatch(intent));
export const settingsGet = (): Promise<Settings> => unwrap(commands.settingsGet());
export const settingsPatch = (patch: SettingsPatch): Promise<Settings> =>
  unwrap(commands.settingsPatch(patch));
export const paletteGet = (): Promise<Palette> => unwrap(commands.paletteGet());
/** Nov 2: write the island scheme as the Hermes skin (palette) or restore the previous skin (null). */
export const hermesThemeSync = (palette: Palette | null): Promise<string> => unwrap(commands.hermesThemeSync(palette));
/** Nov 2: the active scheme's roles for the native notch (null = follow the Hermes skin). */
export const notchSchemeSet = (palette: Palette | null): Promise<null> => unwrap(commands.notchSchemeSet(palette));
export const terminalAttach = (channel: Channel<TerminalEvent>): Promise<TerminalSnapshot> =>
  unwrap(commands.terminalAttach(channel));
export const terminalInput = async (data: string): Promise<void> => {
  await unwrap(commands.terminalInput(data));
};
export const terminalResize = async (cols: number, rows: number): Promise<void> => {
  await unwrap(commands.terminalResize(cols, rows));
};
export const terminalRestart = (): Promise<TerminalSnapshot> => unwrap(commands.terminalRestart());
export const terminalAck = async (seq: number): Promise<void> => {
  await unwrap(commands.terminalAck(seq));
};
export const systemStats = (): Promise<SystemStats> => unwrap(commands.systemStats());
export const aiUsage = (): Promise<AiUsage> => unwrap(commands.aiUsage());
export const aiConnect = (provider: AiProvider): Promise<AiConnectResult> =>
  unwrap(commands.aiConnect(provider));
export const voiceCommand = (op: VoiceOp): Promise<VoiceStatus> => unwrap(commands.voiceCommand(op));
export const voicePanel = (op: VoicePanelOp): Promise<VoicePanel> => unwrap(commands.voicePanel(op));

export const newTerminalChannel = (): Channel<TerminalEvent> => new Channel<TerminalEvent>();

/** Each returns the unlisten function once the listener is registered. */
export const onPaletteChanged = (cb: (palette: Palette) => void) =>
  events.paletteChanged.listen((event) => cb(event.payload));
export const onSettingsChanged = (cb: (settings: Settings) => void) =>
  events.settingsChanged.listen((event) => cb(event.payload));
export const onAnchorSignal = (cb: (kind: AnchorKind) => void) =>
  events.anchorSignal.listen((event) => cb(event.payload));
export const onIslandStateChanged = (cb: (state: IslandState) => void) =>
  events.islandStateChanged.listen((event) => cb(event.payload));
export const onIslandCue = (cb: (cue: { kind: "click" }) => void) =>
  events.islandCue.listen((event) => cb(event.payload));

// Phase 7a: additive revisioned visual topic. Legacy wrappers above are unchanged.
export type { Appearance, VisualChanged, IslandMode, IslandView, RoundedRect, Registry, ModuleEntry, JsonValue } from "./bindings";
export const visualGet = () => unwrap(commands.visualGet());
export const appearanceSet = (appearance: import("./bindings").Appearance) => unwrap(commands.appearanceSet(appearance));
export const onVisualChanged = (cb: (snapshot: import("./bindings").VisualChanged) => void) =>
  events.visualChanged.listen(event => cb(event.payload));

// Phase 7b: generated DTOs only; components use explicit presentation projections.
export type { NowPlaying, LyricsChanged, EqualizerChanged, MediaCommand } from "./bindings";
export type { AppsSnapshot, TimerChanged, TimerKind, TimerOp, CalendarChanged, CalendarQuery, CalendarOp, AgendaRow,
  ScratchpadChanged, ScratchpadOp, DownloadRow, OpenDownload, DownloadsChanged, MixerChanged, MixerOp,
  BrightnessChanged, BrightnessOp, WeatherState, PowerState, BtDevice, AgentsChanged,
  TogglesChanged, TogglesOp, TilesChanged, TilesOp, UpdatesChanged, ServiceStatus, MediaChanged,
  LauncherChanged, LauncherOp, LauncherItem, LayoutChanged, LayoutModel, LayoutOp, LayoutAction,
  LayoutSide, LayoutPreset, LayoutGeometry, LayoutGeometryQuery } from "./bindings";
export const launcherGet = () => unwrap(commands.launcherGet());
export const launcherCommand = (op: import('./bindings').LauncherOp) => unwrap(commands.launcherCommand(op));
export const layoutGet = () => unwrap(commands.layoutGet());
export const layoutCommand = (op: import('./bindings').LayoutOp) => unwrap(commands.layoutCommand(op));
export const layoutGeometry = (query: import('./bindings').LayoutGeometryQuery) => unwrap(commands.layoutGeometry(query));
export const onLauncherChanged = (cb: (v: import('./bindings').LauncherChanged) => void) => events.launcherChanged.listen(e => cb(e.payload));
export const onLayoutChanged = (cb: (v: import('./bindings').LayoutChanged) => void) => events.layoutChanged.listen(e => cb(e.payload));
export const mediaSetVisible = (visible: boolean) => unwrap(commands.mediaSetVisible(visible));
export const mediaControl = (cmd: import('./bindings').MediaCommand) => unwrap(commands.mediaControl(cmd));
export const appApi = {
  timer: () => unwrap(commands.timerGet()), scratchpad: () => unwrap(commands.scratchpadGet()),
  downloads: () => unwrap(commands.downloadsGet()), brightness: () => unwrap(commands.brightnessGet()),
  agents: () => unwrap(commands.agentsGet()), mixer: () => unwrap(commands.mixerGet()),
  toggles: () => unwrap(commands.togglesGet()), tiles: () => unwrap(commands.tilesGet()), updates: () => unwrap(commands.updatesGet()),
  timerCommand: (op: import('./bindings').TimerOp) => unwrap(commands.timerCommand(op)),
  scratchpadCommand: (op: import('./bindings').ScratchpadOp) => unwrap(commands.scratchpadCommand(op)),
  downloadsOpen: (name: string, op: import('./bindings').OpenDownload) => unwrap(commands.downloadsOpen(name, op)),
  calendar: (q: import('./bindings').CalendarQuery) => unwrap(commands.calendarGet(q)),
  calendarCommand: (op: import('./bindings').CalendarOp) => unwrap(commands.calendarCommand(op)),
  calendarRefresh: () => unwrap(commands.calendarRefresh()),
  calendarSetSubscription: (url: string | null) => unwrap(commands.calendarSetSubscription(url)),
  mixerCommand: (op: import('./bindings').MixerOp) => unwrap(commands.mixerCommand(op)),
  brightnessCommand: (op: import('./bindings').BrightnessOp) => unwrap(commands.brightnessCommand(op)),
  togglesCommand: (op: import('./bindings').TogglesOp) => unwrap(commands.togglesCommand(op)),
  tilesCommand: (op: import('./bindings').TilesOp) => unwrap(commands.tilesCommand(op)),
  updatesRefresh: () => unwrap(commands.updatesRefresh()),
};
type AppListeners = { [K in keyof import('./bindings').AppsSnapshot]: (value: NonNullable<import('./bindings').AppsSnapshot[K]>) => void };
export async function onAppTopics(h: AppListeners) {
  const off: (() => void)[] = [];
  try {
    off.push(await events.timerChanged.listen(e => h.timer(e.payload)));
    off.push(await events.scratchpadChanged.listen(e => h.scratchpad(e.payload)));
    off.push(await events.downloadsChanged.listen(e => h.downloads(e.payload)));
    off.push(await events.calendarChanged.listen(e => h.calendar(e.payload)));
    off.push(await events.brightnessChanged.listen(e => h.brightness(e.payload)));
    off.push(await events.agentsChanged.listen(e => h.agents(e.payload)));
    off.push(await events.mixerChanged.listen(e => h.mixer(e.payload)));
    off.push(await events.togglesChanged.listen(e => h.toggles(e.payload)));
    off.push(await events.tilesChanged.listen(e => h.tiles(e.payload)));
    off.push(await events.updatesChanged.listen(e => h.updates(e.payload)));
    return () => off.forEach(f => f());
  } catch (error) { off.forEach(f => f()); throw error; }
}
export async function onSystemTopics(h: {
  media(v: import('./bindings').MediaChanged): void; lyrics(v: import('./bindings').LyricsChanged): void;
  equalizer(v: import('./bindings').EqualizerChanged): void; weather(v: import('./bindings').WeatherState): void;
  power(v: import('./bindings').PowerState): void; bluetooth(v: import('./bindings').BluetoothChanged): void;
  status(v: import('./bindings').ServiceStatus): void;
}) {
  const off: (() => void)[] = [];
  try {
    off.push(await events.mediaChanged.listen(e => h.media(e.payload)));
    off.push(await events.lyricsChanged.listen(e => h.lyrics(e.payload)));
    off.push(await events.equalizerChanged.listen(e => h.equalizer(e.payload)));
    off.push(await events.weatherChanged.listen(e => h.weather(e.payload)));
    off.push(await events.powerChanged.listen(e => h.power(e.payload)));
    off.push(await events.bluetoothChanged.listen(e => h.bluetooth(e.payload)));
    off.push(await events.serviceStatus.listen(e => h.status(e.payload)));
    return () => off.forEach(f => f());
  } catch (error) { off.forEach(f => f()); throw error; }
}
