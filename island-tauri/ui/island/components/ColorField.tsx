import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode, type PointerEvent as ReactPointerEvent } from 'react';
import { AnimatePresence, motion, useIsPresent } from 'framer-motion';
import { useIslandState } from '../../ipc/island-state';
import { useReducedMotionAll } from './motion-preference';
import { DUR, tween } from '../motion-profile';

/**
 * In-island colour picker. On Wayland the native GTK colour chooser that `<input type=color>`
 * opens is a normal toplevel and lands BEHIND the keep-above island, so it was unusable.
 * This panel is drawn by the island itself, docked beside it like an extension of the app
 * (outside its right edge and side buttons when there is room), and joins the input/blur
 * region through `[data-island-extension]` in `geometryReport`.
 */
type Request = { id: string; label: string; value: string; anchorX: number; anchorY: number; onPick(hex: string): void; onReset?(): void; resetLabel?: string };
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
export function parseHexInput(value: string): string | null {
  const text = value.trim().toLowerCase(), full = text.startsWith('#') ? text : `#${text}`;
  return isHex(full) ? full : null;
}
type Hsv = { h: number; s: number; v: number };
type Rgb = { r: number; g: number; b: number };
const channel = (value: number) => Math.round(Math.max(0, Math.min(255, Number.isFinite(value) ? value : 0)));
export function hexToRgb(hex: string): Rgb {
  const n = isHex(hex) ? parseInt(hex.slice(1), 16) : 0;
  return { r: n >> 16 & 255, g: n >> 8 & 255, b: n & 255 };
}
export function rgbToHex({ r, g, b }: Rgb): string {
  return '#' + [r, g, b].map(x => channel(x).toString(16).padStart(2, '0')).join('');
}
export function rgbToHsv(rgb: Rgb): Hsv {
  const r = channel(rgb.r) / 255, g = channel(rgb.g) / 255, b = channel(rgb.b) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
  let h = 0;
  if (d) h = max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return { h: h * 60, s: max ? d / max : 0, v: max };
}
export const hexToHsv = (hex: string): Hsv => rgbToHsv(hexToRgb(hex));
export function hsvToRgb({ h, s, v }: Hsv): Rgb {
  h = ((h % 360) + 360) % 360; s = Math.max(0, Math.min(1, s)); v = Math.max(0, Math.min(1, v));
  const f = (n: number) => { const k = (n + h / 60) % 6; return channel((v - v * s * Math.max(0, Math.min(k, 4 - k, 1))) * 255); };
  return { r: f(5), g: f(3), b: f(1) };
}
export const hsvToHex = (hsv: Hsv): string => rgbToHex(hsvToRgb(hsv));

/** A swatch button that opens the docked picker. Same footprint as the old native input. */
export function ColorSwatch({ id, label, value, onPick, onReset, resetLabel, following = false, className = '' }: {
  id: string; label: string; value: string; onPick(hex: string): void; onReset?(): void; resetLabel?: string; following?: boolean; className?: string;
}) {
  const open = useSyncExternalStore(colorPicker.subscribe, () => colorPicker.get()?.id === id);
  const ref = useRef<HTMLButtonElement>(null);
  // Keep the open panel's callbacks current (settings round-trips re-render the caller).
  useEffect(() => { const a = colorPicker.get(); if (open && a) { a.onPick = onPick; a.onReset = onReset; a.value = value; a.label = label; a.resetLabel = resetLabel; } }, [open, onPick, onReset, value, label, resetLabel]);
  return <button ref={ref} type="button" id={id} className={`color-swatch ${className}`} aria-label={label} aria-expanded={open} aria-haspopup="dialog" aria-controls={open ? 'color-picker-panel' : undefined} data-open={open} data-following={following || undefined}
    title={following ? "Follow skin — showing the skin's colour" : undefined}
    style={{ ['--swatch' as string]: value }}
    onClick={() => {
      if (open) { colorPicker.close(); return; }
      const r = ref.current?.getBoundingClientRect();
      colorPicker.open({ id, label, value, anchorX: r ? r.left + r.width / 2 : innerWidth / 2, anchorY: r ? r.top + r.height / 2 : innerHeight / 3, onPick, onReset, resetLabel });
    }}><span /></button>;
}

