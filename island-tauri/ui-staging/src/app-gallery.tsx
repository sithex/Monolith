import { useState, type ReactNode } from 'react';
import type { IslandIntents, Service } from './models';
import type { MixerModel, DisplayView } from './app-models';
import { IslandShell, CalendarApp, ScratchpadApp, DownloadsApp, VolumeMixer, Brightness, NotificationsView, ClipboardView, NowPlaying, QuickInfo, SettingsModules } from './components';
import * as mocks from './app-mocks';
export const appStoryIds = ['calendar', 'scratchpad', 'downloads', 'mixer', 'brightness', 'notifications', 'clipboard', 'music', 'quickinfo', 'modules'];
function Fixture<T>({ id, title, options, intents, children }: { id: string; title: string; options: Record<string, T>; intents: IslandIntents; children(value: T, setValue: (value: T) => void): ReactNode }) {
  const first = Object.keys(options)[0];
  const requested = new URLSearchParams(location.search).get('fixture');
  const initial = requested && requested in options ? requested : first;
  const [choice, setChoice] = useState(initial);
  const [value, setValue] = useState(options[initial]);
  return <article className="story" id={id}><div className="story-header"><h2>{title}</h2><span>Fictional snapshot · Round 2</span></div><div className="story-frame"><div className="fixture-picker"><label className="select-label">Fixture<select aria-label={`${title} fixture`} value={choice} onChange={e => { setChoice(e.target.value); setValue(options[e.target.value]); }}>{Object.keys(options).map(key => <option key={key}>{key}</option>)}</select></label></div><IslandShell title={title} intents={intents}>{children(value, setValue)}</IslandShell></div></article>;
}
export function AppStories({ intents }: { intents: IslandIntents }) {
  return <>
    <Fixture id="calendar" title="Calendar" options={mocks.calendarStories} intents={intents}>{(model, set) => <CalendarApp model={model} intents={{ calendar(action) { intents.calendar(action); if (action.type === 'view') set({ ...model, view: action.view }); } }}/>}</Fixture>
    <Fixture id="scratchpad" title="Scratchpad" options={mocks.scratchpadStories} intents={intents}>{(model, set) => <ScratchpadApp model={model} intents={{ scratchpad(action) {
      intents.scratchpad(action);
      if (action.type === 'select') set({ ...model, selectedId: action.id });
      if (action.type === 'edit') set({ ...model, notes: model.notes.map(n => n.id === action.id ? { ...n, body: action.body } : n), save: { status: 'saving' } });
      if (action.type === 'pin') set({ ...model, notes: model.notes.map(n => n.id === action.id ? { ...n, pinned: action.pinned } : n) });
      if (action.type === 'delete') set({ ...model, selectedId: model.selectedId === action.id ? null : model.selectedId, notes: model.notes.filter(n => n.id !== action.id) });
      if (action.type === 'create') { let number = 1; while (model.notes.some(n => n.id === `mock-note-${number}`)) number++; const id = `mock-note-${number}`; set({ ...model, selectedId: id, notes: [...model.notes, { id, title: 'New mock note', body: '', pinned: false, createdMs: 1000, updatedMs: 1000 }] }); }
    } }}/>}</Fixture>
    <Fixture id="downloads" title="Downloads" options={{ recent: mocks.downloads, empty: [] }} intents={intents}>{items => <DownloadsApp items={items} intents={intents}/>}</Fixture>
    <Fixture<Service<MixerModel>> id="mixer" title="Volume mixer" options={{ mixed: mocks.mixer, unavailable: { available: false, reason: 'Audio server offline (mock)' }, empty: { available: true, value: { master: null, outputs: [], outputName: null, streams: [] } } }} intents={intents}>{(model, set) => <VolumeMixer model={model} intents={{ mixer(action) {
      intents.mixer(action); if (!model.available) return;
      if (action.type === 'output') { set({ available: true, value: { ...model.value, outputName: action.name } }); return; }
      const change = <T extends { index: number }>(channel: T): T => channel.index !== action.index ? channel : { ...channel, ...(action.type === 'mute' ? { muted: action.muted } : { volumePercent: action.percent }) };
      set({ available: true, value: { ...model.value, master: action.target === 'sink' && model.value.master ? change(model.value.master) : model.value.master, streams: action.target === 'stream' ? model.value.streams.map(change) : model.value.streams } });
    } }}/>}</Fixture>
    <Fixture<Service<DisplayView[]>> id="brightness" title="Brightness" options={{ displays: mocks.brightness, unavailable: { available: false, reason: 'PowerDevil unavailable (mock)' }, empty: { available: true, value: [] } }} intents={intents}>{(displays, set) => <Brightness displays={displays} intents={{ brightness(action) { intents.brightness(action); if (displays.available) set({ available: true, value: displays.value.map(d => d.name === action.name ? { ...d, percent: action.percent, dragging: action.phase === 'change' } : d) }); } }}/>}</Fixture>
    <Fixture id="notifications" title="Notifications" options={{ grouped: mocks.notifications, empty: { dnd: false, items: [] }, noActions: { ...mocks.notifications, dnd: false, items: mocks.notifications.items.map(n => ({ ...n, canInvokeActions: false })) } }} intents={intents}>{(model, set) => <NotificationsView model={model} intents={{ notification(action) { intents.notification(action); if (action.type === 'clearAll') set({ ...model, items: [] }); if (action.type === 'dismiss') set({ ...model, items: model.items.filter(n => n.id !== action.id) }); } }}/>}</Fixture>
    <Fixture id="clipboard" title="Clipboard" options={{ history: mocks.clipboard, paused: { ...mocks.clipboard, paused: true }, empty: { paused: false, items: [] } }} intents={intents}>{(model, set) => <ClipboardView model={model} intents={{ clipboard(action) { intents.clipboard(action); if (action.type === 'pause') set({ ...model, paused: action.paused }); if (action.type === 'delete') set({ ...model, items: model.items.filter(i => i.id !== action.id) }); if (action.type === 'pin') set({ ...model, items: model.items.map(i => i.id === action.id ? { ...i, pinned: action.pinned } : i) }); } }}/>}</Fixture>
    <Fixture id="music" title="Now playing" options={mocks.musicStories} intents={intents}>{(model, set) => <NowPlaying model={model} intents={{ media(action) { intents.media(action); if (action.type === 'playPause' && model.nowPlaying) set({ ...model, nowPlaying: { ...model.nowPlaying, playing: !model.nowPlaying.playing }, equalizer: { active: false, bands: [] } }); } }}/>}</Fixture>
    <Fixture id="quickinfo" title="Quick info" options={mocks.quickinfoStories} intents={intents}>{model => <QuickInfo model={model}/>}</Fixture>
    <Fixture id="modules" title="Module settings" options={{ registry: mocks.modules }} intents={intents}>{(model, set) => <SettingsModules model={model} intents={{ modules(action) { intents.modules(action); if (action.type === 'enable') set({ ...model, modules: model.modules.map(m => m.id === action.id ? { ...m, enabled: action.enabled } : m) }); if (action.type === 'order') set({ ...model, modules: model.modules.map(m => ({ ...m, order: action.order.indexOf(m.id) })) }); } }}/>}</Fixture>
  </>;
}
