// The renderer's view of the Rust state actor (SPEC §8 Phase 3). See island-state-core.ts for
// the store semantics; this file wires it to the generated bindings and to React.
import { useSyncExternalStore } from "react";
import { createIslandStore, type Cue, type IslandState, type UiIntent } from "./island-state-core";
import { islandDispatch, onIslandCue, onIslandStateChanged } from "./client";

export type { Cue, IslandState, UiIntent };

const store = createIslandStore(islandDispatch);

/**
 * Step 1 of 2, BEFORE `island_bootstrap`: subscribe to the actor's events so nothing that
 * happens between the bootstrap read and the subscription can be lost. Events that arrive
 * before the seed are applied too; the rev guard keeps whichever state is newest.
 */
export async function listenIslandState() {
  await onIslandStateChanged((state) => store.apply(state));
  await onIslandCue((cue) => store.cue(cue));
}

/** Step 2 of 2: seed from `island_bootstrap().state` (never regresses a newer event). */
export const seedIslandState = (state: IslandState): void => store.seed(state);

/** The actor's state; throws if used before the shim seeded the store. */
export function useIslandState(): IslandState {
  useSyncExternalStore(store.subscribe, store.getSnapshot);
  return store.requireSnapshot();
}

/** The actor's current state outside React (side actions); throws before the seed. */
export const currentIslandState = (): IslandState => store.requireSnapshot();

/** Send a typed intent to the actor. Rejects with the backend error (route it to `fail`). */
export const dispatch = (intent: UiIntent): Promise<void> => store.dispatch(intent);

/** Subscribe to the actor's click cue (the renderer plays it through click-sound.cjs). */
export const subscribeCue = (listener: (cue: Cue) => void): (() => void) =>
  store.subscribeCue(listener);
