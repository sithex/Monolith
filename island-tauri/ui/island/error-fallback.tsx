// What the island shows when its page cannot start or a render throws: a small card under the
// notch with one line of error text and a Reload button, instead of a transparent window that
// silently swallows clicks. The card reports its own box as the window's input/blur region, so
// it is visible and clickable even though the normal page never reported any geometry.
import { Component, useEffect, useRef, useState, type ErrorInfo, type ReactNode } from 'react';
import type { GeometryReport, IslandState } from '../ipc/bindings';
import { beforeFirstRender } from '../stores/topic';

/** The few IPC calls the card needs; injected so tests run without Tauri. */
export type FallbackIpc = {
  /** Collapse the island (the card is collapsed-size) and return the resulting state. */
  collapse(): Promise<IslandState>;
  report(report: GeometryReport): Promise<unknown>;
  onState(listener: (state: IslandState) => void): Promise<() => void>;
  reload(): void;
};

const MAX_LINE = 140;
/** One short line, never a stack or a multi-line dump. */
export function errorLine(error: unknown): string {
  const raw = error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  const line = raw.split('\n').map(part => part.trim()).find(Boolean) ?? '';
  if (!line) return 'Unknown error.';
  return line.length > MAX_LINE ? `${line.slice(0, MAX_LINE - 1)}…` : line;
}

/** Boot the page; any rejection or throw (load, seed or first render) shows the fallback. */
export async function startIsland<T>(load: () => Promise<T>, seed: (snapshot: T) => void, mount: () => void, fail: (error: unknown) => void) {
  try {
    await beforeFirstRender(load, seed, mount);
  } catch (error) {
    console.error('Island failed to start', error);
    fail(error);
  }
}

/** The card's geometry report for the actor's current state: the card twice, as the island
 * and as an extension, so collapsed (where only extensions take input) and open both cover it. */
export function fallbackReport(card: { x: number; y: number; width: number; height: number; radius: number }, state: Pick<IslandState, 'rev' | 'mode'>, scale: number): GeometryReport {
  const rect = { x: Math.max(0, card.x), y: Math.max(0, card.y), width: card.width, height: card.height, radius: Math.min(Math.max(0, card.radius), 100) };
  return { stateRev: state.rev, mode: state.mode, scale, settled: true, rects: [rect, rect] };
}

/** Keeps the window's input/blur region on the card until disposed. */
export function holdFallbackGeometry(card: HTMLElement, ipc: FallbackIpc): () => void {
  let disposed = false, state: IslandState | null = null, unlisten: (() => void) | null = null;
  const send = () => {
    if (disposed || !state) return;
    const box = card.getBoundingClientRect();
    if (box.width < 1 || box.height < 1) return;
    const radius = parseFloat(getComputedStyle(card).borderRadius) || 0;
    void ipc.report(fallbackReport({ x: box.x, y: box.y, width: box.width, height: box.height, radius }, state, window.devicePixelRatio || 1))
      .catch(error => console.error('Fallback geometry report failed', error));
  };
  const follow = (next: IslandState) => { state = next; send(); };
  void ipc.onState(follow).then(off => { if (disposed) off(); else unlisten = off; }).catch(() => undefined);
  void ipc.collapse().then(follow).catch(error => console.error('Fallback collapse failed', error));
  window.addEventListener('resize', send);
  window.addEventListener('island-native-pill', send);
  return () => {
    disposed = true; unlisten?.();
    window.removeEventListener('resize', send);
    window.removeEventListener('island-native-pill', send);
  };
}

export function FallbackCard({ error, ipc }: { error: unknown; ipc: FallbackIpc }) {
  const card = useRef<HTMLElement>(null);
  useEffect(() => card.current ? holdFallbackGeometry(card.current, ipc) : undefined, [ipc]);
  return <section ref={card} className="island-fallback" role="alert" data-island-fallback="">
    <p className="island-fallback-text"><strong>Monolith hit an error.</strong> {errorLine(error)}</p>
    <button type="button" className="island-fallback-reload" onClick={() => ipc.reload()}>Reload</button>
  </section>;
}

type BoundaryState = { failed: boolean; error: unknown };
/** Top-level boundary: a render that throws anywhere below swaps the island for the card. */
export class IslandErrorBoundary extends Component<{ ipc: FallbackIpc; children: ReactNode }, BoundaryState> {
  state: BoundaryState = { failed: false, error: null };
  static getDerivedStateFromError(error: unknown): BoundaryState {
    return { failed: true, error };
  }
  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error('Island render failed', error, info.componentStack);
  }
  render() {
    return this.state.failed ? <FallbackCard error={this.state.error} ipc={this.props.ipc}/> : this.props.children;
  }
}

/** Test instances only: `window.__islandTestCrash()` makes the next render throw. */
export function TestCrashSwitch({ enabled }: { enabled: boolean }) {
  const [crash, setCrash] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    Object.defineProperty(window, '__islandTestCrash', { configurable: true, value: () => setCrash(true) });
    return () => { delete (window as { __islandTestCrash?: unknown }).__islandTestCrash; };
  }, [enabled]);
  if (crash) throw new Error('Test-requested render failure');
  return null;
}

/** Test instances only: a one-shot bootstrap failure armed through sessionStorage. */
export function takeTestFault(storage: Pick<Storage, 'getItem' | 'removeItem'> | undefined, enabled: boolean): boolean {
  if (!enabled || !storage) return false;
  try {
    if (storage.getItem('island-test-fault') !== 'bootstrap') return false;
    storage.removeItem('island-test-fault');
    return true;
  } catch { return false; }
}
