import { useEffect, useState } from 'react';
import { useSettings, runtimeStore } from '../../stores';
import { useIslandState } from '../../ipc/island-state';
import { usePopups } from '../../ipc/notifications';
import type { Popup } from '../../ipc/client';

/** Popups visible right now (the same filter the Rust state machine applies). */
export function useShownPopups(): Popup[] {
  const settings = useSettings(), state = useIslandState(), snapshot = usePopups();
  const [fixture, setFixture] = useState<Popup[] | null>(null);
  useEffect(() => {
    if (!runtimeStore.getSnapshot().testHooks) return;
    Object.defineProperty(window, '__islandNotificationsTest', { configurable: true, value: (rows: Popup[] | null) => { fixtureRows = rows; fixtureListeners.forEach(l => l()); } });
    return () => { delete (window as unknown as Record<string, unknown>).__islandNotificationsTest; };
  }, []);
  useEffect(() => { const l = () => setFixture(fixtureRows); fixtureListeners.add(l); l(); return () => { fixtureListeners.delete(l); }; }, []);
  return settings.notificationsEnabled && !state.dnd && !state.gameMode ? (fixture ?? snapshot.items) : [];
}
let fixtureRows: Popup[] | null = null;
const fixtureListeners = new Set<() => void>();

/** Hover size while popup cards are shown inside the notch: the surface grows to the
 *  measured stack (IslandSurface reads this the way it reads the approval notice size). */
export const NOTCH_CARD_WIDTH = 420;
const TOPLINE = 42, BOTTOM_PAD = 12;
export const MAX_STACK = 3;
let measured = 0;
const sizeListeners = new Set<() => void>();
export const setMeasured = (h: number) => { if (Math.abs(measured - h) >= 1) { measured = h; sizeListeners.forEach(l => l()); } };
export function useNotificationHover(): { active: boolean; width: number; height: number } {
  const state = useIslandState(), shown = useShownPopups();
  const [, force] = useState(0);
  useEffect(() => { const l = () => force(n => n + 1); sizeListeners.add(l); return () => { sizeListeners.delete(l); }; }, []);
  const active = state.mode === 'hover' && state.hoverCause?.type !== 'alert' && shown.length > 0;
  const fallback = 150 * Math.min(MAX_STACK, shown.length);
  return { active, width: NOTCH_CARD_WIDTH, height: TOPLINE + (measured || fallback) + BOTTOM_PAD };
}

