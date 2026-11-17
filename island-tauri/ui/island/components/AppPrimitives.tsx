import { useEffect, useRef, useState, type ReactNode, type InputHTMLAttributes } from 'react';
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
export function Fader({ value, label, muted, onCommit }: { value: number; label: string; muted: boolean; onCommit(v: number): void }) {
  const [draft, setDraft] = useState(value), [dragging, setDragging] = useState(false), [direct, setDirect] = useState(false);
  const track = useRef<HTMLDivElement>(null), last = useRef(0), pending = useRef<number | null>(null), timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const commit = useRef(onCommit); commit.current = onCommit;
  useEffect(() => { if (!dragging) { setDraft(value); setDirect(false); } }, [value, dragging]);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  const flush = () => { if (timer.current) { clearTimeout(timer.current); timer.current = null; } if (pending.current !== null) { commit.current(pending.current); pending.current = null; last.current = performance.now(); } };
  const set = (v: number) => {
    v = Math.max(0, Math.min(100, Math.round(v))); setDirect(true); setDraft(v); pending.current = v;
    const wait = 60 - (performance.now() - last.current);
    if (wait <= 0) flush(); else if (!timer.current) timer.current = setTimeout(flush, wait);
  };
  const fromPointer = (y: number) => { const r = track.current?.getBoundingClientRect(); if (r) set((r.bottom - y) / r.height * 100); };
  return <div className="fader" role="slider" tabIndex={0} aria-label={`${label} volume`} aria-orientation="vertical" aria-valuemin={0} aria-valuemax={100} aria-valuenow={draft} aria-valuetext={`${draft}%`} data-muted={muted}
    onPointerDown={e => { if (e.button !== 0) return; e.currentTarget.setPointerCapture(e.pointerId); setDragging(true); fromPointer(e.clientY); }}
    onPointerMove={e => { if (dragging) fromPointer(e.clientY); }}
    onPointerUp={() => { setDragging(false); flush(); }} onPointerCancel={() => { setDragging(false); flush(); }}
    onWheel={e => { e.stopPropagation(); set(draft - Math.sign(e.deltaY || -e.deltaX) * 2); }}
    onKeyDown={e => {
      const step = e.key === 'ArrowUp' || e.key === 'ArrowRight' ? 2 : e.key === 'ArrowDown' || e.key === 'ArrowLeft' ? -2 : e.key === 'PageUp' ? 10 : e.key === 'PageDown' ? -10 : 0;
      if (step || e.key === 'Home' || e.key === 'End') { e.preventDefault(); e.stopPropagation(); set(e.key === 'Home' ? 0 : e.key === 'End' ? 100 : draft + step); }
    }}>
    <div className="fader-track" ref={track}>
      <div className="fader-fill" data-direct={dragging || direct} style={{ transform: `scaleY(${draft / 100})` }} />
      <span className="fader-thumb" style={{ bottom: `${draft}%` }} aria-hidden="true" />
    </div>
  </div>;
}
import { Icon } from './Icon';
import type { IconName, Service } from '../models';
import { describeServiceReason, type ServiceReasonSource } from '../service-reasons';
import { SettingsLink } from '../../views/settings/SettingsLink';
export { Dropdown } from './Dropdown';
export function Empty({ icon, children }: { icon: IconName; children: ReactNode }) {
  return <div className="app-empty"><Icon name={icon}/><p>{children}</p></div>;
}
/**
 * A service that can't show its content: icon, one plain sentence, an optional jump to the
 * Settings row that fixes it, and the raw code (muted) only when the code is unknown.
 * `inline` drops the card chrome for use inside an existing card.
 */
export function ServiceNotice({ icon, reason, source, inline = false }: { icon: IconName; reason: string; source?: ServiceReasonSource; inline?: boolean }) {
  const r = describeServiceReason(reason, source);
  const body = <>
    <Icon name={icon}/>
    <p className="service-notice-text">{r.text}{r.code && <small className="service-notice-code"> · {r.code}</small>}</p>
    {r.action?.settingId && <span className="service-notice-action"><SettingsLink rowId={r.action.settingId} label={r.action.label} variant="text"/></span>}
  </>;
  return inline
    ? <div className="service-notice service-notice--inline" data-kind={r.kind} role="status" aria-busy={r.kind === 'loading'}>{body}</div>
    : <section className="card service-notice" data-kind={r.kind} role="status" aria-busy={r.kind === 'loading'}>{body}</section>;
}
export function ServiceCard<T>({ title, icon = 'spark', source, service, children }: { title: string; icon?: IconName; source?: ServiceReasonSource; service: Service<T>; children(value: T): ReactNode }) {
  return <section className="card"><h3>{title}</h3>{service.available ? children(service.value) : <ServiceNotice icon={icon} reason={service.reason} source={source} inline/>}</section>;
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
