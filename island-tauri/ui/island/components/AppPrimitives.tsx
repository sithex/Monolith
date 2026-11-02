import { useEffect, useId, useRef, useState, type ReactNode, type InputHTMLAttributes } from 'react';
/**
 * Range input that keeps a local draft while the pointer/keyboard is driving it.
 * The authoritative value (Rust) only replaces the draft when the user is not
 * interacting, so a slow round-trip can never yank the thumb back mid-drag.
 * Commits are throttled while dragging and always flushed on release.
 */
export function DraftRange({ value, onCommit, throttleMs = 90, live = true, onDraft, ...rest }: Omit<InputHTMLAttributes<HTMLInputElement>, 'value' | 'onChange' | 'type'> & {
  value: number; onCommit(value: number, final: boolean): void; throttleMs?: number;
  /** false: commit only on release (the thumb still follows the pointer); for sliders that move the island itself. */
  live?: boolean;
  /** Draft value while interacting, null once released. */
  onDraft?(value: number | null): void;
}) {
  const [draft, setDraft] = useState(value);
  const active = useRef(false), last = useRef(0), pending = useRef<number | null>(null), timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const commit = useRef(onCommit); commit.current = onCommit;
  const report = useRef(onDraft); report.current = onDraft;
  useEffect(() => { if (!active.current) setDraft(value); }, [value]);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  const flush = (final: boolean) => {
    if (timer.current) { clearTimeout(timer.current); timer.current = null; }
    if (pending.current === null) return;
    const v = pending.current; pending.current = null; last.current = performance.now();
    if (!final) sent.current = v;
    commit.current(v, final);
  };
  const change = (v: number) => {
    setDraft(v); pending.current = v; report.current?.(v);
    if (!live) return;
    const wait = throttleMs - (performance.now() - last.current);
    if (wait <= 0) flush(false);
    else if (!timer.current) timer.current = setTimeout(() => { timer.current = null; flush(false); }, wait);
  };
  const sent = useRef<number | null>(null);
  const end = () => {
    if (!active.current) return; active.current = false;
    if (pending.current === null && sent.current !== null) pending.current = sent.current;
    sent.current = null; flush(true); report.current?.(null);
  };
  const [dragging, setDragging] = useState(false);
  const lo = Number(rest.min ?? 0), hi = Number(rest.max ?? 100);
  const fill = hi > lo ? Math.max(0, Math.min(100, (draft - lo) / (hi - lo) * 100)) : 0;
  return <input {...rest} type="range" value={draft} data-dragging={dragging}
    style={{ ...rest.style, ['--fill' as string]: `${fill}%` }}
    onPointerDown={e => { active.current = true; setDragging(true); rest.onPointerDown?.(e); }}
    onPointerUp={e => { setDragging(false); end(); rest.onPointerUp?.(e); }} onPointerCancel={() => { setDragging(false); end(); }} onBlur={e => { setDragging(false); end(); rest.onBlur?.(e); }}
    onKeyDown={e => { active.current = true; rest.onKeyDown?.(e); }} onKeyUp={e => { end(); rest.onKeyUp?.(e); }}
    onChange={e => change(Number(e.target.value))} />;
}
import { Icon } from './Icon';
import type { IconName, Service } from '../models';
/**
 * Island-styled replacement for a native <select> (WebKitGTK draws GTK's own popup,
 * which ignores the island theme). Listbox semantics + keyboard: arrows, Home/End,
 * Enter/Space, Escape (kept from closing the island), click outside closes.
 */
export function Dropdown({ label, value, options, onChange, disabled = false, placeholder = 'Choose…' }: {
  label: string; value: string | null; options: { value: string; label: string }[];
  onChange(value: string): void; disabled?: boolean; placeholder?: string;
}) {
  const [open, setOpen] = useState(false), [focus, setFocus] = useState(0), [up, setUp] = useState(false);
  const root = useRef<HTMLDivElement>(null), button = useRef<HTMLButtonElement>(null);
  const id = useId();
  const current = options.find(o => o.value === value);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('pointerdown', away, true);
    return () => document.removeEventListener('pointerdown', away, true);
  }, [open]);
  const show = () => {
    if (disabled || !options.length) return;
    // Open upwards when the scroll pane has no room below (the pane clips the menu).
    const r = root.current?.getBoundingClientRect(), pane = root.current?.closest('.app-pane')?.getBoundingClientRect();
    const need = Math.min(options.length, 7) * 34 + 12;
    setUp(!!r && !!pane && pane.bottom - r.bottom < need && r.top - pane.top > pane.bottom - r.bottom);
    setFocus(Math.max(0, options.findIndex(o => o.value === value))); setOpen(true);
  };
  const pick = (v: string) => { setOpen(false); button.current?.focus(); if (v !== value) onChange(v); };
  return <div className="dropdown" ref={root} data-open={open} data-up={up} onKeyDown={e => {
    if (!open) { if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) { e.preventDefault(); show(); } return; }
    const move = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (move) { e.preventDefault(); setFocus(f => (f + move + options.length) % options.length); }
    else if (e.key === 'Home' || e.key === 'End') { e.preventDefault(); setFocus(e.key === 'Home' ? 0 : options.length - 1); }
    else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); const o = options[focus]; if (o) pick(o.value); }
    else if (e.key === 'Escape' || e.key === 'Tab') { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); } setOpen(false); }
  }}>
    <button ref={button} type="button" className="dropdown-trigger" aria-label={label} aria-haspopup="listbox" aria-expanded={open} aria-controls={id} disabled={disabled || !options.length}
      onClick={() => open ? setOpen(false) : show()}>
      <span className="dropdown-value">{current?.label ?? placeholder}</span>
      <svg className="dropdown-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
    </button>
    {open && <ul className="dropdown-menu" role="listbox" id={id} aria-label={label}>
      {options.map((o, i) => <li key={o.value} role="option" aria-selected={o.value === value} data-focus={i === focus}
        onPointerEnter={() => setFocus(i)} onClick={() => pick(o.value)}>
        <span>{o.label}</span>
        {o.value === value && <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m5 12 5 5 9-10" /></svg>}
      </li>)}
    </ul>}
  </div>;
}
export function Empty({ icon, children }: { icon: IconName; children: ReactNode }) {
  return <div className="app-empty"><Icon name={icon}/><p>{children}</p></div>;
}
export function ServiceCard<T>({ title, service, children }: { title: string; service: Service<T>; children(value: T): ReactNode }) {
  return <section className="card"><h3>{title}</h3>{service.available ? children(service.value) : <p className="unavailable">Unavailable · {service.reason}</p>}</section>;
}
// No external image fetches or file:// access from staging. Phase 7 supplies
// bounded service-owned PNGs (including proxied artwork), not arbitrary URLs.
export function SafeImage({ src, alt, className = '' }: { src: string | null; alt: string; className?: string }) {
  if (!src || src.length > 1400000 || !/^data:image\/(?:png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/.test(src)) return null;
  return <img className={`bounded-image ${className}`} src={src} alt={alt} loading="lazy" referrerPolicy="no-referrer"/>;
}
export function formatSize(bytes: number) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']; let value = Math.max(0, bytes); let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  return `${unit ? (Math.floor(value * 10) / 10).toFixed(1) : value} ${units[unit]}`;
}
