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
const companioncube = () => <svg viewBox="0 0 20 20" width="28" height="28" shapeRendering="crispEdges" aria-hidden="true"><path d="M3 2h15v1h-15zM3 3h1v1h-1zM17 3h1v1h-1zM3 4h1v1h-1zM17 4h1v1h-1zM3 5h1v1h-1zM17 5h1v1h-1zM3 6h1v1h-1zM17 6h1v1h-1zM3 7h1v1h-1zM17 7h1v1h-1zM3 8h1v1h-1zM17 8h1v1h-1zM3 9h1v1h-1zM17 9h1v1h-1zM3 10h1v1h-1zM17 10h1v1h-1zM3 11h1v1h-1zM17 11h1v1h-1zM3 12h1v1h-1zM17 12h1v1h-1zM3 13h1v1h-1zM17 13h1v1h-1zM3 14h1v1h-1zM17 14h1v1h-1zM3 15h1v1h-1zM17 15h1v1h-1zM3 16h15v1h-15z" fill="#2c2d33"/><path d="M4 3h4v1h-4zM13 3h4v1h-4zM4 4h4v1h-4zM13 4h4v1h-4zM4 5h3v1h-3zM14 5h3v1h-3zM4 6h2v1h-2zM15 6h2v1h-2zM4 12h2v1h-2zM15 12h2v1h-2zM4 13h3v1h-3zM14 13h3v1h-3zM4 14h4v1h-4zM13 14h4v1h-4zM4 15h4v1h-4zM13 15h4v1h-4z" fill="#e6e6ea"/><path d="M8 3h1v1h-1zM12 3h1v1h-1zM8 4h1v1h-1zM12 4h1v1h-1zM7 5h1v1h-1zM13 5h1v1h-1zM6 6h1v1h-1zM14 6h1v1h-1zM4 7h2v1h-2zM15 7h2v1h-2zM4 11h2v1h-2zM15 11h2v1h-2zM6 12h1v1h-1zM14 12h1v1h-1zM7 13h1v1h-1zM13 13h1v1h-1zM8 14h1v1h-1zM12 14h1v1h-1zM8 15h1v1h-1zM12 15h1v1h-1z" fill="#b9bac1"/><path d="M9 3h1v1h-1zM11 3h1v1h-1zM9 4h1v1h-1zM11 4h1v1h-1zM4 8h2v1h-2zM15 8h2v1h-2zM4 10h2v1h-2zM15 10h2v1h-2zM9 14h1v1h-1zM11 14h1v1h-1zM9 15h1v1h-1zM11 15h1v1h-1z" fill="#6b6d76"/><path d="M10 3h1v1h-1zM10 4h1v1h-1zM4 9h2v1h-2zM15 9h2v1h-2zM10 14h1v1h-1zM10 15h1v1h-1z" fill="#e58cb6"/><path d="M8 5h1v1h-1zM12 5h1v1h-1zM8 13h1v1h-1zM12 13h1v1h-1z" fill="#8a8c95"/><path d="M9 5h3v1h-3zM7 6h2v1h-2zM12 6h2v1h-2zM6 7h1v1h-1zM14 7h1v1h-1zM6 8h1v1h-1zM14 8h1v1h-1zM6 9h1v1h-1zM14 9h1v1h-1zM6 10h1v1h-1zM14 10h1v1h-1zM6 11h1v1h-1zM14 11h1v1h-1zM7 12h2v1h-2zM12 12h2v1h-2zM9 13h3v1h-3z" fill="#5a5c66"/><path d="M9 6h3v1h-3zM7 7h1v1h-1zM10 7h1v1h-1zM13 7h1v1h-1zM7 8h1v1h-1zM13 8h1v1h-1zM7 9h1v1h-1zM13 9h1v1h-1zM7 10h2v1h-2zM12 10h2v1h-2zM7 11h3v1h-3zM11 11h3v1h-3zM9 12h3v1h-3z" fill="#f3f1f3"/><path d="M8 7h2v1h-2zM11 7h2v1h-2zM9 8h4v1h-4zM8 9h5v1h-5zM9 10h3v1h-3zM10 11h1v1h-1z" fill="#ec6aa6"/><path d="M8 8h1v1h-1z" fill="#ffb5d6"/></svg>;
const nurgling = () => <svg viewBox="0 0 20 20" width="28" height="28" shapeRendering="crispEdges" aria-hidden="true"><path d="M11 0h1v1h-1zM13 0h1v1h-1zM11 1h1v1h-1zM14 1h1v1h-1zM10 2h1v1h-1zM13 2h1v1h-1zM9 3h1v1h-1zM12 3h1v1h-1zM5 4h4v1h-4zM11 4h3v1h-3zM4 5h1v1h-1zM14 5h1v1h-1zM3 6h1v1h-1zM15 6h1v1h-1zM3 7h1v1h-1zM15 7h1v1h-1zM3 8h1v1h-1zM15 8h1v1h-1zM2 9h1v1h-1zM16 9h1v1h-1zM2 10h1v1h-1zM16 10h1v1h-1zM2 11h1v1h-1zM16 11h1v1h-1zM1 12h1v1h-1zM3 12h1v1h-1zM5 12h1v1h-1zM14 12h1v1h-1zM16 12h1v1h-1zM18 12h1v1h-1zM1 13h1v1h-1zM4 13h1v1h-1zM15 13h1v1h-1zM18 13h1v1h-1zM2 14h2v1h-2zM16 14h2v1h-2zM3 15h1v1h-1zM17 15h1v1h-1zM3 16h1v1h-1zM17 16h1v1h-1zM3 17h1v1h-1zM17 17h1v1h-1zM4 18h2v1h-2zM8 18h5v1h-5zM15 18h2v1h-2zM4 19h1v1h-1zM8 19h1v1h-1zM12 19h1v1h-1zM16 19h1v1h-1z" fill="#27301a"/><path d="M12 0h1v1h-1zM12 1h1v1h-1zM11 2h1v1h-1zM10 3h1v1h-1zM9 4h1v1h-1zM9 5h1v1h-1z" fill="#e6dcb0"/><path d="M13 1h1v1h-1zM12 2h1v1h-1zM11 3h1v1h-1zM10 4h1v1h-1z" fill="#a8996a"/><path d="M5 5h4v1h-4zM10 5h4v1h-4zM4 6h11v1h-11zM4 7h1v1h-1zM7 7h4v1h-4zM13 7h2v1h-2zM4 8h1v1h-1zM7 8h4v1h-4zM13 8h2v1h-2zM3 9h1v1h-1zM5 9h9v1h-9zM15 9h1v1h-1zM3 10h1v1h-1zM15 10h1v1h-1zM3 11h1v1h-1zM15 11h1v1h-1zM2 12h1v1h-1zM13 12h1v1h-1zM17 12h1v1h-1zM3 13h1v1h-1zM5 13h2v1h-2zM12 13h3v1h-3zM16 13h1v1h-1zM4 14h1v1h-1zM15 14h1v1h-1zM4 15h1v1h-1zM16 15h1v1h-1zM4 16h1v1h-1zM16 16h1v1h-1zM4 17h1v1h-1zM16 17h1v1h-1zM7 18h1v1h-1zM13 18h1v1h-1z" fill="#8fa548"/><path d="M5 7h1v1h-1zM12 7h1v1h-1zM5 8h2v1h-2zM11 8h2v1h-2z" fill="#1a1a10"/><path d="M6 7h1v1h-1zM11 7h1v1h-1z" fill="#e8e070"/><path d="M4 9h1v1h-1zM14 9h1v1h-1zM4 10h2v1h-2zM7 10h1v1h-1zM9 10h1v1h-1zM11 10h1v1h-1zM13 10h2v1h-2zM5 11h9v1h-9zM7 12h1v1h-1zM9 12h1v1h-1zM11 12h1v1h-1zM8 13h3v1h-3z" fill="#3a1e1a"/><path d="M6 10h1v1h-1zM8 10h1v1h-1zM10 10h1v1h-1zM12 10h1v1h-1zM8 12h1v1h-1zM10 12h1v1h-1z" fill="#f0e8c8"/><path d="M4 11h1v1h-1zM14 11h1v1h-1zM6 12h1v1h-1zM12 12h1v1h-1zM2 13h1v1h-1zM7 13h1v1h-1zM11 13h1v1h-1zM17 13h1v1h-1zM9 16h1v1h-1zM5 17h1v1h-1zM15 17h1v1h-1zM6 18h1v1h-1zM14 18h1v1h-1z" fill="#6f8436"/><path d="M5 14h10v1h-10zM5 15h8v1h-8zM15 15h1v1h-1zM5 16h4v1h-4zM10 16h3v1h-3zM14 16h2v1h-2zM6 17h9v1h-9z" fill="#b9c46c"/><path d="M13 15h1v1h-1z" fill="#e8d070"/><path d="M14 15h1v1h-1zM13 16h1v1h-1z" fill="#c8a850"/><path d="M5 19h3v1h-3zM13 19h3v1h-3z" fill="#5f7230"/></svg>;
const hollowknight = () => <svg viewBox="0 0 20 20" width="28" height="28" shapeRendering="crispEdges" aria-hidden="true"><path d="M3 0h1v1h-1zM15 0h1v1h-1zM3 1h2v1h-2zM14 1h2v1h-2zM4 2h1v1h-1zM14 2h1v1h-1zM4 3h2v1h-2zM13 3h2v1h-2zM5 4h9v1h-9zM4 5h11v1h-11zM3 6h3v1h-3zM8 6h3v1h-3zM13 6h3v1h-3zM3 7h2v1h-2zM9 7h1v1h-1zM14 7h2v1h-2zM3 8h2v1h-2zM9 8h1v1h-1zM14 8h2v1h-2zM3 9h3v1h-3zM8 9h3v1h-3zM13 9h2v1h-2zM3 10h12v1h-12zM5 11h9v1h-9zM7 12h5v1h-5z" fill="#f4f1ea"/><path d="M6 6h2v1h-2zM11 6h2v1h-2zM5 7h4v1h-4zM10 7h4v1h-4zM5 8h4v1h-4zM10 8h4v1h-4zM6 9h2v1h-2zM11 9h2v1h-2z" fill="#07070b"/><path d="M15 9h1v1h-1zM15 10h1v1h-1zM4 11h1v1h-1zM14 11h1v1h-1zM5 12h2v1h-2zM12 12h2v1h-2z" fill="#bdbac6"/><path d="M6 13h1v1h-1zM12 13h1v1h-1zM5 14h1v1h-1zM14 14h1v1h-1zM4 15h1v1h-1zM14 15h1v1h-1zM4 16h1v1h-1zM14 16h1v1h-1zM5 17h1v1h-1zM7 17h1v1h-1zM11 17h1v1h-1zM13 17h1v1h-1zM7 18h1v1h-1zM11 18h1v1h-1z" fill="#2a2f3f"/><path d="M7 13h5v1h-5zM6 14h1v1h-1zM8 14h4v1h-4zM13 14h1v1h-1zM5 15h2v1h-2zM8 15h6v1h-6zM5 16h9v1h-9z" fill="#3f475e"/><path d="M7 14h1v1h-1zM12 14h1v1h-1zM7 15h1v1h-1z" fill="#5c6683"/></svg>;

