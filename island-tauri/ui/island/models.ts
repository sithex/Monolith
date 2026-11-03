// Presentation projections only. All IPC data enters through generated client types
// and explicit adapters; these are not command/event DTOs.
export type Service<T> = { available: true; value: T } | { available: false; reason: string };
export interface CostLine { key: string; tokens: number | null; costUsd: number | null; rows: number }
export interface Dashboard {
  limits: { provider: string; label: string; usedPercent: number | null; resetsInS: number | null; resetText: string | null }[];
  spendTodayUsd: number | null; spendTodayPartial: boolean;
  active: { sessionId: string; provider: string; model: string; startedAtS: number } | null;
  hourly: { startS: number; tokens: number | null; sessions: number }[];
  byModel: CostLine[]; byProject: CostLine[]; heatmap: number[][]; rowsIgnored: number | null;
}
export type IconName = 'terminal' | 'grid' | 'timer' | 'settings' | 'volume' | 'music' | 'controls' | 'clipboard' | 'calendar' | 'bell' | 'download' | 'note' | 'spark' | 'monitor' | 'back' | 'more' | 'mic' | 'moon' | 'power' | 'coffee' | 'network' | 'camera' | 'picker' | 'weather' | 'sun' | 'cloud' | 'rain' | 'snow' | 'storm' | 'bluetooth' | 'play' | 'next' | 'prev' | 'minimize' | 'resize' | 'compact' | 'spacious' | 'bigger' | 'volumeOff' | 'quote';
export interface IslandIntents { navigate(appId: string): void }
export interface ModuleSettingsModel {
  version: number;
  modules: { id: string; label: string; enabled: boolean; locked: boolean; known: boolean; order: number }[];
}
export type ModuleAction = { type: 'order'; order: string[] } | { type: 'enable'; id: string; enabled: boolean };
export const duration = (ms: number) => {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(seconds / 3600)}h ${Math.floor(seconds / 60) % 60}m`;
};
