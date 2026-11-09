import { useLayoutEffect, useRef, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { animate } from 'framer-motion';
import { dispatch, useIslandState } from '../ipc/island-state';
import { useSettings } from '../stores';
import { arrowDirection, canSwipe, nextView, visibleViews } from './navigation';
import type { IslandView } from '../ipc/client';
import { swipeAxis, swipeDirection, swipeOffset } from './launcher-motion';
import { uiSpring } from './motion-profile';
import { prefersReducedMotion } from './components/motion-preference';

type Swipe = { x: number; y: number; id: number; axis: 'pending' | 'x' | 'y'; base: number; lastX: number; lastT: number; v: number };
/** One page change in flight. `pos` runs from the release offset to −2·dir·width: the old page
 *  leaves over the first width; once the island has switched view the new page is drawn at
 *  pos + 2·dir·width, so it enters from the far edge on the same spring. One continuous motion
 *  seeded with the finger's velocity: no seam between drag, exit and entry. */
type Flight = { dir: -1 | 1; width: number; from: IslandView; navigated: boolean; swapped: boolean; done: boolean; stop(): void };

export function ViewPager({ children, fail, views }: { children: ReactNode; fail(error: unknown): void; views?: readonly IslandView[] }) {
  const state = useIslandState(), settings = useSettings();
  const start = useRef<Swipe | null>(null), stage = useRef<HTMLDivElement>(null), suppressClick = useRef(false);
  const pos = useRef(0), flight = useRef<Flight | null>(null), settle = useRef<{ stop(): void } | null>(null);
  const wheel = useRef({ accumulated: 0, until: 0 });
  const ids = views ?? visibleViews(settings);
  const swipe = settings.swipeNavigation !== false;
  const navigate = (direction: number) => {
    const view = nextView(state.view, direction, ids);
    if (view) void dispatch({ type: 'selectView', view }).catch(fail);
  };
  const index = Math.max(0, ids.indexOf(state.view));

  // Direct transform writes: no React render per pointer move or animation frame.
  const draw = () => {
    const el = stage.current; if (!el) return;
    const f = flight.current, shown = f?.swapped ? pos.current + 2 * f.dir * f.width : pos.current;
    el.style.transform = Math.abs(shown) < .5 ? '' : `translateX(${shown}px)`;
    // While a swipe owns the stage, the CSS page-in keyframe must not fight the slide. The flag
    // stays set after landing and clears with the next view change (so the keyframe can't replay).
    if (Math.abs(shown) >= .5 || f) el.dataset.swiping = 'true';
  };
  const set = (value: number) => { pos.current = value; draw(); };
  const reset = () => { settle.current?.stop(); settle.current = null; flight.current?.stop(); flight.current = null; set(0); };

  useLayoutEffect(() => {
    const f = flight.current, el = stage.current;
    // A view change the swipe didn't cause: give the page-in fade back.
    if (!f) { if (el && !start.current && !settle.current) delete el.dataset.swiping; return; }
    // The island switched view: hand the slide over to the incoming page.
    if (!f.navigated || f.swapped || state.view === f.from) return;
    f.swapped = true;
    if (f.done) { flight.current = null; set(0); } else draw();
  }, [state.view]);
  useLayoutEffect(() => { if (state.mode !== 'large') reset(); }, [state.mode]);
  useLayoutEffect(() => () => { settle.current?.stop(); flight.current?.stop(); }, []);

  const springBack = (from: number, velocity: number) => {
    if (prefersReducedMotion()) { set(0); return; }
    // A missed swipe returns quickly from the finger's speed, without overshooting.
    settle.current = animate(from, 0, { ...uiSpring(.25, 0), velocity: velocity * 1000, onUpdate: set,
      onComplete: () => { settle.current = null; set(0); } });
  };
  const commit = (dir: -1 | 1, from: number, velocity: number, width: number) => {
    if (prefersReducedMotion() || !(width > 0)) { set(0); navigate(dir); return; }
    pos.current = from;
    const f: Flight = { dir, width, from: state.view, navigated: false, swapped: false, done: false, stop: () => {} };
    flight.current = f;
    const controls = animate(from, -2 * dir * width, {
      ...uiSpring(.42, 0), velocity: velocity * 1000,
      onUpdate: value => {
        if (flight.current !== f) return;
        set(value);
        if (!f.navigated && value * dir <= -width) { f.navigated = true; navigate(dir); }
      },
      onComplete: () => {
        if (flight.current !== f) return;
        f.done = true;
        if (!f.navigated) { f.navigated = true; navigate(dir); }
        if (f.swapped) { flight.current = null; set(0); return; }
        // The switch hasn't landed (slow or failed IPC): never strand the island on a blank stage.
        setTimeout(() => { if (flight.current === f && !f.swapped) { flight.current = null; set(0); } }, 400);
      },
    });
    f.stop = () => controls.stop();
  };

  const end = (event: ReactPointerEvent<HTMLDivElement>, cancelled: boolean) => {
    const s = start.current; start.current = null;
    if (!s || s.id !== event.pointerId) return;
    if (s.axis !== 'x' && s.base === 0) return;
    if (s.axis === 'x') suppressClick.current = true;
    const width = stage.current?.clientWidth ?? 0;
    const raw = s.base + (s.axis === 'x' ? event.clientX - s.x : 0);
    const shown = swipeOffset(raw, index, ids.length, width);
    const dir = cancelled ? 0 : swipeDirection(raw, 0, width, s.v, index, ids.length);
    if (dir) commit(dir, shown, s.v, width); else springBack(shown, s.v);
  };

  return <div ref={stage} className="view-stage" onClickCapture={event => {
    if (suppressClick.current) { suppressClick.current = false; event.preventDefault(); event.stopPropagation(); }
  }} onKeyDown={event => {
    if (state.mode !== 'large' || event.defaultPrevented) return;
    const direction = arrowDirection(event.key, event.target as HTMLElement);
    if (direction) { event.preventDefault(); reset(); navigate(direction); }
  }} onPointerDown={event => {
    start.current = null; suppressClick.current = false;
    if (!swipe || state.mode !== 'large' || event.button !== 0 || !canSwipe(event.target as HTMLElement)) return;
    // Catch a page mid-flight: stop its spring and continue from where it is on screen.
    let base = 0;
    const f = flight.current;
    if (f) {
      if (f.navigated && !f.swapped) return; // the view switch is landing; let it finish
      f.stop(); flight.current = null;
      base = f.swapped ? pos.current + 2 * f.dir * f.width : pos.current;
      set(base);
    } else if (settle.current) { settle.current.stop(); settle.current = null; base = pos.current; }
    if (Math.abs(base) < .5) base = 0;
    const now = performance.now();
    start.current = { x: event.clientX, y: event.clientY, id: event.pointerId, axis: base ? 'x' : 'pending', base, lastX: event.clientX, lastT: now, v: 0 };
    if (base) event.currentTarget.setPointerCapture(event.pointerId);
  }} onPointerMove={event => {
    const s = start.current; if (!s || s.id !== event.pointerId || s.axis === 'y') return;
    const dx = event.clientX - s.x, dy = event.clientY - s.y;
    if (s.axis === 'pending') {
      s.axis = swipeAxis(dx, dy);
      if (s.axis !== 'x') return;
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    const now = performance.now(), dt = now - s.lastT;
    if (dt > 0) s.v = .7 * ((event.clientX - s.lastX) / dt) + .3 * s.v;
    s.lastX = event.clientX; s.lastT = now;
    // 1:1 with the pointer while tracking: no easing (rubber-banded only past either end).
    set(swipeOffset(s.base + dx, index, ids.length, stage.current?.clientWidth ?? 0));
  }} onPointerCancel={event => end(event, true)} onLostPointerCapture={event => end(event, true)} onPointerUp={event => {
    // A pause before release is not a flick.
    if (start.current && performance.now() - start.current.lastT > 80) start.current.v = 0;
    end(event, false);
  }} onWheel={event => {
    if (!swipe || state.mode !== 'large' || (event.target as HTMLElement).closest('.terminal, input, select, textarea, .wheel, .fader, .app-grid[data-editing="true"]') || Math.abs(event.deltaY) > Math.abs(event.deltaX)) return;
    const now = performance.now();
    if (now < wheel.current.until) return;
    wheel.current.accumulated += event.deltaX * (event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 800 : 1);
    if (Math.abs(wheel.current.accumulated) >= 60) {
      reset();
      navigate(wheel.current.accumulated > 0 ? 1 : -1);
      wheel.current = { accumulated: 0, until: now + 500 };
    }
  }}>{children}</div>;
}