// The source palette intentionally repeats colours. Indices identify cells, not hexes.
export const APPLE_GRID_COLORS = [ '#FFFFFF', '#EBEBEB', '#D6D6D6', '#C2C2C2', '#ADADAD', '#999999', '#858585', '#707070', '#5C5C5C', '#474747', '#333333', '#000000', '#003366', '#336699', '#3366CC', '#003399', '#000099', '#0000CC', '#000066', '#333366', '#663399', '#660099', '#330066', '#330033', '#006699', '#0099CC', '#0066CC', '#0033CC', '#0000FF', '#3333FF', '#333399', '#6633CC', '#9933CC', '#9900CC', '#6600CC', '#660066', '#0099CC', '#00CCFF', '#0099FF', '#0066FF', '#3366FF', '#6666FF', '#6666CC', '#9966CC', '#CC66FF', '#CC33FF', '#9900FF', '#990099', '#33CCCC', '#66FFFF', '#33CCFF', '#3399FF', '#6699FF', '#9999FF', '#9999CC', '#CC99FF', '#FF99FF', '#FF66FF', '#CC00FF', '#CC00CC', '#66CCCC', '#99FFFF', '#66CCFF', '#6699FF', '#99CCFF', '#CCCCFF', '#CC99CC', '#FFCCFF', '#FF99FF', '#FF66FF', '#FF33FF', '#FF00FF', '#99CCCC', '#CCFFFF', '#99CCFF', '#9999FF', '#CCCCFF', '#FFFFFF', '#FFCCFF', '#FF99FF', '#FF66FF', '#FF00FF', '#CC00CC', '#990099', '#CCFFCC', '#FFFFCC', '#FFFF99', '#FFFF66', '#FFFF33', '#FFFF00', '#FFCC00', '#FF9900', '#FF6600', '#FF3300', '#FF0000', '#CC0000', '#99FF99', '#CCFF99', '#CCCC66', '#CCCC33', '#CCCC00', '#CC9900', '#CC6600', '#CC3300', '#CC0000', '#990000', '#660000', '#330000', '#66FF66', '#99FF66', '#99CC66', '#99CC33', '#999900', '#996600', '#993300', '#990000', '#660000', '#330000', '#000000', '#000000', '#33FF33', '#66FF33', '#66CC33', '#669933', '#666600', '#663300', '#660000', '#330000', '#000000', '#000000', '#000000', '#000000', '#00FF00', '#33FF00', '#33CC00', '#339900', '#336600', '#333300', '#330000', '#000000', '#000000', '#000000', '#000000', '#000000' ];
export function gridSelection(hex: string) {
  return APPLE_GRID_COLORS.findIndex(color => color.toLowerCase() === hex.toLowerCase());
}
export function gridNavigation(index: number, key: string, count = APPLE_GRID_COLORS.length): number {
  if (!count) return -1;
  const last = count - 1, current = Math.max(0, Math.min(last, index));
  switch (key) {
    case 'ArrowLeft': return Math.max(0, current - 1);
    case 'ArrowRight': return Math.min(last, current + 1);
    case 'ArrowUp': return Math.max(0, current - 12);
    case 'ArrowDown': return Math.min(last, current + 12);
    case 'Home': return Math.floor(current / 12) * 12;
    case 'End': return Math.min(last, Math.floor(current / 12) * 12 + 11);
    default: return -1;
  }
}
export function ColorGrid({ hex, onPick }: { hex: string; onPick(hex: string): void }) {
  const selected = gridSelection(hex);
  const [focus, setFocus] = useState(Math.max(0, selected));
  return <div className="color-pop-grid" role="radiogroup" aria-label="Colour grid">
    {APPLE_GRID_COLORS.map((color, index) => <button key={index} type="button" role="radio" aria-label={`${color.toLowerCase()}, row ${Math.floor(index / 12) + 1}, column ${index % 12 + 1}`}
      aria-checked={index === selected} tabIndex={index === focus ? 0 : -1} style={{ background: color }}
      onFocus={() => setFocus(index)} onClick={() => { setFocus(index); onPick(color.toLowerCase()); }}
      onKeyDown={event => {
        const next = gridNavigation(index, event.key);
        if (next < 0) return;
        event.preventDefault(); setFocus(next);
        (event.currentTarget.parentElement?.children[next] as HTMLElement | undefined)?.focus();
      }} />)}
  </div>;
}

