import type { CalendarModel, ScratchpadModel, DownloadItem, MixerModel, DisplayView, NotificationsModel, ClipboardModel, NowPlayingModel, QuickInfoModel, ModuleSettingsModel } from './app-models';
import type { Service } from './models';
import { apps } from './mocks';
// Fictional snapshots only. Dates/labels are preformatted, not computed by views.
// Generated 64px orbit artwork using the existing neutral theme's token colours.
export const fixturePng = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAACPElEQVR4nO1aO27CQBB9HnEB+wApQsuNUuUGFqnSgEKTKhE3oMqNaEmRA5grRNhgnMXe7wzOIJ4Q0lrr3TczO56PneV5Ds0gKAdBOQjKQVAOgnIQlIOgHBO5pcv3n+5w/fogsUsmEcgM6qJiZOwCtOwNrkPX/5cPWFi2Vyz2GVkAp44lZCBww35C2H2AuBZqlOrDr5nDZQSCctCV1c9uBIJyZOlxwFB/NSvt84vtuvfGkVMJJ+/LmSuOfZMEaKisBhRsv8sYFq67mI/QWYtP85RjUNanaPH12QwjxAh24mpWtuyj1WagOK3TXVzkCLFTb9Es2Kx/+H9ZHq8/52wWkGPf4rjyif1h080eLBZIdLUAvCzNrTd7ix0ohX36g3ydHJLdFgj1qha73Xd3OJ0+QgDkOS/08Bjse6/0b3RxWux+LJILDXGNkeHjzT6Z2H3XztJfhqL+Oc+w+myUoBwE5SDctgBFJ0XxhP15HxQNKo9HiIgFhlhKxDLynBcajy+5hrKv/HZ0pxLFdh2XTbDou3CFIC8LDHlCeipWDqSD/gE0LBeKqJhCUZ228Az/FFf4QQZVeM1EVytehUrtSWLxytLbqRKK1aTO3LEvVHdW/FsjRktmUd8YXawmNbaaXUvMe/kFLTJ+a9Hfxc+9UfyRfMyXfBHVPUtnF7efjXoiNCRzqR93C8QYgVH9kLCAXQbet9zgFcD5HlviawMCKywyCH0rkd2/VrnF74WuCYJyEJSDoBwE5SAoB41NIBW/6lkJqiVw6O0AAAAASUVORK5CYII=';
export const calendar: CalendarModel = {
  configured: true, stale: false, staleSinceLabel: null, fetchFailed: false, view: 'month', monthLabel: 'October 2026', weekdayLabels: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'],
  days: Array.from({ length: 35 }, (_, n) => ({ key: `day-${n}`, label: String(n < 3 ? 28 + n : n < 34 ? n - 2 : 1), inMonth: n >= 3 && n < 34, today: n === 33 })),
  agenda: [
    { uid: 'local-retreat', instanceKey: 'retreat-1', summary: 'Design retreat', location: 'Studio (mock)', startS: 100, endS: 300, allDay: true, source: 'local', readOnly: false, dayKeys: ['day-18', 'day-19', 'day-20'], dateLabel: 'Oct 16–18', timeLabel: '', spanLabel: '3 days', recurring: false },
    { uid: 'subscription-sync', instanceKey: 'sync-1', summary: 'Island design sync', location: 'Online (mock)', startS: 400, endS: 500, allDay: false, source: 'subscription', readOnly: true, dayKeys: ['day-21'], dateLabel: 'Mon, Oct 19', timeLabel: '10:30–11:00 AM', spanLabel: null, recurring: true },
    { uid: 'local-release', instanceKey: 'release-1', summary: 'Release notes', location: null, startS: 600, endS: 700, allDay: false, source: 'local', readOnly: false, dayKeys: ['day-28'], dateLabel: 'Mon, Oct 26', timeLabel: '2:00–2:30 PM', spanLabel: null, recurring: false },
  ],
};
export const calendarStories: Record<string, CalendarModel> = {
  month: calendar,
  agenda: { ...calendar, view: 'agenda' },
  unconfigured: { ...calendar, view: 'agenda', configured: false, agenda: calendar.agenda.filter(e => e.source === 'local') },
  stale: { ...calendar, view: 'agenda', stale: true, fetchFailed: true, staleSinceLabel: '10:42 AM' },
  empty: { ...calendar, view: 'agenda', agenda: [] },
};
export const scratchpad: ScratchpadModel = { selectedId: 'note-brief', save: { status: 'saved' }, notes: [
  { id: 'note-later', title: 'Later', body: 'Review the component gallery.', createdMs: 200, updatedMs: 500, pinned: false },
  { id: 'note-brief', title: 'Island design brief', body: 'Hermes is home.\n\nA quiet surface for the tools I use every day.\n\nKeep data private. Show unknowns honestly.', createdMs: 100, updatedMs: 300, pinned: true },
] };
export const scratchpadStories: Record<string, ScratchpadModel> = { saved: scratchpad, saving: { ...scratchpad, save: { status: 'saving' } }, error: { ...scratchpad, save: { status: 'error', message: 'Storage unavailable (mock)' } }, empty: { notes: [], selectedId: null, save: { status: 'saved' } } };
export const downloads: DownloadItem[] = [
  { name: 'island-concepts.zip', sizeBytes: 28123456, mtimeMs: 400, kind: 'archive', state: 'ready', ageLabel: '2 minutes ago' },
  { name: 'night-sky.png', sizeBytes: 1480000, mtimeMs: 300, kind: 'image', state: 'ready', ageLabel: '12 minutes ago' },
  { name: 'recording.webm.part', sizeBytes: 5200000, mtimeMs: 200, kind: 'video', state: 'inProgress', ageLabel: 'Just now' },
  { name: 'notes.txt', sizeBytes: 1320, mtimeMs: 100, kind: 'document', state: 'settling', ageLabel: 'Just now' },
];
export const mixer: Service<MixerModel> = { available: true, value: {
  master: { index: 1, label: 'Master', volumePercent: 45, muted: false }, outputName: 'fixture-headset', outputs: [{ name: 'fixture-headset', description: 'USB headset (mock)' }, { name: 'fixture-display', description: 'Display speakers (mock)' }],
  streams: [{ index: 10, sinkIndex: 1, label: 'Music player', volumePercent: 72, muted: false, corked: false }, { index: 11, sinkIndex: 1, label: 'Browser', volumePercent: null, muted: null, corked: true }, { index: 12, sinkIndex: 1, label: 'Messenger', volumePercent: 25, muted: true, corked: false }],
} };
export const brightness: Service<DisplayView[]> = { available: true, value: [{ name: 'fixture-internal', label: 'Laptop display', isInternal: true, percent: 65, dragging: false }, { name: 'fixture-external', label: 'Studio display', isInternal: false, percent: 42, dragging: true }, { name: 'fixture-unknown', label: 'Projector', isInternal: false, percent: null, dragging: false }] };
export const notifications: NotificationsModel = { dnd: true, items: [
  { id: 'n2', appName: 'Build system', title: 'Gallery build complete', body: 'All components are ready for review.', hidden: false, receivedAt: 300, ageLabel: '1 minute ago', iconPng: null, imagePng: fixturePng, actions: [{ key: 'review', label: 'Review build' }], canInvokeActions: true },
  { id: 'n1', appName: 'Build system', title: 'Typecheck complete', body: 'No type errors found.', hidden: false, receivedAt: 200, ageLabel: '3 minutes ago', iconPng: null, imagePng: null, actions: [], canInvokeActions: false },
  { id: 'n3', appName: 'Messages', title: 'Private message', body: 'HIDDEN BODY SENTINEL', hidden: true, receivedAt: 100, ageLabel: '5 minutes ago', iconPng: null, imagePng: fixturePng, actions: [{ key: 'private', label: 'PRIVATE ACTION SENTINEL' }], canInvokeActions: true },
] };
export const clipboard: ClipboardModel = { paused: false, items: [
  { id: 'clip-text', kind: 'text', preview: 'A small island. A clear mind.', bytes: 29, pinned: true, copiedAt: 300, sourceApp: 'Editor (mock)', ageLabel: '1 minute ago' },
  { id: 'clip-image', kind: 'image', preview: 'Image · 800 × 600', bytes: 48120, pinned: false, copiedAt: 200, sourceApp: null, ageLabel: '4 minutes ago' },
  { id: 'clip-files', kind: 'files', preview: 'concept-notes.txt\nlayout-preview.png', bytes: 120, pinned: false, copiedAt: 100, sourceApp: 'Files (mock)', ageLabel: '7 minutes ago' },
] };
export const music: NowPlayingModel = { nowPlaying: { player: 'Fixture player', title: 'A quiet orbit', artist: 'The Signal Garden', album: 'After the last build', playing: true, positionMs: 83000, durationMs: 242000, artUrl: fixturePng, canControl: true, busName: 'fixture-player', status: 'playing', rate: 1, sampledAtMs: 1000, trackId: 'fixture-track', canSeek: false, canGoNext: true, canGoPrevious: true }, lyrics: { status: 'synced', lines: [{ timeMs: 0, text: 'We leave the noise behind' }, { timeMs: 60000, text: 'A little light in a quiet orbit' }, { timeMs: 90000, text: 'And find the space to begin' }], activeIndex: 1, plain: null }, equalizer: { active: true, bands: [.1, .4, .7, .9, .6, .3, .5, .8, .6, .4, .2, .1, .3, .5, .2, .1] } };
export const musicStories: Record<string, NowPlayingModel> = { synced: music, unavailable: { ...music, lyrics: { status: 'notFound', lines: [], plain: null, activeIndex: null } }, plain: { ...music, lyrics: { status: 'plain', lines: [], plain: 'A little light\nIn a quiet orbit', activeIndex: null } }, noPlayer: { ...music, nowPlaying: null, equalizer: { active: false, bands: [] } }, paused: { ...music, nowPlaying: music.nowPlaying && { ...music.nowPlaying, playing: false, status: 'paused' }, equalizer: { active: false, bands: [] } } };
export const quickinfo: QuickInfoModel = { weather: { available: true, value: { stale: false, tempC: 23, code: 2, isDay: true, place: { name: 'Demo coast' }, updatedLabel: 'Updated just now' } }, power: { available: true, value: { present: true, percentage: 82, state: 'Charging', onBattery: false, timeToEmptyS: null, timeToFullS: 1800, alert: null } }, bluetooth: { available: true, value: [{ path: '/fixture/headset', displayName: 'Studio headset (mock)', connected: true, paired: true, batteryPercent: 74 }, { path: '/fixture/keyboard', displayName: 'Keyboard (mock)', connected: false, paired: true, batteryPercent: null }] } };
export const quickinfoStories: Record<string, QuickInfoModel> = { normal: quickinfo, unknownWeather: { ...quickinfo, weather: { available: true, value: { ...quickinfo.weather.available && quickinfo.weather.value, tempC: null, code: 42, isDay: null, place: null, stale: true, updatedLabel: null } } }, lowBattery: { ...quickinfo, power: { available: true, value: { present: true, percentage: 8, state: 'Discharging', onBattery: true, timeToEmptyS: 900, timeToFullS: null, alert: 'Critical' } } }, unavailable: { weather: { available: false, reason: 'Location not configured (mock)' }, power: { available: false, reason: 'UPower unavailable (mock)' }, bluetooth: { available: false, reason: 'BlueZ unavailable (mock)' } } };
export const modules: ModuleSettingsModel = { version: 1, modules: apps.filter(a => ['terminal', 'settings', 'calendar', 'music', 'future-module'].includes(a.id)).map((app, order) => ({ id: app.id, label: app.label, enabled: app.enabled, locked: app.locked, known: app.known, order, config: {} })) };
