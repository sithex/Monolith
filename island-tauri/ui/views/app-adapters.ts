// Runtime boundary: unknown JSON is checked field by field, never asserted to a DTO.
// These adapters own no clocks, requests, persistence or revision fencing.
import type { CalendarChanged, AgendaRow as WireAgendaRow, MixerChanged, IslandNotification, WeatherState, PowerState, BtDevice, TogglesChanged, TilesChanged, UpdatesChanged, ClipSummary, ClipboardStatus, MediaChanged, LyricsChanged, EqualizerChanged } from '../ipc/client';
import type { Service } from '../island/models';
import type { TimerSnapshot, CalendarModel, CalendarPresentation, CalendarLabels, ScratchpadModel, DownloadItem, MixerModel, DisplayView, NotificationsModel, QuickInfoModel, ControlsModel, AgentUpdate } from '../island/app-presentations';
const bad = (): never => { throw new TypeError('Malformed app snapshot'); };
function record(x: unknown): Record<string, unknown> { if (x === null || typeof x !== 'object' || Array.isArray(x)) return bad(); return x as Record<string, unknown>; }
function str(x: unknown, max = 4096): string { if (typeof x !== 'string' || new TextEncoder().encode(x).length > max || x.includes('\0')) return bad(); return x; }
function text(x: unknown, max = 4096): string { const s = str(x, max); if (/[\u0000-\u001f\u007f]/.test(s)) return bad(); return s; }
function bool(x: unknown): boolean { return typeof x === 'boolean' ? x : bad(); }
function num(x: unknown, min = 0, max = Number.MAX_SAFE_INTEGER): number { return typeof x === 'number' && Number.isFinite(x) && x >= min && x <= max ? x : bad(); }
function int(x: unknown, min = 0, max = Number.MAX_SAFE_INTEGER): number { const n = num(x, min, max); return Number.isSafeInteger(n) ? n : bad(); }
function nullable<T>(x: unknown, parse: (v: unknown) => T): T | null { return x === null ? null : parse(x); }
function list<T>(x: unknown, parse: (v: unknown) => T, max = 2000): T[] { if (!Array.isArray(x) || x.length > max) return bad(); return x.map(v => parse(v)); }
function choice<const T extends readonly string[]>(x: unknown, values: T): T[number] { for (const v of values) if (x === v) return v; return bad(); }
function unique<T>(items: T[], key: (v: T) => string | number): T[] { if (new Set(items.map(key)).size !== items.length) return bad(); return items; }
function topic(x: unknown) { const r = record(x); int(r.rev, 0, 4294967295); return { r, available: bool(r.available), detail: text(r.detail) }; }
function service<T>(available: boolean, detail: string, value: T): Service<T> { return available ? { available: true, value } : { available: false, reason: detail || 'Service unavailable' }; }
const uint = (x: unknown) => int(x, 0, 4294967295);
const nonnegative = (x: unknown) => num(x);
const short = (x: unknown) => text(x, 256);
const optionalText = (x: unknown) => nullable(x, text);
const noteId = (x: unknown) => { const s = text(x, 64); return /^[A-Za-z0-9_-]{1,64}$/.test(s) ? s : bad(); };
const deviceName = (x: unknown) => { const s = text(x, 256); return s.length && !s.startsWith('-') && !/\s/u.test(s) ? s : bad(); };
export function adaptTimer(raw: unknown): TimerSnapshot {
  const { r, available, detail } = topic(raw);
  return { rev: uint(r.rev), available, detail, sampledAtMs: num(r.sampledAtMs), completedKind: nullable(r.completedKind, x => choice(x, ['countdown','pomodoro'])),
    countdownStatus: choice(r.countdownStatus, ['idle', 'running', 'paused', 'finished']),
    pomodoroStatus: choice(r.pomodoroStatus, ['idle', 'running', 'paused']),
    stopwatchStatus: choice(r.stopwatchStatus, ['idle', 'running', 'paused']),
    phase: choice(r.phase, ['focus', 'shortBreak', 'longBreak']), remainingMs: num(r.remainingMs),
    phaseRemainingMs: num(r.phaseRemainingMs), elapsedMs: num(r.elapsedMs), round: uint(r.round),
    laps: unique(list(r.laps, x => { const l = record(x); return { number: int(l.number, 1), elapsedMs: num(l.elapsedMs) }; }, 1000), l => l.number),
  };
}
function agendaRow(x: unknown): WireAgendaRow {
  const r = record(x); const source = choice(r.source, ['local', 'subscription']);
  const startS = num(r.startS, -8640000000000, 8640000000000); const endS = num(r.endS, startS, 8640000000000);
  const readOnly = bool(r.readOnly); if (source === 'subscription' && !readOnly) return bad();
  return { uid: text(r.uid, 1024), summary: text(r.summary, 8192), location: nullable(r.location, x => text(x, 8192)), startS, endS, allDay: bool(r.allDay), source, readOnly };
}
const dayKey = (x: unknown) => {
  const s = text(x, 10);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(s)) return bad();
  const d = new Date(`${s}T00:00:00Z`);
  return Number.isFinite(d.getTime()) && d.toISOString().slice(0, 10) === s ? s : bad();
};
function calendarLabels(raw: unknown): CalendarLabels {
  const r = record(raw); return { instanceKey: text(r.instanceKey, 2048), dayKeys: unique(list(r.dayKeys, dayKey, 366), x => x), dateLabel: short(r.dateLabel), timeLabel: short(r.timeLabel), spanLabel: nullable(r.spanLabel, short), recurring: nullable(r.recurring, bool) };
}
/** Local-zone/day expansion is supplied by the host; no URL or raw fetch detail enters props. */
export function adaptCalendar(raw: unknown, presentation: CalendarPresentation, projectRow: (row: WireAgendaRow, index: number) => CalendarLabels): CalendarModel {
  const { r, available, detail } = topic(raw); nullable(r.ageMs, nonnegative); nullable(r.updatedAt, nonnegative);
  const p = record(presentation);
  const weekdayLabels = list(p.weekdayLabels, short, 7);
  if (weekdayLabels.length !== 7) return bad();
  const state: Pick<CalendarChanged, 'configured' | 'stale'> = { configured: bool(r.configured), stale: bool(r.stale) };
  return { ...state, available, fetchFailed: detail === 'fetch-failed', monthLabel: short(p.monthLabel),
    weekdayLabels,
    days: unique(list(p.days, x => { const d = record(x); return { key: dayKey(d.key), label: text(d.label, 32), inMonth: bool(d.inMonth), today: bool(d.today) }; }, 42), d => d.key),
    view: choice(p.view, ['month', 'agenda']), staleSinceLabel: nullable(p.staleSinceLabel, short),
    agenda: unique(list(r.agenda, agendaRow).map((row, i) => ({ ...row, ...calendarLabels(projectRow(row, i)) })), row => row.instanceKey),
  };
}
export function adaptScratchpad(raw: unknown, selectedId: string | null): ScratchpadModel {
  const { r, available, detail } = topic(raw); const dirty = bool(r.dirty);
  const notes: ScratchpadModel['notes'] = unique(list(r.notes, x => {
    const n = record(x); const createdMs = num(n.createdMs); const updatedMs = num(n.updatedMs, createdMs);
    const title = text(n.title, 320); if ([...title].length > 80) return bad();
    return { id: noteId(n.id), title, body: str(n.body, 262144), createdMs, updatedMs, pinned: bool(n.pinned) };
  }, 500), n => n.id);
  const selection = selectedId === null ? null : noteId(selectedId);
  return { notes, selectedId: notes.some(n => n.id === selection) ? selection : null,
    save: detail === 'save-failed' || !available ? { status: 'error', message: detail === 'save-failed' ? 'Could not save notes. Try again.' : detail } : { status: dirty ? 'saving' : 'saved' } };
}
export function adaptDownloads(raw: unknown, ageLabel: (mtimeMs: number) => string = () => 'Age unavailable'): Service<DownloadItem[]> {
  const { r, available, detail } = topic(raw);
  const items: DownloadItem[] = unique(list(r.items, x => {
    const f = record(x); const name = text(f.name, 255);
    if (!name || name === '.' || name === '..' || /[/\\]/.test(name)) return bad();
    const mtimeMs = num(f.mtimeMs);
    return { name, mtimeMs, sizeBytes: int(f.sizeBytes), kind: choice(f.kind, ['archive', 'image', 'video', 'audio', 'document', 'package', 'code', 'other']), state: choice(f.state, ['inProgress', 'settling', 'ready']), ageLabel: short(ageLabel(mtimeMs)) };
  }, 500), f => f.name);
  return service(available, detail, items);
}
export function adaptMixer(raw: unknown): Service<MixerModel> {
  const { r, available, detail } = topic(raw);
  const sinks: MixerChanged['sinks'] = unique(list(r.sinks, x => {
    const s = record(x); return { index: uint(s.index), name: deviceName(s.name), description: text(s.description), muted: bool(s.muted), volumePercent: nullable(s.volumePercent, uint), channels: int(s.channels, 0, 255), state: choice(s.state, ['', 'running', 'idle', 'suspended', 'unknown', 'init', 'unlinked', 'invalid', 'RUNNING', 'IDLE', 'SUSPENDED', 'UNKNOWN', 'INIT', 'UNLINKED', 'INVALID']) };
  }, 256), s => s.index);
  unique(sinks, s => s.name);
  const streams: MixerChanged['streams'] = unique(list(r.streams, x => {
    const s = record(x); return { index: uint(s.index), sinkIndex: uint(s.sinkIndex), appName: text(s.appName), binary: optionalText(s.binary), pid: nullable(s.pid, uint), mediaName: optionalText(s.mediaName), iconName: optionalText(s.iconName), muted: bool(s.muted), corked: bool(s.corked), volumePercent: nullable(s.volumePercent, uint) };
  }), s => s.index);
  return service(available, detail, { sinks, streams, defaultSink: nullable(r.defaultSink, deviceName) });
}
export function adaptBrightness(raw: unknown): Service<DisplayView[]> {
  const { r, available, detail } = topic(raw);
  const displays: DisplayView[] = unique(list(r.displays, x => { const d = record(x); return { name: deviceName(d.name), label: short(d.label), isInternal: bool(d.isInternal), percent: nullable(d.percent, x => int(x, 0, 100)), dragging: bool(d.dragging) }; }, 64), d => d.name);
  return service(available, detail, displays);
}
function notification(x: unknown): Omit<IslandNotification, 'rev'> {
  const r = record(x); uint(r.rev);
  const iconPng = nullable(r.iconPng, x => str(x, 1400000));
  if (iconPng !== null && !/^data:image\/png;base64,[A-Za-z0-9+/=]+$/.test(iconPng)) return bad();
  return { id: text(r.id, 256), source: choice(r.source, ['system', 'hermes', 'alert']), appName: text(r.appName, 256), title: text(r.title, 4096), body: nullable(r.body, x => str(x, 16384)), iconPng,
    urgency: choice(r.urgency, ['low', 'normal', 'critical']), receivedAt: num(r.receivedAt),
    actions: unique(list(r.actions, x => { const a = record(x); return { key: text(a.key, 256), label: text(a.label, 1024) }; }, 64), a => a.key),
    canInvokeActions: bool(r.canInvokeActions), progress: nullable(r.progress, x => num(x, 0, 1)) };
}
export function adaptNotifications(raw: unknown, dnd: boolean, redactBodies: boolean, ageLabel: (receivedAt: number) => string = () => 'Age unavailable'): NotificationsModel {
  bool(dnd); bool(redactBodies);
  return { dnd, items: unique(list(raw, notification, 200), n => n.id).map(n => ({ ...n, hidden: redactBodies, body: redactBodies ? null : n.body, iconPng: redactBodies ? null : n.iconPng, actions: redactBodies ? [] : n.actions, canInvokeActions: false, ageLabel: short(ageLabel(n.receivedAt)) })) };
}
function weather(raw: unknown): WeatherState {
  const r = record(raw); const p = r.place === null ? null : record(r.place);
  return { rev: uint(r.rev), available: bool(r.available), stale: bool(r.stale), tempC: nullable(r.tempC, x => num(x, -273.15, 1000)), temp: nullable(r.temp, x => num(x, -459.67, 2000)), unit: choice(r.unit, ['celsius', 'fahrenheit']), code: nullable(r.code, x => int(x, 0, 65535)), icon: optionalText(r.icon), isDay: nullable(r.isDay, bool), updatedAt: nullable(r.updatedAt, nonnegative),
    place: p && { name: short(p.name), country: short(p.country), latitude: num(p.latitude, -90, 90), longitude: num(p.longitude, -180, 180) } };
}
function power(raw: unknown): PowerState {
  const r = record(raw); return { rev: uint(r.rev), available: bool(r.available), present: bool(r.present), percent: nullable(r.percent, x => num(x, 0, 100)), state: choice(r.state, ['unknown', 'charging', 'discharging', 'empty', 'fullyCharged', 'pendingCharge', 'pendingDischarge']), charging: bool(r.charging), onBattery: nullable(r.onBattery, bool), timeToEmpty: nullable(r.timeToEmpty, nonnegative), timeToFull: nullable(r.timeToFull, nonnegative) };
}
export function adaptQuickInfo(rawWeather: unknown, rawPower: unknown, rawBluetooth: unknown, reasons: { weather: string; power: string; bluetooth: string | null }, updatedLabel: string | null = null): QuickInfoModel {
  const w = rawWeather === null ? null : weather(rawWeather); const p = rawPower === null ? null : power(rawPower);
  const devices: BtDevice[] = unique(list(rawBluetooth, x => { const d = record(x); return { address: text(d.address, 64), name: short(d.name), icon: optionalText(d.icon), path: text(d.path, 1024), connected: bool(d.connected), paired: bool(d.paired), batteryPercent: nullable(d.batteryPercent, x => int(x, 0, 100)) }; }, 256), d => d.path);
  const label = nullable(updatedLabel, short);
  const weatherReason = text(reasons.weather); const powerReason = text(reasons.power); const btReason = nullable(reasons.bluetooth, text);
  return { weather: w ? service(w.available, weatherReason, { ...w, updatedLabel: label }) : { available: false, reason: weatherReason || 'Weather not available' },
    power: p ? service(p.available, powerReason, p) : { available: false, reason: powerReason || 'Power not available' },
    bluetooth: service(btReason === null, btReason ?? '', devices) };
}
export function adaptAgents(raw: unknown): Service<AgentUpdate> {
  const { r, available, detail } = topic(raw);
  const chips: AgentUpdate['chips'] = unique(list(r.chips, x => { const c = record(x); return { id: text(c.id, 256), label: short(c.label), model: nullable(c.model, short), provider: nullable(c.provider, short), elapsedSecs: num(c.elapsedSecs), apiCallCount: nullable(c.apiCallCount, nonnegative), isDelegated: bool(c.isDelegated), children: uint(c.children) }; }, 1000), c => c.id);
  return service(available, detail, { chips, overflow: uint(r.overflow) });
}
function toggles(raw: unknown): Omit<TogglesChanged, 'rev'> {
  const { r, available, detail } = topic(raw);
  const profile = (x: unknown) => choice(x, ['power-saver', 'balanced', 'performance']);
  return { available, detail, powerAvailable: bool(r.powerAvailable), powerDetail: text(r.powerDetail), powerProfile: nullable(r.powerProfile, profile), powerChoices: unique(list(r.powerChoices, profile, 3), x => x), degraded: optionalText(r.degraded), heldBy: list(r.heldBy, text, 128), micAvailable: bool(r.micAvailable), micDetail: text(r.micDetail), micMuted: nullable(r.micMuted, bool), dnd: bool(r.dnd), powerRequest: nullable(r.powerRequest, x => text(x, 128)), micRequest: nullable(r.micRequest, x => text(x, 128)) };
}
function tiles(raw: unknown): Omit<TilesChanged, 'rev'> {
  const { r, available, detail } = topic(raw);
  const k = record(r.keepAwake); const o = record(r.output); const n = record(r.network); const t = record(r.tailscale); const v = record(r.voice);
  const status = (s: Record<string, unknown>) => ({ available: bool(s.available), detail: text(s.detail) });
  return { available, detail,
    keepAwake: { ...status(k), on: nullable(k.on, bool), remainingMs: nullable(k.remainingMs, nonnegative) },
    output: { ...status(o), current: nullable(o.current, deviceName), choices: unique(list(o.choices, x => { const s = record(x); return { name: deviceName(s.name), description: text(s.description), muted: bool(s.muted) }; }, 256), s => s.name) },
    network: { ...status(n), state: nullable(n.state, x => choice(x, ['unknown', 'asleep', 'disconnected', 'disconnecting', 'connecting', 'connectedLocal', 'connectedSite', 'connectedGlobal'])), connectivity: nullable(n.connectivity, x => choice(x, ['unknown', 'none', 'portal', 'limited', 'full'])), kind: nullable(n.kind, x => choice(x, ['wired', 'wifi', 'other', 'none'])), showWifi: bool(n.showWifi), wifiEnabled: nullable(n.wifiEnabled, bool) },
    // BackendState::Unknown carries an intentional opaque string, not a new enum variant.
    tailscale: { ...status(t), backend: nullable(t.backend, short), selfName: nullable(t.selfName, short), onlineCount: nullable(t.onlineCount, uint), peers: unique(list(t.peers, x => { const p = record(x); return { hostName: short(p.hostName), online: nullable(p.online, bool), os: short(p.os), lastSeen: nullable(p.lastSeen, short), firstIp: nullable(p.firstIp, short) }; }, 2000), p => p.hostName) },
    voice: { ...status(v), active: nullable(v.active, short), choices: unique(list(v.choices, short, 256), x => x) },
    pickerAvailable: bool(r.pickerAvailable), pickerBusy: bool(r.pickerBusy), screenshotBusy: bool(r.screenshotBusy), lastColour: nullable(r.lastColour, x => { const s = text(x, 7); return /^#[0-9a-fA-F]{6}$/.test(s) ? s : bad(); }), actionError: optionalText(r.actionError),
  };
}
function updates(raw: unknown): Omit<UpdatesChanged, 'rev'> {
  const { r, available, detail } = topic(raw);
  // Snapshots from before the backend field were all pacman; any other value must be a known backend.
  const backend = r.backend === undefined ? 'pacman' : choice(r.backend, ['pacman', 'dnf', 'unsupported']);
  return { backend, available, detail, count: nullable(r.count, uint), repoCount: nullable(r.repoCount, uint), aurCount: nullable(r.aurCount, uint), needsReboot: bool(r.needsReboot), stale: bool(r.stale), error: optionalText(r.error), ageSecs: nullable(r.ageSecs, nonnegative), nextCheckInSecs: num(r.nextCheckInSecs),
    items: list(r.items, x => { const p = record(x); return { name: short(p.name), oldVersion: short(p.oldVersion), newVersion: short(p.newVersion), ignored: bool(p.ignored), source: choice(p.source, ['repo', 'aur', 'dnf']), arch: p.arch === undefined ? null : nullable(p.arch, x => text(x, 32)) }; }, 20000) };
}
export function adaptControls(rawToggles: unknown, rawTiles: unknown, rawUpdates: unknown): ControlsModel {
  return { toggles: toggles(rawToggles), tiles: tiles(rawTiles), updates: updates(rawUpdates) };
}
export function adaptIndependentControls(rawToggles: unknown, rawTiles: unknown, rawUpdates: unknown): ControlsModel {
  return { toggles: rawToggles === null ? null : toggles(rawToggles), tiles: rawTiles === null ? null : tiles(rawTiles), updates: rawUpdates === null ? null : updates(rawUpdates) };
}
export function parseClip(raw: unknown): ClipSummary {
  const r = record(raw); const id = text(r.id, 128);
  if (!/^c-[1-9][0-9]*$/.test(id) || r.concealed === true || r.sensitive === true) return bad();
  const kind = choice(r.kind, ['text', 'image', 'files']);
  const preview = str(r.preview, 1120); if ([...preview].length > 280) return bad();
  return { rev: uint(r.rev), id, kind, preview, bytes: int(r.bytes, 0, kind === 'image' ? 8388608 : 1048576), pinned: bool(r.pinned), copiedAt: num(r.copiedAt), sourceApp: nullable(r.sourceApp, x => str(x, 4096)) };
}
export function parseClips(raw: unknown): ClipSummary[] {
  const rows = unique(list(raw, parseClip, 500), r => r.id);
  if (rows.some(r => r.rev !== rows[0]?.rev)) return bad();
  return rows;
}
export function parseClipboardStatus(raw: unknown): ClipboardStatus {
  const r = record(raw);
  return { available: bool(r.available), enabled: bool(r.enabled), paused: bool(r.paused), detail: text(r.detail), watcherRunning: bool(r.watcherRunning), connectionsOpen: uint(r.connectionsOpen), connectionAttempts: uint(r.connectionAttempts), dropped: uint(r.dropped), captured: uint(r.captured), sensitiveSkipped: uint(r.sensitiveSkipped), denied: uint(r.denied), echoes: uint(r.echoes) };
}
// Artwork is a data URL of up to 1.4 MB that arrives again with every play/pause or volume
// change; re-validating the same picture each time is the costliest part of parsing.
let validArt: string | null = null;
export function parseMedia(raw: unknown): MediaChanged {
  const r = record(raw), rev = uint(r.rev);
  if (r.nowPlaying === null) return { rev, nowPlaying: null };
  const t = record(r.nowPlaying); const busName = text(t.busName, 256);
  if (!/^org\.mpris\.MediaPlayer2\.[A-Za-z0-9_.-]+$/.test(busName)) return bad();
  const trackId = nullable(t.trackId, x => text(x, 4096));
  if (trackId !== null && !/^\/(?:[A-Za-z0-9_]+(?:\/[A-Za-z0-9_]+)*)?$/.test(trackId)) return bad();
  const artUrl = t.artUrl !== null && t.artUrl === validArt ? validArt : nullable(t.artUrl, x => str(x, 1400000));
  if (artUrl !== null && artUrl !== validArt) {
    if (!/^data:image\/(?:png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/.test(artUrl)) return bad();
    validArt = artUrl;
  }
  return { rev, nowPlaying: { player: text(t.player, 4096), title: str(t.title, 16384), artist: str(t.artist, 16384), album: str(t.album, 16384), playing: bool(t.playing), positionMs: nullable(t.positionMs, nonnegative), durationMs: nullable(t.durationMs, nonnegative), artUrl, canControl: bool(t.canControl), busName, status: choice(t.status, ['playing','paused','stopped']), rate: num(t.rate, -1000, 1000), sampledAtMs: num(t.sampledAtMs), trackId, canSeek: bool(t.canSeek), canGoNext: bool(t.canGoNext), canGoPrevious: bool(t.canGoPrevious), volumePercent: t.volumePercent === undefined ? null : nullable(t.volumePercent, nonnegative), volumeWritable: t.volumeWritable === undefined ? false : bool(t.volumeWritable), pid: t.pid === undefined ? null : nullable(t.pid, uint) } };
}
export function parseLyrics(raw: unknown): LyricsChanged {
  const r = record(raw); const lines = list(r.lines, x => { const l = record(x); return { timeMs: num(l.timeMs), text: str(l.text, 16384) }; }, 10000);
  if (lines.some((l, i) => i > 0 && l.timeMs < (lines[i-1]?.timeMs ?? 0))) return bad();
  return { rev: uint(r.rev), status: choice(r.status, ['off','idle','loading','synced','plain','instrumental','notFound','error']), trackKey: nullable(r.trackKey, x => { const key = text(x, 16); return /^[a-f0-9]{16}$/.test(key) ? key : bad(); }), lines, plain: nullable(r.plain, x => str(x, 1048576)), activeIndex: nullable(r.activeIndex, x => int(x, 0, lines.length-1)) };
}
export function parseEqualizer(raw: unknown): EqualizerChanged {
  const r = record(raw), active = bool(r.active), bands = list(r.bands, x => num(x, 0, 1), 32);
  if (active && bands.length < 8) return bad();
  return { rev: uint(r.rev), active, bands };
}
