/** Decide the gesture once it clears click slop; keep vertical scrolling out of paging. */
export function swipeAxis(dx: number, dy: number): 'pending' | 'x' | 'y' {
  if (Math.hypot(dx, dy) < 6) return 'pending';
  return Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
}

/** Positive direction advances a page. Velocity is the recent horizontal speed in px/ms. */
export function swipeDirection(dx: number, dy: number, width: number, velocity: number, page: number, count: number): -1 | 0 | 1 {
  if (!(width > 0) || swipeAxis(dx, dy) !== 'x') return 0;
  const flick = Math.abs(velocity) > 0.45;
  if (!flick && Math.abs(dx) <= width * 0.3) return 0;
  const direction = (flick ? velocity : dx) < 0 ? 1 : -1;
  return page + direction < 0 || page + direction >= count ? 0 : direction;
}

/** Track 1:1 between pages, but resist a pull beyond either end. */
export function swipeOffset(dx: number, page: number, count: number): number {
  return (dx > 0 && page <= 0) || (dx < 0 && page >= count - 1) ? dx * 0.3 : dx;
}

/** Sample once per tile, not per render or grid position. */
export function wiggleTiming(periodSample: number, phaseSample: number) {
  const period = 0.22 + periodSample * 0.06;
  return { '--wiggle-period': `${period}s`, '--wiggle-delay': `${-(0.001 + phaseSample * period)}s` };
}
