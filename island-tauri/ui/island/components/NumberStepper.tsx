import { useEffect, useRef, useState } from 'react';

/** Count input: typing is a local draft; buttons/keys commit bounded, whole steps. */
export function NumberStepper({ value, min, max, step = 1, unit = '', onCommit, disabled = false, ...aria }: {
  value: number; min: number; max: number; step?: number; unit?: string; disabled?: boolean;
  'aria-label': string; onCommit(value: number, final: boolean): void;
}) {
  const [draft, setDraft] = useState(String(value));
  const current = useRef(String(value)), editing = useRef(false);
  const set = (text: string) => { current.current = text; setDraft(text); };
  useEffect(() => { if (!editing.current) set(String(value)); }, [value]);
  const bound = (n: number) => Math.max(min, Math.min(max, min + Math.round((n - min) / step) * step));
  const number = () => current.current.trim() && Number.isFinite(Number(current.current)) ? Number(current.current) : value;
  const commit = (n: number) => { const next = bound(n); set(String(next)); editing.current = false; if (next !== value) onCommit(next, true); };
  const move = (delta: number) => commit(bound(Number(number())) + delta * step);
  return <div className="number-stepper" role="group" aria-label={`${aria['aria-label']} control`}>
    <button type="button" data-variant="quiet" aria-label={`Decrease ${aria['aria-label'].toLowerCase()}`} disabled={disabled || Number(number()) <= min} onClick={() => move(-1)}>−</button>
    <input {...aria} type="number" inputMode="numeric" aria-valuetext={unit.trim() ? `${draft} ${unit.trim()}` : undefined} min={min} max={max} step={step} value={draft} disabled={disabled}
      onFocus={() => { editing.current = true; }} onChange={e => { editing.current = true; set(e.target.value); }}
      onBlur={() => { if (editing.current) commit(Number(number())); }}
      onKeyDown={e => {
        if (['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', 'Enter', 'Escape'].includes(e.key)) {
          e.preventDefault(); e.stopPropagation();
          if (e.key === 'Escape') { editing.current = false; set(String(value)); }
          else if (e.key === 'Home') commit(min);
          else if (e.key === 'End') commit(max);
          else if (e.key === 'Enter') commit(Number(number()));
          else move(e.key === 'ArrowUp' ? 1 : e.key === 'ArrowDown' ? -1 : e.key === 'PageUp' ? 10 : -10);
        }
      }} />
    <button type="button" data-variant="quiet" aria-label={`Increase ${aria['aria-label'].toLowerCase()}`} disabled={disabled || Number(number()) >= max} onClick={() => move(1)}>+</button>
    {unit.trim() && <span className="control-value">{unit.trim()}</span>}
  </div>;
}
