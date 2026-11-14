// Turns the short machine codes Rust services publish in `detail` into a sentence a person can act on.
// Pure and import-free: node tests load it standalone, and the coverage test in
// tests/phase7b-service-reasons.test.mjs fails when Rust emits a code that has no entry here.

export type ServiceReasonKind = 'loading' | 'off' | 'setup' | 'error';
export interface ServiceReason {
  kind: ServiceReasonKind;
  text: string;
  /** A button that jumps to the Settings row (`data-setting-id`) that fixes this state. */
  action?: { label: string; settingId?: string };
  /** Present only for codes this table does not know, so a new code stays diagnosable. */
  code?: string;
}
/** Which service the code came from, for codes whose meaning depends on it. */
export type ServiceReasonSource = 'weather' | 'downloads' | 'calendar' | 'scratchpad' | 'clipboard' | 'agents' | 'updates' | 'notifications';

type Entry = Omit<ServiceReason, 'code'>;
const loading = (text: string): Entry => ({ kind: 'loading', text });
const off = (text: string): Entry => ({ kind: 'off', text });
const setup = (text: string, action?: Entry['action']): Entry => action ? { kind: 'setup', text, action } : { kind: 'setup', text };
const error = (text: string): Entry => ({ kind: 'error', text });

/** Every non-healthy code a Rust service can publish. */
export const SERVICE_REASONS: Readonly<Record<string, Entry>> = {
  'starting': loading('Starting…'),
  'fetching': loading('Loading…'),
  'disabled': off('This is turned off.'),
  'idle': off('Nothing to show right now.'),
  'not-configured': setup('This hasn’t been set up yet.'),
  'no-location': setup('Choose a location to see the weather.', { label: 'Choose location', settingId: 'weatherLocation' }),
  'stopped': error('This service stopped. Restart Monolith to bring it back.'),
  'fetch-failed': error('Couldn’t fetch the latest data. It will try again.'),
  'no-owner': error('The desktop service this reads from isn’t running.'),
  'name-lost': error('Lost the connection to the desktop service this reads from.'),
  'bus-unavailable': error('Can’t reach the desktop message bus.'),
  'no-system-bus-configured': error('No system message bus is available.'),
  'no-session-bus-configured': error('No session message bus is available.'),
  'no-adapter': error('No Bluetooth adapter found.'),
  'unsupported-platform': error('Not supported on this system.'),
  'unsupported-system': error('Not supported on this system.'),
  'unsupported': error('Not supported on this system.'),
  'directory-unavailable': error('The folder can’t be opened. Check that it still exists.'),
  'watcher-unavailable': error('Can’t watch the folder for changes.'),
  'watch-failed': error('Stopped watching the folder for changes.'),
  'watch-error': error('Watching the folder for changes failed.'),
  'scan-failed': error('Couldn’t read the folder.'),
  'capture-ended': error('Audio capture stopped.'),
  'unreadable': error('Couldn’t read its data file.'),
  'save-failed': error('Couldn’t save your changes. The file on disk was left as it was.'),
  'local-load-failed-file-preserved': error('Couldn’t load the saved file. It was left untouched.'),
  'load-failed-file-preserved': error('Couldn’t load the saved file. It was left untouched.'),
  'no-private-system-bus': error('No system message bus is available.'),
  'pactl-unavailable': error('Can’t reach the sound server (pactl isn’t working).'),
  'checks-unavailable': error('Update checks aren’t working right now.'),
  'no-notification-owner': error('No notification service is running.'),
  'monitor-permission-denied': error('The desktop refused access to notifications.'),
  'monitor-state-poisoned': error('Notification watching hit an internal error. Restart Monolith.'),
  'tailscale-unavailable': error('Tailscale isn’t running or isn’t installed.'),
  'voice-runner-unavailable': error('Hermes voice tools aren’t installed.'),
  'no-fixture-database': error('No test database is configured.'),
  'start-failed': error('This failed to start. Restart Monolith to try again; the log says why.'),
  'ingest-unavailable': error('Hermes can’t send alerts to the island right now. Desktop notifications still work.'),
  'delivery-restarting': loading('Hit a problem and is restarting…'),
  'delivery-stopped': error('This stopped after repeated failures. Restart Monolith to bring it back.'),
};

/** Codes whose meaning depends on the service. Looked up as `<source>:<code>` before the table above. */
const BY_SOURCE: Readonly<Record<string, Entry>> = {
  'downloads:not-configured': setup('No Downloads folder found. Set one in your desktop’s user folders.'),
  'calendar:not-configured': setup('Connect a calendar link to see your events.'),
  'scratchpad:save-failed': error('Couldn’t save your notes. The file on disk was left as it was.'),
  'clipboard:disabled': off('Clipboard history is turned off.'),
  'weather:disabled': off('Weather is turned off.'),
  'notifications:start-failed': error('Notifications failed to start. Restart Monolith to try again.'),
  'notifications:delivery-restarting': loading('Notifications hit a problem and are restarting…'),
  'notifications:delivery-stopped': error('Notifications stopped after repeated failures. Restart Monolith to bring them back.'),
  'clipboard:start-failed': error('Clipboard history failed to start. Your saved history was left untouched.'),
  'clipboard:delivery-restarting': loading('Clipboard history hit a problem and is restarting…'),
  'clipboard:delivery-stopped': error('Clipboard history stopped after repeated failures. Restart Monolith to bring it back.'),
};

/** `detail` values that describe a working backend. They are never error text. */
export const HEALTHY_DETAILS: ReadonlySet<string> = new Set([
  'inotify', 'lrclib', 'pactl-subscribe', 'dbus', 'ready', 'fresh', 'monitoring', 'ext-data-control',
  'wlr-data-control', 'power-profiles-daemon', 'controls', 'saved-voices', 'read-only-status', 'owner-present',
  'read-only-checks', 'default-source', 'system-tiles',
]);

const CODE = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;
export const GENERIC_REASON = 'Something went wrong with this service.';

export function describeServiceReason(raw: string, source?: ServiceReasonSource): ServiceReason {
  const code = raw.trim();
  if (!code) return { kind: 'error', text: GENERIC_REASON };
  // Many services already publish a full sentence ("Timer module disabled"); show it as written.
  if (!CODE.test(code)) return { kind: 'error', text: code };
  const known = (source && BY_SOURCE[`${source}:${code}`]) || SERVICE_REASONS[code];
  if (known) return { ...known };
  // A healthy backend name with available=false means the service is between states.
  if (HEALTHY_DETAILS.has(code)) return { kind: 'loading', text: 'Getting ready…' };
  return { kind: 'error', text: GENERIC_REASON, code };
}
