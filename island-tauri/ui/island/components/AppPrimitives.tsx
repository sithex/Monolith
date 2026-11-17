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
  return <span className={`pill-range${rest.className ? ` ${rest.className}` : ''}`} data-dragging={dragging}
    style={{ ...rest.style, ['--fill-ratio' as string]: fill / 100 }}>
    <span className="pill-range-track" aria-hidden="true">
      <span className="pill-range-fill" />
      <span className="pill-range-grip" />
    </span>
    <input {...rest} type="range" value={draft} data-dragging={dragging}
      onPointerDown={e => { active.current = true; setDragging(true); rest.onPointerDown?.(e); }}
      onPointerUp={e => { setDragging(false); end(); rest.onPointerUp?.(e); }}
      onPointerCancel={e => { setDragging(false); end(); rest.onPointerCancel?.(e); }}
      onBlur={e => { setDragging(false); end(); rest.onBlur?.(e); }}
      onKeyDown={e => { active.current = true; rest.onKeyDown?.(e); }} onKeyUp={e => { end(); rest.onKeyUp?.(e); }}
      onChange={e => change(Number(e.target.value))} />
  </span>;
}
/** Speaker glyph whose waves follow the level, as on iOS: muted/0 → none, then 1–3 waves. */
function LevelGlyph({ value, muted }: { value: number; muted: boolean }) {
  const waves = muted || value <= 0 ? 0 : value < 34 ? 1 : value < 67 ? 2 : 3;
  return <svg className="fader-glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M3.5 9.5h3.2L11 5.5v13l-4.3-4H3.5z" fill="currentColor" />
    {muted ? <path d="M15.5 9.5l5 5m0-5-5 5" /> : <>
      <path d="M14.6 9.6a3.4 3.4 0 0 1 0 4.8" opacity={waves >= 1 ? 1 : .25} />
      <path d="M17.2 7.2a6.8 6.8 0 0 1 0 9.6" opacity={waves >= 2 ? 1 : .25} />
      <path d="M19.8 4.8a10.2 10.2 0 0 1 0 14.4" opacity={waves >= 3 ? 1 : .25} />
    </>}
  </svg>;
}
/** Settle window: after local input the fader keeps its own value until the backend reports it
 *  (±1 for pactl rounding) or this long passes, so late replies can never replay the drag. */
const FADER_HOLD_MS = 1200, FADER_COMMIT_MS = 33, FADER_STRETCH_PX = 12;
/**
 * iOS Control Centre style vertical volume fader: wide rounded slab, fill from the bottom, no knob,
 * level glyph inside. Dragging is relative (grab anywhere, no jump), overshoot rubber-bands the slab,
 * a tap without movement jumps to the tapped level. Pointer input paints directly; commits are
 * throttled to ~30 Hz and the caller coalesces them (latest wins), so the audio server never backs up.
 */
