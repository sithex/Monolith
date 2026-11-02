// Round 2 view projections. Not generated IPC. See VERIFICATION.md provenance.
import type { Service } from './models';
export interface AgendaRow {
  uid: string; summary: string; location: string | null; startS: number; endS: number;
  allDay: boolean; source: 'local' | 'subscription'; readOnly: boolean;
  // Host formats/expands local dates (including exclusive end) and recurrence.
  instanceKey: string; dayKeys: string[]; dateLabel: string; timeLabel: string;
  spanLabel: string | null; recurring: boolean;
}
export interface CalendarModel {
  configured: boolean; stale: boolean; staleSinceLabel: string | null; fetchFailed: boolean;
  monthLabel: string; weekdayLabels: string[]; days: { key: string; label: string; inMonth: boolean; today: boolean }[];
  agenda: AgendaRow[]; view: 'month' | 'agenda';
  // Intentionally no subscription URL, untrusted fetch detail or credentials.
}
export interface AppNote { id: string; title: string; body: string; createdMs: number; updatedMs: number; pinned: boolean }
export interface ScratchpadModel { notes: AppNote[]; selectedId: string | null; save: { status: 'saving' | 'saved' | 'error'; message?: string } }
export type FileKind = 'archive' | 'image' | 'video' | 'audio' | 'document' | 'package' | 'code' | 'other';
export interface DownloadItem { name: string; sizeBytes: number; mtimeMs: number; kind: FileKind; state: 'inProgress' | 'settling' | 'ready'; ageLabel: string }
// Core Sink/Stream fields projected for display. Nullable muted explicitly models
// adapter-unavailable/unconfirmed state; core's confirmed bool is never invented.
export interface MixerChannel { index: number; label: string; volumePercent: number | null; muted: boolean | null }
export interface MixerModel { master: MixerChannel | null; outputs: { name: string; description: string }[]; outputName: string | null; streams: (MixerChannel & { sinkIndex: number; corked: boolean })[] }
export interface DisplayView { name: string; label: string; isInternal: boolean; percent: number | null; dragging: boolean }
export interface NotificationItem {
  id: string; appName: string; title: string; body: string | null; hidden: boolean;
  receivedAt: number; ageLabel: string; iconPng: string | null; imagePng: string | null;
  actions: { key: string; label: string }[]; canInvokeActions: boolean;
}
export interface NotificationsModel { dnd: boolean; items: NotificationItem[] }
// Sensitive offers are rejected in Rust BEFORE a ClipSummary exists. No concealed
// content variant exists here. The component also rejects malformed runtime input.
export interface ClipSummary { id: string; kind: 'text' | 'image' | 'files'; preview: string; bytes: number; pinned: boolean; copiedAt: number; sourceApp: string | null; ageLabel: string }
export interface ClipboardModel { paused: boolean; items: ClipSummary[] }
// Aligned with IPC NowPlaying (not core's PascalCase PlaybackStatus).
export interface MediaTrack {
  player: string; title: string; artist: string; album: string; playing: boolean;
  positionMs: number | null; durationMs: number | null; artUrl: string | null;
  canControl: boolean; busName: string; status: string; rate: number; sampledAtMs: number;
  trackId: string | null; canSeek: boolean; canGoNext: boolean; canGoPrevious: boolean;
}
export interface LyricsView { status: 'off' | 'idle' | 'loading' | 'synced' | 'plain' | 'instrumental' | 'notFound' | 'error'; lines: { timeMs: number; text: string }[]; plain: string | null; activeIndex: number | null }
export interface NowPlayingModel { nowPlaying: MediaTrack | null; lyrics: LyricsView; equalizer: { active: boolean; bands: number[] } }
export interface WeatherView { stale: boolean; tempC: number | null; code: number | null; isDay: boolean | null; place: { name: string } | null; updatedLabel: string | null }
export interface PowerView { present: boolean; percentage: number | null; state: 'Unknown' | 'Charging' | 'Discharging' | 'Empty' | 'FullyCharged' | 'PendingCharge' | 'PendingDischarge'; onBattery: boolean | null; timeToEmptyS: number | null; timeToFullS: number | null; alert: 'Low' | 'Critical' | 'Full' | null }
export interface BluetoothDevice { path: string; displayName: string; connected: boolean; paired: boolean; batteryPercent: number | null }
export interface QuickInfoModel { weather: Service<WeatherView>; power: Service<PowerView>; bluetooth: Service<BluetoothDevice[]> }
export interface SettingsModule { id: string; enabled: boolean; order: number; config: unknown; label: string; known: boolean; locked: boolean }
export interface ModuleSettingsModel { version: number; modules: SettingsModule[] }
export interface AppIntents {
  calendar(action: { type: 'view'; view: 'month' | 'agenda' } | { type: 'previous' | 'next' | 'refresh' | 'configure' } | { type: 'select'; instanceKey: string }): void;
  scratchpad(action: { type: 'select' | 'delete'; id: string } | { type: 'create' } | { type: 'edit'; id: string; body: string } | { type: 'pin'; id: string; pinned: boolean }): void;
  download(action: { type: 'open' | 'reveal'; name: string }): void;
  mixer(action: { type: 'output'; name: string } | { type: 'volume'; target: 'sink' | 'stream'; index: number; percent: number } | { type: 'mute'; target: 'sink' | 'stream'; index: number; muted: boolean }): void;
  brightness(action: { name: string; percent: number; phase: 'change' | 'end' }): void;
  notification(action: { type: 'dismiss'; id: string } | { type: 'clearAll' } | { type: 'action'; id: string; key: string }): void;
  clipboard(action: { type: 'pause'; paused: boolean } | { type: 'pin'; id: string; pinned: boolean } | { type: 'delete' | 'copy'; id: string }): void;
  media(action: { type: 'playPause' | 'next' | 'previous' }): void;
  modules(action: { type: 'enable'; id: string; enabled: boolean } | { type: 'order'; order: string[] } | { type: 'settings'; id: string }): void;
}
