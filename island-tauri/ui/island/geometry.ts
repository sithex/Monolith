import type { GeometryReport, IslandState, LayoutModel, LayoutAction, IslandView } from '../ipc/client';

export type Rect = { x: number; y: number; width: number; height: number };

/** Quick actions (Nov 2) do something in place instead of opening a page. */
export const QUICK_ACTIONS = ['playPause', 'nextTrack', 'previousTrack', 'toggleDnd', 'micMute', 'cyclePowerProfile', 'screenshot', 'pickColour', 'keepAwake', 'collapse'] as const;
export type QuickAction = typeof QUICK_ACTIONS[number];
export const isQuickAction = (type: string): type is QuickAction => (QUICK_ACTIONS as readonly string[]).includes(type);
/** The page a navigation action opens; quick actions map to the app that owns them (used only for visibility). */
export function layoutActionView(action: LayoutAction): IslandView {
  switch (action.type) {
    case 'openApp': return action.view;
    case 'appGrid': return 'launcher';
    case 'settings': return 'settings';
    case 'volume': return 'mixer';
    case 'nowPlaying': case 'playPause': case 'nextTrack': case 'previousTrack': return 'music';
    case 'timer': return 'timer';
    default: return 'launcher';
  }
}
/** A page side button turns into Back while its own page is showing and there is somewhere
 *  else to return to; quick actions never do. Returns the back target or null. */
