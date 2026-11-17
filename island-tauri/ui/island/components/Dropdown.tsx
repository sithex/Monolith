import { useEffect, useId, useRef, useState } from 'react';
import { Icon } from './Icon';
import type { IconName } from '../models';
import { prefersReducedMotion } from './motion-preference';
// Styles: ./dropdown.css, imported from Island.tsx (the test loaders do not load CSS).

export type DropdownOption = { value: string; label: string; hint?: string };

/** Longest exit (150 ms at speed 1) plus slack: unmounts even if `animationend` never fires. */
export const DROPDOWN_EXIT_FALLBACK_MS = 240;
/** Row height and visible-row cap shared with dropdown.css (36 px rows, 7 rows, 6 px padding). */
export const DROPDOWN_ROW_PX = 36, DROPDOWN_VISIBLE_ROWS = 7;

/** Keyboard-driven or reduced motion: open and close change state instantly, nothing animates. */
export function instantMenuMotion(): boolean {
  if (typeof document === 'undefined') return true;
  return document.documentElement?.dataset?.navVia === 'key' || prefersReducedMotion();
}

/**
 * Open/closing/closed state for an island popover menu. Closing keeps the menu mounted
 * (data-state="closing", inert) until its exit animation ends, so the exit actually plays;
 * keyboard / reduced-motion closes unmount at once.
 */
export function useMenuPresence() {
  const [open, setOpen] = useState(false), [closing, setClosing] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const clear = () => { if (timer.current !== null) { clearTimeout(timer.current); timer.current = null; } };
  useEffect(() => clear, []);
  const show = () => { clear(); setClosing(false); setOpen(true); };
  const hide = (instant = false) => {
    clear(); setOpen(false);
    if (instant || instantMenuMotion()) { setClosing(false); return; }
    setClosing(true);
    timer.current = setTimeout(() => { timer.current = null; setClosing(false); }, DROPDOWN_EXIT_FALLBACK_MS);
  };
  // animationend bubbles from the rows: only the card's own exit settles the menu.
  const settled = (e: { target: unknown; currentTarget: unknown }) => {
    if (!open && e.target === e.currentTarget) { clear(); setClosing(false); }
  };
  return { open, mounted: open || closing, state: open ? 'open' as const : 'closing' as const, show, hide, settled };
}

const Check = () => <svg className="dropdown-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m5 12 5 5 9-10" /></svg>;
export const DropdownCheck = Check;
export const DropdownChevron = () => <svg className="dropdown-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>;

/**
 * Island-styled replacement for a native <select> (WebKitGTK draws GTK's own popup,
 * which ignores the island theme): a pill trigger and a popover card of listbox rows.
 * Keyboard: arrows, Home/End, Enter/Space, Escape (kept from closing the island), Tab
 * closes; click outside closes. The menu opens upwards when its `.app-pane` lacks room.
 * `icon` / `prefix` decorate the trigger ("[icon] Voice  Name"); `align="end"` hangs the
 * menu from the trigger's right edge (controls at the right of a row).
 */
export function Dropdown({ label, value, options, onChange, disabled = false, placeholder = 'Choose…', icon, prefix, align = 'start' }: {
  label: string; value: string | null; options: DropdownOption[];
  onChange(value: string): void; disabled?: boolean; placeholder?: string;
  icon?: IconName; prefix?: string; align?: 'start' | 'end';
}) {
  const menu = useMenuPresence();
  const [focus, setFocus] = useState(0), [up, setUp] = useState(false);
  const root = useRef<HTMLDivElement>(null), button = useRef<HTMLButtonElement>(null);
  const id = useId();
  const current = options.find(o => o.value === value);
  const { open } = menu;
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) menu.hide(); };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps
  const show = () => {
    if (disabled || !options.length) return;
    // Open upwards when the scroll pane has no room below (the pane clips the menu).
    const r = root.current?.getBoundingClientRect(), pane = root.current?.closest('.app-pane')?.getBoundingClientRect();
    const need = Math.min(options.length, DROPDOWN_VISIBLE_ROWS) * DROPDOWN_ROW_PX + 12 + 8;
    setUp(!!r && !!pane && pane.bottom - r.bottom < need && r.top - pane.top > pane.bottom - r.bottom);
    setFocus(Math.max(0, options.findIndex(o => o.value === value))); menu.show();
  };
  const pick = (v: string, instant = false) => { menu.hide(instant); button.current?.focus(); if (v !== value) onChange(v); };
  return <div className="dropdown" ref={root} data-open={open} data-up={up} data-align={align} onKeyDown={e => {
    if (!open) { if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) { e.preventDefault(); show(); } return; }
    const move = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (move) { e.preventDefault(); setFocus(f => (f + move + options.length) % options.length); }
    else if (e.key === 'Home' || e.key === 'End') { e.preventDefault(); setFocus(e.key === 'Home' ? 0 : options.length - 1); }
    else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); const o = options[focus]; if (o) pick(o.value, true); }
    else if (e.key === 'Escape' || e.key === 'Tab') { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); } menu.hide(true); }
  }}>
    <button ref={button} type="button" className="dropdown-trigger" aria-label={label} aria-haspopup="listbox" aria-expanded={open} aria-controls={id} disabled={disabled || !options.length}
      onClick={() => open ? menu.hide() : show()}>
      {icon && <span className="dropdown-icon" aria-hidden="true"><Icon name={icon} /></span>}
      {prefix && <span className="dropdown-prefix">{prefix}</span>}
      <span className="dropdown-value" data-placeholder={current ? undefined : true}>{current?.label ?? placeholder}</span>
      <DropdownChevron />
    </button>
    {menu.mounted && <ul className="dropdown-menu" role="listbox" id={id} aria-label={label} data-state={menu.state}
      inert={!open || undefined} onAnimationEnd={menu.settled}>
      {options.map((o, i) => <li key={o.value} role="option" aria-selected={o.value === value} data-focus={i === focus}
        onPointerEnter={() => setFocus(i)} onClick={() => pick(o.value)}>
        <span className="dropdown-label">{o.label}</span>
        {o.hint && <span className="dropdown-hint">{o.hint}</span>}
        {o.value === value ? <Check /> : <span className="dropdown-check-slot" aria-hidden="true" />}
      </li>)}
    </ul>}
  </div>;
}
