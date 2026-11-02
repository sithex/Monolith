// ESM port of ui/legacy/click-sound.cjs (identical sound). The Vite dev server
// serves .cjs files untransformed, so the CommonJS module has no ES exports in dev.
//
// A running AudioContext keeps a live PipeWire sink input pumping silent 44.1 kHz PCM
// forever (~1000 wakeups/s across WebKit's webaudio threads). Suspend it shortly after the
// last blip and close it after a longer quiet spell; the next click resumes/recreates it.
export const SUSPEND_AFTER_MS = 1500;
export const CLOSE_AFTER_MS = 20_000;
export function createClickSound(makeContext: () => AudioContext = () => new globalThis.AudioContext(),
  timers: { set(fn: () => void, ms: number): unknown; clear(id: unknown): void } = {
    set: (fn, ms) => setTimeout(fn, ms), clear: id => clearTimeout(id as ReturnType<typeof setTimeout>) }) {
  let context: AudioContext | undefined;
  let suspendTimer: unknown, closeTimer: unknown;
  const idle = (ctx: AudioContext) => {
    if (suspendTimer !== undefined) timers.clear(suspendTimer);
    if (closeTimer !== undefined) timers.clear(closeTimer);
    suspendTimer = timers.set(() => { suspendTimer = undefined; if (ctx.state === 'running') void ctx.suspend().catch(() => undefined); }, SUSPEND_AFTER_MS);
    closeTimer = timers.set(() => {
      closeTimer = undefined;
      if (context === ctx) context = undefined;
      if (ctx.state !== 'closed') void ctx.close().catch(() => undefined);
    }, CLOSE_AFTER_MS);
  };
  return async (enabled: boolean) => {
    if (!enabled) return;
    try {
      if (!context || context.state === 'closed') context = makeContext();
      const ctx = context;
      if (ctx.state === 'suspended') await ctx.resume();
      const now = ctx.currentTime;
      const oscillator = ctx.createOscillator();
      const gain = ctx.createGain();
      oscillator.type = 'sine';
      oscillator.frequency.setValueAtTime(720, now);
      gain.gain.setValueAtTime(0.07, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.065);
      oscillator.connect(gain);
      gain.connect(ctx.destination);
      oscillator.start(now);
      oscillator.onended = () => { oscillator.disconnect(); gain.disconnect(); };
      oscillator.stop(now + 0.07);
      idle(ctx);
    } catch { /* Audio support or a device may be unavailable: UI stays usable. */ }
  };
}
