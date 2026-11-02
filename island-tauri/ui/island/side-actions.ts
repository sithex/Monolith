// Side-button actions (Nov 2): every app page plus one-tap quick actions.
// Quick actions reuse existing typed commands only; nothing runs a shell.
import { appApi, mediaControl, type IslandView, type LayoutAction } from '../ipc/client';
import { dispatch } from '../ipc/island-state';
import { appStores } from '../stores/apps';
import { settingsStore } from '../stores';
import { viewIcons, viewLabels } from './Launcher';
import { isQuickAction, layoutActionView, type QuickAction } from './geometry';
import type { IconName } from './models';

export type ActionOption = { key: string; action: LayoutAction; label: string; icon: IconName; group: 'Apps' | 'Quick actions' };
const PAGES: IslandView[] = ['launcher', 'terminal', 'settings', 'mixer', 'music', 'timer', 'calendar', 'ai', 'monitor', 'notifications', 'clipboard', 'scratchpad', 'downloads', 'brightness', 'updates', 'agents', 'quickInfo'];
const pageAction = (view: IslandView): LayoutAction =>
  view === 'launcher' ? { type: 'appGrid' } : view === 'settings' ? { type: 'settings' } : view === 'mixer' ? { type: 'volume' } : view === 'music' ? { type: 'nowPlaying' } : view === 'timer' ? { type: 'timer' } : { type: 'openApp', view };
const QUICK: { type: QuickAction; label: string; icon: IconName }[] = [
  { type: 'playPause', label: 'Play / pause', icon: 'play' }, { type: 'nextTrack', label: 'Next track', icon: 'next' }, { type: 'previousTrack', label: 'Previous track', icon: 'prev' },
  { type: 'toggleDnd', label: 'Do Not Disturb', icon: 'moon' }, { type: 'micMute', label: 'Mute microphone', icon: 'mic' }, { type: 'cyclePowerProfile', label: 'Cycle power profile', icon: 'power' },
  { type: 'keepAwake', label: 'Keep awake', icon: 'coffee' }, { type: 'screenshot', label: 'Screenshot', icon: 'camera' }, { type: 'pickColour', label: 'Colour picker', icon: 'picker' },
  { type: 'collapse', label: 'Minimize island', icon: 'minimize' },
];
export const ACTION_OPTIONS: ActionOption[] = [
  ...PAGES.map(view => ({ key: view === 'launcher' ? 'appGrid' : `open:${view}`, action: pageAction(view), label: viewLabels[view], icon: viewIcons[view] ?? 'grid', group: 'Apps' as const })),
  ...QUICK.map(q => ({ key: q.type, action: { type: q.type } as LayoutAction, label: q.label, icon: q.icon, group: 'Quick actions' as const })),
];
export function actionKey(action: LayoutAction) {
  if (isQuickAction(action.type)) return action.type;
  const view = layoutActionView(action);
  return view === 'launcher' ? 'appGrid' : `open:${view}`;
}
export const actionOption = (action: LayoutAction) => ACTION_OPTIONS.find(o => o.key === actionKey(action));
export const actionIcon = (action: LayoutAction): IconName => actionOption(action)?.icon ?? 'spark';
export const actionLabel = (action: LayoutAction) => actionOption(action)?.label ?? 'Button';

/** Run a side button: pages navigate, quick actions fire their command and keep the page. */
export async function runSideAction(action: LayoutAction, navigate: (view: IslandView) => void) {
  if (!isQuickAction(action.type)) { navigate(layoutActionView(action)); return; }
  switch (action.type) {
    case 'playPause': return mediaControl({ type: 'playPause' });
    case 'nextTrack': return mediaControl({ type: 'next' });
    case 'previousTrack': return mediaControl({ type: 'previous' });
    case 'toggleDnd': return dispatch({ type: 'setDnd', on: !settingsStore.getSnapshot().dnd });
    case 'micMute': { const t = appStores.toggles.getSnapshot(); if (!t?.micAvailable) throw new Error('Microphone control unavailable'); appStores.toggles.apply(await appApi.togglesCommand({ type: 'micMute', muted: !t.micMuted })); return; }
    case 'cyclePowerProfile': appStores.toggles.apply(await appApi.togglesCommand({ type: 'cycleProfile' })); return;
    case 'keepAwake': { const k = appStores.tiles.getSnapshot()?.keepAwake; appStores.tiles.apply(await appApi.tilesCommand({ type: 'keepAwake', on: !k?.on, durationMs: null })); return; }
    case 'screenshot': await dispatch({ type: 'collapse' }); appStores.tiles.apply(await appApi.tilesCommand({ type: 'screenshot' })); return;
    case 'pickColour': await dispatch({ type: 'collapse' }); appStores.tiles.apply(await appApi.tilesCommand({ type: 'pickColour' })); return;
    case 'collapse': return dispatch({ type: 'collapse' });
  }
}
/** Live on/off state for toggle-like quick actions (drawn as a lit button). */
export function quickActive(action: LayoutAction, s: { dnd: boolean; micMuted: boolean | null | undefined; awake: boolean | null | undefined; playing: boolean }) {
  return action.type === 'toggleDnd' ? s.dnd : action.type === 'micMute' ? !!s.micMuted : action.type === 'keepAwake' ? !!s.awake : action.type === 'playPause' ? s.playing : false;
}
