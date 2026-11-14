import { useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { motion, useAnimationControls } from 'framer-motion';
import { useReducedMotionAll } from './components/motion-preference';
import { useIslandState, dispatch } from '../ipc/island-state';
import { reportGeometry } from '../ipc/client';
import { useApprovalHover } from './ApprovalNotice';
import { useNotificationHover } from './notifications/notification-hover';
import { useSettings, usePalette } from '../stores';
import { notchMorphPath, notchMorphSpan, notchProgress, NOTCH_MORPH_SPAN } from './notch-profile';
import { useLayout, useLauncher } from '../stores/apps';
import { UI_SPRING } from './components/IslandShell';
import { createGeometryReporter, fitSurface, effectiveLayout, visibleLayout, nativePill, presetSize, sideColumnExtent } from './geometry';
import { pointerIntent } from './navigation';
import type { IslandView } from '../ipc/bindings';
import { arrivalTime, axisSpring, motionSpeed, glassFadePlan, glassOpenness, GLASS_OPEN, handOffFade, scaled, surfaceRadius, WITHDRAW_LEAD } from './motion-profile';
export { UI_SPRING };
/** Glass fully shut whatever the drawn height (glassOpenness clamps to 0). */
const GLASS_SHUT = { solidHeight: Infinity, range: 1 };
/** The shell has handed the notch back: invisible, out of every morph rule. */
function arrive(root: HTMLElement) {
  root.style.opacity = '0'; root.dataset.arrived = 'true'; root.dataset.morphing = 'false';
}
/** Island width while the Apps page is open (see the width rule in IslandSurface). */
export const LAUNCHER_WIDTH = 560;
/** Pages with little content open at the Apps page's width instead of the island size setting
 *  (Nick, Nov 9: no dead space). Each still grows to fit wider content (a `[data-fit-width]` row,
 *  e.g. the mixer's faders) up to the size setting; height always follows the content. */
export const COMPACT_VIEWS: ReadonlySet<IslandView> = new Set<IslandView>(['launcher', 'agents', 'mixer', 'brightness', 'timer', 'quickInfo', 'downloads', 'clipboard', 'notifications', 'scratchpad', 'controls']);
/** Lowest open height for content-fitted pages: top bar + one line of content. */
export const MIN_FIT_HEIGHT = 80;
export function IslandSurface({ children, fail, paletteOpen = false, scheme }: { children: ReactNode; fail(error: unknown): void; paletteOpen?: boolean;
  /** The colour scheme the page and the native notch use; the raw skin palette can be a different skin (theme sync keeps the base). */
  scheme?: { surface?: string; accent?: string } }) {
  const state = useIslandState(), settings = useSettings();
  // Settings → Motion → Reduce motion forces the same path as the OS preference.
  const reduced = useReducedMotionAll() || settings.reduceMotion === true;
  const layout = useLayout(), launcher = useLauncher(), pill = nativePill(), skinPalette = usePalette();
  const palette = { surface: scheme?.surface || skinPalette.surface, accent: scheme?.accent || skinPalette.accent };
  const notchPath = useRef<SVGPathElement | null>(null);
  const outer = useRef<HTMLElement | null>(null), current = useRef(state), config = useRef(settings);
  const owner = useRef<ReturnType<typeof createGeometryReporter> | null>(null), animating = useRef(false);
  const animation = useAnimationControls();
  current.current = state; config.current = settings;
  const [viewport, setViewport] = useState({ width: innerWidth, height: innerHeight });
  // Rust moves `--native-pill-*` when the notch is resized (Settings → Notch → Width).
  const [, setPillRev] = useState(0);
  useLayoutEffect(() => {
    const moved = () => setPillRev(n => n + 1);
    addEventListener('island-native-pill', moved);
    return () => removeEventListener('island-native-pill', moved);
  }, []);
  const visible = visibleLayout(layout.layout, launcher.items.filter(i => i.enabled && i.known && i.view).map(i => i.view!));
  const liveLayout = useRef(visible);
  useLayoutEffect(() => {
    if (!outer.current) return;
    owner.current = createGeometryReporter(outer.current, () => current.current,
      () => ({ x: config.current.offsetX, y: config.current.offsetY }), reportGeometry, fail, () => liveLayout.current);
    const resize = () => setViewport({ width: innerWidth, height: innerHeight });
    window.addEventListener('resize', resize);
    return () => { owner.current?.dispose(); owner.current = null; window.removeEventListener('resize', resize); };
  }, []);
  const [contentH, setContentH] = useState<number | null>(null), [contentW, setContentW] = useState<number | null>(null);
  const fitsContent = state.mode === 'large' && state.view !== 'terminal';
  useLayoutEffect(() => {
    const root = outer.current;
    if (!fitsContent || !root) { setContentH(null); setContentW(null); return; }
    let frame = 0, watched: Element | null = null;
    const ro = new ResizeObserver(() => schedule());
    const measure = () => {
      frame = 0;
      const pane = root.querySelector<HTMLElement>('.view-pane > .app-pane:not([hidden])');
      const child = pane?.firstElementChild as HTMLElement | null | undefined;
      if (!pane || !child) return;
      if (watched !== child) { if (watched) ro.unobserve(watched); ro.observe(child); watched = child; }
      const cs = getComputedStyle(pane), wrap = root.querySelector<HTMLElement>('.terminal-wrap');
      const chrome = (root.querySelector<HTMLElement>('.topline')?.offsetHeight ?? 42) + (wrap?.offsetHeight ?? 0) - pane.offsetHeight + 2;
      // A view with its own scroller (Settings' .settings-content) reports only its visible
      // height; add what that scroller hides so the island still grows to fit (up to its max).
      const scroller = child.querySelector<HTMLElement>('[data-fit-scroll]');
      const hidden = scroller ? Math.max(0, scroller.scrollHeight - scroller.clientHeight) : 0;
      const want = Math.ceil(chrome + parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom) + child.offsetHeight + hidden);
      setContentH(h => h !== null && Math.abs(h - want) < 2 ? h : want);
      // Width a row needs unclipped: its children's extent, plus everything around the row.
      const row = child.querySelector<HTMLElement>('[data-fit-width]');
      let wantW: number | null = null;
      if (row && row.children.length) {
        // Measured against the page and the content wrap, never the island's live width: while the
        // island springs, the wrap already has the TARGET width (morph rule), and mixing the two
        // made the target chase itself. Assumes the row's right inset mirrors its left one.
        const r = row.getBoundingClientRect(), p = pane.getBoundingClientRect(), rs = getComputedStyle(row);
        const extent = Math.max(...Array.from(row.children, c => c.getBoundingClientRect().right)) - r.left + row.scrollLeft + parseFloat(rs.paddingRight);
        wantW = Math.ceil((wrap?.offsetWidth ?? root.offsetWidth - 2) + 2 - p.width + 2 * (r.left - p.left) + extent);
      }
      setContentW(w => w !== null && wantW !== null && Math.abs(w - wantW) < 2 ? w : wantW);
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(measure); };
    const mo = new MutationObserver(schedule);
    mo.observe(root.querySelector('.view-pane') ?? root, { childList: true, subtree: true, attributes: true, attributeFilter: ['hidden'] });
    schedule();
    return () => { ro.disconnect(); mo.disconnect(); if (frame) cancelAnimationFrame(frame); };
  }, [fitsContent, state.view]);
  useLayoutEffect(() => { owner.current?.schedule(!animating.current);  }, [state.rev, state.mode, state.view, settings, layout.rev, launcher.rev, contentH, contentW]);
  const preset = layout.layout.preset;
  const { width: customWidth, maxHeight: customHeight } = presetSize(preset);
  const approvalHover = useApprovalHover(), notificationHover = useNotificationHover();
  // An approval prompt outranks popup cards; otherwise the notch grows into the cards.
  const hover = approvalHover.active || !notificationHover.active ? approvalHover : notificationHover;
  // Apps page (Nick's reference, Nov 9): the island hugs the 5-column grid of 100px tiles
  // (5 x 100 + 4 x 8 gaps + 14px margins = 560), whatever the island size setting is.
  // Other compact pages share that width and widen only for content that needs it.
  const sized = !layout.unchangedSizing ? customWidth : state.view === 'terminal' ? 1040 : 640;
  const compact = state.view === 'launcher' ? LAUNCHER_WIDTH
    : COMPACT_VIEWS.has(state.view) ? Math.min(Math.max(sized, LAUNCHER_WIDTH), Math.max(LAUNCHER_WIDTH, contentW ?? 0)) : null;
  const width = state.mode === 'still' ? 150 : state.mode === 'hover' ? hover.width : compact ?? sized;
  const baseHeight = state.mode === 'still' ? 42 : state.mode === 'hover' ? hover.height : state.view === 'terminal' ? 672 : state.view === 'monitor' ? 500 : state.view === 'ai' ? 740 : 640;
  // An explicit preset replaces the legacy per-app ceiling; otherwise increasing the
  // height slider never grows the terminal past 672px (or Settings past 640px).
  // Content-sized pages still shrink to their content below this ceiling.
  const maxHeight = state.mode === 'large' && !layout.unchangedSizing ? customHeight : baseHeight;
  // The palette needs a usable list even above a short content-fitted settings page.
  // A content-fitted page is never shorter than the side-button columns, so the circles run
  // along the island's edges instead of hanging below a short page (e.g. an empty Agent runs).
  // Keep the user's explicit height ceiling and viewport constraints authoritative.
  const fitFloor = Math.max(paletteOpen ? 560 : MIN_FIT_HEIGHT, sideColumnExtent(visible));
  const height = fitsContent && contentH !== null ? Math.min(maxHeight, Math.max(fitFloor, contentH)) : maxHeight;
  const fit = fitSurface(width, height, viewport, state.mode === 'large' ? visible : undefined);
  const limit = fitSurface(Infinity, Infinity, viewport, state.mode === 'large' ? visible : undefined);
  liveLayout.current = effectiveLayout(visible, fit.buttonScale);
  const targetW = state.mode === 'still' && pill ? pill.width : fit.width, targetH = state.mode === 'still' && pill ? pill.height : fit.height;
  const previousMode = useRef(state.mode), previousPalette = useRef(paletteOpen), notchSpan = useRef(NOTCH_MORPH_SPAN);
  const morph = useMemo(() => {
    const drawn = outer.current?.getBoundingClientRect() ?? { width: targetW, height: targetH };
    // The notch silhouette morphs over the travel to the open height (notchMorphSpan): an open
    // island always rests as its plain rounded shell, however close below the notch it stops.
    // A close keeps the span of the height it leaves from, so the shape does not jump.
    if (pill && state.mode !== 'still') notchSpan.current = notchMorphSpan(pill.height, targetH);
    else if (pill && previousMode.current !== 'still') notchSpan.current = notchMorphSpan(pill.height, drawn.height);
    const withdrawing = previousMode.current === 'large' && state.mode !== 'large'; previousMode.current = state.mode;
    const delay = !reduced && withdrawing ? WITHDRAW_LEAD : 0;
    // Opening/closing the command palette is keyboard-driven: its height floor applies at once.
    const paletteOnly = previousPalette.current !== paletteOpen && state.mode === 'large' && !withdrawing; previousPalette.current = paletteOpen;
    const width = { ...axisSpring(drawn.width, targetW, 'width'), delay };
    const height = { ...axisSpring(drawn.height, targetH, 'height'), delay };
    return { sideDelay: reduced ? 0 : arrivalTime(drawn, { width: targetW, height: targetH }), withdrawing,
      transition: reduced || paletteOnly ? { duration: 0 } : { width, height, left: width, top: height, borderRadius: height, default: height } };
  }, [targetW, targetH, state.mode, reduced, paletteOpen, motionSpeed()]);
  const radius = state.mode === 'still' ? (pill ? 0 : 20) : Math.min(28, Math.round(targetH * .34));
  const left = pill ? (state.mode === 'still' ? pill.x : pill.x + pill.width / 2 - fit.width / 2) : undefined, top = pill?.y;
  // Arrival under the notch is a pure function of the drawn size (onUpdate/onAnimationComplete),
  // never a timer that could race a reopen. A close that has nothing left to animate arrives now.
  useLayoutEffect(() => {
    const root = outer.current;
    if (!root || !pill) return;
    if (state.mode !== 'still') { delete root.dataset.arrived; return; }
    const drawn = root.getBoundingClientRect();
    if (Math.abs(drawn.width - pill.width) < .5 && Math.abs(drawn.height - pill.height) < .5) arrive(root);
  }, [state.mode, pill !== null]);
  // Under the notch the corner radius is a function of the drawn height
  // (surfaceRadius), written with the size each frame; it is not a third spring.
  useLayoutEffect(() => {
    const root = outer.current;
    if (root && pill) root.style.borderRadius = `${surfaceRadius(root.getBoundingClientRect().height)}px`;
  }, [pill !== null]);
  // The glass lip (motion-profile.ts glassFadePlan): solid while growing out of the
  // notch, opening over the last 48 px; shut at once on close.
  const glassFade = useRef(GLASS_OPEN), glassNow = useRef(1);
  const setGlass = (root: HTMLElement, value: number) => {
    if ((value === glassNow.current || (Math.abs(value - glassNow.current) < .002 && value > 0 && value < 1)) && root.style.getPropertyValue('--glass-open') !== '') return;
    glassNow.current = value; root.style.setProperty('--glass-open', String(+value.toFixed(3)));
  };
  // Frost after settle (Nov 10, laptop ghosting): KWin's blur region follows the page through
  // async geometry reports, so while the shell grows the blur trails the painted edge by a
  // frame or two. At 240 Hz that is ~4-8 ms and invisible; at 120/60 Hz on an iGPU the
  // translucent rim showed sharp, unblurred desktop around the growing island, worst at high
  // glass strength. So opening out of the notch or a hover preview stays solid for the whole
  // morph, and the glass fades in only after Rust has applied the settled region
  // (whenApplied) plus two presented frames. Page-to-page resizes keep the old height ramp.
  const frost = useRef({ token: 0, waiting: false }), glassMode = useRef(state.mode);
  const frostIn = (token: number) => {
    const root = outer.current;
    if (!root || frost.current.token !== token) return;
    const from = glassNow.current, start = performance.now(), duration = scaled(.18) * 1000;
    const step = (now: number) => {
      if (frost.current.token !== token || !outer.current) return;
      const t = Math.min(1, (now - start) / duration), eased = 1 - (1 - t) ** 3;
      setGlass(outer.current, from + (1 - from) * eased);
      if (t < 1) requestAnimationFrame(step); else glassFade.current = GLASS_OPEN;
    };
    requestAnimationFrame(step);
  };
  useLayoutEffect(() => {
    const root = outer.current;
    if (root) {
      const drawn = root.getBoundingClientRect(), start = drawn.height;
      const entering = glassMode.current !== state.mode && state.mode !== 'still'; glassMode.current = state.mode;
      frost.current.token++;
      if (entering && !reduced && (targetH - start > 2 || targetW - drawn.width > 2)) {
        glassFade.current = GLASS_SHUT; frost.current.waiting = true; setGlass(root, 0);
      } else {
        // Re-targeted while still waiting to frost (e.g. content height settling): stay solid.
        if (frost.current.waiting && state.mode !== 'still') glassFade.current = GLASS_SHUT;
        else { frost.current.waiting = false; glassFade.current = glassFadePlan(start, targetH, state.mode !== 'still', glassNow.current); }
        setGlass(root, glassOpenness(glassFade.current, start));
      }
    }
    animation.stop();
    void animation.start({ width: targetW, height: targetH, ...(pill ? {} : { borderRadius: radius }),
      ...(left !== undefined && top !== undefined ? { left, top } : {}), transition: morph.transition });
    return () => animation.stop();
  }, [animation, targetW, targetH, radius, left, top, morph]);
  return <motion.section ref={outer} className="island" data-state={state.mode} data-view={state.view} data-side-delay={morph.sideDelay} data-withdrawing={morph.withdrawing}
    style={{ maxWidth: Math.max(pill?.width ?? 0, limit.width), maxHeight: Math.max(pill?.height ?? 0, limit.height),
      '--open-content-width': `${fit.width - 2}px`, '--open-content-height': `${Math.max(0, fit.height - 44)}px`, ...(pill ? { position: 'fixed' } : {}) } as CSSProperties}
    initial={{ width: targetW, height: targetH, ...(pill ? {} : { borderRadius: radius }), ...(left !== undefined && top !== undefined ? { left, top } : {}) }} animate={animation}
    onAnimationStart={() => { animating.current = true; if (outer.current) outer.current.dataset.morphing = 'true'; owner.current?.schedule(false); }}
    onUpdate={latest => {
      const root = outer.current;
      if (root && pill) {
        const w = Number(latest.width), h = Number(latest.height);
        const r = surfaceRadius(h);
        if (Number.isFinite(h)) { setGlass(root, glassOpenness(glassFade.current, h)); root.style.borderRadius = `${+r.toFixed(2)}px`; }
        // Seamless notch hand-off: over the last few pixels the shell (already the notch's
        // exact silhouette and fill) fades out, so the native notch and its clock show
        // through instead of a second, clipped clock. Opening starts from that same fade.
        // The fade waits for BOTH axes (handOffFade): width settles after height, and a
        // shell still wider than the notch would fade straight to whatever is behind the panel.
        if (Number.isFinite(h)) {
          const fade = handOffFade(Number.isFinite(w) ? w : pill.width, h, pill);
          if (current.current.mode === 'still' && fade <= 0) arrive(root);
          else { root.style.opacity = fade >= 1 ? '' : String(+fade.toFixed(3)); delete root.dataset.arrived; }
        }
        const p = notchProgress(h, pill.height, notchSpan.current);
        if (Number.isFinite(w) && Number.isFinite(h) && p > 0 && notchPath.current) {
          root.dataset.notching = 'true'; root.dataset.notchProgress = String(p);
          const cfg = config.current, bw = cfg.notchBorderEnabled === false ? 0 : (cfg.notchBorderWidth ?? .8);
          notchPath.current.setAttribute('d', notchMorphPath(w, h, r, p, bw));
          notchPath.current.setAttribute('fill', `color-mix(in srgb, var(--ui-background) ${(1 - p) * 100}%, ${cfg.panelClockColor || cfg.background || palette.surface})`);
          notchPath.current.setAttribute('stroke', cfg.notchBorderColor || cfg.accent || palette.accent);
          notchPath.current.setAttribute('stroke-width', String(bw));
          notchPath.current.setAttribute('stroke-opacity', String((cfg.notchBorderOpacity ?? .1) * p));
        } else { delete root.dataset.notching; delete root.dataset.notchProgress; }
      }
      owner.current?.schedule(false);
    }} onAnimationComplete={() => {
      animating.current = false;
      const root = outer.current;
      if (root) { root.dataset.morphing = 'false'; if (pill && current.current.mode === 'still') arrive(root); }
      owner.current?.schedule(true);
      if (frost.current.waiting && current.current.mode !== 'still') {
        frost.current.waiting = false;
        const token = frost.current.token;
        owner.current?.whenApplied(() => requestAnimationFrame(() => requestAnimationFrame(() => frostIn(token))));
      }
    }}
    onMouseEnter={() => { if (!pill || current.current.mode !== 'still') void pointerIntent(true, dispatch).catch(fail); }}
    onMouseLeave={() => { void pointerIntent(false, dispatch).catch(fail); }}
    onKeyDown={event => {
      if (event.key === 'Escape' && !event.defaultPrevented && !(event.target as HTMLElement).closest('.terminal, input, select, textarea, [contenteditable]')) void dispatch({ type: 'collapse' }).catch(fail);
    }}><svg className="notch-morph-skin" aria-hidden="true"><path ref={notchPath} /></svg>{children}</motion.section>;
}
