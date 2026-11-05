import { useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { motion, useAnimationControls, useReducedMotion } from 'framer-motion';
import { useIslandState, dispatch } from '../ipc/island-state';
import { reportGeometry } from '../ipc/client';
import { useSettings, usePalette } from '../stores';
import { notchMorphPath } from './notch-profile';
import { useLayout, useLauncher } from '../stores/apps';
import { UI_SPRING } from './components/IslandShell';
import { createGeometryReporter, fitSurface, effectiveLayout, visibleLayout, nativePill, presetSize } from './geometry';
import { pointerIntent } from './navigation';
import { arrivalTime, axisSpring, motionSpeed, glassFadePlan, glassOpenness, GLASS_OPEN, handOffFade, surfaceRadius, WITHDRAW_SECONDS } from './motion-profile';
export { UI_SPRING };
/** The shell has handed the notch back: invisible, out of every morph rule. */
function arrive(root: HTMLElement) {
  root.style.opacity = '0'; root.dataset.arrived = 'true'; root.dataset.morphing = 'false';
}
export function IslandSurface({ children, fail }: { children: ReactNode; fail(error: unknown): void }) {
  const state = useIslandState(), settings = useSettings();
  // Settings → Motion → Reduce motion forces the same path as the OS preference.
  const reduced = useReducedMotion() === true || settings.reduceMotion === true;
  const layout = useLayout(), launcher = useLauncher(), pill = nativePill(), palette = usePalette();
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
  const [contentH, setContentH] = useState<number | null>(null);
  const fitsContent = state.mode === 'large' && state.view !== 'terminal';
  useLayoutEffect(() => {
    const root = outer.current;
    if (!fitsContent || !root) { setContentH(null); return; }
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
      const want = Math.ceil(chrome + parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom) + child.offsetHeight);
      setContentH(h => h !== null && Math.abs(h - want) < 2 ? h : want);
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(measure); };
    const mo = new MutationObserver(schedule);
    mo.observe(root.querySelector('.view-pane') ?? root, { childList: true, subtree: true, attributes: true, attributeFilter: ['hidden'] });
    schedule();
    return () => { ro.disconnect(); mo.disconnect(); if (frame) cancelAnimationFrame(frame); };
  }, [fitsContent, state.view]);
  useLayoutEffect(() => { owner.current?.schedule(!animating.current); }, [state.rev, state.mode, state.view, settings, layout.rev, launcher.rev, contentH]);
  const preset = layout.layout.preset;
  const { width: customWidth, maxHeight: customHeight } = presetSize(preset);
  const width = state.mode === 'still' ? 150 : state.mode === 'hover' ? 360 : !layout.unchangedSizing ? customWidth : state.view === 'terminal' ? 1040 : 640;
  const baseHeight = state.mode === 'still' ? 42 : state.mode === 'hover' ? 116 : state.view === 'terminal' ? 672 : state.view === 'monitor' ? 500 : state.view === 'ai' ? 740 : 640;
  // An explicit preset replaces the legacy per-app ceiling; otherwise increasing the
  // height slider never grows the terminal past 672px (or Settings past 640px).
  // Content-sized pages still shrink to their content below this ceiling.
  const maxHeight = state.mode === 'large' && !layout.unchangedSizing ? customHeight : baseHeight;
  const height = fitsContent && contentH !== null ? Math.min(maxHeight, Math.max(200, contentH)) : maxHeight;
  const fit = fitSurface(width, height, viewport, state.mode === 'large' ? visible : undefined);
  const limit = fitSurface(Infinity, Infinity, viewport, state.mode === 'large' ? visible : undefined);
  liveLayout.current = effectiveLayout(visible, fit.buttonScale);
  const targetW = state.mode === 'still' && pill ? pill.width : fit.width, targetH = state.mode === 'still' && pill ? pill.height : fit.height;
  const previousMode = useRef(state.mode);
  const morph = useMemo(() => {
    const drawn = outer.current?.getBoundingClientRect() ?? { width: targetW, height: targetH };
    const withdrawing = previousMode.current === 'large' && state.mode !== 'large'; previousMode.current = state.mode;
    const delay = !reduced && withdrawing ? WITHDRAW_SECONDS : 0;
    const width = { ...axisSpring(drawn.width, targetW, 'width'), delay };
    const height = { ...axisSpring(drawn.height, targetH, 'height'), delay };
    return { sideDelay: reduced ? 0 : arrivalTime(drawn, { width: targetW, height: targetH }), withdrawing,
      transition: reduced ? { duration: 0 } : { width, height, left: width, top: height, borderRadius: height, default: height } };
  }, [targetW, targetH, state.mode, reduced, motionSpeed()]);
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
  useLayoutEffect(() => {
    const root = outer.current;
    if (root) {
      const start = root.getBoundingClientRect().height;
      glassFade.current = glassFadePlan(start, targetH, state.mode !== 'still', glassNow.current);
      setGlass(root, glassOpenness(glassFade.current, start));
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
        const p = Math.max(0, Math.min(1, 1 - (h - pill.height) / 72));
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
    }}
    onMouseEnter={() => { if (!pill || current.current.mode !== 'still') void pointerIntent(true, dispatch).catch(fail); }}
    onMouseLeave={() => { void pointerIntent(false, dispatch).catch(fail); }}
    onKeyDown={event => {
      if (event.key === 'Escape' && !event.defaultPrevented && !(event.target as HTMLElement).closest('.terminal, input, select, textarea, [contenteditable]')) void dispatch({ type: 'collapse' }).catch(fail);
    }}><svg className="notch-morph-skin" aria-hidden="true"><path ref={notchPath} /></svg>{children}</motion.section>;
}
