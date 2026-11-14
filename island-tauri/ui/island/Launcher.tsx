import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type RefObject, type PointerEvent as ReactPointerEvent } from 'react';
import { createPortal } from 'react-dom';
import { motion, LayoutGroup, AnimatePresence, useAnimationControls } from 'framer-motion';
import { launcherCommand, type LauncherOp, type IslandView, type LauncherItem } from '../ipc/client';
import { useIslandState } from '../ipc/island-state';
import { DUR, SPRING, tween } from './motion-profile';
import { wiggleTiming } from './launcher-motion';
import { launcherStore, useLauncher } from '../stores/apps';
import { Icon } from './components/Icon';
import { useReducedMotionPreference } from './components/motion-preference';
import type { IconName } from './models';
export const viewLabels: Record<IslandView,string> = { terminal:'Hermes',monitor:'System monitor',ai:'AI Agents',settings:'Settings',launcher:'Apps',notifications:'Notifications',clipboard:'Clipboard',music:'Now playing',quickInfo:'Quick info',timer:'Timer',calendar:'Calendar',scratchpad:'Scratchpad',downloads:'Downloads',controls:'Controls',mixer:'Volume mixer',brightness:'Brightness',agents:'Agent runs',updates:'Package updates',shelf:'Shelf' };
export const viewIcons: Partial<Record<IslandView,IconName>> = { terminal:'terminal',monitor:'monitor',ai:'spark',settings:'settings',notifications:'bell',clipboard:'clipboard',music:'music',timer:'timer',calendar:'calendar',scratchpad:'note',downloads:'download',controls:'controls',mixer:'volume',brightness:'sun',agents:'spark',updates:'download',quickInfo:'weather',launcher:'grid',shelf:'shelf' };
// Tile tints (Nov 9): every app has its own system colour (motion-launcher.css --app-*);
// Hermes keeps the theme accent. Tiles stay neutral, only the glyph is coloured.
const tint: Partial<Record<IslandView,string>> = Object.fromEntries((['monitor','ai','agents','settings','notifications','clipboard','music','timer','calendar',
  'scratchpad','downloads','shelf','updates','controls','mixer','brightness','quickInfo'] as IslandView[]).map(v => [v, `var(--app-${v})`]));
tint.terminal = 'var(--ui-accent)';
const tone = (view: IslandView | null | undefined) => (view && tint[view]) || 'var(--tile-ink, rgb(255 255 255 / .85))';
const LONG_PRESS_MS = 450;

type Drag = { id: string; from: 'shown' | 'hidden'; pointer: number; grabX: number; grabY: number; x: number; y: number; w: number };
type Landing = { id: string; x: number; y: number; w: number; scale: number; vx?: number; vy?: number };
const label = (i: LauncherItem) => i.view ? viewLabels[i.view] : i.id;
/** Only Hermes home is pinned in place. Settings is `locked` (can't be hidden) but moves freely. */
const fixed = (i: LauncherItem) => i.id === 'terminal';
/** Insert `id` into `list` at `index` (removing it first). */
function place(list: string[], id: string, index: number) {
  const next = list.filter(v => v !== id);
  next.splice(Math.max(0, Math.min(index, next.length)), 0, id);
  return next;
}

/** Apps open out of the tile that launched them and Back settles the grid toward it again
 *  (motion-launcher.css). Origin is stored relative to the page stage; the flag is short-lived. */
