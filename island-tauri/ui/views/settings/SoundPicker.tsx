import { useEffect, useState } from 'react';
import { commands } from '../../ipc/bindings';
import { Dropdown } from '../../island/components/AppPrimitives';
import { soundOptions } from './sounds';

let installed: Promise<string[]> | null = null;
const loadSounds = () => installed ??= commands.notificationSounds().then(r => r.status === 'ok' ? r.data : []).catch(() => []);
const preview = (id: string) => { void commands.notificationSoundPreview(id).catch(() => undefined); };

/** Sound dropdown with a play button; picking a sound also plays it once so it can be heard. */
export function SoundPicker({ label, value, emptyLabel, previewEmpty = '', disabled = false, onChange }: {
  label: string; value: string; emptyLabel: string; previewEmpty?: string; disabled?: boolean; onChange(id: string): void;
}) {
  const [ids, setIds] = useState<string[]>([]);
  useEffect(() => { let live = true; void loadSounds().then(v => { if (live) setIds(v); }); return () => { live = false; }; }, []);
  return <div className="sound-picker">
    <Dropdown label={label} value={value} disabled={disabled} options={soundOptions(ids, emptyLabel)}
      onChange={id => { preview(id || previewEmpty); onChange(id); }} />
    <button type="button" className="icon-button sound-preview" aria-label={`Play ${label.toLowerCase()}`} title="Play" disabled={disabled}
      onClick={() => preview(value || previewEmpty)}>
      <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.6-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5Z" /></svg>
    </button>
  </div>;
}
