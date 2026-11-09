// Safety windows use monotonic time, never the configurable animation speed.
export const CONFIRM_ARM_MS = 600;
export const CONFIRM_EXPIRE_MS = 4000;
export function confirmationPhase(startedAt: number | null, now: number): 'idle' | 'arming' | 'ready' {
  if (startedAt === null || !Number.isFinite(startedAt) || !Number.isFinite(now)) return 'idle';
  const elapsed = now - startedAt;
  if (elapsed < 0 || elapsed >= CONFIRM_EXPIRE_MS) return 'idle';
  return elapsed < CONFIRM_ARM_MS ? 'arming' : 'ready';
}