let launchClear: ReturnType<typeof setTimeout> | undefined;
let lastLaunch = 0;
function markLaunch(kind: 'open' | 'return') {
  const root = document.documentElement;
  root.dataset.launch = kind;
  clearTimeout(launchClear);
  // Never just delete the flag: the page would fall back to the page-in keyframe and fade in a
  // second time. 'settled' switches the keyframe off; ViewPager clears it on the next view change.
  launchClear = setTimeout(() => { root.dataset.launch = 'settled'; }, 600);
}
function launchFrom(tile: HTMLElement | undefined) {
  const stage = tile?.closest?.('.view-stage'); if (!tile || !stage) return;
  const s = stage.getBoundingClientRect(), r = tile.getBoundingClientRect(), root = document.documentElement;
  root.style.setProperty('--launch-x', `${Math.round(r.left + r.width / 2 - s.left)}px`);
  root.style.setProperty('--launch-y', `${Math.round(r.top + r.height / 2 - s.top)}px`);
  lastLaunch = performance.now();
  markLaunch('open');
}

/** Columns: 5 per row (Nick, Nov 9); fewer only when the island is too narrow for 5.
 *  Tiles stretch to fill their column, so the count never shrinks for a short list: a row of
 *  2 hidden apps keeps the same tile size and leaves the rest of the row empty. */
export function balancedColumns(_count: number, fit: number): number {
  return Math.max(1, Math.min(5, Math.floor(fit)));
}
const TILE_MIN = 100, TILE_GAP = 8; // fixed 100px tiles (surface.css .app-grid)
function useColumns(grid: RefObject<HTMLDivElement | null>, count: number) {
  const [fit, setFit] = useState(5);
  useLayoutEffect(() => {
    const el = grid.current; if (!el || typeof ResizeObserver !== 'function') return;
    const measure = () => setFit(Math.floor((el.clientWidth + TILE_GAP + 1) / (TILE_MIN + TILE_GAP)) || 1);
    measure();
    const observer = new ResizeObserver(measure); observer.observe(el);
    return () => observer.disconnect();
  }, [grid]);
  return balancedColumns(count, fit);
}

/** Edit lives in the page title bar so the grid starts right under it. */
export function LauncherEditButton({ fail }: { fail(error: unknown): void }) {
  const state = useLauncher();
  if (state.editing) return null;
  return <button className="pill-button" aria-label="Edit apps" onClick={() => {
    void launcherCommand({ type: 'begin' }).then(next => launcherStore.apply(next, next.rev)).catch(fail);
  }}>Edit</button>;
}

