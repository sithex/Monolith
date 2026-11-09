import { lazy, memo, Suspense, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { AnimatePresence, MotionConfig, motion } from 'framer-motion';
import { useReducedMotionAll } from './components/motion-preference';
import { dispatch, subscribeCue, useIslandState } from '../ipc/island-state';
import { useSettings, usePalette, useAppearance, runtimeStore } from '../stores';
import { IslandSurface } from './IslandSurface';
import { IslandShell } from './components/IslandShell';
import { Topline } from './Topline';
import { ViewPager } from './ViewPager';
import { Launcher, viewLabels } from './Launcher';
import { SideControls } from './SideControls';
import { CommandPalette } from './CommandPalette';
import { VoiceControls } from './VoiceControls';
import { ColorPickerPanel } from './components/ColorField';
import { AppsView as AppsViewInner } from '../views/AppsView';
import { configureAppTopics, useLauncher, useMedia } from '../stores/apps';
import type { IslandView } from '../ipc/client';
import { SettingsView as SettingsViewInner } from '../views/SettingsView';
import { MonitorView as MonitorViewInner } from '../views/MonitorView';
import { AiStatsView as AiStatsViewInner } from '../views/AiStatsView';
import { Toast } from '../widgets/Toast';
// xterm is about half the island's JavaScript. It loads in its own chunk, fetched as soon as
// the first frame has painted; once loaded the component never suspends again, so the
// terminal (and its PTY) is still created exactly once.
const loadTerminal = () => import('../views/TerminalView');
const TerminalView = lazy(() => loadTerminal().then(m => ({ default: memo(m.TerminalView) })));
// Every island state change (hover, rev, view) re-renders this root. The panes read their own
// stores, so they re-render only when their props (or their stores) actually change.
const AppsView = memo(AppsViewInner), SettingsView = memo(SettingsViewInner);
const MonitorView = memo(MonitorViewInner), AiStatsView = memo(AiStatsViewInner);
if (typeof window !== 'undefined') window.setTimeout(() => { void loadTerminal(); }, 0);
import { PreviewNotice } from './PreviewNotice';
import { NotchNotifications, NotificationCards, useNotificationHover } from './notifications/NotificationCards';
import { ApprovalNotice, useApprovalHover } from './ApprovalNotice';
import { createClickSound } from './click-sound';
import './island.css';
import './polish.css';
import './tokens/light-tone.css';
import './tokens/scheme-picker.css';
import '../views/settings/settings-controls.css';
import { schemePalette, schemeById, isLightPalette } from './tokens/schemes';
import { useHermesThemeSync, themePayload } from './hermes-theme';
import { notchSchemeSet } from '../ipc/client';
import { useAlbumScheme } from './album-scheme';
import { ALBUM_FADE_MS } from './tokens/album';
import { DUR, EASE_OUT, scaled, setMotionSpeed, tween } from './motion-profile';

const playClick = createClickSound();
/** framer's default (tween .3 s) at the chosen speed; undefined at 100 % keeps framer's own defaults. */
const motionSpeedDefault = (percent: number | undefined) => !percent || percent === 100 ? undefined : { duration: 0.3 * 100 / Math.min(200, Math.max(50, percent)) };
export function Island() {
  const state = useIslandState(), settings = useSettings(), palette = usePalette(), appearance = useAppearance(), approvalHover = useApprovalHover(), notificationHover = useNotificationHover();
  const launcher = useLauncher(), media = useMedia();
  // Settings → Motion: set before any child builds a spring this render (motion-profile).
  setMotionSpeed(settings.animationSpeed ?? 100);
  const [error, setError] = useState(''), [paletteOpen, setPaletteOpen] = useState(false);
  const fail = useCallback((error: unknown) => setError(error instanceof Error ? error.message : String(error)), []);
  // An error is news, not state: it used to stay on screen until the island reloaded.
  useEffect(() => {
    if (!error) return;
    const clear = window.setTimeout(() => setError(''), 8000);
    return () => window.clearTimeout(clear);
  }, [error]);
  useEffect(() => { void configureAppTopics(settings).catch(fail); }, [settings.moduleRegistry]);
  // Scheme presets replace every palette role (island CSS + xterm); explicit overrides still win.
  // Album (Nov 5): accent + tint from the playing artwork; null (no artwork) follows the skin.
  const album = useAlbumScheme(appearance === 'album', media?.nowPlaying?.artUrl ?? null, state.view === 'settings');
  const scheme = useMemo(() => appearance === 'album' ? album : schemePalette(appearance, settings),
    [appearance, album, settings.customAccent, settings.customBackground, settings.customText]);
  const effective = useMemo(() => { const base = { ...palette, ...(scheme ?? {}) };
    return { ...base, background: settings.background || base.background, accent: settings.accent || base.accent }; }, [palette, scheme, settings.background, settings.accent]);
  useHermesThemeSync(settings.hermesThemeSync === true, scheme === null ? null : effective, fail);
  // The native notch follows the scheme too (hover tint = accent, clock = text).
  const notchScheme = scheme ? JSON.stringify(themePayload(scheme)) : 'null';
  useEffect(() => { void notchSchemeSet(JSON.parse(notchScheme)).catch(fail); }, [notchScheme]);
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = appearance;
    root.dataset.tone = isLightPalette(effective) ? 'light' : 'dark';
    root.dataset.reduceMotion = String(settings.reduceMotion === true);
    root.dataset.font = settings.uiFont;
    root.dataset.glass = String(settings.translucent !== false);
    // 0..100 → the body's see-through fraction (55 = the default look).
    root.style.setProperty('--glass-strength', String(Math.max(0, Math.min(100, settings.glassStrength ?? 55)) / 100));
    // The native notch draws its clock in the Hermes skin text colour (PanelPill.qml).
    root.style.setProperty('--notch-text', (scheme ?? palette).text);
    root.dataset.panelAnchor = String(runtimeStore.getSnapshot().panelAnchor);
    root.style.setProperty('--ui-font', settings.uiFont === 'system' ? '"Noto Sans", system-ui, sans-serif' : '"OpenRunde", "Noto Sans", system-ui, sans-serif');
    // Album crossfades: its roles go through registered `--album-*` colours that transition
    // (polish.css); every other scheme sets `--skin-*` directly, as before. `--album-*` always
    // tracks the live palette so choosing Album fades from the previous scheme, not from grey.
    const fade = appearance === 'album';
    for (const [role, value] of Object.entries(effective)) if (role !== 'name') {
      root.style.setProperty(`--album-${role}`, value);
      root.style.setProperty(`--skin-${role}`, fade ? `var(--album-${role})` : value);
    }
    root.style.setProperty('--album-fade', `${settings.reduceMotion ? 0 : ALBUM_FADE_MS}ms`);
    root.style.setProperty('--motion-speed', String(Math.min(200, Math.max(50, settings.animationSpeed ?? 100)) / 100));
    root.style.setProperty('--island-border', settings.borderEnabled ? settings.borderColor || 'var(--ui-border)' : 'transparent');
  }, [effective, settings, appearance, palette.text, scheme]);
  useLayoutEffect(() => subscribeCue(() => playClick(settings.clickSounds)), [settings.clickSounds]);
  // Input modality on <html data-nav-via>: keyboard-driven changes (arrow paging, palette,
  // shortcuts) skip decorative page motion; pointer-driven ones keep it (polish.css).
  useEffect(() => {
    const root = document.documentElement;
    const key = (e: KeyboardEvent) => { if (!e.repeat || root.dataset.navVia !== 'key') root.dataset.navVia = 'key'; };
    const pointer = () => { root.dataset.navVia = 'pointer'; };
    window.addEventListener('keydown', key, true); window.addEventListener('pointerdown', pointer, true);
    return () => { window.removeEventListener('keydown', key, true); window.removeEventListener('pointerdown', pointer, true); };
  }, []);
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
  const reduced = useReducedMotionAll() || settings.reduceMotion === true;
  // Reveal: content holds while the shell grows, then fades/scales in from the top. No blur:
  // a filter over the whole page (xterm included) was costly and lingered as blur(0px) at rest.
  // Nov 9: ~0.30 s (was 0.45), holds only the first quarter of the shell growth, starts at .94
  // (was .86) so the most-opened page settles sooner and zooms less.
  const shown = { opacity: [0, 0, 1], scale: [0.94, 0.94, 1] };
  const showTransition = { duration: scaled(0.3), times: [0, 0.25, 1], ease: EASE_OUT };
  // Reduced motion: no scale/hold, only a short opacity fade.
  const fadeOnly = { duration: 0.12, ease: EASE_OUT };
  const reveal = reduced ? { initial: { opacity: 0 }, animate: { opacity: 1 }, transition: fadeOnly } : { initial: { opacity: 0, scale: 0.94 }, animate: shown, transition: showTransition };
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
      timer = setTimeout(() => setClosing(null), scaled(750));
    } else if (active) setClosing(null);
    wasActive.current = active;
    return () => { if (timer) clearTimeout(timer); };
  }, [active, reduced]);
  // The open content is never remounted (xterm/PTY identity); it is re-revealed in place.
  const openMotion = reduced ? { initial: false as const, animate: { opacity: active ? 1 : 0, scale: 1 }, transition: fadeOnly } : { initial: false as const,
    animate: active ? shown : { opacity: 0, scale: 1 },
    transition: active ? showTransition : tween(0.14) };
  const views = [...new Set(launcher.items.filter(i => i.enabled && i.known && i.view && i.view !== 'controls').map(i => i.view as IslandView))];
  // One place for framer-wide motion: Reduce motion = 'always' (transforms/layout snap, opacity
  // still fades); otherwise the OS preference. Default transitions follow the speed setting.
  return <MotionConfig reducedMotion={settings.reduceMotion ? 'always' : 'user'} transition={motionSpeedDefault(settings.animationSpeed)}><main onClickCapture={event => {
    if ((event.target as HTMLElement).closest('button:not(:disabled)')) playClick(settings.clickSounds);
  }}><NotificationCards fail={fail} /><IslandSurface fail={fail} paletteOpen={paletteOpen}>
    <Topline fail={fail} />
    <CommandPalette fail={fail} onOpenChange={setPaletteOpen} />
    <AnimatePresence>{state.mode === 'hover' && <motion.div key="preview" className="preview island-content" {...reveal}
      exit={reduced ? { opacity: 0, transition: fadeOnly } : { opacity: 0, scale: 0.96, y: -6, transition: tween(DUR.fast) }}
      style={{ position: 'absolute', top: 42, left: 0, right: 0 }}>{approvalHover.active ? <ApprovalNotice fail={fail} /> : notificationHover.active ? <NotchNotifications fail={fail} placement="hover" /> : <PreviewNotice fail={fail} />}</motion.div>}</AnimatePresence>
    <motion.div ref={wrap} className="terminal-wrap island-content" hidden={!active && !lingering} aria-hidden={!active} style={closing ?? undefined} {...openMotion}>
      <ViewPager fail={fail} views={views}>
        <IslandShell title={viewLabels[view]} home={view === 'terminal'} intents={{ navigate: () => navigate('terminal') }} back={{ label: viewLabels[previous], go: goBack }}
          actions={<VoiceControls shown={view === 'terminal'} active={active && view === 'terminal'} fail={fail} />}>
          <div className="view-pane" data-pane={view}>
            <div className="app-pane" hidden={view !== 'launcher'}>{view === 'launcher' && <Launcher navigate={navigate} fail={fail} />}</div>
            <div className="app-pane terminal-pane" hidden={view !== 'terminal'}>
              {state.terminalStarted && <Suspense fallback={null}><TerminalView active={active && view === 'terminal'} palette={effective} ansi16={schemeById(appearance).ansi16} fontSize={settings.terminalFontSize}
                fontFamily={settings.terminalFontFamily ?? ''} scrollback={settings.terminalScrollback ?? 5000}
                cursorStyle={settings.terminalCursorStyle ?? 'block'} cursorBlink={settings.terminalCursorBlink !== false} fail={fail} /></Suspense>}
            </div>
            <div className="app-pane" hidden={view !== 'monitor'}><MonitorView active={active && view === 'monitor'} /></div>
            <div className="app-pane" hidden={view !== 'ai'}><AiStatsView active={active && view === 'ai'} /></div>
            <div className="app-pane" hidden={view !== 'settings'}><SettingsView /></div>
            <div className="app-pane" hidden={['terminal','monitor','ai','settings','launcher'].includes(view)}><AppsView view={view} active={active && !['terminal','monitor','ai','settings','launcher'].includes(view)} /></div>
          </div>
        </IslandShell>
      </ViewPager>
    </motion.div>
    {active && <NotchNotifications fail={fail} placement="open" />}
    {/* Errors are a short message inside the open island (under its blur, clickable to
        dismiss), not loose text on the desktop below it. */}
    <AnimatePresence>{error && active && <motion.div key="error" className="island-error" role="alert" title="Dismiss" onClick={() => setError('')}
      initial={reduced ? false : { opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }}
      exit={reduced ? { opacity: 0, transition: fadeOnly } : { opacity: 0, y: 10, transition: tween(DUR.fast) }}
      transition={tween(0.24)}>{error}</motion.div>}</AnimatePresence>
  </IslandSurface><SideControls navigate={navigate} fail={fail} view={view} back={{ view: previous, label: viewLabels[previous], go: goBack }} /><ColorPickerPanel /><Toast previews={false} /></main></MotionConfig>;
}
