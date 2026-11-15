import { AnimatePresence, motion } from 'framer-motion';
import { DUR, tween } from '../motion-profile';

/** Odometer text: each character that changes rolls vertically (0.28 s; `fast` 0.15 s for values
 *  that change many times a second, like a dragged volume or a running click count). Visual only:
 *  callers keep the real value in an aria-label / sr-only text. */
export function RollingText({ text, reduced, fast = false }: { text: string; reduced: boolean; fast?: boolean }) {
  // Ambient (it can tick every second): never tied to the last input modality.
  const still = reduced;
  const chars = text.split('');
  return <span aria-hidden="true" className="rolling-text" style={{ display: 'inline-flex', overflow: 'hidden' }}>{chars.map((c, i) => {
    const slot = chars.length - i;  // keyed from the right so the ones keep their slot when a digit is added
    return <span key={slot} style={{ position: 'relative', display: 'inline-block' }}>
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span key={c} className="digit" initial={still ? { opacity: 0 } : { y: '-70%', opacity: 0 }} animate={still ? { opacity: 1 } : { y: '0%', opacity: 1 }}
          exit={still ? { opacity: 0 } : { y: '70%', opacity: 0 }} transition={tween(reduced ? DUR.press : fast ? DUR.fast : DUR.slow)}>{c === ' ' ? '\u00a0' : c}</motion.span>
      </AnimatePresence></span>;
  })}</span>;
}