export function Launcher({ navigate, fail }: { navigate(view: IslandView): void; fail(error: unknown): void }) {
  const state = useLauncher(), island = useIslandState(), reduced = useReducedMotionPreference();
  const [pageHidden, setPageHidden] = useState(() => document.hidden);
  const visible = island.mode === 'large' && island.view === 'launcher' && !pageHidden;
  const keyboard = document.documentElement.dataset.navVia === 'key', quiet = reduced || keyboard;
  const timings = useRef(new Map<string, ReturnType<typeof wiggleTiming>>());
  const byId = new Map(state.items.map(i => [i.id, i]));
  const serverShown = state.items.filter(i => i.enabled).map(i => i.id), serverHidden = state.items.filter(i => !i.enabled).map(i => i.id);
  // Local arrangement during a drag (iOS-style live reflow); otherwise mirrors the Rust preview.
  const [shown, setShown] = useState(serverShown), [hidden, setHidden] = useState(serverHidden);
  const [drag, setDrag] = useState<Drag | null>(null), [overHidden, setOverHidden] = useState(false);
  const [landing, setLanding] = useState<Landing | null>(null);
  const ghostMotion = useAnimationControls();
  const dragRef = useRef<Drag | null>(null); dragRef.current = drag;
  const arrangement = useRef({ shown, hidden }); arrangement.current = { shown, hidden };
  const press = useRef<{ timer: ReturnType<typeof setTimeout> | null; x: number; y: number; id: string; armed: boolean } | null>(null);
  const tail = useRef<Promise<unknown>>(Promise.resolve()), grid = useRef<HTMLDivElement>(null), hiddenGrid = useRef<HTMLDivElement>(null);
  const suppressClick = useRef(false);
  // Recent pointer velocity (px/s), so a dropped tile leaves the finger at the speed it was moving.
  const fling = useRef({ x: 0, y: 0, t: 0, vx: 0, vy: 0 });
  const key = JSON.stringify([serverShown, serverHidden]);
  useLayoutEffect(() => { if (!dragRef.current) { setShown(serverShown); setHidden(serverHidden); } }, [key]);
  useEffect(() => () => { if (press.current?.timer) clearTimeout(press.current.timer); }, []);
  // Coming back from an app this grid launched: settle toward its tile (before first paint).
  useLayoutEffect(() => { if (lastLaunch) { lastLaunch = 0; markLaunch('return'); } }, []);
  useEffect(() => {
    const changed = () => setPageHidden(document.hidden);
    document.addEventListener('visibilitychange', changed);
    return () => document.removeEventListener('visibilitychange', changed);
  }, []);
  useEffect(() => {
    if (visible) return;
    if (press.current?.timer) clearTimeout(press.current.timer);
    press.current = null; setDrag(null); setLanding(null); setOverHidden(false);
  }, [visible]);
  useLayoutEffect(() => {
    if (drag) { ghostMotion.set({ x: drag.x, y: drag.y, scale: 1 }); return; }
    if (!landing) return;
    if (quiet || !visible) { setLanding(null); return; }
    let cancelled = false;
    const spring = SPRING.ui();
    void ghostMotion.start({ x: landing.x, y: landing.y, scale: landing.scale,
      transition: { x: { ...spring, velocity: landing.vx ?? 0 }, y: { ...spring, velocity: landing.vy ?? 0 }, scale: spring } }).then(() => {
      if (!cancelled) setLanding(current => current === landing ? null : current);
    });
    return () => { cancelled = true; ghostMotion.stop(); };
  }, [drag?.x, drag?.y, landing, quiet, visible, ghostMotion]);

  const send = (op: LauncherOp) => {
    const run = tail.current.catch(() => undefined).then(async () => { const next = await launcherCommand(op); launcherStore.apply(next, next.rev); });
    tail.current = run.catch(error => {
      fail(error);
      const snap = launcherStore.getSnapshot();  // roll the local arrangement back to Rust's
      setShown(snap.items.filter(i => i.enabled).map(i => i.id)); setHidden(snap.items.filter(i => !i.enabled).map(i => i.id));
    });
    return run;
  };
  /** Hermes home keeps the first slot; Settings floats, so tiles may land before or after it. */
  const clampShown = (rest: string[], index: number) => {
    let lo = 0;
    while (lo < rest.length && rest[lo] === 'terminal') lo++;
    return Math.max(lo, Math.min(index, rest.length));
  };
  const pickUp = (id: string, e: { clientX: number; clientY: number; pointerId: number }, target: HTMLElement) => {
    const r = target.getBoundingClientRect();
    setLanding(null);
    target.setPointerCapture(e.pointerId);
    const from = arrangement.current.shown.includes(id) ? 'shown' as const : 'hidden' as const;
    setDrag({ id, from, pointer: e.pointerId, grabX: e.clientX - r.left, grabY: e.clientY - r.top, x: r.left, y: r.top, w: r.width });
    fling.current = { x: e.clientX, y: e.clientY, t: performance.now(), vx: 0, vy: 0 };
    suppressClick.current = true;
  };
  const inside = (el: HTMLElement | null, x: number, y: number) => { const r = el?.getBoundingClientRect(); return !!r && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom; };
  const tileUnder = (container: HTMLElement | null, x: number, y: number, self: string) => {
    const hit = [...(container?.querySelectorAll<HTMLElement>('[data-tile]') ?? [])].find(t => t.dataset.tile !== self && inside(t, x, y));
    if (!hit) return null;
    const r = hit.getBoundingClientRect();
    return { id: hit.dataset.tile!, after: x > r.left + r.width / 2 };
  };
  const move = (e: PointerEvent) => {
    const d = dragRef.current; if (!d || e.pointerId !== d.pointer) return;
    setDrag({ ...d, x: e.clientX - d.grabX, y: e.clientY - d.grabY });
    const f = fling.current, now = performance.now(), dt = now - f.t;
    if (dt > 0) fling.current = { x: e.clientX, y: e.clientY, t: now,
      vx: .7 * ((e.clientX - f.x) / dt * 1000) + .3 * f.vx, vy: .7 * ((e.clientY - f.y) / dt * 1000) + .3 * f.vy };
    const { shown: s, hidden: h } = arrangement.current;
    if (inside(hiddenGrid.current, e.clientX, e.clientY)) {
      if (byId.get(d.id)?.locked) return;   // Settings can be moved but never hidden
      setOverHidden(true);
      if (!h.includes(d.id)) { setShown(s.filter(v => v !== d.id)); setHidden([...h, d.id]); }
      return;
    }
    setOverHidden(false);
    if (!inside(grid.current, e.clientX, e.clientY)) return;
    const rest = s.filter(v => v !== d.id), hit = tileUnder(grid.current, e.clientX, e.clientY, d.id);
    const index = clampShown(rest, hit ? rest.indexOf(hit.id) + (hit.after ? 1 : 0) : s.includes(d.id) ? s.indexOf(d.id) : rest.length);
    const next = place(s, d.id, index);
    if (next.join() !== s.join()) { setShown(next); setHidden(h.filter(v => v !== d.id)); }
  };
  const drop = async (e: PointerEvent) => {
    const d = dragRef.current; if (!d || e.pointerId !== d.pointer) return;
    // Layout offsets ignore the neighbours' in-flight reflow transforms. Wiggle lives
    // on a child, so neither its rotation nor a button press shifts the landing slot.
    const container = arrangement.current.shown.includes(d.id) ? grid.current : hiddenGrid.current;
    const slot = [...(container?.querySelectorAll<HTMLElement>('[data-tile]') ?? [])].find(el => el.dataset.tile === d.id);
    if (slot && container && visible && !quiet) {
      const r = container.getBoundingClientRect();
      // Held still before letting go: no fling. Capped so a wild flick can't overshoot far.
      const f = fling.current, still = performance.now() - f.t > 80, cap = (v: number) => Math.max(-3000, Math.min(3000, v));
      setLanding({ id: d.id, x: r.left + slot.offsetLeft - container.scrollLeft, y: r.top + slot.offsetTop - container.scrollTop, w: d.w, scale: slot.offsetWidth / d.w,
        vx: still ? 0 : cap(f.vx), vy: still ? 0 : cap(f.vy) });
    }
    setDrag(null); setOverHidden(false);
    const s = arrangement.current.shown, wasShown = d.from === 'shown', nowShown = s.includes(d.id);
    try {
      if (wasShown && !nowShown) await send({ type: 'disable', id: d.id });
      else if (!wasShown && nowShown) { await send({ type: 'enable', id: d.id }); await send({ type: 'move', id: d.id, to: s.indexOf(d.id) }); }
      else if (nowShown && serverShown.indexOf(d.id) !== s.indexOf(d.id)) await send({ type: 'move', id: d.id, to: s.indexOf(d.id) });
    } catch { /* rolled back in send */ }
    setTimeout(() => { suppressClick.current = false; }, 0);
  };
  useEffect(() => {
    if (!drag) return;
    const up = (e: PointerEvent) => { void drop(e); };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up); window.addEventListener('pointercancel', up);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); window.removeEventListener('pointercancel', up); };
  }, [drag?.id, drag?.pointer]);

  const onDown = (item: LauncherItem, e: ReactPointerEvent<HTMLButtonElement>) => {
    if (e.button !== 0 || (fixed(item) && state.editing)) return;
    const target = e.currentTarget, pointerId = e.pointerId;
    press.current = { timer: null, x: e.clientX, y: e.clientY, id: item.id, armed: state.editing };
    if (!state.editing) press.current.timer = setTimeout(() => {   // iOS: hold to enter jiggle mode, keep holding to drag
      const p = press.current; if (!p) return;
      p.armed = true; p.timer = null; suppressClick.current = true; void send({ type: 'begin' });
      if (!fixed(item)) { press.current = null; pickUp(item.id, { clientX: p.x, clientY: p.y, pointerId }, target); }
    }, LONG_PRESS_MS);
  };
  const onMove = (item: LauncherItem, e: ReactPointerEvent<HTMLButtonElement>) => {
    const p = press.current; if (!p || p.id !== item.id || dragRef.current) return;
    if (Math.hypot(e.clientX - p.x, e.clientY - p.y) <= 6) return;
    if (p.armed && !fixed(item)) { press.current = null; pickUp(item.id, e, e.currentTarget); }
    else { if (p.timer) clearTimeout(p.timer); press.current = null; }
  };
  const onUp = () => { if (press.current?.timer) clearTimeout(press.current.timer); press.current = null; setTimeout(() => { if (!dragRef.current) suppressClick.current = false; }, 0); };

  const tile = (id: string, index: number, zone: 'shown' | 'hidden') => {
    // The Controls app was retired (Nov 2): its tile stays in the arrangement so
    // Rust's move indices are unchanged, but it is never drawn.
    const item = byId.get(id); if (!item || item.view === 'controls') return null;
    const view = item.view, name = label(item), dragging = drag?.id === id || landing?.id === id;
    if (!timings.current.has(id)) timings.current.set(id, wiggleTiming(Math.random(), Math.random()));
    return <motion.div key={id} layout={!quiet} className="tile-wrap" data-tile={id} data-disabled={zone === 'hidden'} style={{ opacity: dragging ? 0 : 1 }}
      transition={quiet ? { duration: 0 } : SPRING.ui()}>
      <div className="tile-face" data-wiggle={state.editing && !fixed(item) && !dragging} style={timings.current.get(id) as CSSProperties}>
      <button className="app-tile" style={{ ['--tone' as string]: tone(view) }}
        aria-label={`Open ${name}`} data-testid={zone === 'shown' ? `view-${view ?? id}` : `hidden-${view ?? id}`} aria-disabled={!item.known || zone === 'hidden'} data-locked={item.locked}
        onPointerDown={e => onDown(item, e)} onPointerMove={e => onMove(item, e)} onPointerUp={onUp} onPointerCancel={onUp}
        onKeyDown={e => {
          if (!state.editing || zone !== 'shown' || fixed(item) || !e.altKey) return;
          const to = e.key === 'ArrowLeft' ? index - 1 : e.key === 'ArrowRight' ? index + 1 : -1;
          if (to >= 0) { e.preventDefault(); void send({ type: 'move', id, to }); }
        }}
        onClick={e => { if (suppressClick.current) return; if (!state.editing && zone === 'shown' && item.known && view) { launchFrom(e?.currentTarget); navigate(view); } }}>
        <Icon name={view ? viewIcons[view] ?? 'grid' : 'grid'} /><span>{name}</span>
      </button>
      <AnimatePresence>{state.editing && !item.locked && <motion.button key="badge" initial={{ scale: quiet ? 1 : 0.9, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} exit={{ scale: quiet ? 1 : 0.9, opacity: 0 }} transition={tween(keyboard ? 0 : DUR.fast)}
        className={`tile-badge ${zone === 'shown' ? 'remove' : 'add'}`} aria-label={zone === 'shown' ? `Hide ${name}` : `Show ${name}`}
        onPointerDown={e => e.stopPropagation()} onClick={() => { void send(zone === 'shown' ? { type: 'disable', id } : { type: 'enable', id }).catch(() => undefined); }}>{zone === 'shown' ? '−' : '+'}</motion.button>}</AnimatePresence>
      {state.editing && zone === 'shown' && !fixed(item) && <span className="sr-only">
        <button aria-label={`Move ${name} earlier`} disabled={index === 0} onClick={() => { void send({ type: 'move', id, to: index - 1 }).catch(() => undefined); }}>←</button>
        <button aria-label={`Move ${name} later`} disabled={index === shown.length - 1} onClick={() => { void send({ type: 'move', id, to: index + 1 }).catch(() => undefined); }}>→</button></span>}
      </div>
    </motion.div>;
  };
  const ghost = drag ?? landing, dragged = ghost ? byId.get(ghost.id) : undefined;
  const drawn = (ids: string[]) => ids.filter(id => { const it = byId.get(id); return it && it.view !== 'controls'; }).length;
  const shownCols = useColumns(grid, drawn(shown) + (state.editing ? 0 : 1)), hiddenCols = useColumns(hiddenGrid, Math.max(drawn(hidden), 1));
  return <div className="launcher" data-editing={state.editing} data-motion-active={visible && state.editing && !reduced} data-reduced-motion={reduced} onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); e.preventDefault(); if (state.editing) void send({ type: 'cancel' }).catch(() => undefined); } }}>
    <LayoutGroup>
      <div ref={grid} className="app-grid" aria-label="Apps" data-editing={state.editing} data-cols={shownCols} style={{ ['--cols' as string]: shownCols }}>
        {state.editing && <div className="launcher-toolbar"><span>Drag to arrange · drop in Hidden to hide</span>
          <button className="pill-button" onClick={() => { void send({ type: 'cancel' }).catch(() => undefined); }}>Cancel</button><button className="pill-button primary" onClick={() => { void send({ type: 'commit' }).catch(() => undefined); }}>Done</button>
        </div>}
        {shown.map((id, i) => tile(id, i, 'shown'))}
        {!state.editing && <motion.div layout={!quiet} transition={quiet ? { duration: 0 } : SPRING.ui()} className="tile-wrap"><button className="app-tile" data-testid="view-quickInfo" style={{ ['--tone' as string]: tone('quickInfo') }} onClick={e => { launchFrom(e?.currentTarget); navigate('quickInfo'); }}><Icon name="weather" /><span>Quick info</span></button></motion.div>}
      </div>
      <AnimatePresence>{state.editing && <motion.div key="hidden" initial={{ opacity: 0, y: quiet ? 0 : 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: quiet ? 0 : 8 }} transition={tween(keyboard ? 0 : DUR.base)}>
        <div ref={hiddenGrid} className="app-grid hidden-grid" aria-label="Hidden apps" data-editing={state.editing} data-over={overHidden} data-cols={hiddenCols} style={{ ['--cols' as string]: hiddenCols }}>
          <div className="app-section-label">Hidden apps</div>{hidden.map((id, i) => tile(id, i, 'hidden'))}</div>
      </motion.div>}</AnimatePresence>
    </LayoutGroup>
    {ghost && dragged && createPortal(<motion.div key={ghost.id} className="drag-ghost launcher-drag-ghost" initial={{ x: ghost.x, y: ghost.y, scale: 1 }} animate={ghostMotion} style={{ left: 0, top: 0, width: ghost.w, transformOrigin: '0 0' }}>
      <motion.div initial={false} animate={{ scale: drag && !quiet ? 1.08 : 1 }} transition={quiet ? { duration: 0 } : SPRING.ui()}>
        <div className="app-tile" style={{ ['--tone' as string]: tone(dragged.view) }}><Icon name={dragged.view ? viewIcons[dragged.view] ?? 'grid' : 'grid'} /><span>{label(dragged)}</span></div>
      </motion.div></motion.div>, document.body)}
    <span role="status" className="sr-only">{state.editing ? 'Editing apps' : 'App editing closed'}</span>
  </div>;
}