const frog = () => <svg viewBox="0 0 20 20" width="28" height="28" shapeRendering="crispEdges" aria-hidden="true"><path d="M4 7h3v1h-3zM13 7h3v1h-3zM3 8h1v1h-1zM7 8h1v1h-1zM12 8h1v1h-1zM16 8h1v1h-1zM2 9h1v1h-1zM8 9h4v1h-4zM17 9h1v1h-1zM2 10h1v1h-1zM17 10h1v1h-1zM2 11h1v1h-1zM17 11h1v1h-1zM2 12h1v1h-1zM17 12h1v1h-1zM2 13h1v1h-1zM17 13h1v1h-1zM2 14h1v1h-1zM17 14h1v1h-1zM3 15h1v1h-1zM16 15h1v1h-1zM3 16h1v1h-1zM16 16h1v1h-1zM2 17h1v1h-1zM5 17h1v1h-1zM14 17h1v1h-1zM17 17h1v1h-1zM2 18h1v1h-1zM6 18h1v1h-1zM13 18h1v1h-1zM17 18h1v1h-1zM3 19h14v1h-14z" fill="#173018"/><path d="M4 8h3v1h-3zM13 8h3v1h-3zM3 9h1v1h-1zM6 9h2v1h-2zM12 9h1v1h-1zM15 9h2v1h-2zM3 10h1v1h-1zM6 10h2v1h-2zM12 10h1v1h-1zM15 10h2v1h-2zM4 11h3v1h-3zM13 11h3v1h-3z" fill="#f6f6ee"/><path d="M4 9h2v1h-2zM13 9h2v1h-2zM4 10h2v1h-2zM13 10h2v1h-2z" fill="#101010"/><path d="M8 10h4v1h-4zM3 11h1v1h-1zM7 11h2v1h-2zM11 11h2v1h-2zM16 11h1v1h-1zM3 12h6v1h-6zM11 12h6v1h-6zM3 13h1v1h-1zM5 13h10v1h-10zM16 13h1v1h-1zM3 14h2v1h-2zM6 14h8v1h-8zM15 14h2v1h-2zM4 15h2v1h-2zM14 15h2v1h-2zM4 16h3v1h-3zM13 16h3v1h-3zM3 17h1v1h-1zM16 17h1v1h-1zM4 18h1v1h-1zM15 18h1v1h-1z" fill="#58b648"/><path d="M9 11h2v1h-2zM9 12h2v1h-2z" fill="#86d264"/><path d="M4 13h1v1h-1zM15 13h1v1h-1z" fill="#2f6e2b"/><path d="M5 14h1v1h-1zM14 14h1v1h-1zM6 15h8v1h-8z" fill="#1d3a1c"/><path d="M7 16h6v1h-6zM6 17h8v1h-8zM7 18h6v1h-6z" fill="#d8e7a0"/><path d="M4 17h1v1h-1zM15 17h1v1h-1zM3 18h1v1h-1zM5 18h1v1h-1zM14 18h1v1h-1zM16 18h1v1h-1z" fill="#3c8a36"/></svg>;

export const COMPANION_CHARACTERS: readonly CompanionChoice[] = [
  { id: 'blob', name: 'Blob', preview: blob },
  { id: 'isaac', name: 'Isaac', preview: isaac },
  { id: 'notchmimic', name: 'Null', preview: notchmimic },
  { id: 'companioncube', name: 'Companion Cube · Portal', preview: companioncube },
  { id: 'nurgling', name: 'Nurgling', preview: nurgling },
  { id: 'hollowknight', name: 'Hollow Knight', preview: hollowknight },
  { id: 'frog', name: 'Frog', preview: frog },
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
