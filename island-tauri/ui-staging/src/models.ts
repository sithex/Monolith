import type { AppIntents } from './app-models';
// Hand-mirrored staging-core DTOs, NOT generated Tauri bindings. See README mapping.
export interface GridItem { id: string; enabled: boolean; locked: boolean }
export interface ArrangeCommit { order: string[]; enabledChanges: { id: string; enabled: boolean }[] }
export type Side = 'left' | 'right' | 'bottom';
export interface Rect { x: number; y: number; w: number; h: number }
export type ButtonAction = 'appGrid' | 'settings' | 'volume' | 'nowPlaying' | 'timer' | { openApp: { appId: string } } | { command: { id: string } };
export type SizePreset = 'compact' | 'spacious' | { custom: { width: number; maxHeight: number } };
export interface SideButton { id: string; side: Side; slot: number; action: ButtonAction; label: string | null }
export interface Layout { version: number; preset: SizePreset; buttonDiameter: number; gap: number; buttons: SideButton[] }
// Geometry is computed outside components by layout-core. Empty-slot rects are supplied too.
export interface LayoutGeometry { canvas: { w: number; h: number }; island: Rect; buttons: { id: string; rect: Rect }[]; emptySlots: { side: Side; slot: number; rect: Rect }[] }
export type TimerTab = 'timer' | 'pomodoro' | 'stopwatch';
export type TimerStatus = 'idle' | 'running' | 'paused';
export interface TimerSnapshot {
  countdown: { status: TimerStatus | 'finished'; remainingMs: number };
  pomodoro: { status: TimerStatus; phase: 'focus' | 'shortBreak' | 'longBreak'; remainingMs: number; round: number };
  stopwatch: { status: TimerStatus; elapsedMs: number; laps: { number: number; elapsedMs: number }[] };
}
// ToggleState is not serializable in the pure core: adapter exposes its accessors.
export interface Pending<T> { value: T; sentAtMs: number; requestId: number }
export interface ToggleState<T> { confirmed: T | null; pending: Pending<T> | null }
export type Service<T> = { available: true; value: T } | { available: false; reason: string };
export type Profile = 'power-saver' | 'balanced' | 'performance';
export interface PowerTile { available: boolean; unavailableReason: string | null; active: Profile | null; choices: Profile[]; degraded: string | null; heldBy: string[]; ignored: number }
export interface Sink { index: number; name: string; description: string; state: string; muted: boolean }
export interface OutputTile { current: Sink | null; choices: Sink[] }
export interface NetworkTile { available: boolean; reason: string | null; state: 'unknown' | 'asleep' | 'disconnected' | 'disconnecting' | 'connecting' | 'connectedLocal' | 'connectedSite' | 'connectedGlobal' | null; connectivity: 'unknown' | 'none' | 'portal' | 'limited' | 'full' | null; kind: 'wired' | 'wifi' | 'other' | 'none'; showWifiToggle: boolean; wifiEnabled: boolean | null }
export type BackendState = 'running' | 'stopped' | 'needsLogin' | 'starting' | 'noState' | { unknown: string };
export interface Peer { hostName: string; online: boolean; os: string; lastSeen: string | null; firstIp: string | null }
export interface TailscaleTile { backend: BackendState; selfName: string | null; peers: Peer[]; onlineCount: number }
export interface VoiceList { voices: string[]; active: string | null }
export interface UpdateBadge { count: number | null; repoCount: number | null; aurCount: number | null; needsReboot: boolean; stale: boolean; error: string | null; items: { name: string; oldVersion: string; newVersion: string; ignored: boolean; source: 'repo' | 'aur' }[] }
// KeepAwakeState is not a wire type; expose a cookie-free presentation projection.
export interface KeepAwakeView { status: 'off' | 'requesting' | 'on'; cancelPending: boolean; autoOffInMs: number | null }
export interface ControlsModel {
  dnd: Service<ToggleState<boolean>>; mic: Service<ToggleState<boolean>>;
  power: PowerTile; powerToggle: ToggleState<Profile>;
  keepAwake: Service<KeepAwakeView>; output: Service<OutputTile>;
  network: NetworkTile; tailscale: Service<TailscaleTile>; voices: Service<VoiceList>;
  screenshot: Service<null>; colourPicker: Service<null>; updates: UpdateBadge;
}
export interface LimitCard { provider: string; label: string; usedPercent: number | null; resetsInS: number | null; resetText: string | null }
export interface CostLine { key: string; tokens: number | null; costUsd: number | null; rows: number }
export interface Dashboard { limits: LimitCard[]; spendTodayUsd: number | null; spendTodayPartial: boolean; active: { sessionId: string; provider: string; model: string; startedAtS: number } | null; hourly: { startS: number; tokens: number | null; sessions: number }[]; byModel: CostLine[]; byProject: CostLine[]; heatmap: number[][]; rowsIgnored: number }
export interface Chip { id: string; label: string; model: string | null; provider: string | null; elapsedSecs: number; apiCallCount: number | null; isDelegated: boolean; children: number }
export interface AgentUpdate { chips: Chip[]; overflow: number }
export type Tone = 'accent' | 'blue' | 'amber' | 'green' | 'pink' | 'violet';
export interface AppTile extends GridItem { label: string; icon: IconName; tone: Tone; known: boolean }
export type IconName = 'terminal' | 'grid' | 'timer' | 'settings' | 'volume' | 'music' | 'controls' | 'clipboard' | 'calendar' | 'bell' | 'download' | 'note' | 'spark' | 'monitor' | 'back' | 'more' | 'mic' | 'moon' | 'power' | 'coffee' | 'network' | 'camera' | 'picker' | 'weather' | 'sun' | 'cloud' | 'rain' | 'snow' | 'storm' | 'bluetooth';
export interface IslandIntents extends AppIntents {
  navigate(appId: string): void;
  arrange(commit: ArrangeCommit | null): void;
  sideAction(action: ButtonAction): void;
  layout(action: { type: 'preset'; preset: SizePreset } | { type: 'add'; side: Side; slot: number } | { type: 'edit'; id: string } | { type: 'move'; id: string; side: Side; slot: number }): void;
  timer(action: { type: 'tab'; tab: TimerTab } | { type: 'minutes'; minutes: number } | { type: 'start' | 'pause' | 'resume' | 'reset' | 'lap'; tab: TimerTab }): void;
  control(action: { type: 'dnd' | 'mic'; on: boolean } | { type: 'power'; profile: Profile } | { type: 'keepAwake'; durationMs: number | null; on: boolean } | { type: 'output'; name: string } | { type: 'voice'; name: string } | { type: 'screenshot' | 'colourPicker' | 'checkUpdates' }): void;
  agent(id: string | null): void;
}
