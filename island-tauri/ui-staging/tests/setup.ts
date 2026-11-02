import { afterEach, beforeEach } from 'vitest';
import { cleanup } from '@testing-library/react';
export let reducedMotion = false;
export const setReducedMotion = (value: boolean) => { reducedMotion = value; };
Object.defineProperty(window, 'matchMedia', { writable: true, value: (query: string) => ({ matches: query.includes('prefers-reduced-motion') && reducedMotion, media: query, onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return true; } }) });
// jsdom has no layout or PointerEvent; preserve button/coordinates for hold tests.
class TestPointerEvent extends MouseEvent { pointerId: number; pointerType: string; isPrimary: boolean;
  constructor(type: string, options: PointerEventInit = {}) { super(type, options); this.pointerId = options.pointerId ?? 1; this.pointerType = options.pointerType ?? 'mouse'; this.isPrimary = true; }
}
Object.defineProperty(window, 'PointerEvent', { value: TestPointerEvent });
beforeEach(() => { reducedMotion = false; });
afterEach(cleanup);
