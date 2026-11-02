import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { dispatch, subscribeCue, useIslandState } from '../ipc/island-state';
import { useSettings, usePalette, useAppearance, runtimeStore } from '../stores';
import { IslandSurface } from './IslandSurface';
import { IslandShell } from './components/IslandShell';
import { Topline } from './Topline';
import { ViewPager } from './ViewPager';
import { Launcher, viewLabels } from './Launcher';
import { SideControls } from './SideControls';
import { ColorPickerPanel } from './components/ColorField';
import { AppsView } from '../views/AppsView';
import { configureAppTopics, useLauncher } from '../stores/apps';
import type { IslandView } from '../ipc/client';
import { TerminalView } from '../views/TerminalView';
import { SettingsView } from '../views/SettingsView';
import { MonitorView } from '../views/MonitorView';
import { AiStatsView } from '../views/AiStatsView';
import { Toast } from '../widgets/Toast';
import { createClickSound } from './click-sound';
import './island.css';
import './polish.css';
import { schemePalette } from './tokens/schemes';
import { useHermesThemeSync, themePayload } from './hermes-theme';
import { notchSchemeSet } from '../ipc/client';

const playClick = createClickSound();
export function Island() {
  const state = useIslandState(), settings = useSettings(), palette = usePalette(), appearance = useAppearance();
  const launcher = useLauncher();
  const [error, setError] = useState('');
  const fail = (error: unknown) => setError(error instanceof Error ? error.message : String(error));
  useEffect(() => { void configureAppTopics(settings).catch(fail); }, [settings.moduleRegistry]);
  // Scheme presets replace every palette role (island CSS + xterm); explicit overrides still win.
  const scheme = useMemo(() => schemePalette(appearance, settings), [appearance, settings.customAccent, settings.customBackground, settings.customText]);
  const effective = useMemo(() => { const base = { ...palette, ...(scheme ?? {}) };
    return { ...base, background: settings.background || base.background, accent: settings.accent || base.accent }; }, [palette, scheme, settings.background, settings.accent]);
  useHermesThemeSync(settings.hermesThemeSync === true, scheme === null ? null : effective, fail);
  // The native notch follows the scheme too (hover tint = accent, clock = text).
  const notchScheme = scheme ? JSON.stringify(themePayload(scheme)) : 'null';
  useEffect(() => { void notchSchemeSet(JSON.parse(notchScheme)).catch(fail); }, [notchScheme]);
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = appearance;
    root.dataset.font = settings.uiFont;
    root.dataset.glass = String(settings.translucent !== false);
    // 0..100 → the body's see-through fraction (55 = the original Vorssaint look).
    root.style.setProperty('--glass-strength', String(Math.max(0, Math.min(100, settings.glassStrength ?? 55)) / 100));
    // The native notch draws its clock in the Hermes skin text colour (PanelPill.qml).
    root.style.setProperty('--notch-text', (scheme ?? palette).text);
    root.dataset.panelAnchor = String(runtimeStore.getSnapshot().panelAnchor);
    root.style.setProperty('--ui-font', settings.uiFont === 'system' ? '"Noto Sans", system-ui, sans-serif' : '"OpenRunde", "Noto Sans", system-ui, sans-serif');
    for (const [role, value] of Object.entries(effective)) if (role !== 'name') root.style.setProperty(`--skin-${role}`, value);
    root.style.setProperty('--island-border', settings.borderEnabled ? settings.borderColor || 'var(--ui-border)' : 'transparent');
  }, [effective, settings, appearance, palette.text, scheme]);
  useLayoutEffect(() => subscribeCue(() => playClick(settings.clickSounds)), [settings.clickSounds]);
  const active = state.mode === 'large';
  const view = state.view;
  const navigate = (view: IslandView) => { void dispatch({ type:'selectView',view }).catch(fail); };
  // Back history: every view change pushes the page you left; Back pops it.
  const history = useRef<IslandView[]>([]), shownView = useRef(view), goingBack = useRef(false);
  if (shownView.current !== view) {
    if (!goingBack.current) { history.current.push(shownView.current); if (history.current.length > 24) history.current.shift(); }
    goingBack.current = false; shownView.current = view;
  }
  const previous = (() => { for (let i = history.current.length - 1; i >= 0; i--) if (history.current[i] !== view) return history.current[i]!; return 'terminal' as IslandView; })();
  const goBack = () => {
    while (history.current.length && history.current[history.current.length - 1] === view) history.current.pop();
    const target = history.current.pop() ?? 'terminal';
    if (target === 'terminal') history.current = [];
    goingBack.current = true; navigate(target);
  };
  const reduced = useReducedMotion();
  // Vorssaint reveal: content holds while the shell grows, then blurs/scales in from the top.
  const shown = { opacity: [0, 0, 1], scale: [0.86, 0.86, 1], filter: ['blur(12px)', 'blur(6px)', 'blur(0px)'] };
  const showTransition = { duration: 0.45, times: [0, 0.35, 1], ease: [0.22, 1, 0.36, 1] as const };
  const reveal = reduced ? { initial: false as const } : { initial: { opacity: 0, scale: 0.86, filter: 'blur(12px)' }, animate: shown, transition: showTransition };
  // The departing page keeps its last layout while it fades; the shell contracts only
  // after the side controls have withdrawn. The terminal is never remounted.
  const wrap = useRef<HTMLDivElement>(null), wasActive = useRef(active);
  const [closing, setClosing] = useState<CSSProperties | null>(null);
  const lingering = !reduced && !active && (closing !== null || wasActive.current);
  useLayoutEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    if (!active && wasActive.current && wrap.current && !reduced) {
      const e = wrap.current;
      setClosing({ position: 'absolute', left: e.offsetLeft, top: e.offsetTop, width: e.offsetWidth, height: e.offsetHeight, flex: 'none', pointerEvents: 'none' });
      timer = setTimeout(() => setClosing(null), 750);
    } else if (active) setClosing(null);
    wasActive.current = active;
    return () => { if (timer) clearTimeout(timer); };
  }, [active, reduced]);
  // The open content is never remounted (xterm/PTY identity); it is re-revealed in place.
  const openMotion = reduced ? { initial: false as const } : { initial: false as const,
    animate: active ? shown : { opacity: 0, scale: 1, filter: 'blur(0px)' },
    transition: active ? showTransition : { duration: 0.16, ease: 'easeIn' as const } };
  const views = [...new Set(launcher.items.filter(i => i.enabled && i.known && i.view && i.view !== 'controls').map(i => i.view as IslandView))];
  return <main onClickCapture={event => {
    if ((event.target as HTMLElement).closest('button:not(:disabled)')) playClick(settings.clickSounds);
  }}><IslandSurface fail={fail}>
    <Topline fail={fail} />
    <AnimatePresence>{state.mode === 'hover' && <motion.div key="preview" className="preview island-content" {...reveal}
      exit={reduced ? { opacity: 0, transition: { duration: 0 } } : { opacity: 0, scale: 0.9, y: -10, transition: { duration: 0.14, ease: [0.5, 0, 0.9, 0.4] } }}
      style={{ position: 'absolute', top: 42, left: 0, right: 0 }}><h2>A quiet place to think.</h2><p className="muted">{state.terminalStarted ? 'Your session is still here.' : 'Your real Hermes session, one click away.'}</p><button className="primary" data-testid="open" onClick={() => { void dispatch({ type: 'pin' }).catch(fail); }}>{state.terminalStarted ? 'Resume terminal' : 'Open terminal'} ↗</button></motion.div>}</AnimatePresence>
    <motion.div ref={wrap} className="terminal-wrap island-content" hidden={!active && !lingering} aria-hidden={!active} style={closing ?? undefined} {...openMotion}>
      <ViewPager fail={fail} views={views}>
        <IslandShell title={viewLabels[view]} home={view === 'terminal'} intents={{ navigate: () => navigate('terminal') }} back={{ label: viewLabels[previous], go: goBack }}>
          <div className="view-pane" data-pane={view}>
            <div className="app-pane" hidden={view !== 'launcher'}>{view === 'launcher' && <Launcher navigate={navigate} fail={fail} />}</div>
            <motion.div className="app-pane terminal-pane" hidden={view !== 'terminal'} initial={false} animate={{ opacity: view === 'terminal' ? 1 : 0 }}>
              {state.terminalStarted && <TerminalView active={active && view === 'terminal'} palette={effective} fontSize={settings.terminalFontSize} fail={fail} />}
            </motion.div>
            <motion.div className="app-pane" hidden={view !== 'monitor'} initial={false} animate={{ opacity: view === 'monitor' ? 1 : 0 }}><MonitorView active={active && view === 'monitor'} /></motion.div>
            <motion.div className="app-pane" hidden={view !== 'ai'} initial={false} animate={{ opacity: view === 'ai' ? 1 : 0 }}><AiStatsView active={active && view === 'ai'} /></motion.div>
            <motion.div className="app-pane" hidden={view !== 'settings'} initial={false} animate={{ opacity: view === 'settings' ? 1 : 0 }}><SettingsView /></motion.div>
            <div className="app-pane" hidden={['terminal','monitor','ai','settings','launcher'].includes(view)}><AppsView view={view} active={active && !['terminal','monitor','ai','settings','launcher'].includes(view)} /></div>
          </div>
        </IslandShell>
      </ViewPager>
    </motion.div>
  </IslandSurface><SideControls navigate={navigate} fail={fail} /><ColorPickerPanel /><Toast />{error && <p role="alert">{error}</p>}</main>;
}
