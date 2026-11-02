import type { AgentUpdate, AppTile, ControlsModel, Dashboard, Layout, LayoutGeometry, TimerSnapshot } from './models';
// All values below are fictional fixtures. Never read real services or user state.
export const apps: AppTile[] = [
  { id: 'terminal', label: 'Hermes', icon: 'terminal', tone: 'accent', enabled: true, locked: true, known: true },
  { id: 'controls', label: 'Controls', icon: 'controls', tone: 'blue', enabled: true, locked: false, known: true },
  { id: 'ai', label: 'AI dashboard', icon: 'spark', tone: 'violet', enabled: true, locked: false, known: true },
  { id: 'timer', label: 'Timer', icon: 'timer', tone: 'amber', enabled: true, locked: false, known: true },
  { id: 'monitor', label: 'System', icon: 'monitor', tone: 'green', enabled: true, locked: false, known: true },
  { id: 'notifications', label: 'Notifications', icon: 'bell', tone: 'amber', enabled: true, locked: false, known: true },
  { id: 'calendar', label: 'Calendar', icon: 'calendar', tone: 'pink', enabled: true, locked: false, known: true },
  { id: 'scratchpad', label: 'Scratchpad', icon: 'note', tone: 'blue', enabled: true, locked: false, known: true },
  { id: 'downloads', label: 'Downloads', icon: 'download', tone: 'green', enabled: true, locked: false, known: true },
  { id: 'settings', label: 'Settings', icon: 'settings', tone: 'accent', enabled: true, locked: true, known: true },
  { id: 'music', label: 'Now playing', icon: 'music', tone: 'pink', enabled: false, locked: false, known: true },
  { id: 'clipboard', label: 'Clipboard', icon: 'clipboard', tone: 'blue', enabled: false, locked: false, known: true },
  { id: 'future-module', label: 'Future module', icon: 'spark', tone: 'accent', enabled: false, locked: false, known: false },
];
export const layout: Layout = { version: 1, preset: { custom: { width: 600, maxHeight: 616 } }, buttonDiameter: 36, gap: 10, buttons: [
  { id: 'apps', side: 'left', slot: 0, action: 'appGrid', label: 'Apps' }, { id: 'timer', side: 'left', slot: 1, action: 'timer', label: 'Timer' },
  { id: 'settings', side: 'right', slot: 0, action: 'settings', label: 'Settings' }, { id: 'volume', side: 'right', slot: 1, action: 'volume', label: 'Volume' },
  { id: 'nowplaying', side: 'bottom', slot: 0, action: 'nowPlaying', label: 'Now playing' },
] };
// Supplied example geometry in logical px; no geometry algorithm is implemented in TS.
export const geometry: LayoutGeometry = { canvas: { w: 664, h: 438 }, island: { x: 64, y: 20, w: 536, h: 352 }, buttons: [
  { id: 'apps', rect: { x: 18, y: 20, w: 36, h: 36 } }, { id: 'timer', rect: { x: 18, y: 66, w: 36, h: 36 } },
  { id: 'settings', rect: { x: 610, y: 20, w: 36, h: 36 } }, { id: 'volume', rect: { x: 610, y: 66, w: 36, h: 36 } },
  { id: 'nowplaying', rect: { x: 314, y: 382, w: 36, h: 36 } },
], emptySlots: [] };
export const miniGeometry: LayoutGeometry = { canvas: { w: 472, h: 326 }, island: { x: 64, y: 20, w: 344, h: 240 }, buttons: [
  { id: 'apps', rect: { x: 18, y: 20, w: 36, h: 36 } }, { id: 'timer', rect: { x: 18, y: 66, w: 36, h: 36 } },
  { id: 'settings', rect: { x: 418, y: 20, w: 36, h: 36 } }, { id: 'volume', rect: { x: 418, y: 66, w: 36, h: 36 } },
  { id: 'nowplaying', rect: { x: 218, y: 270, w: 36, h: 36 } },
], emptySlots: [{ side: 'left', slot: 2, rect: { x: 18, y: 112, w: 36, h: 36 } }, { side: 'right', slot: 2, rect: { x: 418, y: 112, w: 36, h: 36 } }, { side: 'bottom', slot: 1, rect: { x: 264, y: 270, w: 36, h: 36 } }] };
export const timerSnapshot: TimerSnapshot = { countdown: { status: 'idle', remainingMs: 900000 }, pomodoro: { status: 'paused', phase: 'focus', remainingMs: 1320000, round: 1 }, stopwatch: { status: 'paused', elapsedMs: 83420, laps: [{ number: 1, elapsedMs: 42600 }] } };
const headset = { index: 1, name: 'fixture-headset', description: 'USB headset (mock)', state: 'RUNNING', muted: false };
export const controls: ControlsModel = {
  dnd: { available: true, value: { confirmed: false, pending: null } },
  mic: { available: true, value: { confirmed: false, pending: { value: true, sentAtMs: 1000, requestId: 7 } } },
  power: { available: true, unavailableReason: null, active: 'balanced', choices: ['power-saver', 'balanced', 'performance'], degraded: null, heldBy: [], ignored: 0 }, powerToggle: { confirmed: 'balanced', pending: null },
  keepAwake: { available: true, value: { status: 'on', cancelPending: false, autoOffInMs: 1800000 } },
  output: { available: true, value: { current: headset, choices: [headset, { index: 2, name: 'fixture-monitor', description: 'Display audio (mock)', state: 'IDLE', muted: false }] } },
  network: { available: true, reason: null, state: 'connectedGlobal', connectivity: 'full', kind: 'wired', showWifiToggle: false, wifiEnabled: null },
  tailscale: { available: true, value: { backend: 'running', selfName: 'workstation (mock)', onlineCount: 1, peers: [{ hostName: 'laptop (mock)', online: true, os: 'linux', lastSeen: null, firstIp: null }, { hostName: 'phone (mock)', online: false, os: 'android', lastSeen: null, firstIp: null }] } },
  voices: { available: true, value: { voices: ['necron4', 'narrator', 'soft-spoken'], active: 'necron4' } },
  screenshot: { available: true, value: null }, colourPicker: { available: false, reason: 'KWin colour picker not connected (mock)' },
  updates: { count: null, repoCount: null, aurCount: null, needsReboot: false, stale: true, error: 'Not checked in this gallery', items: [] },
};
export const dashboard: Dashboard = {
  limits: [
    { provider: 'Claude', label: 'Session', usedPercent: 63, resetsInS: 7920, resetText: '2h 12m' }, { provider: 'Claude', label: 'Week', usedPercent: 42, resetsInS: 270000, resetText: '3d 3h' },
    { provider: 'Codex', label: 'Session', usedPercent: 78, resetsInS: 13140, resetText: '3h 39m' }, { provider: 'Codex', label: 'Week', usedPercent: null, resetsInS: null, resetText: null },
  ], spendTodayUsd: 20.94, spendTodayPartial: true, active: { sessionId: 'demo-ui-staging', provider: 'Mock provider', model: 'Mock coding model', startedAtS: 1000 },
  hourly: Array.from({ length: 24 }, (_, hour) => ({ startS: hour * 3600, tokens: hour >= 8 && hour <= 15 ? [1200, 4200, 6300, 8000, 9200, 7800, 3600, 1100][hour - 8] : null, sessions: hour >= 8 && hour <= 15 ? 1 : 0 })),
  byModel: [{ key: 'Mock model A', costUsd: 10.9, tokens: 14000, rows: 4 }, { key: 'Mock model B', costUsd: 4.2, tokens: 9000, rows: 2 }, { key: 'Unattributed', costUsd: null, tokens: null, rows: 1 }],
  byProject: [{ key: 'island-ui', costUsd: 9.8, tokens: 8000, rows: 3 }, { key: 'core-library', costUsd: 5.12, tokens: 6000, rows: 2 }],
  heatmap: Array.from({ length: 7 }, (_, day) => Array.from({ length: 24 }, (_, hour) => hour >= 8 && hour <= 19 ? (day * 3 + hour * 7) % 6 : 0)), rowsIgnored: 1,
};
export const agentUpdate: AgentUpdate = { chips: [{ id: 'demo-main', label: 'UI staging', model: 'Mock coding model', provider: 'Mock provider', elapsedSecs: 726, apiCallCount: 18, isDelegated: false, children: 2 }, { id: 'demo-child', label: 'Core review', model: null, provider: null, elapsedSecs: 83, apiCallCount: null, isDelegated: true, children: 0 }], overflow: 3 };
