// Pure core of the island-state store (SPEC §8 Phase 3, "Renderer ownership seam").
//
// The Rust state actor is the ONLY IslandState authority. This store:
//   * is seeded once from `island_bootstrap().state`;
//   * is advanced ONLY by `IslandStateChanged` events (`apply`), never by a dispatch reply;
//   * drops any state whose `rev` is not newer (wrapping u32 comparison);
//   * sends typed intents through `island_dispatch`, strictly one at a time and in call
//     order (async Tauri commands may otherwise reorder enter/leave pairs);
//   * relays the actor's `IslandCue` events.
// No Tauri/React import here, so `node --test` can exercise it with fakes.
import type { IslandState, UiIntent } from "./bindings";

export type { IslandState, UiIntent };
export type Cue = { kind: "click" };
export type Dispatcher = (intent: UiIntent) => Promise<unknown>;

/** Is `a` strictly older than `b` under u32 wrap-around? */
export const revBefore = (a: number, b: number): boolean => ((a - b) | 0) < 0;

export interface IslandStore {
  /** Seed from bootstrap. Never regresses a newer state that already arrived by event. */
  seed(state: IslandState): void;
  /** Apply an `IslandStateChanged` payload; returns whether it advanced the store. */
  apply(state: IslandState): boolean;
  /** Relay a cue event to subscribers. */
  cue(cue: Cue): void;
  getSnapshot(): IslandState;
  /** Throws until seeded: the renderer must never render a guessed state. */
  requireSnapshot(): IslandState;
  subscribe(listener: () => void): () => void;
  subscribeCue(listener: (cue: Cue) => void): () => void;
  /** Rejects with the backend's error, so callers can route it to their `fail`. */
  dispatch(intent: UiIntent): Promise<void>;
}

export function createIslandStore(send: Dispatcher): IslandStore {
  let state: IslandState | null = null;
  let tail: Promise<void> = Promise.resolve();
  const listeners = new Set<() => void>();
  const cueListeners = new Set<(cue: Cue) => void>();
  const advance = (next: IslandState): boolean => {
    if (state !== null && !revBefore(state.rev, next.rev)) return false;
    state = next;
    for (const listener of [...listeners]) listener();
    return true;
  };
  return {
    seed(next) {
      advance(next);
    },
    apply: advance,
    cue(cue) {
      for (const listener of [...cueListeners]) {
        try {
          listener(cue);
        } catch (error) {
          console.error(error);
        }
      }
    },
    getSnapshot() {
      return state as IslandState;
    },
    requireSnapshot() {
      if (state === null) throw new Error("island state is not seeded (bootstrap missing)");
      return state;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    subscribeCue(listener) {
      cueListeners.add(listener);
      return () => {
        cueListeners.delete(listener);
      };
    },
    dispatch(intent) {
      // Serialize: the next intent leaves only after the previous one settled (ok or error).
      const result = tail.then(() => send(intent)).then(() => undefined);
      tail = result.then(
        () => undefined,
        () => undefined,
      );
      return result;
    },
  };
}