const RECENT_KEY = 'monolith.colorPicker.recent';
export function recentColors(values: unknown): string[] {
  return Array.isArray(values) ? [...new Set(values.filter((value): value is string => typeof value === 'string' && isHex(value)).map(value => value.toLowerCase()))].slice(0, 12) : [];
}
export function addRecentColor(values: unknown, value: string): string[] {
  return recentColors(isHex(value) ? [value, ...recentColors(values)] : values);
}
function readRecent(): string[] {
  try { return recentColors(JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]')); } catch { return []; }
}

const TABS = ['Grid', 'Spectrum', 'Sliders'] as const;
type Tab = typeof TABS[number];
const TAB_KEY = 'monolith.colorPicker.tab';
function readTab(): Tab {
  try { const tab = localStorage.getItem(TAB_KEY); if (TABS.includes(tab as Tab)) return tab as Tab; } catch { /* Storage may be unavailable. */ }
  return 'Grid';
}
function savePreference(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* Picking still works without storage. */ }
}

export function ColorTabPanel({ tab, instant, children }: { tab: Tab; instant: boolean; children: ReactNode }) {
  const present = useIsPresent();
  const transition = instant ? { duration: 0 } : { ...tween(DUR.fast), duration: Math.min(.18, tween(DUR.fast).duration) };
  return <motion.div className="color-pop-tab-panel" role="tabpanel" id={`color-body-${tab}`} aria-labelledby={`color-tab-${tab}`}
    aria-hidden={!present || undefined} inert={!present}
    initial={{ opacity: instant ? 1 : 0, transform: instant ? 'translateX(0px)' : 'translateX(4px)' }}
    animate={{ opacity: 1, transform: 'translateX(0px)' }} exit="hidden"
    variants={{ hidden: ({ instant: skip }: { instant: boolean }) => ({ opacity: 0, transform: skip ? 'translateX(0px)' : 'translateX(-4px)', transition: skip ? { duration: 0 } : transition }) }}
    transition={transition}>{children}</motion.div>;
}

export const PANEL_W = 280, PANEL_H = 420;
const GAP = 14, MARGIN = 8;
export function dock(anchorY: number) {
  const width = Math.max(1, Math.min(PANEL_W, innerWidth - 2 * MARGIN));
  const height = Math.max(1, Math.min(PANEL_H, innerHeight - 2 * MARGIN));
  const clampX = (x: number) => Math.max(MARGIN, Math.min(x, innerWidth - width - MARGIN));
  const clampY = (y: number) => Math.max(MARGIN, Math.min(y, innerHeight - height - MARGIN));
  const island = document.querySelector<HTMLElement>('.island[data-state]')?.getBoundingClientRect();
  if (!island) return { x: clampX(innerWidth - width - MARGIN), y: MARGIN, width, height, side: 'inside' as const };
  let right = island.right, left = island.left;
  for (const b of document.querySelectorAll<HTMLElement>('[data-side-button]')) {
    if (getComputedStyle(b).visibility === 'hidden') continue;
    const r = b.getBoundingClientRect();
    if (r.top < island.bottom && r.left >= island.right - 1) right = Math.max(right, r.right);
    if (r.top < island.bottom && r.right <= island.left + 1) left = Math.min(left, r.left);
  }
  const y = clampY(Math.max(island.top, Math.min(anchorY - 70, island.bottom - height)));
  if (right + GAP + width <= innerWidth - MARGIN) return { x: right + GAP, y, width, height, side: 'right' as const };
  if (left - GAP - width >= MARGIN) return { x: left - GAP - width, y, width, height, side: 'left' as const };
  return { x: clampX(island.right - width - 12), y, width, height, side: 'inside' as const };
}

function useDrag(onMove: (fx: number, fy: number) => void, onEnd: () => void, scope: string) {
  const cleanup = useRef<(() => void) | null>(null);
  useEffect(() => () => { cleanup.current?.(); cleanup.current = null; }, [scope]);
  return (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button !== 0 || !e.isPrimary || cleanup.current) return;
    e.preventDefault();
    const el = e.currentTarget, pointer = e.pointerId, rect = el.getBoundingClientRect();
    el.focus(); el.setPointerCapture(pointer);
    const at = (ev: { clientX: number; clientY: number }) => onMove(
      Math.max(0, Math.min(1, (ev.clientX - rect.left) / rect.width)),
      Math.max(0, Math.min(1, (ev.clientY - rect.top) / rect.height)));
    const move = (ev: PointerEvent) => { if (ev.pointerId === pointer) at(ev); };
    const clear = () => {
      el.removeEventListener('pointermove', move); el.removeEventListener('pointerup', up);
      el.removeEventListener('pointercancel', up); el.removeEventListener('lostpointercapture', up);
      cleanup.current = null;
      if (el.hasPointerCapture(pointer)) el.releasePointerCapture(pointer);
    };
    const up = (ev: PointerEvent) => {
      if (ev.pointerId !== pointer) return;
      if (ev.type === 'pointerup') at(ev);
      clear(); onEnd();
    };
    cleanup.current = clear;
    el.addEventListener('pointermove', move); el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up); el.addEventListener('lostpointercapture', up);
    at(e);
  };
}

