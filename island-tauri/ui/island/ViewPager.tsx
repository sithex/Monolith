import { useRef, type ReactNode } from 'react';
import { dispatch, useIslandState } from '../ipc/island-state';
import { useSettings } from '../stores';
import { arrowDirection, canSwipe, nextView, visibleViews } from './navigation';
import type { IslandView } from '../ipc/client';
export function ViewPager({ children, fail, views }: { children: ReactNode; fail(error: unknown): void; views?: readonly IslandView[] }) {
  const state = useIslandState(), settings = useSettings();
  const start = useRef<{ x: number; y: number } | null>(null);
  const wheel = useRef({ accumulated: 0, until: 0 });
  const ids = views ?? visibleViews(settings);
  const swipe = settings.swipeNavigation !== false;
  const navigate = (direction: number) => {
    const view = nextView(state.view, direction, ids);
    if (view) void dispatch({ type: 'selectView', view }).catch(fail);
  };
  return <div className="view-stage" onKeyDown={event => {
    if (state.mode !== 'large' || event.defaultPrevented) return;
    const direction = arrowDirection(event.key, event.target as HTMLElement);
    if (direction) { event.preventDefault(); navigate(direction); }
  }} onPointerDown={event => {
    start.current = null;
    if (!swipe || state.mode !== 'large' || event.button !== 0 || !canSwipe(event.target as HTMLElement)) return;
    start.current = { x: event.clientX, y: event.clientY };
  }} onPointerCancel={() => { start.current = null; }} onPointerUp={event => {
    const origin = start.current; start.current = null;
    if (!origin || Math.abs(event.clientY - origin.y) > Math.abs(event.clientX - origin.x)) return;
    const delta = event.clientX - origin.x;
    if (Math.abs(delta) >= 60) { event.preventDefault(); navigate(delta < 0 ? 1 : -1); }
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
