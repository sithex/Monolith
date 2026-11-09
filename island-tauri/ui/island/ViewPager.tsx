import { useRef, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { dispatch, useIslandState } from '../ipc/island-state';
import { useSettings } from '../stores';
import { arrowDirection, canSwipe, nextView, visibleViews } from './navigation';
import type { IslandView } from '../ipc/client';
import { swipeAxis, swipeDirection, swipeOffset } from './launcher-motion';
import { prefersReducedMotion } from './components/motion-preference';

type Swipe = { x: number; y: number; id: number; axis: 'pending' | 'x' | 'y'; lastX: number; lastT: number; v: number };
export function ViewPager({ children, fail, views }: { children: ReactNode; fail(error: unknown): void; views?: readonly IslandView[] }) {
  const state = useIslandState(), settings = useSettings();
  const start = useRef<Swipe | null>(null), stage = useRef<HTMLDivElement>(null), suppressClick = useRef(false);
  const wheel = useRef({ accumulated: 0, until: 0 });
  const ids = views ?? visibleViews(settings);
  const swipe = settings.swipeNavigation !== false;
  const navigate = (direction: number) => {
    const view = nextView(state.view, direction, ids);
    if (view) void dispatch({ type: 'selectView', view }).catch(fail);
  };
  const index = Math.max(0, ids.indexOf(state.view));
  // The page follows the pointer 1:1 (rubber-banded at either end) and settles back with the
  // shared ease-out when the swipe doesn't commit. Direct DOM writes: no React render per move.
  const place = (offset: number, settle: boolean) => {
    const el = stage.current; if (!el) return;
    el.style.transition = settle ? 'transform var(--dur-slow) var(--ease-out)' : 'none';
    el.style.transform = offset ? `translateX(${offset}px)` : '';
  };
  const end = (event: ReactPointerEvent<HTMLDivElement>, cancelled: boolean) => {
    const s = start.current; start.current = null;
    if (!s || s.id !== event.pointerId) return;
    if (s.axis !== 'x') return;
    suppressClick.current = true;
    const dx = event.clientX - s.x, width = stage.current?.clientWidth ?? 0;
    const direction = cancelled ? 0 : swipeDirection(dx, event.clientY - s.y, width, s.v, index, ids.length);
    if (direction) { place(0, false); navigate(direction); } else place(0, !prefersReducedMotion());
  };
  return <div ref={stage} className="view-stage" onClickCapture={event => {
    if (suppressClick.current) { suppressClick.current = false; event.preventDefault(); event.stopPropagation(); }
  }} onKeyDown={event => {
    if (state.mode !== 'large' || event.defaultPrevented) return;
    const direction = arrowDirection(event.key, event.target as HTMLElement);
    if (direction) { event.preventDefault(); navigate(direction); }
  }} onPointerDown={event => {
    start.current = null; suppressClick.current = false;
    if (!swipe || state.mode !== 'large' || event.button !== 0 || !canSwipe(event.target as HTMLElement)) return;
    const now = performance.now();
    start.current = { x: event.clientX, y: event.clientY, id: event.pointerId, axis: 'pending', lastX: event.clientX, lastT: now, v: 0 };
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
    place(swipeOffset(dx, index, ids.length), false);
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
      navigate(wheel.current.accumulated > 0 ? 1 : -1);
      wheel.current = { accumulated: 0, until: now + 500 };
    }
  }}>{children}</div>;
}