export function sideBackFor<B extends { view: IslandView }>(action: LayoutAction, view: IslandView | undefined, back: B | null | undefined): B | null {
  if (!back || view === undefined || back.view === view || isQuickAction(action.type)) return null;
  return layoutActionView(action) === view ? back : null;
}
export function visibleLayout(layout: LayoutModel, views: readonly IslandView[]): LayoutModel {
  return { ...layout, buttons: layout.buttons.filter(b => isQuickAction(b.action.type) || ['launcher', 'terminal', 'settings', 'quickInfo'].includes(layoutActionView(b.action)) || views.includes(layoutActionView(b.action))) };
}
/** Island size presets, identical to layout_core's constants. */
export const PRESET_SIZES = { compact: { width: 360, maxHeight: 420 }, spacious: { width: 520, maxHeight: 640 }, bigger: { width: 760, maxHeight: 700 } } as const;
export function presetSize(preset: LayoutModel['preset']): { width: number; maxHeight: number } {
  return preset.type === 'custom' ? { width: preset.width, maxHeight: preset.maxHeight } : PRESET_SIZES[preset.type];
}
export type SideRect = { id: string; rect: Rect };
/** Projection only: layout state is Rust-owned. Keep the short-island rule identical to workspace::geometry. */
export function sideRects(layout: LayoutModel, island: Rect): SideRect[] {
  const d = layout.buttonDiameter, g = layout.gap;
  const sides = (side: string) => layout.buttons.filter(b => b.side === side).sort((a, b) => a.slot - b.slot);
  const left = sides('left'), right = sides('right'), bottom = sides('bottom');
  // Slots are honoured (an empty slot leaves a hole), identical to workspace::geometry.
  const extent = (v: typeof left) => v.length ? (Math.max(...v.map(b => b.slot)) + 1) * d + Math.max(...v.map(b => b.slot)) * g : 0;
  const columnBottom = Math.max(island.height, extent(left), extent(right));
  const first = bottom[0]?.slot ?? 0, span = bottom.length ? (bottom[bottom.length - 1]!.slot - first + 1) : 0;
  return [...left.map(b => ({ id: b.id, rect: { x: island.x - g - d, y: island.y + b.slot * (d + g), width: d, height: d } })),
    ...right.map(b => ({ id: b.id, rect: { x: island.x + island.width + g, y: island.y + b.slot * (d + g), width: d, height: d } })),
    ...bottom.map(b => ({ id: b.id, rect: { x: island.x + island.width / 2 - (span * d + (span - 1) * g) / 2 + (b.slot - first) * (d + g), y: island.y + columnBottom + g, width: d, height: d } }))];
}
export function bounds(rects: Rect[]): Rect {
  const x = Math.min(...rects.map(r => r.x)), y = Math.min(...rects.map(r => r.y));
  return { x, y, width: Math.max(...rects.map(r => r.x + r.width)) - x, height: Math.max(...rects.map(r => r.y + r.height)) - y };
}
/** Fit the WHOLE union. Only impossibly small viewports scale circles/gaps; preferences never change. */
export function fitSurface(width: number, height: number, viewport: { width: number; height: number }, layout?: LayoutModel) {
  const vw = Math.max(1, viewport.width), vh = Math.max(1, viewport.height);
  width = Math.max(1, Math.min(width, Math.max(1, vw - 64)));
  height = Math.max(1, Math.min(height, Math.max(1, vh - 64)));
  if (!layout || !layout.buttons.length) return { width, height, buttonScale: 1 };
  // Occupied extent in slots (holes count), so the fit matches sideRects.
  const count = (side: string) => { const s = layout.buttons.filter(b => b.side === side).map(b => b.slot); return s.length ? (side === 'bottom' ? Math.max(...s) - Math.min(...s) + 1 : Math.max(...s) + 1) : 0; };
  const l = count('left'), r = count('right'), b = count('bottom'), d = layout.buttonDiameter, g = layout.gap;
  const left = l ? d + g : 0, right = r ? d + g : 0;
  const horizontal = left + right;
  const row = b ? b * d + (b - 1) * g : 0;
  const column = Math.max(l, r) ? Math.max(l, r) * d + (Math.max(l, r) - 1) * g : 0;
  const below = b ? d + g : 0;
  // Side columns and the bottom row share horizontal space; do not add both widths.
  // For asymmetric columns the mixed side/half-row extent is also a real constraint.
  const buttonScale = Math.min(1, (vw - 1) / Math.max(1, horizontal, row, left + row / 2, right + row / 2), (vh - 1) / Math.max(1, column + below));
  return { width: Math.max(1, Math.min(width, vw - horizontal * buttonScale,
    2 * vw - (2 * left + row) * buttonScale, 2 * vw - (2 * right + row) * buttonScale)),
    height: Math.max(1, Math.min(height, vh - below * buttonScale)), buttonScale };
}
export function clampUnion(rects: Rect[], desired: { x: number; y: number }, viewport: { width: number; height: number }) {
  const box = bounds(rects);
  return { x: Math.max(-box.x, Math.min(viewport.width - box.x - box.width, desired.x)),
    y: Math.max(-box.y, Math.min(viewport.height - box.y - box.height, desired.y)) };
}
export function effectiveLayout(layout: LayoutModel, buttonScale: number): LayoutModel {
  return { ...layout, buttonDiameter: layout.buttonDiameter * buttonScale, gap: layout.gap * buttonScale };
}
/** Native placement installs only CSS geometry before the island entry runs; no second mode authority. */
export function nativePill(): (Rect & { radius: number }) | null {
  if (typeof document === 'undefined') return null;
  const style = getComputedStyle(document.documentElement);
  const [x = NaN, y = NaN, width = NaN, height = NaN, radius = NaN] = ['x', 'y', 'width', 'height', 'radius'].map(key => parseFloat(style.getPropertyValue(`--native-pill-${key}`)));
  if (![x, y, width, height, radius].every(Number.isFinite) || width < 1 || height < 1) return null;
  return { x, y, width, height, radius };
}
export function geometryReport(outer: HTMLElement, state: IslandState, settled: boolean, scale: number): GeometryReport {
  const rect = outer.getBoundingClientRect();
  const buttons = state.mode === 'large' ? Array.from(outer.ownerDocument?.querySelectorAll<HTMLElement>('[data-side-button]') ?? [])
    .filter(button => button.getClientRects().length > 0 && getComputedStyle(button).visibility !== 'hidden')
    .flatMap(button => {
      // Interactive: the layout box, not the painted one, so hover/press transforms on the
      // circles never shrink or shift the input region (fixed elements: offset* = viewport coords).
      if (button.dataset?.interactive !== 'false') return [{ x: button.offsetLeft, y: button.offsetTop, width: button.offsetWidth, height: button.offsetHeight, radius: parseFloat(getComputedStyle(button).borderRadius) || 0 }];
      // Still emerging: the circle actually drawn, so KWin blurs behind it from its first
      // frame instead of showing the sharp desktop until the button settles.
      const face = button.querySelector<HTMLElement>('.live-side-button')?.getBoundingClientRect();
      return face && face.width >= 2 ? [{ x: face.x, y: face.y, width: face.width, height: face.height, radius: face.width / 2 }] : [];
    }) : [];
  // Docked island extensions (the colour picker): layout box of a fixed element, like an
  // interactive side button, so its entrance animation never shifts the input region.
  const extensions = state.mode === 'large' ? Array.from(outer.ownerDocument?.querySelectorAll<HTMLElement>('[data-island-extension]') ?? [])
    .filter(e => !!e.dataset?.islandExtension && e.getClientRects().length > 0)
    .map(e => ({ x: e.offsetLeft, y: e.offsetTop, width: e.offsetWidth, height: e.offsetHeight, radius: parseFloat(getComputedStyle(e).borderRadius) || 0 })) : [];
  return { stateRev: state.rev, mode: state.mode, scale, settled,
    rects: [{ x: rect.x, y: rect.y, width: rect.width, height: rect.height,
      radius: parseFloat(getComputedStyle(outer).borderRadius) || 0 }, ...buttons, ...extensions] };
}
/** Single coalesced owner. Side geometry is projected synchronously, never a native round-trip feedback loop. */
/** How long a closing shell keeps KWin's blur under it (see the reporter's flush). */
export const BLUR_RELEASE_MS = 80;
export function createGeometryReporter(outer: HTMLElement, current: () => IslandState,
  offsets: () => { x: number; y: number }, send: (report: GeometryReport) => Promise<unknown>,
  fail: (error: unknown) => void, layout?: () => LayoutModel) {
  let frame: number | null = null, settled = false, last = '', surfaceKey = '', disposed = false;
  let inFlight = false, pending: GeometryReport | null = null;

  // Translation glide: offset changes and open/close travel to/from the pill animate via a CSS
  // transform transition instead of jumping (a dragged offset slider used to teleport the island).
  let lastOffset: { x: number; y: number } | null = null, lastStill: boolean | null = null, glideUntil = 0;
  let stillSince = -Infinity;
  let glideTimer: ReturnType<typeof setTimeout> | null = null;
  const root = outer.ownerDocument?.documentElement;
  const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const startGlide = (kind: 'move' | 'in' | 'out') => {
    if (!root || reducedMotion()) return;
    const ms = kind === 'in' ? 340 : 460;
    root.dataset.glide = kind; glideUntil = performance.now() + ms;
    if (glideTimer) clearTimeout(glideTimer);
    glideTimer = setTimeout(() => { glideTimer = null; delete root.dataset.glide; schedule(); }, ms + 20);
  };
  /** Current own translation (includes an in-flight transition), so the layout rect is exact. */
  const ownTranslation = () => {

    const t = getComputedStyle(outer).transform;
    if (!t || t === 'none') return { x: 0, y: 0 };
    const m = new DOMMatrixReadOnly(t);
    return { x: m.m41, y: m.m42 };
  };
  async function drain() {
    if (inFlight || disposed || !pending) return;
    const report = pending; pending = null; inFlight = true;
    try {
      if (report.stateRev === current().rev && report.mode === current().mode) await send(report);
    } catch (error) { fail(error); }
    finally { inFlight = false; if (pending) void drain(); }
  }
  const flush = () => {
    frame = null;
    const state = current(), offset = offsets();
    // Measure the layout rect by removing our own (possibly mid-transition) translation;
    // never derive the next clamp from last frame's clamp.
    const own = ownTranslation(), measured = outer.getBoundingClientRect();
    const rect: Rect = { x: measured.x - own.x, y: measured.y - own.y, width: measured.width, height: measured.height };
    const still = state.mode === 'still';
    if (still && lastStill === false) stillSince = performance.now();
    // WebKit presents the solid "glass shut" frame a frame or two after this rAF, while Rust
    // drops the KWin blur the moment it receives the settled still report. Releasing at once
    // showed the still-translucent shell over UNblurred windows for a frame. So for
    // BLUR_RELEASE_MS the still shell reports unsettled (blur follows its measured, shrinking,
    // opaque box; input is already released for any still frame under the notch).
    const blurHeld = still && performance.now() - stillSince < BLUR_RELEASE_MS;
    if (lastOffset && (lastOffset.x !== offset.x || lastOffset.y !== offset.y) && !still) startGlide('move');
    else if (lastStill !== null && lastStill !== still && (offset.x !== 0 || offset.y !== 0)) startGlide(still ? 'in' : 'out');
    lastOffset = offset; lastStill = still;
    const model = state.mode === 'large' ? layout?.() : undefined;
    const sides = model ? sideRects(model, rect) : [];
    const translation = still ? { x: 0, y: 0 } : clampUnion([rect, ...sides.map(b => b.rect)], offset, { width: innerWidth, height: innerHeight });
    outer.style.setProperty('--popup-x', `${translation.x}px`); outer.style.setProperty('--popup-y', `${translation.y}px`);
    const moved = outer.getBoundingClientRect();
    // Update the real circles in the same flush as the shell. No stale side report during a spring.
    const buttons = model ? Array.from(outer.ownerDocument?.querySelectorAll<HTMLElement>('[data-side-button]') ?? []) : [];
    if (model) for (const b of sideRects(model, moved)) {
      const element = buttons.find(e => e.dataset.sideButton === b.id);
      if (element) Object.assign(element.style, { left: `${b.rect.x}px`, top: `${b.rect.y}px`, width: `${b.rect.width}px`, height: `${b.rect.height}px`, borderRadius: `${b.rect.width / 2}px` });
    }
    const report = geometryReport(outer, state, settled || (still && !blurHeld && nativePill() !== null), devicePixelRatio);
    const keySurface = JSON.stringify(report.rects[0]);
    if (keySurface !== surfaceKey) { surfaceKey = keySurface; window.dispatchEvent?.(new Event('island-geometry')); }
    // Under the native notch a still shell owns no input and is opaque (glass shuts at
    // once on close), so every still frame is final for Rust: it releases ShapeInput AND
    // the KWin blur-behind region at the first withdrawing frame, and needs no further
    // reports while the shell contracts. An unsettled still frame used to keep KWin
    // blurring the region under the fading shell, smearing the notch clock into a flat
    // grey box for the last ~350 ms of every close (concept-pics/bug2.png).
    const key = report.mode === 'still' && report.settled && nativePill() !== null
      ? JSON.stringify({ rev: report.stateRev, mode: report.mode, scale: report.scale }) : JSON.stringify(report);
    if (key !== last) { last = key; pending = report; void drain(); }
    // Follow a running translation glide every frame so ShapeInput tracks the moving surface,
    // and keep reporting through the blur hold so the final settled report is sent.
    if (performance.now() < glideUntil || blurHeld) schedule();
  };
  const schedule = (complete?: boolean) => {
    if (disposed) return;
    if (complete !== undefined) settled = complete;
    if (frame === null) frame = requestAnimationFrame(flush);
  };
  const observer = new ResizeObserver(() => schedule()); observer.observe(outer);
  const resize = () => schedule();
  window.addEventListener('resize', resize); window.addEventListener('island-surfaces-changed', resize);
  schedule();
  return { schedule, dispose() {
    disposed = true; pending = null;

    if (glideTimer) clearTimeout(glideTimer);
    if (root) delete root.dataset.glide;
    observer.disconnect(); window.removeEventListener('resize', resize); window.removeEventListener('island-surfaces-changed', resize);
    if (frame !== null) cancelAnimationFrame(frame);
  } };
}
