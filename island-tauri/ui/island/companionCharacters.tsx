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
  <ellipse cx="9" cy="8.8" rx="8.1" ry="7.9" fill="#f2d0c0" stroke="#5a3a33" strokeWidth=".8" />
  <ellipse cx="5.9" cy="9" rx="2.07" ry="2.34" fill="#1b1113" />
  <ellipse cx="12.1" cy="9" rx="2.07" ry="2.34" fill="#1b1113" />
  <circle cx="5.3" cy="8.1" r=".8" fill="#fff" /><circle cx="11.5" cy="8.1" r=".8" fill="#fff" />
  <path d="M7.4 14.4 Q9 13 10.6 14.4" fill="none" stroke="#5a3a33" strokeWidth="1" strokeLinecap="round" />
  <ellipse cx="12.1" cy="12.6" rx=".95" ry="1.3" fill="#9fd4ff" stroke="#4a8fd0" strokeWidth=".4" />
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
