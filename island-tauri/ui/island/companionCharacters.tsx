import type { ReactElement } from 'react';
import type { CompanionCharacter } from '../ipc/bindings';

// The notch companion's characters, in picker order. Adding one: a variant of
// `CompanionCharacter` (settings contract), a drawing in the notch widget's characters/
// folder (listed in Companion.qml) and an entry here with a small preview.
export type CompanionChoice = { id: CompanionCharacter; name: string; preview(): ReactElement };

const blob = () => <svg viewBox="0 0 16 16" width="28" height="28" aria-hidden="true">
  <circle cx="8" cy="8.5" r="7" fill="currentColor" />
  <rect x="5.1" y="6.5" width="1.9" height="3.8" rx=".95" fill="#171717" />
  <rect x="9" y="6.5" width="1.9" height="3.8" rx=".95" fill="#171717" />
</svg>;

const isaac = () => <svg viewBox="0 0 18 18" width="28" height="28" aria-hidden="true">
  <ellipse cx="9" cy="8.8" rx="8.3" ry="7.6" fill="#cf9c9b" stroke="#140606" strokeWidth="1.4" />
  <ellipse cx="9" cy="8.6" rx="7" ry="6.5" fill="#e3c6c5" />
  <rect x="3.6" y="11" width="2.7" height="4.5" rx="1.35" fill="#9cc8cc" stroke="#68989c" strokeWidth=".35" />
  <rect x="11.7" y="11" width="2.7" height="4.5" rx="1.35" fill="#9cc8cc" stroke="#68989c" strokeWidth=".35" />
  <rect x="2.25" y="6.9" width="4.5" height="4.9" rx="1.9" fill="#140606" />
  <rect x="11.25" y="6.9" width="4.5" height="4.9" rx="1.9" fill="#140606" />
  <rect x="3.2" y="7.8" width="1.5" height="1.5" rx=".45" fill="#fff" /><rect x="12.2" y="7.8" width="1.5" height="1.5" rx=".45" fill="#fff" />
  <rect x="7" y="11.5" width="4" height="2.9" rx=".9" fill="#140606" />
  <rect x="7.75" y="12.9" width="2.5" height=".8" rx=".4" fill="#fff" />
</svg>;

export const COMPANION_CHARACTERS: readonly CompanionChoice[] = [
  { id: 'blob', name: 'Blob', preview: blob },
  { id: 'isaac', name: 'Isaac', preview: isaac },
];

export function CompanionPicker({ value, onPick, disabled }: {
  value: CompanionCharacter; onPick(id: CompanionCharacter): void; disabled?: boolean;
}) {
  return <div className="companion-picker" role="radiogroup" aria-label="Companion character">
    {COMPANION_CHARACTERS.map(c => <button key={c.id} type="button" role="radio" aria-checked={value === c.id}
      aria-label={c.name} disabled={disabled} onClick={() => onPick(c.id)}>
      <span className="companion-preview">{c.preview()}</span><span>{c.name}</span>
    </button>)}
  </div>;
}
