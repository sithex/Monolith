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

const notchmimic = () => <svg viewBox="0 0 20 20" width="28" height="28" aria-hidden="true"><path d="M2 3Q10 0 18 3V16H2Z" fill="#202025" stroke="#777780"/><rect x="6" y="7" width="3" height="4" rx="1" fill="#f1ede4"/><rect x="12" y="7" width="3" height="4" rx="1" fill="#f1ede4"/><path d="M1 14v4m18-4v4" stroke="#b7b2a7" strokeWidth="2"/></svg>;
const companioncube = () => <svg viewBox="0 0 20 20" width="28" height="28" aria-hidden="true"><rect x="2" y="2" width="16" height="16" rx="2" fill="#75767e"/><path d="M10 2v16M2 10h16" stroke="#b9b9bf" strokeWidth="4"/><path d="M2 2h5v5H2Zm11 0h5v5h-5ZM2 13h5v5H2Zm11 0h5v5h-5Z" fill="#d6d5d8"/><circle cx="10" cy="10" r="5" fill="#c2a5b5" stroke="#52535d"/><path d="M10 13C4 9 8 6 10 9C12 6 16 9 10 13Z" fill="#ae668c"/></svg>;
const nurgling = () => <svg viewBox="0 0 20 20" width="28" height="28" aria-hidden="true"><path d="M9 5 Q6 3 8 0 L9 0 Q7 2 12 3 L12 5 Z" fill="#ded2a2" stroke="#756341" strokeWidth="0.7"/><path d="M5 15 L4 19 8 19 9 16 M12 16 L13 19 17 19 15 15 Z" fill="#788345" stroke="#343c25" strokeWidth="0.8"/><path d="M5 9 Q2 8 1 12 L1 15 3 14 4 12 6 12 Z" fill="#899449" stroke="#343c25" strokeWidth="0.8"/><path d="M6 4 Q9 2 13 4 L15 7 14 9 Q18 12 16 16 Q15 19 9 18 Q4 18 4 14 Q3 11 6 9 Q3 6 6 4 Z" fill="#929d50" stroke="#343c25" strokeWidth="0.8"/><path d="M7 11 Q12 10 15 13 Q17 17 11 17 Q6 18 5 14 Z" fill="#b3b96a" stroke="transparent" strokeWidth="0"/><path d="M14 10 Q17 8 18 10 L19 14 17 15 16 12 14 13 Z" fill="#8b984b" stroke="#343c25" strokeWidth="0.8"/><path d="M5 8 Q9 10 14 7 Q15 11 11 12 Q7 12 5 8 Z" fill="#35291e" stroke="transparent" strokeWidth="0"/><rect x="6" y="5" width="1" height="1" rx=".6" fill="#242619"/><rect x="11" y="5" width="1" height="1" rx=".6" fill="#242619"/><rect x="6" y="9" width="1" height="1" fill="#e2d5aa"/><rect x="8" y="10" width="1" height="1" fill="#e2d5aa"/><rect x="11" y="9" width="1" height="1" fill="#e2d5aa"/><rect x="13" y="8" width="1" height="1" fill="#e2d5aa"/><rect x="10" y="14" width="2" height="2" rx="1" fill="#646b36"/><rect x="10" y="14" width="1" height="1" rx=".5" fill="#3e4528"/><circle cx="7" cy="13" r="1" fill="#d2bb69"/><rect x="14" y="15" width="1" height="1" fill="#d2bb69"/><path d="M17 13v2m1-2v1M5 18v1m9-1v1" stroke="#d2c99c" strokeWidth="1"/></svg>;
const hollowknight = () => <svg viewBox="0 0 20 20" width="28" height="28" aria-hidden="true"><path d="M7 12h6l3 7-4-1-2 2-2-2-3 1Z" fill="#454d63" stroke="#777783" strokeWidth=".5"/><path d="M6 6Q3 3 6 0Q4 5 8 5h4Q16 5 14 0Q17 3 14 6Q17 7 16 11Q15 14 10 14Q5 14 4 11Q3 7 6 6Z" fill="#f1ede4" stroke="#96939b" strokeWidth=".5"/><ellipse cx="7" cy="10" rx="1.5" ry="2" fill="#101118"/><ellipse cx="13" cy="10" rx="1.5" ry="2" fill="#101118"/></svg>;

export const COMPANION_CHARACTERS: readonly CompanionChoice[] = [
  { id: 'blob', name: 'Blob', preview: blob },
  { id: 'isaac', name: 'Isaac', preview: isaac },
  { id: 'notchmimic', name: 'Null', preview: notchmimic },
  { id: 'companioncube', name: 'Companion Cube · Portal', preview: companioncube },
  { id: 'nurgling', name: 'Nurgling', preview: nurgling },
  { id: 'hollowknight', name: 'Hollow Knight', preview: hollowknight },
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
