import { useEffect, useMemo, useRef, type InputHTMLAttributes } from 'react';
import { motion } from 'framer-motion';
import { tween } from '../motion-profile';
import { useReducedMotionAll } from './motion-preference';

/** Native checkbox semantics, with change-only motion. CSS is loaded by Island.tsx so
 * the production graph remains readable by the shared non-browser test loader. */
export function Switch({ checked = false, size = 'default', disabled, ...input }: Omit<InputHTMLAttributes<HTMLInputElement>, 'size' | 'type' | 'role'> & { size?: 'default' | 'sm' }) {
  const reduce = useReducedMotionAll();
  const instant = reduce || (typeof document !== 'undefined' && document.documentElement.dataset.navVia === 'key');
  const previous = useRef(checked);
  const travel = size === 'sm' ? 14 : 20;
  const pose = (on: boolean, x = 1, y = 1) => `translateX(${on ? travel : 0}px) scale(${x}, ${y})`;
  const transform = useMemo(() => {
    const from = previous.current;
    // Scale a fixed-size circle: 20 → 28 → 20 wide, 20 → 14 → 20 high.
    return from !== checked && !instant ? [pose(from), `translateX(${travel / 2}px) scale(1.4, .7)`, pose(checked)] : pose(checked);
  }, [checked, instant, travel]);
  // Keep render/memo pure: StrictMode may evaluate a memo twice before committing.
  useEffect(() => { previous.current = checked; }, [checked]);
  return <label className="switch-track" data-size={size} data-checked={checked} data-disabled={disabled || undefined}>
    <input {...input} className={`switch${input.className ? ` ${input.className}` : ''}`} type="checkbox" role="switch" checked={checked} disabled={disabled} />
    <motion.span className="switch-knob" aria-hidden="true" initial={false}
      animate={{ transform }} transition={instant ? { duration: 0 } : tween(.26)} />
  </label>;
}
