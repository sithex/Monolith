export const WITHDRAW_SECONDS = 0.16;

/**
 * Global animation speed (Settings → Motion → Animation speed, Nov 5): 1 = as designed,
 * 2 = twice as fast, .5 = half speed. The island root sets it from `animationSpeed` (50..200 %)
 * before children render; every spring/duration built here divides its time by it, so the
 * shell morph, `arrivalTime` (side-button hand-off) and the content reveal stay in step.
 * WITHDRAW_SECONDS and BLUR_RELEASE_MS are deliberately NOT scaled: they pair with CSS
 * (`.island[data-withdrawing]` .16s) and the KWin blur region, not with spring length.
 */
let speed = 1;
export function setMotionSpeed(percent: number): void {
  speed = Number.isFinite(percent) ? Math.min(200, Math.max(50, percent)) / 100 : 1;
}
export const motionSpeed = () => speed;
/** A designed duration (seconds or ms) at the current speed. */
export const scaled = (time: number) => time / speed;
/** A UI spring (framer `visualDuration` form) at the current speed. */
export const uiSpring = (visualDuration: number, bounce: number) => ({ type: 'spring' as const, visualDuration: visualDuration / speed, bounce });
type Axis = 'width' | 'height';
type Size = { width: number; height: number };
export type PhysicalSpring = { type: 'spring'; mass: number; stiffness: number; damping: number; restDelta: number; restSpeed: number };

export function axisSpring(from: number, to: number, axis: Axis): PhysicalSpring {
  const growing = to > from;
  const duration = (axis === 'width' ? (growing ? .44 : .30) : (growing ? .38 : .26)) / speed;
  let ratio = growing ? (axis === 'width' ? .75 : .78) : 1;
  const travel = Math.abs(to - from);
  if (growing && travel > 12) {
    const log = Math.log(12 / travel);
    ratio = Math.max(ratio, -log / Math.sqrt(Math.PI ** 2 + log ** 2));
  }
  const omega = 2 * Math.PI / duration;
  return { type: 'spring', mass: 1, stiffness: omega ** 2, damping: 2 * ratio * omega, restDelta: .25, restSpeed: .5 };
}

export function springProgress(spring: PhysicalSpring, seconds: number): number {
  if (seconds <= 0) return 0;
  const w = Math.sqrt(spring.stiffness / spring.mass), z = spring.damping / (2 * Math.sqrt(spring.stiffness * spring.mass));
  const decay = Math.exp(-z * w * seconds);
  if (z >= 1) return 1 - decay * (1 + w * seconds);
  const wd = w * Math.sqrt(1 - z * z);
  return 1 - decay * (Math.cos(wd * seconds) + z * w / wd * Math.sin(wd * seconds));
}

export function arrivalTime(from: Size, to: Size): number {
  const axes = (['width', 'height'] as const).filter(axis => from[axis] !== to[axis]);
  if (!axes.length) return 0;
  const springs = axes.map(axis => axisSpring(from[axis], to[axis], axis));
  for (let frame = 1; frame <= 480; frame++) {
    const time = frame / 240;
    if (springs.every(spring => springProgress(spring, time) >= .99)) return time;
  }
  return 2;
}

export function sidePose(side: string, diameter: number, gap: number, progress: number) {
  const p = Math.min(1.1, Math.max(0, progress));
  const inset = (1 - p) * (14 + gap + diameter / 2);
  return { x: (side === 'left' ? inset : side === 'right' ? -inset : 0) || 0,
    y: (side === 'bottom' ? -inset : 0) || 0, scale: .4 + .6 * p, opacity: Math.min(1, p) };
}

export function sideDropPath(side: string, diameter: number, gap: number, progress: number): string {
  if (progress <= .001 || progress >= .84) return '';
  const pose = sidePose(side, diameter, gap, progress);
  const r = diameter / 2 * pose.scale, cx = diameter / 2 + pose.x, cy = diameter / 2 + pose.y;
  const direction = side === 'left' ? { x: 1, y: 0 } : side === 'right' ? { x: -1, y: 0 } : { x: 0, y: -1 };
  const edge = side === 'bottom' ? cy + gap : side === 'left' ? diameter + gap - cx : cx + gap;
  if (edge <= r) return '';
  const neck = r * Math.max(0, 1 - progress / .84);
  const point = (along: number, across: number) => `${cx + direction.x * along - direction.y * across},${cy + direction.y * along + direction.x * across}`;
  const middle = (r + edge) / 2;
  return `M${point(r * .6, r * .5)} Q${point(middle, neck)} ${point(edge + 1, neck)} L${point(edge + 1, -neck)} Q${point(middle, -neck)} ${point(r * .6, -r * .5)} Z`;
}

export const GLASS_STRETCH = 48;
export type GlassFade = { solidHeight: number; range: number };
export const GLASS_OPEN: GlassFade = { solidHeight: 0, range: 1 };
export function glassOpenness(fade: GlassFade, height: number): number {
  if (!Number.isFinite(height) || !(fade.range > 0)) return 1;
  return Math.min(1, Math.max(0, (height - fade.solidHeight) / fade.range));
}
export function glassFadePlan(start: number, end: number, endsInGlass: boolean, current: number): GlassFade {
  if (!Number.isFinite(start) || !Number.isFinite(end)) return GLASS_OPEN;
  const now = Math.min(1, Math.max(0, Number.isFinite(current) ? current : 1));
  const travel = Math.abs(end - start);
  if (endsInGlass) {
    if (!(end > start) || now >= 1) return GLASS_OPEN;
    if (now === 0) { const range = Math.min(GLASS_STRETCH, travel); return { solidHeight: end - range, range: Math.max(1, range) }; }
    const range = travel / (1 - now);
    return { solidHeight: start - now * range, range: Math.max(1, range) };
  }
  return { solidHeight: Math.max(start, end), range: 1 };
}

/** Shell opacity during the hand-off to the native notch: 1 until the shell is within 14 px
 *  of the notch height AND 24 px of its width, then fading to 0 exactly at the notch box.
 *  Gating on both axes keeps the fade inside the (opaque) notch's footprint. */
export const HAND_OFF_HEIGHT = 14, HAND_OFF_WIDTH = 24;
export function handOffFade(width: number, height: number, pill: { width: number; height: number }): number {
  const byHeight = (height - pill.height) / HAND_OFF_HEIGHT, byWidth = (width - pill.width) / HAND_OFF_WIDTH;
  return Math.max(0, Math.min(1, Math.max(byHeight, byWidth)));
}

export function surfaceRadius(height: number): number {
  return Number.isFinite(height) ? Math.min(28, Math.max(0, height) * .34) : 0;
}
