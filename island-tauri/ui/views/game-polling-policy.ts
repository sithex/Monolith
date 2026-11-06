/** Pause periodic work, not explicit user actions. Effective game state is backend-owned. */
export function gamePollingActive(active: boolean, gameMode: boolean, pausePollers: boolean): boolean {
  return active && !(gameMode && pausePollers);
}
