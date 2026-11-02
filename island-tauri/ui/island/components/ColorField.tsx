import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type PointerEvent as ReactPointerEvent } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { useIslandState } from '../../ipc/island-state';

/**
 * In-island colour picker. On Wayland the native GTK colour chooser that `<input type=color>`
 * opens is a normal toplevel and lands BEHIND the keep-above island, so it was unusable.
 * This panel is drawn by the island itself, docked beside it like an extension of the app
 * (outside its right edge and side buttons when there is room), and joins the input/blur
 * region through `[data-island-extension]` in `geometryReport`.
 */
type Request = { id: string; label: string; value: string; anchorY: number; onPick(hex: string): void; onReset?(): void; resetLabel?: string };
let active: Request | null = null;
const listeners = new Set<() => void>();
const emit = () => { for (const l of listeners) l(); window.dispatchEvent(new Event('island-surfaces-changed')); };
export const colorPicker = {
  open(request: Request) { active = request; emit(); },
  close() { if (active) { active = null; emit(); } },
  subscribe(l: () => void) { listeners.add(l); return () => { listeners.delete(l); }; },
  get: () => active,
};

const HEX = /^#[0-9a-f]{6}$/i;
export const isHex = (v: string) => HEX.test(v);
type Hsv = { h: number; s: number; v: number };
export function hexToHsv(hex: string): Hsv {
  const n = isHex(hex) ? parseInt(hex.slice(1), 16) : 0;
  const r = (n >> 16 & 255) / 255, g = (n >> 8 & 255) / 255, b = (n & 255) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
  let h = 0;
  if (d) h = max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return { h: h * 60, s: max ? d / max : 0, v: max };
}
export function hsvToHex({ h, s, v }: Hsv): string {
  const f = (n: number) => { const k = (n + h / 60) % 6; return v - v * s * Math.max(0, Math.min(k, 4 - k, 1)); };
  return '#' + [f(5), f(3), f(1)].map(x => Math.round(x * 255).toString(16).padStart(2, '0')).join('');
}

/** A swatch button that opens the docked picker. Same footprint as the old native input. */
export function ColorSwatch({ id, label, value, onPick, onReset, resetLabel, className = '' }: {
  id: string; label: string; value: string; onPick(hex: string): void; onReset?(): void; resetLabel?: string; className?: string;
}) {
  const open = useSyncExternalStore(colorPicker.subscribe, () => colorPicker.get()?.id === id);
  const ref = useRef<HTMLButtonElement>(null);
  // Keep the open panel's callbacks current (settings round-trips re-render the caller).
  useEffect(() => { const a = colorPicker.get(); if (open && a) { a.onPick = onPick; a.onReset = onReset; a.value = value; } });
  return <button ref={ref} type="button" id={id} className={`color-swatch ${className}`} aria-label={label} aria-expanded={open} data-open={open}
    style={{ ['--swatch' as string]: value }}
    onClick={() => {
      if (open) { colorPicker.close(); return; }
      const r = ref.current?.getBoundingClientRect();
      colorPicker.open({ id, label, value, anchorY: r ? r.top + r.height / 2 : innerHeight / 3, onPick, onReset, resetLabel });
    }}><span /></button>;
}

const PANEL_W = 248, PANEL_H = 312, GAP = 14, MARGIN = 8;
function dock(anchorY: number) {
  const island = document.querySelector<HTMLElement>('.island[data-state]')?.getBoundingClientRect();
  if (!island) return { x: innerWidth - PANEL_W - MARGIN, y: MARGIN, side: 'inside' as const };
  let right = island.right, left = island.left;
  for (const b of document.querySelectorAll<HTMLElement>('[data-side-button]')) {
    if (getComputedStyle(b).visibility === 'hidden') continue;
    const r = b.getBoundingClientRect();
    if (r.top < island.bottom && r.left >= island.right - 1) right = Math.max(right, r.right);
    if (r.top < island.bottom && r.right <= island.left + 1) left = Math.min(left, r.left);
  }
  const y = Math.max(island.top, Math.min(anchorY - 70, island.bottom - PANEL_H, innerHeight - PANEL_H - MARGIN));
  if (right + GAP + PANEL_W <= innerWidth - MARGIN) return { x: right + GAP, y, side: 'right' as const };
  if (left - GAP - PANEL_W >= MARGIN) return { x: left - GAP - PANEL_W, y, side: 'left' as const };
  return { x: Math.max(MARGIN, island.right - PANEL_W - 12), y, side: 'inside' as const };
}

function useDrag(onMove: (fx: number, fy: number) => void, onEnd: () => void) {
  return (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = e.currentTarget; el.setPointerCapture(e.pointerId);
    const at = (ev: { clientX: number; clientY: number }) => { const r = el.getBoundingClientRect();
      onMove(Math.max(0, Math.min(1, (ev.clientX - r.left) / r.width)), Math.max(0, Math.min(1, (ev.clientY - r.top) / r.height))); };
    at(e);
    const move = (ev: PointerEvent) => at(ev);
    const up = () => { el.removeEventListener('pointermove', move); el.removeEventListener('pointerup', up); el.removeEventListener('pointercancel', up); onEnd(); };
    el.addEventListener('pointermove', move); el.addEventListener('pointerup', up); el.addEventListener('pointercancel', up);
  };
}

