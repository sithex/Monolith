// A tiny local feedback sound; no files, network, voice service or autoplay.
function createClickSound(makeContext = () => new globalThis.AudioContext()) {
  let context;
  return async (enabled) => {
    if (!enabled) return;
    try {
      if (!context || context.state === 'closed') context = makeContext();
      if (context.state === 'suspended') await context.resume();
      const now = context.currentTime;
      const oscillator = context.createOscillator();
      const gain = context.createGain();
      oscillator.type = 'sine';
      oscillator.frequency.setValueAtTime(720, now);
      // A short but discernible UI tick, independent of system/TTS volume.
      gain.gain.setValueAtTime(0.07, now);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.065);
      oscillator.connect(gain);
      gain.connect(context.destination);
      oscillator.start(now);
      oscillator.onended = () => { oscillator.disconnect?.(); gain.disconnect?.(); };
      oscillator.stop(now + 0.07);
    } catch { /* Audio support or a device may be unavailable: UI stays usable. */ }
  };
}
module.exports = { createClickSound };
