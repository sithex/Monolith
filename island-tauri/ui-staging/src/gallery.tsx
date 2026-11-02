import { useMemo, useReducer, useState, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import { AppGrid, IslandShell, TerminalHome, SideButtons, LayoutEditor, TimerApp, ControlsApp, AiDashboard, AgentRunChips } from './components';
import { arrangementReducer, createArrangement } from './arrangement';
import type { IslandIntents, TimerTab } from './models';
import * as fixtures from './mocks';
import './styles.css';
import './app-styles.css';
import { AppStories, appStoryIds } from './app-gallery';
const params = new URLSearchParams(location.search);
function Story({ id, title, note, children, wide = false }: { id: string; title: string; note: string; children: ReactNode; wide?: boolean }) {
  return <article className={`story ${wide ? 'story-wide' : ''}`} id={id}><div className="story-header"><h2>{title}</h2><span>{note}</span></div><div className="story-frame">{children}</div></article>;
}
function Gallery() {
  const [theme, setTheme] = useState(params.get('theme') === 'neutral' ? 'neutral' : 'skin');
  const [demoSkin, setDemoSkin] = useState(params.get('skin') === 'legacy' ? 'legacy' : 'hermes');
  const [state, dispatch] = useReducer(arrangementReducer, fixtures.apps, createArrangement);
  const [timer, setTimer] = useState(fixtures.timerSnapshot);
  const [tab, setTab] = useState<TimerTab>('timer');
  const [minutes, setMinutes] = useState(15);
  const [layout, setLayout] = useState(fixtures.layout);
  const [model, setModel] = useState(fixtures.controls);
  const [log, setLog] = useState('No action sent. This gallery never calls a system service.');
  const intents = useMemo<IslandIntents>(() => ({
    navigate(appId) { setLog(`Navigate: ${appId} (gallery only)`); const id = ({ apps: 'grid', terminal: 'shell', ai: 'dashboard', settings: 'layout' } as Record<string, string>)[appId] ?? appId; document.getElementById(id)?.scrollIntoView({ block: 'start', behavior: 'instant' }); },
    arrange(commit) { setLog(`Arrange commit: ${JSON.stringify(commit)}`); },
    sideAction(action) { setLog(`Side action: ${JSON.stringify(action)} (mock)`); },
    layout(action) { if (action.type === 'preset') setLayout(old => ({ ...old, preset: action.preset })); setLog(`Layout: ${JSON.stringify(action)} · geometry remains the supplied fixture`); },
    timer(action) {
      setLog(`Timer: ${JSON.stringify(action)} (mock snapshot, no background clock)`);
      if (action.type === 'tab') { setTab(action.tab); return; }
      if (action.type === 'minutes') { setMinutes(action.minutes); setTimer(old => ({ ...old, countdown: { status: 'idle', remainingMs: action.minutes * 60000 } })); return; }
      const key = action.tab === 'timer' ? 'countdown' : action.tab;
      if (action.type !== 'lap') setTimer(old => ({ ...old, [key]: { ...old[key], status: action.type === 'pause' ? 'paused' : action.type === 'reset' ? 'idle' : 'running' } }));
    },
    control(action) {
      setLog(`Control: ${JSON.stringify(action)} (mock only; no system change)`);
      if (action.type === 'dnd' || action.type === 'mic') setModel(old => ({ ...old, [action.type]: { available: true, value: { confirmed: action.on, pending: null } } }));
      if (action.type === 'power') setModel(old => ({ ...old, powerToggle: { confirmed: action.profile, pending: null }, power: { ...old.power, active: action.profile } }));
      if (action.type === 'voice') setModel(old => ({ ...old, voices: { available: true, value: { voices: fixtures.controls.voices.available ? fixtures.controls.voices.value.voices : [], active: action.name } } }));
      if (action.type === 'output') setModel(old => {
        if (!old.output.available) return old;
        return { ...old, output: { available: true, value: { ...old.output.value, current: old.output.value.choices.find(s => s.name === action.name) ?? null } } };
      });
      if (action.type === 'keepAwake') setModel(old => ({ ...old, keepAwake: { available: true, value: { status: action.on ? 'on' : 'off', cancelPending: false, autoOffInMs: action.durationMs } } }));
    },
    agent(id) { setLog(`Agent details: ${id ?? 'overflow list'} (mock)`); },
    calendar(action) { setLog(`Calendar: ${JSON.stringify(action)} (mock)`); },
    scratchpad(action) { setLog(`Scratchpad: ${action.type} (mock; body not logged)`); },
    download(action) { setLog(`Download: ${JSON.stringify(action)} (mock; no file opened)`); },
    mixer(action) { setLog(`Mixer: ${JSON.stringify(action)} (mock; audio untouched)`); },
    brightness(action) { setLog(`Brightness: ${JSON.stringify(action)} (mock; display untouched)`); },
    notification(action) { setLog(`Notification: ${JSON.stringify(action)} (mock)`); },
    clipboard(action) { setLog(`Clipboard: ${JSON.stringify(action)} (mock; real clipboard untouched)`); },
    media(action) { setLog(`Media: ${JSON.stringify(action)} (mock)`); },
    modules(action) { setLog(`Modules: ${JSON.stringify(action)} (mock)`); },
  }), []);
  const sendArrange = (action: Parameters<typeof dispatch>[0]) => {
    if (action.type === 'commit') intents.arrange(arrangementReducer(state, action).commit);
    dispatch(action);
  };
  return <main className="gallery" data-theme={theme} data-demo-skin={demoSkin} data-solo={params.get('story') ?? undefined}>
    <header className="gallery-header"><div><h1>Hermes Island</h1><p>Phase 7 component workshop · fictional data, real interactions.<br/>A standalone staging gallery. Not the running island.</p></div><div className="gallery-toolbar"><label className="select-label">Appearance<select aria-label="Appearance" value={theme} onChange={e => setTheme(e.target.value)}><option value="skin">Follow Hermes skin</option><option value="neutral">Neutral black</option></select></label>{theme === 'skin' && <label className="select-label">Demo palette<select aria-label="Demo palette" value={demoSkin} onChange={e => setDemoSkin(e.target.value)}><option value="hermes">Illustrative Hermes green</option><option value="legacy">Legacy fallback</option></select></label>}</div></header>
    <nav className="gallery-nav" aria-label="Component stories">{['shell', 'grid', 'layout', 'timer', 'controls', 'dashboard', 'chips', ...appStoryIds].map(id => <a key={id} href={`#${id}`}>{id}</a>)}</nav>
    <div className="stories">
      <Story id="shell" title="01 · Hermes is home" note="IslandShell + SideButtons" wide><SideButtons layout={layout} geometry={fixtures.geometry} intents={intents}><IslandShell title="Hermes" intents={intents} home><TerminalHome/></IslandShell></SideButtons></Story>
      <Story id="grid" title="02 · Your apps" note="Hold 500 ms or ⋯ → Edit"><IslandShell title="Apps" intents={intents} menuItems={[{ label: 'Edit apps', action: () => { if (!state.snapshot) dispatch({ type: 'begin' }); } }]}><AppGrid apps={fixtures.apps} state={state} dispatch={sendArrange} intents={intents}/></IslandShell></Story>
      <Story id="timer" title="03 · Time, well spent" note="Three independent snapshots"><IslandShell title="Timer" intents={intents}><TimerApp snapshot={timer} tab={tab} minutes={minutes} intents={intents}/></IslandShell></Story>
      <Story id="dashboard" title="04 · AI dashboard" note="Known, partial and unavailable data"><IslandShell title="AI agents" intents={intents}><AiDashboard dashboard={{ available: true, value: fixtures.dashboard }}/></IslandShell></Story>
      <Story id="controls" title="05 · Controls" note="Includes pending and unavailable states"><IslandShell title="Controls" intents={intents}><ControlsApp model={model} intents={intents}/></IslandShell></Story>
      <Story id="layout" title="06 · Make it yours" note="Fixed, host-supplied geometry"><IslandShell title="Layout" intents={intents}><LayoutEditor layout={layout} geometry={fixtures.miniGeometry} intents={intents}/></IslandShell></Story>
      <Story id="chips" title="07 · Work in motion" note="Core-capped chips + supplied overflow"><IslandShell title="Agent runs" intents={intents}><AgentRunChips update={fixtures.agentUpdate} intents={intents}/></IslandShell></Story>
      <AppStories intents={intents}/>
    </div>
    <p role="status" className="gallery-log">{log}</p>
  </main>;
}
createRoot(document.getElementById('root')!).render(<Gallery/>);
