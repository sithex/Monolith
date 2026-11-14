// Generated wire projections plus explicitly local display/selection state.
import type {
  TimerChanged, TimerKind, TimerOp, CalendarChanged, AgendaRow as WireAgendaRow,
  ScratchpadChanged, ScratchpadOp, DownloadRow, OpenDownload, TransferRow, MixerChanged, MixerOp,
  BrightnessChanged, BrightnessOp, IslandNotification, WeatherState, PowerState,
  BtDevice, AgentsChanged, TogglesChanged, TogglesOp, TilesChanged, TilesOp, UpdatesChanged,
} from '../ipc/client';
import type { Service } from './models';
export type TimerTab = 'timer' | 'pomodoro' | 'stopwatch';
// Pomodoro phase completion is an activity, not a fictional "finished" status.
export type CompletedTimerKind = Exclude<TimerKind, 'stopwatch'>;
export type TimerSnapshot = Omit<TimerChanged, 'countdownStatus' | 'pomodoroStatus' | 'stopwatchStatus' | 'phase'> & {
  countdownStatus: 'idle' | 'running' | 'paused' | 'finished';
  pomodoroStatus: 'idle' | 'running' | 'paused';
  stopwatchStatus: 'idle' | 'running' | 'paused';
  phase: 'focus' | 'shortBreak' | 'longBreak';
};
export const timerKind = (tab: TimerTab): TimerKind => tab === 'timer' ? 'countdown' : tab;
export type CalendarLabels = {
  instanceKey: string; dayKeys: string[]; dateLabel: string; timeLabel: string;
  spanLabel: string | null; recurring: boolean | null;
};
export type AgendaRow = WireAgendaRow & CalendarLabels;
export type CalendarPresentation = {
  monthLabel: string; weekdayLabels: string[];
  days: { key: string; label: string; inMonth: boolean; today: boolean }[];
  view: 'month' | 'agenda'; staleSinceLabel: string | null;
};
export type CalendarModel = Pick<CalendarChanged, 'available' | 'configured' | 'stale'> & CalendarPresentation & {
  fetchFailed: boolean; agenda: AgendaRow[];
};
export type ScratchpadModel = Pick<ScratchpadChanged, 'notes'> & {
  selectedId: string | null; save: { status: 'saving' | 'saved' | 'error'; message?: string };
};
export type FileKind = 'archive' | 'image' | 'video' | 'audio' | 'document' | 'package' | 'code' | 'other';
export type DownloadItem = Omit<DownloadRow, 'kind' | 'state'> & {
  kind: FileKind; state: 'inProgress' | 'settling' | 'ready'; ageLabel: string;
};
/** A running KDE file copy or move (Downloads page, Transfers card). */
export type TransferItem = Omit<TransferRow, 'percent'> & { percent: number | null };
/** One parked path on the drop shelf (Shelf page). */
export type ShelfItem = { id: string; name: string; path: string; uri: string; folder: boolean; present: boolean; addedMs: number };
export type ShelfModel = { enabled: boolean; maxItems: number; items: ShelfItem[] };
export type MixerModel = Pick<MixerChanged, 'sinks' | 'streams' | 'defaultSink'>;
export type MixerChannel = Pick<MixerChanged['sinks'][number], 'index' | 'volumePercent' | 'muted'> & { label: string; name: string | null };
export type DisplayView = BrightnessChanged['displays'][number];
export type NotificationItem = Omit<IslandNotification, 'rev'> & { hidden: boolean; ageLabel: string };
export type NotificationsModel = { dnd: boolean; items: NotificationItem[] };
export type WeatherView = Omit<WeatherState, 'rev' | 'available'> & { updatedLabel: string | null };
export type PowerView = Omit<PowerState, 'rev' | 'available'>;
export type QuickInfoModel = { weather: Service<WeatherView>; power: Service<PowerView>; bluetooth: Service<BtDevice[]> };
export type ControlsModel = {
  toggles: Omit<TogglesChanged, 'rev'> | null;
  tiles: Omit<TilesChanged, 'rev'> | null;
  updates: Omit<UpdatesChanged, 'rev'> | null;
};
export type AgentUpdate = Pick<AgentsChanged, 'chips' | 'overflow'>;
export interface AppViewIntents {
  timer(action: TimerOp): void;
  calendar(action: { type: 'view'; view: 'month' | 'agenda' } | { type: 'previous' | 'next' | 'refresh' | 'configure' | 'create' } | { type: 'select'; instanceKey: string }): void;
  scratchpad(action: Exclude<ScratchpadOp, { type: 'create' }> | { type: 'select'; id: string } | { type: 'create' }): void;
  download(action: { type: OpenDownload; name: string }): void;
  transfer(id: string): void;
  mixer(action: MixerOp): void;
  brightness(action: BrightnessOp): void;
  notification(action: { type: 'dismiss'; id: string } | { type: 'clearAll' }): void;
  toggle(action: TogglesOp): void;
  tile(action: TilesOp): void;
  // updates_refresh has no generated operation argument; this is a local intent.
  updates(action: { type: 'refresh' }): void;
  agent(id: string | null): void;
}
