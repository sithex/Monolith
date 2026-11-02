// Ported from the accepted workshop AppGrid. Shared by grid and spectrum.
import { useSyncExternalStore } from 'react';
const query = '(prefers-reduced-motion: reduce)';
const subscribeMotion = (notify: () => void) => {
  const media = window.matchMedia(query); media.addEventListener('change', notify);
  return () => media.removeEventListener('change', notify);
};
export function useReducedMotionPreference() {
  return useSyncExternalStore(subscribeMotion, () => window.matchMedia(query).matches, () => true);
}
