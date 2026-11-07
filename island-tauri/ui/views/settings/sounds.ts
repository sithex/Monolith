// Notification sound picker model (pure). Ids are freedesktop sound-theme names installed on
// this system (Rust `notification_sounds`); Rust `sound_id_ok` has the final word.

/** Good notification sounds, shown first (only the ones actually installed). */
export const FEATURED = ['message-new-instant', 'message', 'message-highlight', 'message-new-email', 'message-contact-in',
  'bell', 'complete', 'completion-success', 'outcome-success', 'dialog-information', 'window-attention', 'device-added', 'service-login'];
/** Not useful as a notification sound (test tones, channel checks, alarms that loop). */
const SKIP = /^(audio-channel-|audio-test-signal|theme-demo|phone-outgoing-|alarm-clock-elapsed$)/;

export const soundLabel = (id: string) => {
  const text = id.replace(/[-_.]+/g, ' ').trim();
  return text.charAt(0).toUpperCase() + text.slice(1);
};

/** Dropdown options: the "none chosen" row, installed featured sounds, then the rest A–Z. */
export function soundOptions(installed: readonly string[], emptyLabel: string): { value: string; label: string }[] {
  const have = new Set(installed.filter(id => !SKIP.test(id)));
  const featured = FEATURED.filter(id => have.has(id));
  const rest = [...have].filter(id => !featured.includes(id)).sort();
  return [{ value: '', label: emptyLabel }, ...[...featured, ...rest].map(id => ({ value: id, label: soundLabel(id) }))];
}
