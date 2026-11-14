import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { appApi, clipboardCopy, clipboardDelete, clipboardPin, clipboardSetPaused, notificationsClear, notificationsDismiss, mediaControl, mediaSetVisible,
  type IslandView, type CalendarChanged, type CalendarQuery } from '../ipc/client';
import { useSettings } from '../stores';
import { useNotifications } from '../ipc/notifications';
import { appStores, clipboardHistory, useClipboardHistory, useApp, useMedia, useLyrics, useEqualizer, useWeather, usePower, useBluetooth, serviceStatus } from '../stores/apps';
import { CalendarEventEditor } from './CalendarEventEditor';
import { enqueueEpochCommand } from './app-lifecycle';
import { createEpochTopic } from '../stores/epoch-topic';
import { createVisiblePoller } from './visible-poller';
import { createMediaVisibility } from './media-visibility';
import * as adapt from './app-adapters';
import { ClipboardView } from '../island/components/ClipboardView';
import { NowPlaying } from '../island/components/NowPlaying';
import { TimerApp } from '../island/components/TimerApp';
import { CalendarApp } from '../island/components/CalendarApp';
import { ScratchpadApp } from '../island/components/ScratchpadApp';
import { DownloadsApp, TransfersCard } from '../island/components/DownloadsApp';
import { ShelfApp } from '../island/components/ShelfApp';
import { copyText } from './agent-live-ipc';
import { VolumeMixer, Brightness } from '../island/components/VolumeMixer';
import { NotificationsView } from '../island/components/NotificationsView';
import { QuickInfo } from '../island/components/QuickInfo';
import { ControlsApp } from '../island/components/ControlsApp';
import { AgentRunChips } from '../island/components/AgentRunChips';
import { UpdatesApp } from '../island/components/UpdatesApp';
import { SettingsLink } from './settings/SettingsLink';
import { ServiceNotice } from '../island/components/AppPrimitives';
import type { CalendarPresentation, CalendarLabels } from '../island/app-presentations';
const dateKey = (date: Date) => `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`;
export function AppsView({ view, active }: { view: IslandView; active: boolean }) {
  const settings = useSettings(), notifications = useNotifications();
  const timer = useApp('timer'), scratchpad = useApp('scratchpad'), downloads = useApp('downloads'), calendar = useApp('calendar'), transfers = useApp('transfers'), shelf = useApp('shelf');
  const mixer = useApp('mixer'), brightness = useApp('brightness'), toggles = useApp('toggles'), tiles = useApp('tiles'), updates = useApp('updates'), agents = useApp('agents');
  const media = useMedia(), lyrics = useLyrics(), equalizer = useEqualizer(), weather = useWeather(), power = usePower(), bluetooth = useBluetooth();
  const [notice, setNotice] = useState(''), [selected, setSelected] = useState<string | null>(null);
  const clips = useClipboardHistory();
  const [eventSelection, setEventSelection] = useState<string | null>(null), [calendarReload, setCalendarReload] = useState(0);
  const calendarEpoch = appStores.calendar.epoch();
  const [month, setMonth] = useState(() => new Date(new Date().getFullYear(), new Date().getMonth(), 1));
  const [calendarView, setCalendarView] = useState<'month'|'agenda'>('month');
  const [queryStore] = useState(() => createEpochTopic<CalendarChanged>());
  const calendarData = useSyncExternalStore(queryStore.subscribe, queryStore.getSnapshot);
  const [timerPoller] = useState(() => createVisiblePoller(async () => { const epoch = appStores.timer.epoch(); return { epoch, value: await appApi.timer() }; }, result => { appStores.timer.apply(result.value,result.epoch); }, error => setNotice(String(error))));
  const [mediaVisibility] = useState(() => createMediaVisibility(mediaSetVisible, error => setNotice(String(error))));

  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const run = (work: () => Promise<unknown>) => { queue.current = queue.current.catch(() => undefined).then(work).catch(error => setNotice(String(error))); };
  useEffect(() => { timerPoller.setVisible(active && view === 'timer'); }, [active, view, timerPoller]);
  useEffect(() => () => timerPoller.dispose(), [timerPoller]);
  useEffect(() => {
    mediaVisibility.set(active && view === 'music');
    return () => { mediaVisibility.set(false); };
  }, [active, view, mediaVisibility]);
  useEffect(() => { clipboardHistory.setVisible(active && view === 'clipboard'); return () => clipboardHistory.setVisible(false); }, [active, view]);
  useEffect(() => { setEventSelection(null); }, [active, view, calendarEpoch, month]);
  // Query identity is independent of the calendar service's shared revision.
  const queryBusy = useRef(false), pendingQuery = useRef<{ query: CalendarQuery; epoch: number } | null>(null);
  async function drainQuery() {
    if (queryBusy.current || !pendingQuery.current) return;
    const request = pendingQuery.current; pendingQuery.current = null; queryBusy.current = true;
    try { const value = await appApi.calendar(request.query); queryStore.apply(value, request.epoch); }
    catch (error) { if (request.epoch === queryStore.epoch()) setNotice(String(error)); }
    finally { queryBusy.current = false; if (pendingQuery.current) void drainQuery(); }
  }
  useEffect(() => {
    const epoch = queryStore.reset(); pendingQuery.current = null;
    if (!active || view !== 'calendar') return;
    const start = new Date(month.getFullYear(), month.getMonth(), 1), end = new Date(month.getFullYear(), month.getMonth()+1, 1);
    pendingQuery.current = { epoch, query: { startS: start.getTime()/1000, endS: end.getTime()/1000, localOffsetS: -start.getTimezoneOffset()*60 } };
    void drainQuery();
    return () => { queryStore.reset(); pendingQuery.current = null; };
  }, [active, view, month, calendar?.rev, calendarEpoch, calendarReload, queryStore]);
  if (!active) return null;
  const unavailable = <p className="unavailable">This module is disabled or its service has not supplied a snapshot. <SettingsLink rowId={view === 'clipboard' ? 'clipboardHistory' : 'arrangeApps'} label="Enable it in Settings" variant="text" />.</p>;
  let content;
  try {
    if (view === 'timer' && timer) content = <TimerApp snapshot={adapt.adaptTimer(timer)} completedKind={timer.completedKind} intents={{ timer: op => enqueueEpochCommand(appStores.timer,run,()=>appApi.timerCommand(op)) }} />;
    if (view === 'scratchpad' && scratchpad) content = <ScratchpadApp model={adapt.adaptScratchpad(scratchpad, selected ?? scratchpad.notes[0]?.id ?? null)} intents={{ scratchpad: op => {
      if (op.type === 'select') { setSelected(op.id); return; }
      const request = op.type === 'create' ? { type: 'create' as const, id: crypto.randomUUID().replaceAll('-', '') } : op;
      enqueueEpochCommand(appStores.scratchpad,run,async()=>{ const epoch=appStores.scratchpad.epoch(); const result=await appApi.scratchpadCommand(request); if(epoch===appStores.scratchpad.epoch()&&request.type==='create')setSelected(request.id); return result; });
    } }} />;
    if (view === 'downloads' && downloads) {
      const value = adapt.adaptDownloads(downloads), moving = adapt.adaptTransfers(transfers), reveal = (id: string) => run(() => appApi.transferReveal(id));
      content = value.available ? <DownloadsApp items={value.value} transfers={moving} intents={{ download: op => run(() => appApi.downloadsOpen(op.name, op.type)), transfer: reveal }} />
        : <div className="app-stack"><TransfersCard items={moving} onReveal={reveal}/><ServiceNotice icon="download" source="downloads" reason={value.reason}/></div>;
    }
    if (view === 'shelf' && shelf) content = <ShelfApp model={adapt.adaptShelf(shelf)} removeAfterDrag={settings.shelfRemoveAfterDrag}
      off={<>The notch takes no drops while the shelf is off. <SettingsLink rowId="shelfEnabled" label="Turn it on in Settings" variant="text" />.</>}
      intents={{ shelf: op => enqueueEpochCommand(appStores.shelf, run, () => appApi.shelfCommand(op.type === 'clear' ? { type: 'clear' } : op)),
        copy: path => run(async () => { if (!await copyText(path)) throw Error('Could not copy the path'); setNotice('Path copied'); }) }} />;
    if ((view === 'mixer' || view === 'controls') && mixer) content = <VolumeMixer model={adapt.adaptMixer(mixer)} intents={{ mixer: op => enqueueEpochCommand(appStores.mixer,run,()=>appApi.mixerCommand(op)) }} />;
    if (view === 'brightness' && brightness) content = <Brightness displays={adapt.adaptBrightness(brightness)} intents={{ brightness: op => enqueueEpochCommand(appStores.brightness,run,()=>appApi.brightnessCommand(op)) }} />;
    if (view === 'controls') content = <div className="app-stack" data-testid="controls-runtime"><ControlsApp model={adapt.adaptIndependentControls(toggles,tiles,updates)} intents={{
      toggle: op => enqueueEpochCommand(appStores.toggles,run,()=>appApi.togglesCommand(op)),
      tile: op => enqueueEpochCommand(appStores.tiles,run,()=>appApi.tilesCommand(op)),
      updates: () => enqueueEpochCommand(appStores.updates,run,()=>appApi.updatesRefresh()),
    }} />{content}</div>;
    if (view === 'agents' && agents) { const value = adapt.adaptAgents(agents); content = value.available ? <AgentRunChips update={value.value} intents={{ agent: id => setNotice(id ? `Run ${id}` : 'More active runs') }} /> : <ServiceNotice icon="spark" source="agents" reason={value.reason}/>; }
    if (view === 'updates' && updates) content = <UpdatesApp snapshot={updates} onRefresh={async () => { const epoch = appStores.updates.epoch(); const value = await appApi.updatesRefresh(); appStores.updates.apply(value,epoch); }} />;
    if (view === 'notifications') content = <NotificationsView model={adapt.adaptNotifications(notifications,settings.dnd,settings.redactBodies)} intents={{ notification: op => run(() => op.type === 'clearAll' ? notificationsClear() : notificationsDismiss(op.id)) }} />;
    if (view === 'clipboard') content = <ClipboardView model={{ ...clips, paused: clips.status?.paused ?? false, items: clips.items.map(c => ({ ...c, ageLabel: 'Captured' })) }} intents={{ clipboard: op => {
      if (op.type === 'search') { try { clipboardHistory.setQuery(op.query); } catch (error) { setNotice(String(error)); } return; }
      const epoch = clipboardHistory.epoch();
      run(async () => {
        if (!clipboardHistory.isCurrent(epoch)) return;
        if (op.type === 'pause') await clipboardSetPaused(op.paused);
        else if (op.type === 'copy') await clipboardCopy(op.id); else if (op.type === 'delete') await clipboardDelete(op.id); else if (op.type === 'pin') await clipboardPin(op.id,op.pinned);
        if (clipboardHistory.isCurrent(epoch)) { clipboardHistory.invalidate(); if (op.type === 'pause') await clipboardHistory.refreshStatus(); }
      });
    } }} />;
    if (view === 'music') content = <NowPlaying model={{ nowPlaying: media === null ? null : media.nowPlaying, lyrics: lyrics === null ? { status:'off',lines:[],plain:null,activeIndex:null } : adapt.parseLyrics(lyrics), equalizer: equalizer === null ? { active:false,bands:[] } : adapt.parseEqualizer(equalizer) }} intents={{ media: op => run(() => mediaControl(op)) }} mixer={mixer} onStreamVolume={(index, percent) => enqueueEpochCommand(appStores.mixer,run,()=>appApi.mixerCommand({ type:'streamVolume',index,percent }))} lyricsEnabled={settings.lyricsEnabled} onLyricsEnabled={enabled => run(async () => (await import('../stores')).saveSettings({ lyricsEnabled:enabled }))} />;
    if (view === 'quickInfo') content = <QuickInfo model={adapt.adaptQuickInfo(weather,power,bluetooth?.devices ?? [], { weather: serviceStatus('weather')?.detail ?? 'Unavailable', power: serviceStatus('power')?.detail ?? 'Unavailable', bluetooth: serviceStatus('bluetooth')?.available ? null : serviceStatus('bluetooth')?.detail ?? 'Unavailable' })} />;
    if (view === 'calendar' && calendarData) {
      const first = new Date(month.getFullYear(),month.getMonth(),1); first.setDate(first.getDate()-first.getDay());
      const p: CalendarPresentation = { monthLabel:month.toLocaleDateString([],{month:'long',year:'numeric'}),weekdayLabels:['Sun','Mon','Tue','Wed','Thu','Fri','Sat'],view:calendarView,staleSinceLabel:calendarData.ageMs === null ? null : `${Math.floor(calendarData.ageMs/60000)} minutes ago`,
        days:Array.from({length:42},(_,i)=>{ const d=new Date(first);d.setDate(d.getDate()+i);return {key:dateKey(d),label:String(d.getDate()),inMonth:d.getMonth()===month.getMonth(),today:dateKey(d)===dateKey(new Date())}; }) };
      const model = adapt.adaptCalendar(calendarData,p,row => {
        const start=new Date(row.startS*1000), end=new Date(row.endS*1000); const dayKeys:string[]=[]; const d=new Date(start.getFullYear(),start.getMonth(),start.getDate());
        for(let i=0;i<366&&d.getTime()<end.getTime();i++){dayKeys.push(dateKey(d));d.setDate(d.getDate()+1);}
        const labels: CalendarLabels={instanceKey:`${row.source}:${row.uid}:${row.startS}`,dayKeys,dateLabel:start.toLocaleDateString(),timeLabel:start.toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'}),spanLabel:null,recurring:null};return labels;
      });
      const eventRow = model.agenda.find(row => row.instanceKey === eventSelection);
      content = <><CalendarApp model={model} intents={{ subscription: async url => { await appApi.calendarSetSubscription(url); setCalendarReload(n => n + 1); }, calendar: op => {
        if(op.type==='view') setCalendarView(op.view); else if(op.type==='previous'||op.type==='next')setMonth(new Date(month.getFullYear(),month.getMonth()+(op.type==='next'?1:-1),1));
        else if(op.type==='refresh')run(async()=>{ await appApi.calendarRefresh(); setCalendarReload(n=>n+1); });
        else if(op.type==='configure')setNotice('Subscription setup uses the private calendar-secret-url file (0600) in the island data directory; the secret is never exposed here.');
        else if(op.type==='create')setEventSelection('new'); else if(op.type==='select')setEventSelection(op.instanceKey);
      } }} />{(eventSelection==='new'||eventRow) && <CalendarEventEditor key={`${calendarEpoch}:${eventSelection}`} row={eventRow ?? null} date={month} close={()=>setEventSelection(null)} command={async op=>{
        const epoch=appStores.calendar.epoch();
        if(epoch!==calendarEpoch)throw Error('Calendar changed; reopen the event');
        await appApi.calendarCommand(op);
        if(epoch!==appStores.calendar.epoch())throw Error('Calendar changed while saving; reopen it to verify');
        setCalendarReload(n=>n+1);
      }}/>}</>;
    }
  } catch (error) { content = <p role="alert">{String(error)}</p>; }
  return <div className="app-stack">{notice && <p role="status">{notice}</p>}{content ?? unavailable}</div>;
}
