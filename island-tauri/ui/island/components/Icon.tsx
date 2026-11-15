import { useEffect, useRef } from 'react';
import type { IconName } from '../models';
import { prefersReducedMotion } from './motion-preference';
import { ACTIVE_GLYPHS, GLYPHS, type GlyphPath } from './icon-glyphs';

/**
 * Icons are Framework7 Icons glyphs (filled, iOS-style, 56×56 grid; icon-glyphs.ts). A lit
 * toggle swaps to the glyph's filled variant where one exists, the SF Symbols `.fill` way.
 *
 * Every icon is animated in three independent layers, so no two animations fight over one
 * transform (icon-motion.css has the CSS half):
 *   .icon-glyph appears (scale + fade) when it mounts: page open, button emerging;
 *   .icon-hover plays the icon's own hover gesture while its button is hovered (CSS);
 *   .icon-fx    plays state reactions (Web Animations): `active` turning on/off, and pings
 *               from elsewhere (`pingIcon('bell')` on a new notification).
 * Only transform and opacity move, so it stays on the compositor and costs next to nothing.
 * Lengths below are in glyph units (56 per icon).
 */
type Fx = 'on' | 'off' | 'ping';
const BOUNCE: Keyframe[] = [{ transform: 'scale(1)' }, { transform: 'scale(.8)', offset: .3 }, { transform: 'scale(1.14)', offset: .65 }, { transform: 'scale(1)' }];
const SETTLE: Keyframe[] = [{ transform: 'scale(1)' }, { transform: 'scale(.88)', offset: .4 }, { transform: 'scale(1)' }];
const WOBBLE: Keyframe[] = [0, 16, -13, 9, -5, 2, 0].map(r => ({ transform: `rotate(${r}deg)` }));
const SHAKE: Keyframe[] = [0, -5, 5, -3.5, 2, 0].map(x => ({ transform: `translateX(${x}px)` }));
const SPECIAL: Partial<Record<IconName, Partial<Record<Fx, Keyframe[]>>>> = {
  bell: { ping: WOBBLE, on: WOBBLE },
  moon: { on: [{ transform: 'rotate(0) scale(1)' }, { transform: 'rotate(-28deg) scale(.86)', offset: .4 }, { transform: 'rotate(8deg) scale(1.08)', offset: .75 }, { transform: 'rotate(0) scale(1)' }] },
  picker: { ping: [{ transform: 'translate(0,0) rotate(0)' }, { transform: 'translate(2px,-5px) rotate(-14deg)', offset: .35 }, { transform: 'translate(0,3.5px) rotate(4deg)', offset: .7 }, { transform: 'translate(0,0) rotate(0)' }] },
  mic: { on: SHAKE },
  // Caffeine: the cup gives a little hop, like it was just set down full.
  coffee: { on: [{ transform: 'translateY(0) rotate(0)' }, { transform: 'translateY(-5px) rotate(-6deg)', offset: .35 }, { transform: 'translateY(1px) rotate(2deg)', offset: .7 }, { transform: 'translateY(0) rotate(0)' }] },
  gamepad: { on: WOBBLE },
  cursor: { on: [{ transform: 'translate(0,0) scale(1)' }, { transform: 'translate(3.5px,3.5px) scale(.86)', offset: .35 }, { transform: 'translate(0,0) scale(1)' }] },
  power: { on: [{ transform: 'rotate(0)' }, { transform: 'rotate(360deg)' }] },
  play: { on: BOUNCE, off: SETTLE },
};
const DURATION: Record<Fx, number> = { on: 520, off: 320, ping: 640 };
function motionSpeed() {
  const v = typeof document === 'undefined' ? NaN : parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--motion-speed'));
  return v > 0 ? v : 1;
}
function play(el: SVGGElement | null, name: IconName, fx: Fx) {
  if (!el || typeof el.animate !== 'function' || prefersReducedMotion()) return;
  for (const a of el.getAnimations()) a.cancel();
  const frames = SPECIAL[name]?.[fx] ?? (fx === 'off' ? SETTLE : BOUNCE);
  el.animate(frames, { duration: DURATION[fx] / motionSpeed(), easing: 'cubic-bezier(.23,1,.32,1)' });
}
const PING = 'island-icon-ping';
/** Plays `name`'s ping reaction on every mounted icon of that name (e.g. 'bell' on a new notification). */
export function pingIcon(name: IconName) {
  if (typeof window !== 'undefined') window.dispatchEvent(new CustomEvent(PING, { detail: name }));
}

/** Exported for SectionIcon. Stroked paths are the hand-drawn glyphs (see icon-glyphs.ts). */
export const glyphPath = (p: GlyphPath, i: number) => typeof p === 'string'
  ? <path key={i} d={p} />
  : p.stroke ? <path key={i} d={p.d} transform={p.t} fill={p.fill ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth={p.stroke} strokeLinecap="round" strokeLinejoin="round" />
  : <path key={i} d={p.d} transform={p.t} fillRule={p.evenodd ? 'evenodd' : undefined} />;

export function Icon({ name, active }: { name: IconName; /** A toggle's state: each change plays the on/off reaction. */ active?: boolean }) {
  const fx = useRef<SVGGElement>(null), seen = useRef(active);
  useEffect(() => {
    if (seen.current === active) return;
    const was = seen.current; seen.current = active;
    if (was !== undefined && active !== undefined) play(fx.current, name, active ? 'on' : 'off');
  }, [active, name]);
  useEffect(() => {
    const ping = (e: Event) => { if ((e as CustomEvent).detail === name) play(fx.current, name, 'ping'); };
    window.addEventListener(PING, ping);
    return () => window.removeEventListener(PING, ping);
  }, [name]);
  const glyph = (active && ACTIVE_GLYPHS[name]) || GLYPHS[name];
  return <svg className="icon" data-icon={name} data-active={active} aria-hidden="true" viewBox="0 0 56 56" width="24" height="24" fill="currentColor">
    <g className="icon-hover"><g ref={fx} className="icon-fx"><g className="icon-glyph">{glyph.map(glyphPath)}</g></g></g></svg>;
}
