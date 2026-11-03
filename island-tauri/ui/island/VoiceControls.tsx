import { useEffect, useMemo, useRef, useState } from 'react';
import { voicePanel, type VoicePanel, type VoicePanelOp } from '../ipc/client';
import { createVisiblePoller } from '../views/visible-poller';
import { Dropdown } from './components/AppPrimitives';
import { Icon } from './components/Icon';

/**
 * Hermes page voice controls (title row, right side): a saved-voice picker driven by
 * `hermes-fish` and the TTS mute switch driven by `tts_mute.py`. Each control only appears
 * when its helper exists, so other installs simply see a plain Hermes page.
 * Setup for your own Hermes: docs/voice-controls.md.
 */
// Stays mounted on every page (keeps its last state) but only draws on the Hermes page.
export function VoiceControls({ shown, active, fail }: { shown: boolean; active: boolean; fail(error: unknown): void }) {
  const [panel, setPanel] = useState<VoicePanel | null>(null), [busy, setBusy] = useState(false);
  // A refresh that started before a click must not overwrite the click's result.
  const epoch = useRef(0);
  // Re-read every 10 s while the Hermes page is visible: voices or mute may change from the
  // terminal (`hermes-fish <name>`) or a panel button.
  const poller = useMemo(() => createVisiblePoller(async () => {
    const at = epoch.current; return { at, value: await voicePanel({ type: 'refresh' }) };
  }, ({ at, value }) => { if (at === epoch.current) setPanel(value); },
  () => { /* helpers are optional: a failed refresh keeps the last state */ }, cb => setTimeout(cb, 10000)), []);
  useEffect(() => { poller.setVisible(active); }, [active, poller]);
  useEffect(() => () => poller.dispose(), [poller]);
  const run = (op: VoicePanelOp) => {
    setBusy(true); epoch.current++;
    void voicePanel(op).then(setPanel).catch(fail).finally(() => setBusy(false));
  };
  if (!shown || !panel || (!panel.voicesAvailable && !panel.muteAvailable)) return null;
  return <div className="voice-controls" data-testid="voice-controls">
    {panel.voicesAvailable && panel.choices.length > 0 && <Dropdown label="Hermes voice" value={panel.active} placeholder="Voice"
      disabled={busy} options={panel.choices.map(name => ({ value: name, label: name }))} onChange={name => run({ type: 'select', name })} />}
    {panel.muteAvailable && <button type="button" className="icon-button voice-mute" data-muted={panel.muted} aria-pressed={panel.muted} disabled={busy}
      aria-label={panel.muted ? 'Unmute Hermes voice' : 'Mute Hermes voice'} title={panel.muted ? 'Voice muted' : 'Mute voice'}
      onClick={() => run({ type: 'toggleMute' })}><Icon name={panel.muted ? 'volumeOff' : 'volume'} /></button>}
  </div>;
}
