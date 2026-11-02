// Presentation navigation only. Rust owns the selected view and all hover timers.
import type { IslandView, UiIntent, Settings } from '../ipc/client';
export const VIEWS = ['terminal', 'monitor', 'ai', 'settings'] as const;
export function visibleViews(settings: Settings) {
  return VIEWS.filter(id => id === 'terminal' || id === 'settings' ||
    ((id !== 'monitor' || settings.hardwareMonitor) && settings.moduleRegistry?.modules.find(m => m.id === id)?.enabled !== false));
}
export const FORM_GUARD = '.terminal, button, input, textarea, select, label, a, [contenteditable], .wheel, .fader, .app-grid[data-editing="true"], .hidden-grid';
export function canSwipe(target: { closest(selector: string): unknown }) { return !target.closest(FORM_GUARD); }
export function arrowDirection(key: string, target: { closest(selector: string): unknown }) {
  if (target.closest('.terminal, input, select, textarea, [contenteditable]')) return 0;
  return key === 'ArrowRight' ? 1 : key === 'ArrowLeft' ? -1 : 0;
}
export function nextView(view: IslandView, direction: number, ids: readonly IslandView[]) {
  const index = ids.indexOf(view) + direction;
  return index < 0 || index >= ids.length ? null : ids[index] ?? null;
}
export function pointerIntent(inside: boolean, send: (intent: UiIntent) => Promise<void>) {
  return send({ type: inside ? 'pointerEnter' : 'pointerLeave' });
}
export const minuteDelay = (now: number) => 60000 - now % 60000;