/** Mounted once next to SideControls. */
export function ColorPickerPanel() {
  const request = useSyncExternalStore(colorPicker.subscribe, colorPicker.get);
  const state = useIslandState();
  const [hsv, setHsv] = useState<Hsv>({ h: 0, s: 0, v: 0 });
  const [text, setText] = useState('');
  const [pos, setPos] = useState<ReturnType<typeof dock> | null>(null);
  const hsvRef = useRef(hsv); hsvRef.current = hsv;
  const id = request?.id;
  useEffect(() => { if (request) { const next = hexToHsv(request.value); setHsv(next); setText(request.value.toLowerCase()); } }, [id]);
  useLayoutEffect(() => {
    if (!request) { setPos(null); return; }
    const place = () => setPos(dock(request.anchorY));
    place();
    window.addEventListener('island-geometry', place); window.addEventListener('resize', place);
    return () => { window.removeEventListener('island-geometry', place); window.removeEventListener('resize', place); };
  }, [id]);
  // The panel belongs to the open island: it goes away with a collapse or a page change.
  const view = state.view;
  const opened = useRef(view);
  useEffect(() => { if (request) opened.current = view; }, [id]);
  useEffect(() => { if (request && (state.mode !== 'large' || view !== opened.current)) colorPicker.close(); }, [state.mode, view]);
  useEffect(() => { window.dispatchEvent(new Event('island-surfaces-changed')); }, [pos?.x, pos?.y, !!request]);
  useEffect(() => {
    if (!request) return;
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); e.preventDefault(); colorPicker.close(); } };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [id]);
  const hex = hsvToHex(hsv);
  const commit = (value = hsvToHex(hsvRef.current)) => { colorPicker.get()?.onPick(value); setText(value); };
  const sv = useDrag((fx, fy) => setHsv(c => { const n = { ...c, s: fx, v: 1 - fy }; setText(hsvToHex(n)); return n; }), () => commit());
  const hue = useDrag(fx => setHsv(c => { const n = { ...c, h: Math.min(359.9, fx * 360) }; setText(hsvToHex(n)); return n; }), () => commit());
  const root = typeof document !== 'undefined' ? getComputedStyle(document.documentElement) : null;
  const presets = ['accent', 'text', 'background', 'surface', 'dim'].map(r => root?.getPropertyValue(`--skin-${r}`).trim() ?? '').filter(isHex)
    .concat(['#ffffff', '#000000', '#ffd60a', '#80ff36', '#30d5c8', '#5e9eff', '#bf5af2', '#ff453a']).map(c => c.toLowerCase()).filter((c, i, a) => a.indexOf(c) === i).slice(0, 12);
  // Re-report once the exit animation has removed the panel, or its blur/input rect lingers.
  return <AnimatePresence onExitComplete={() => window.dispatchEvent(new Event('island-surfaces-changed'))}>{request && pos && <motion.div key={request.id} className="color-pop" data-island-extension="color-picker" data-dock={pos.side}
    role="dialog" aria-label={`${request.label} picker`}
    style={{ position: 'fixed', left: pos.x, top: pos.y, width: PANEL_W, height: PANEL_H }}
    initial={{ opacity: 0, x: pos.side === 'left' ? 12 : -12, scale: .97 }} animate={{ opacity: 1, x: 0, scale: 1 }} exit={{ opacity: 0, x: pos.side === 'left' ? 8 : -8, scale: .98 }}
    transition={{ type: 'spring', visualDuration: .26, bounce: .14 }}>
    <div className="color-pop-head"><span className="color-pop-dot" style={{ background: hex }} /><strong>{request.label}</strong>
      <button type="button" className="color-pop-x" aria-label="Close colour picker" onClick={() => colorPicker.close()}>×</button></div>
    <div className="color-pop-sv" style={{ background: `hsl(${hsv.h} 100% 50%)` }} onPointerDown={sv} aria-label="Saturation and brightness" role="slider"
      aria-valuetext={hex}><i style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, background: hex }} /></div>
    <div className="color-pop-hue" onPointerDown={hue} role="slider" aria-label="Hue" aria-valuemin={0} aria-valuemax={360} aria-valuenow={Math.round(hsv.h)}>
      <i style={{ left: `${hsv.h / 3.6}%`, background: `hsl(${hsv.h} 100% 50%)` }} /></div>
    <div className="color-pop-presets">{presets.map(c => <button key={c} type="button" aria-label={`Use ${c}`} title={c} style={{ background: c }} data-on={c.toLowerCase() === hex}
      onClick={() => { const n = hexToHsv(c); setHsv(n); commit(c.toLowerCase()); }} />)}</div>
    <form className="color-pop-row" onSubmit={e => { e.preventDefault(); const v = text.trim().toLowerCase(); const full = v.startsWith('#') ? v : `#${v}`; if (isHex(full)) { setHsv(hexToHsv(full)); commit(full); } }}>
      <input aria-label="Hex colour" spellCheck={false} maxLength={7} value={text} data-invalid={!isHex(text.startsWith('#') ? text : `#${text}`)}
        onChange={e => { const v = e.target.value; setText(v); const full = v.startsWith('#') ? v : `#${v}`; if (isHex(full)) { setHsv(hexToHsv(full)); } }}
        onBlur={() => { const full = text.startsWith('#') ? text : `#${text}`; if (isHex(full) && full.toLowerCase() !== request.value.toLowerCase()) commit(full.toLowerCase()); }} />
      {request.onReset && <button type="button" className="color-pop-link" onClick={() => { request.onReset?.(); colorPicker.close(); }}>{request.resetLabel ?? 'Follow skin'}</button>}
      <button type="button" className="color-pop-done" onClick={() => { const full = text.startsWith('#') ? text : `#${text}`; if (isHex(full)) commit(full.toLowerCase()); colorPicker.close(); }}>Done</button>
    </form>
  </motion.div>}</AnimatePresence>;
}