export function Fader({ value, label, muted, onCommit, onDraft }: { value: number; label: string; muted: boolean; onCommit(v: number): void; onDraft?(v: number | null): void }) {
  const [draft, setDraft] = useState(value), [dragging, setDragging] = useState(false), [direct, setDirect] = useState(false);
  const [stretch, setStretch] = useState(0);  // signed fraction of the slab height
  const track = useRef<HTMLDivElement>(null), last = useRef(0), pending = useRef<number | null>(null), timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hold = useRef<number | null>(null), settle = useRef<ReturnType<typeof setTimeout> | null>(null);
  const grab = useRef<{ y: number; v: number; moved: boolean } | null>(null);
  const draftRef = useRef(draft); draftRef.current = draft;
  const valueRef = useRef(value); valueRef.current = value;
  const draggingRef = useRef(false);
  const commit = useRef(onCommit); commit.current = onCommit;
  const report = useRef(onDraft); report.current = onDraft;
  const release = () => { hold.current = null; setDraft(valueRef.current); setDirect(false); report.current?.(null); };
  const armSettle = () => {
    if (settle.current) clearTimeout(settle.current);
    settle.current = setTimeout(() => { settle.current = null; if (draggingRef.current) armSettle(); else if (hold.current !== null) release(); }, FADER_HOLD_MS);
  };
  useEffect(() => {
    if (draggingRef.current) return;
    if (hold.current !== null) {
      if (Math.abs(value - hold.current) > 1) return;  // a stale reply from earlier in the gesture
      if (settle.current) { clearTimeout(settle.current); settle.current = null; }
    }
    release();
  }, [value]);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); if (settle.current) clearTimeout(settle.current); }, []);
  const flush = () => { if (timer.current) { clearTimeout(timer.current); timer.current = null; } if (pending.current !== null) { commit.current(pending.current); pending.current = null; last.current = performance.now(); } };
  const set = (v: number) => {
    v = Math.max(0, Math.min(100, Math.round(v)));
    setDirect(true); hold.current = v; armSettle(); report.current?.(v);
    if (v === draftRef.current && pending.current === null) return;
    draftRef.current = v; setDraft(v); pending.current = v;
    const wait = FADER_COMMIT_MS - (performance.now() - last.current);
    if (wait <= 0) flush(); else if (!timer.current) timer.current = setTimeout(flush, wait);
  };
  const height = () => track.current?.getBoundingClientRect().height || 1;
  const fromPointer = (y: number) => { const r = track.current?.getBoundingClientRect(); if (r) set((r.bottom - y) / r.height * 100); };
  const end = () => {
    const g = grab.current; grab.current = null; draggingRef.current = false;
    setDragging(false); setStretch(0); flush(); report.current?.(hold.current);
    return g;
  };
  // Rubber band: overshoot past either end stretches the slab with diminishing returns.
  const band = (over: number) => Math.sign(over) * FADER_STRETCH_PX * (1 - Math.exp(-Math.abs(over) / 40));
  return <div className="fader" role="slider" tabIndex={0} aria-label={`${label} volume`} aria-orientation="vertical" aria-valuemin={0} aria-valuemax={100} aria-valuenow={draft} aria-valuetext={`${draft}%`} data-muted={muted} data-dragging={dragging}
    style={{ ['--fill-ratio' as string]: draft / 100 }}
    onPointerDown={e => {
      if (e.button !== 0) return; e.currentTarget.setPointerCapture(e.pointerId);
      grab.current = { y: e.clientY, v: draftRef.current, moved: false }; draggingRef.current = true; setDragging(true); setDirect(true);
    }}
    onPointerMove={e => {
      const g = grab.current; if (!g) return;
      const dy = g.y - e.clientY; if (!g.moved && Math.abs(dy) < 3) return; g.moved = true;
      const px = height(), raw = g.v + dy / px * 100, clamped = Math.max(0, Math.min(100, raw));
      setStretch(band((raw - clamped) / 100 * px) / px); set(clamped);
    }}
    onPointerUp={e => { const g = end(); if (g && !g.moved) { fromPointer(e.clientY); flush(); } }}
    onPointerCancel={() => { end(); }}
    onWheel={e => { e.stopPropagation(); set(draftRef.current - Math.sign(e.deltaY || -e.deltaX) * 2); }}
    onKeyDown={e => {
      const step = e.key === 'ArrowUp' || e.key === 'ArrowRight' ? 2 : e.key === 'ArrowDown' || e.key === 'ArrowLeft' ? -2 : e.key === 'PageUp' ? 10 : e.key === 'PageDown' ? -10 : 0;
      if (step || e.key === 'Home' || e.key === 'End') { e.preventDefault(); e.stopPropagation(); set(e.key === 'Home' ? 0 : e.key === 'End' ? 100 : draftRef.current + step); }
    }}>
    <div className="fader-track" ref={track} data-stretch={stretch > 0 ? 'up' : stretch < 0 ? 'down' : 'none'}
      style={{ ['--stretch' as string]: 1 + Math.abs(stretch) }}>
      <div className="fader-fill" data-direct={dragging || direct} style={{ transform: `scaleY(${draft / 100})` }} />
      <span className="fader-icon" data-covered={draft >= 16} aria-hidden="true"><LevelGlyph value={draft} muted={muted} /></span>
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
