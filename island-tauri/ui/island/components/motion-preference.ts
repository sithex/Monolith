// One reduced-motion answer for the whole island: the OS preference OR Settings → Motion →
// Reduce motion (Island.tsx mirrors that setting onto <html data-reduce-motion>). Hooks,
// imperative animate() calls and plain helpers (geometry.ts) all read the same source, so the
// in-app toggle reaches everything the OS preference reaches.
import { useSyncExternalStore } from 'react';
const query = '(prefers-reduced-motion: reduce)';

/** Non-hook form, for imperative code paths (glides, animate() calls). */
export function prefersReducedMotion(doc: Document | undefined = typeof document === 'undefined' ? undefined : document): boolean {
  const setting = doc?.documentElement?.dataset?.reduceMotion === 'true';
  const os = typeof matchMedia === 'function' && matchMedia(query).matches;
  return setting || os;
}

const subscribeMotion = (notify: () => void) => {
  const media = typeof matchMedia === 'function' ? matchMedia(query) : null;
  media?.addEventListener('change', notify);
  const root = typeof document === 'undefined' ? null : document.documentElement;
  const observer = root && typeof MutationObserver === 'function' ? new MutationObserver(notify) : null;
  if (root && observer) observer.observe(root, { attributes: true, attributeFilter: ['data-reduce-motion'] });
  return () => { media?.removeEventListener('change', notify); observer?.disconnect(); };
};

/** OS preference || in-app Reduce motion setting. */
export function useReducedMotionAll(): boolean {
  return useSyncExternalStore(subscribeMotion, () => prefersReducedMotion(), () => true);
}
/** Kept for existing callers; now also honours the in-app setting. */
export const useReducedMotionPreference = useReducedMotionAll;
