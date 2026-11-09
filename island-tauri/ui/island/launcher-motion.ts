/** Hysteresis before a drag picks an axis (~10 px): small wobbles stay clicks/scrolls. */
export const SWIPE_SLOP = 10;
/** Decide the gesture once it clears the slop; keep vertical scrolling out of paging. */
export function swipeAxis(dx: number, dy: number): 'pending' | 'x' | 'y' {
  if (Math.hypot(dx, dy) < SWIPE_SLOP) return 'pending';
  return Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
}

/** Where a release at `velocity` (px/ms) would coast to under exponential decay (`v·d/(1−d)`,
 *  d per ms). 0.99 is snappier than scroll momentum: sub-flick drifts add at most ~30 px. */
export function projectDistance(velocity: number, decelerationRate = 0.99): number {
  return velocity * decelerationRate / (1 - decelerationRate);
}

/** Speed (px/ms) above which the flick's direction alone decides. */
export const FLICK_VELOCITY = 0.3;

/** Positive direction advances a page. The flick's sign decides when fast (a flick back
 *  against the drag cancels); otherwise the projected landing point must pass half a page. */
export function swipeDirection(dx: number, dy: number, width: number, velocity: number, page: number, count: number): -1 | 0 | 1 {
  if (!(width > 0) || swipeAxis(dx, dy) !== 'x') return 0;
  let travel: number;
  if (Math.abs(velocity) >= FLICK_VELOCITY) {
    if (Math.abs(dx) > SWIPE_SLOP && Math.sign(velocity) !== Math.sign(dx)) return 0;
    travel = velocity;
  } else {
    travel = dx + projectDistance(velocity);
    if (Math.abs(travel) <= width * 0.5) return 0;
  }
  const direction = travel < 0 ? 1 : -1;
  return page + direction < 0 || page + direction >= count ? 0 : direction;
}

/** Progressive resistance past an edge: the further the pull, the less it follows. */
export function rubberband(overshoot: number, dimension: number, constant = 0.55): number {
  if (!(dimension > 0)) return 0;
  return (overshoot * dimension * constant) / (dimension + constant * Math.abs(overshoot));
}

/** Track 1:1 between pages; rubber-band a pull beyond the first or last page. */
export function swipeOffset(dx: number, page: number, count: number, width: number): number {
  return (dx > 0 && page <= 0) || (dx < 0 && page >= count - 1) ? rubberband(dx, width) : dx;
}

/** Sample once per tile, not per render or grid position. */
export function wiggleTiming(periodSample: number, phaseSample: number) {
  const period = 0.22 + periodSample * 0.06;
  return { '--wiggle-period': `${period}s`, '--wiggle-delay': `${-(0.001 + phaseSample * period)}s` };
}