/** Mounted once next to SideControls. */
export function ColorPickerPanel() {
  const request = useSyncExternalStore(colorPicker.subscribe, colorPicker.get);
  const state = useIslandState();
  const reduced = useReducedMotionAll();
  const keyboard = typeof document !== 'undefined' && document.documentElement.dataset.navVia === 'key';
  const [hsv, setHsv] = useState<Hsv>({ h: 0, s: 0, v: 0 });
  const [sampling, setSampling] = useState(false);
  const [text, setText] = useState('');
  const [recent, setRecent] = useState<string[]>(readRecent);
  const recentRef = useRef(recent);
  const remember = (value: string) => { const next = addRecentColor(recentRef.current, value); recentRef.current = next; setRecent(next); savePreference(RECENT_KEY, JSON.stringify(next)); };
  const [tab, setTab] = useState<Tab>(readTab);
  const [tabViaKey, setTabViaKey] = useState(false);
  const instantTab = reduced || keyboard || tabViaKey;
  const selectTab = (next: Tab, viaKey = false) => { setTabViaKey(viaKey); setTab(next); savePreference(TAB_KEY, next); };
  const [pos, setPos] = useState<ReturnType<typeof dock> | null>(null);
  const hsvRef = useRef(hsv); hsvRef.current = hsv;
  const id = request?.id;
  const closePanel = () => { const anchor = colorPicker.get()?.id; colorPicker.close(); if (anchor) document.getElementById(anchor)?.focus({ preventScroll: true }); };
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
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); e.preventDefault(); closePanel(); } };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [id]);
  useEffect(() => {
    if (request && pos) document.querySelector<HTMLElement>('.color-pop [role="tab"][aria-selected="true"]')?.focus({ preventScroll: true });
  }, [id, !!pos]);
  const hex = hsvToHex(hsv), rgb = hexToRgb(hex);
  const rgbDragging = useRef(false);
  const commit = (value = hsvToHex(hsvRef.current)) => {
    if (!colorPicker.get() || !isHex(value)) return;
    value = value.toLowerCase(); colorPicker.get()?.onPick(value); setText(value); remember(value);
  };
  const preview = (next: Hsv) => { hsvRef.current = next; setHsv(next); setText(hsvToHex(next)); };
  const pick = (value: string) => { preview(hexToHsv(value)); commit(value.toLowerCase()); };
  // Settings store opaque #rrggbb values, so this picker deliberately has no opacity control.
  const hasEyeDropper = typeof window !== 'undefined' && 'EyeDropper' in window;
  const sampleScreen = async () => {
    if (!hasEyeDropper || sampling) return;
    const owner = colorPicker.get(); setSampling(true);
    try {
      const { EyeDropper } = window as unknown as { EyeDropper: new () => { open(): Promise<{ sRGBHex: string }> } };
      const result = await new EyeDropper().open();
      if (colorPicker.get() === owner && isHex(result.sRGBHex)) pick(result.sRGBHex);
    } catch { /* Cancelling a sample leaves the current colour in place. */ }
    finally { setSampling(false); }
  };
  const submitHex = () => { const value = parseHexInput(text); if (value) pick(value); };
  const hexField = (label: string) => <input aria-label={label} spellCheck={false} maxLength={7} value={text}
    aria-invalid={!parseHexInput(text)} data-invalid={!parseHexInput(text)}
    onChange={e => {
      const value = e.target.value; setText(value); const full = parseHexInput(value);
      if (full) { const next = hexToHsv(full); hsvRef.current = next; setHsv(next); }
    }} onBlur={submitHex} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); submitHex(); } }} />;
  const dragScope = `${id}:${tab}`;
  const sv = useDrag((fx, fy) => preview({ ...hsvRef.current, s: fx, v: 1 - fy }), () => commit(), dragScope);
  const hue = useDrag(fx => preview({ ...hsvRef.current, h: Math.min(359.9, fx * 360) }), () => commit(), dragScope);
  const changeChannel = (key: keyof Rgb, value: number, final = false) => {
    const valueHex = rgbToHex({ ...hexToRgb(hsvToHex(hsvRef.current)), [key]: channel(value) });
    preview(hexToHsv(valueHex)); if (final) commit(valueHex);
  };
  const root = typeof document !== 'undefined' ? getComputedStyle(document.documentElement) : null;
  const presets = ['accent', 'text', 'background', 'surface', 'dim'].map(r => root?.getPropertyValue(`--skin-${r}`).trim() ?? '').filter(isHex)
    .concat(['#ffffff', '#000000', '#ffd60a', '#80ff36', '#30d5c8', '#5e9eff', '#bf5af2', '#ff453a']).map(c => c.toLowerCase()).filter((c, i, a) => a.indexOf(c) === i).slice(0, 12);
  // Re-report once the exit animation has removed the panel, or its blur/input rect lingers.
  return <AnimatePresence custom={{ keyboard, reduced }} onExitComplete={() => window.dispatchEvent(new Event('island-surfaces-changed'))}>{request && pos && <motion.div key={request.id} className="color-pop" data-island-extension="color-picker" data-dock={pos.side}
    id="color-picker-panel" role="dialog" aria-modal="false" aria-label={`${request.label} picker`}
    style={{ position: 'fixed', left: pos.x, top: pos.y, width: pos.width, height: pos.height,
      transformOrigin: `${Math.max(0, Math.min(pos.width, request.anchorX - pos.x))}px ${Math.max(0, Math.min(pos.height, request.anchorY - pos.y))}px` }}
    initial={{ opacity: 0, x: reduced || keyboard ? 0 : pos.side === 'left' ? 12 : -12, scale: reduced || keyboard ? 1 : .97 }} animate={{ opacity: 1, x: 0, scale: 1 }}
    exit="closed" variants={{ closed: ({ keyboard, reduced }: { keyboard: boolean; reduced: boolean }) => ({
      opacity: 0, x: reduced || keyboard ? 0 : pos.side === 'left' ? 8 : -8, scale: reduced || keyboard ? 1 : .98,
      transition: reduced || keyboard ? { duration: 0 } : tween(DUR.fast),
    }) }}
    transition={reduced || keyboard ? { duration: 0 } : tween(DUR.base)}>
    <div className="color-pop-head"><span className="color-pop-dot" style={{ background: hex }} /><strong>{request.label}</strong>
      {hasEyeDropper && <button type="button" className="color-pop-pipette" aria-label="Pick colour from screen" disabled={sampling} onClick={sampleScreen}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="m2 22 1-1h3l9-9M3 21v-3l9-9m3-3 3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4Z" />
        </svg>
      </button>}
      <button type="button" className="color-pop-x" aria-label="Close colour picker" onClick={closePanel}>×</button></div>
    <div className="color-pop-tabs" data-instant={instantTab} role="tablist" aria-label="Colour controls">
      <span className="color-pop-pill" aria-hidden="true" style={{ transform: `translateX(${TABS.indexOf(tab) * 100}%)` }} />
      {TABS.map((name, i) => <button key={name} type="button" role="tab" id={`color-tab-${name}`} aria-controls={`color-body-${name}`}
        aria-selected={tab === name} tabIndex={tab === name ? 0 : -1} onClick={e => selectTab(name, e.detail === 0)}
        onKeyDown={e => {
          const next = e.key === 'ArrowRight' ? (i + 1) % 3 : e.key === 'ArrowLeft' ? (i + 2) % 3 : e.key === 'Home' ? 0 : e.key === 'End' ? 2 : -1;
          if (next < 0) return;
          e.preventDefault(); selectTab(TABS[next]!, true); document.getElementById(`color-tab-${TABS[next]}`)?.focus();
        }}>{name}</button>)}
    </div>
    <div className="color-pop-body">
      <AnimatePresence initial={false} custom={{ instant: instantTab }}>
      <ColorTabPanel key={tab} tab={tab} instant={instantTab}>
      {tab === 'Grid' && <ColorGrid hex={hex} onPick={pick} />}
      {tab === 'Sliders' && <div className="color-pop-sliders">
        {(['r', 'g', 'b'] as const).map((key, i) => {
          const label = ['Red', 'Green', 'Blue'][i];
          const low = { ...rgb, [key]: 0 }, high = { ...rgb, [key]: 255 };
          return <div className="color-pop-channel" key={key}>
            <label htmlFor={`color-channel-${key}`}>{key.toUpperCase()}</label>
            <input id={`color-channel-${key}`} type="range" aria-label={label} min={0} max={255} step={1} value={rgb[key]}
              style={{ background: `linear-gradient(to right, ${rgbToHex(low)}, ${rgbToHex(high)})` }}
              onPointerDown={e => { if (e.button !== 0 || !e.isPrimary) return; rgbDragging.current = true; e.currentTarget.setPointerCapture(e.pointerId); }}
              onChange={e => changeChannel(key, Number(e.target.value), !rgbDragging.current)}
              onPointerUp={() => { rgbDragging.current = false; commit(); }} onPointerCancel={() => { rgbDragging.current = false; commit(); }}
              onLostPointerCapture={() => { if (rgbDragging.current) { rgbDragging.current = false; commit(); } }}
              onBlur={() => { if (rgbDragging.current) { rgbDragging.current = false; commit(); } }} />
            <input type="number" aria-label={`${label} value`} min={0} max={255} step={1} value={rgb[key]}
              onChange={e => changeChannel(key, Number(e.target.value), true)} />
          </div>;
        })}
        <label className="color-pop-slider-hex">Hex {hexField('Sliders hex colour')}</label>
      </div>}
      {tab === 'Spectrum' && <div className="color-pop-spectrum">
        <div className="color-pop-sv" style={{ backgroundColor: `hsl(${hsv.h} 100% 50%)` }} onPointerDown={sv} aria-label="Saturation and brightness" role="slider" tabIndex={0}
          aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(hsv.s * 100)}
          aria-valuetext={`${Math.round(hsv.s * 100)}% saturation, ${Math.round(hsv.v * 100)}% brightness, ${hex}`}
          onKeyDown={e => {
            if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) return;
            e.preventDefault();
            const delta = e.shiftKey ? .1 : .01;
            preview({ ...hsvRef.current, s: Math.max(0, Math.min(1, hsvRef.current.s + (e.key === 'ArrowRight' ? delta : e.key === 'ArrowLeft' ? -delta : 0))),
              v: Math.max(0, Math.min(1, hsvRef.current.v + (e.key === 'ArrowUp' ? delta : e.key === 'ArrowDown' ? -delta : 0))) }); commit();
          }}><i style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, background: hex }} /></div>
        <div className="color-pop-hue" onPointerDown={hue} tabIndex={0} onKeyDown={e => {
          if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) return;
          e.preventDefault();
          const delta = (e.shiftKey ? 10 : 1) * (e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -1 : 1);
          preview({ ...hsvRef.current, h: e.key === 'Home' ? 0 : e.key === 'End' ? 359.9 : (hsvRef.current.h + delta + 360) % 360 }); commit();
        }} role="slider" aria-label="Hue" aria-valuemin={0} aria-valuemax={360} aria-valuenow={Math.round(hsv.h)}>
          <i style={{ left: `${hsv.h / 3.6}%`, background: `hsl(${hsv.h} 100% 50%)` }} /></div>
      </div>}
      </ColorTabPanel>
      </AnimatePresence>
    </div>
    <div className="color-pop-collection" role="group" aria-label="Recent colours">
      <span className="color-pop-caption">Recent colours</span>
      <div className="color-pop-recent">
        {recent.map(c => <button key={c} type="button" aria-label={`Recent ${c}`} style={{ background: c }} data-on={c === hex} onClick={() => pick(c)} />)}
        <button type="button" className="color-pop-add" aria-label="Pin current colour to recent colours" onClick={() => remember(hex)}>+</button>
      </div>
    </div>
    <div className="color-pop-collection" role="group" aria-label="Theme preset colours">
      <span className="color-pop-caption">Theme presets</span>
      <div className="color-pop-presets">{presets.map(c => <button key={c} type="button" aria-label={`Use ${c}`} data-tooltip={c} style={{ background: c }} data-on={c === hex}
        onClick={() => pick(c)} />)}</div>
    </div>
    <form className="color-pop-row" onSubmit={e => { e.preventDefault(); submitHex(); }}>
      {hexField('Hex colour')}
      {request.onReset && <button type="button" className="color-pop-link" title={request.resetLabel ?? 'Follow skin'} onClick={() => { request.onReset?.(); closePanel(); }}>{request.resetLabel ?? 'Follow skin'}</button>}
      <button type="button" className="color-pop-done" onClick={() => { submitHex(); closePanel(); }}>Done</button>
    </form>
  </motion.div>}</AnimatePresence>;
}
