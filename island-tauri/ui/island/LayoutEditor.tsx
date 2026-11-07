import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { layoutCommand, layoutGeometry, type LayoutGeometry, type LayoutPreset, type LayoutSide, type LayoutOp } from '../ipc/client';
import { layoutStore, useLayout } from '../stores/apps';
import { Icon } from './components/Icon';
import { DraftRange, Dropdown } from './components/AppPrimitives';
import { DesktopAppPicker } from './components/DesktopAppPicker';
import { PRESET_SIZES, presetSize, fitSurface } from './geometry';
import { ACTION_OPTIONS, actionIcon, actionKey, actionLabel } from './side-actions';
import type { IconName } from './models';

export { actionIcon };
const CUSTOM = { minW: 300, maxW: 1800, minH: 240, maxH: 1280 };
const PAD = 18, STRIP = 30, PREVIEW_MAX_H = 500, MIN_FACE = 26;
const SIDE_NAMES = { left: 'Left', right: 'Right', bottom: 'Bottom' } as const;
const snap = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, Math.round(v / 10) * 10));
const TILES: { type: LayoutPreset['type']; label: string; icon: IconName }[] = [
  { type: 'compact', label: 'Compact', icon: 'compact' }, { type: 'spacious', label: 'Spacious', icon: 'spacious' },
  { type: 'bigger', label: 'Bigger', icon: 'bigger' }, { type: 'custom', label: 'Custom', icon: 'resize' },
];

/** Miniature of the open island: notch strip, clock row, page title and a faux terminal. */
function MiniIsland({ width, height }: { width: number; height: number }) {
  const lines = ['hermes › what shall we build today?', '◆ reading SPEC.md …', '✓ 3 files changed', 'hermes ›'];
  return <div className="le-mini" style={{ width, height, borderRadius: Math.min(28, Math.round(height * .34)) }}>
    <div className="le-mini-top"><span>12:43</span><span className="muted">Hermes</span><span className="le-mini-min">−</span></div>
    <div className="le-mini-title"><Icon name="terminal" /><b>Hermes</b></div>
    <div className="le-mini-term">{lines.map((l, i) => <p key={i} style={{ opacity: 1 - i * .12 }}>{l}</p>)}<span className="le-mini-caret" /></div>
  </div>;
}

export function LayoutEditor() {
  const snapshot = useLayout(), layout = snapshot.layout;
  const [geometry, setGeometry] = useState<LayoutGeometry | null>(null), [selected, setSelected] = useState<string | null>(null), [error, setError] = useState('');
  const [drag, setDrag] = useState<{ id: string; x: number; y: number; over: string | null } | null>(null);
  // Size draft: slider or resize-handle value shown in the preview; the island itself
  // only changes on release (it would otherwise resize under the pointer).
  const [draft, setDraft] = useState<{ width: number; maxHeight: number } | null>(null);
  // The selected-button panel clips while it animates open, then lets the Dropdown overflow.
  const [selectedSettled, setSelectedSettled] = useState(false);
  const host = useRef<HTMLDivElement>(null), [hostW, setHostW] = useState(560);
  useLayoutEffect(() => {
    const el = host.current; if (!el) return;
    const ro = new ResizeObserver(() => setHostW(Math.max(260, el.clientWidth)));
    ro.observe(el); return () => ro.disconnect();
  }, []);
  const saved = snapshot.unchangedSizing ? { width: 1040, maxHeight: 672 } : presetSize(layout.preset);
  const size = draft ?? saved;
  const d = layout.buttonDiameter, g = layout.gap;
  // What the live island really draws at this size (the window clamps very tall/wide islands).
  const shown = fitSurface(size.width, size.maxHeight, { width: innerWidth, height: innerHeight }, layout);
  const W = Math.round(shown.width), H = Math.round(shown.height);
  // Scale to the island actually being edited (not the 1800 px maximum, which shrank 44 px
  // circles to ~12 px). Frozen while the resize handle is dragged so the handle never jitters.
  const columnH = Math.max(H, 5 * d + 4 * g), rowW = 7 * d + 6 * g;
  const contentW = Math.max(W + 2 * (g + d), rowW) + 2 * PAD, contentH = columnH + g + d + 2 * PAD + STRIP;
  const liveScale = Math.min(1, (hostW - 4) / contentW, PREVIEW_MAX_H / contentH);
  const frozen = useRef<number | null>(null);
  const scale = frozen.current ?? liveScale;
  // Circles never draw smaller than MIN_FACE on screen; positions/spacing stay true to scale.
  const boost = Math.max(1, MIN_FACE / (d * scale));
  const stageW = Math.max(contentW, (hostW - 4) / scale), stageH = H + g + d + 2 * PAD + STRIP + Math.max(0, columnH - H);
  const maxStageH = stageH;
  const island = { x: Math.round(stageW / 2 - W / 2), y: STRIP + PAD / 2, width: W, height: H };
  const query = useRef(0);
  useEffect(() => {
    const id = ++query.current;
    void layoutGeometry({ island, viewportWidth: Math.ceil(stageW), viewportHeight: Math.ceil(stageH), includeEmpty: true })
      .then(next => { if (query.current === id) setGeometry(next); }).catch(e => { if (query.current === id) setError(String(e)); });
  }, [snapshot.rev, W, H, Math.round(stageW), d, g]);
  const send = async (op: LayoutOp) => { try { const next = await layoutCommand(op); layoutStore.apply(next, next.rev); setError(''); } catch (e) { setError(String(e)); } };
  const preset = (value: LayoutPreset) => { void send({ type: 'preset', preset: value }); };
  const commitCustom = (next: { width: number; maxHeight: number }) => { preset({ type: 'custom', width: next.width, maxHeight: next.maxHeight }); };
  const add = (side: LayoutSide, slot: number) => {
    const id = `button-${crypto.randomUUID()}`;
    void send({ type: 'add', button: { id, side, slot, action: { type: 'appGrid' }, label: null } }).then(() => setSelected(id));
  };
  const button = layout.buttons.find(b => b.id === selected);
  const slotAt = (x: number, y: number) => geometry?.emptySlots.find(s => {
    const el = host.current?.querySelector<HTMLElement>(`[data-slot="${s.side}-${s.slot}"]`); const r = el?.getBoundingClientRect();
    return !!r && x >= r.left - 6 && x <= r.right + 6 && y >= r.top - 6 && y <= r.bottom + 6;
  }) ?? null;
  const resizeStart = (e: ReactPointerEvent<HTMLButtonElement>) => {
    if (e.button !== 0) return;
    e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId);
    frozen.current = scale;
    const start = { x: e.clientX, y: e.clientY, w: size.width, h: size.maxHeight };
    let last = { width: start.w, maxHeight: start.h };
    const target = e.currentTarget;
    const move = (ev: PointerEvent) => {
      // The island is centred under the notch, so width grows on both sides.
      last = { width: snap(start.w + 2 * (ev.clientX - start.x) / scale, CUSTOM.minW, CUSTOM.maxW), maxHeight: snap(start.h + (ev.clientY - start.y) / scale, CUSTOM.minH, CUSTOM.maxH) };
      setDraft(last);
    };
    const up = () => {
      target.removeEventListener('pointermove', move); target.removeEventListener('pointerup', up); target.removeEventListener('pointercancel', up);
      frozen.current = null; setDraft(null);
      if (last.width !== saved.width || last.maxHeight !== saved.maxHeight || layout.preset.type !== 'custom') commitCustom(last);
    };
    target.addEventListener('pointermove', move); target.addEventListener('pointerup', up); target.addEventListener('pointercancel', up);
  };
  const activeTile = snapshot.unchangedSizing ? null : layout.preset.type;
  return <>
    <section className="settings-card layout-editor" data-setting-id="sideButtons" tabIndex={-1} aria-label="Layout editor"><h3>Side buttons</h3>
      <div ref={host} className="le-host" style={{ height: Math.ceil(maxStageH * scale) }}>
        {geometry && <div className="le-scaled" data-live={draft !== null} style={{ ['--le-boost' as string]: boost, width: stageW, height: stageH, transform: `scale(${scale})`, left: `calc(50% - ${stageW * scale / 2}px)` }} data-testid="layout-editor">
          <div className="le-panel" style={{ height: STRIP - 8 }}><span className="le-notch" /></div>
          <div className="le-island" style={{ left: island.x, top: island.y }}><MiniIsland width={W} height={H} /></div>
          {geometry.emptySlots.map(s => <button key={`${s.side}-${s.slot}`} data-slot={`${s.side}-${s.slot}`} className="le-empty" data-over={drag?.over === `${s.side}-${s.slot}`}
            style={{ left: s.rect.x, top: s.rect.y, width: s.rect.width, height: s.rect.height }} aria-label={`Add button ${s.side} ${s.slot + 1}`} onClick={() => add(s.side, s.slot)}><span>+</span></button>)}
          {geometry.buttons.map(b => { const model = layout.buttons.find(v => v.id === b.id); if (!model) return null; const dragging = drag?.id === b.id;
            return <motion.div key={b.id} layout={!dragging} className="le-button" data-selected={selected === b.id} data-dragging={dragging}
              style={{ left: b.rect.x, top: b.rect.y, width: b.rect.width, height: b.rect.height, zIndex: dragging ? 5 : 1, x: dragging ? drag.x : 0, y: dragging ? drag.y : 0 }}
              transition={{ type: 'spring', visualDuration: 0.3, bounce: 0.2 }}>
              <button className="round-button live-side-button" aria-label={`Edit ${model.label ?? actionLabel(model.action)}`} title={model.label ?? actionLabel(model.action)} style={{ cursor: 'grab', touchAction: 'none' }}
                onPointerDown={e => {
                  if (e.button !== 0) return; e.currentTarget.setPointerCapture(e.pointerId);
                  const start = { x: e.clientX, y: e.clientY }; let moved = false;
                  const move = (ev: PointerEvent) => {
                    const dx = ev.clientX - start.x, dy = ev.clientY - start.y; if (!moved && Math.hypot(dx, dy) < 4) return; moved = true;
                    const slot = slotAt(ev.clientX, ev.clientY); setDrag({ id: b.id!, x: dx / scale, y: dy / scale, over: slot ? `${slot.side}-${slot.slot}` : null });
                  };
                  const up = (ev: PointerEvent) => {
                    window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); window.removeEventListener('pointercancel', up);
                    setDrag(null);
                    if (!moved) { setSelected(selected === b.id ? null : b.id); return; }
                    const slot = slotAt(ev.clientX, ev.clientY); if (slot) { setSelected(b.id); void send({ type: 'move', id: b.id!, side: slot.side, slot: slot.slot }); }
                  };
                  window.addEventListener('pointermove', move); window.addEventListener('pointerup', up); window.addEventListener('pointercancel', up);
                }}><Icon name={actionIcon(model.action)} /></button>
            </motion.div>; })}
        </div>}
        {geometry && <button className="le-resize" aria-label="Drag to resize the island" title="Drag to resize"
          style={{ left: `calc(50% + ${(island.x + W - stageW / 2) * scale - 13}px)`, top: (island.y + H) * scale - 13 }}
          onPointerDown={resizeStart}><Icon name="resize" /></button>}
        <AnimatePresence>{draft && <motion.output className="le-size-badge" initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }}>{draft.width} × {draft.maxHeight}</motion.output>}</AnimatePresence>
      </div>
      {layout.buttons.length > 0 && <div className="le-chips" role="list" aria-label="Side buttons">
        {(['left', 'right', 'bottom'] as const).map(side => {
          const list = layout.buttons.filter(b => b.side === side).sort((a, b) => a.slot - b.slot);
          return list.length > 0 && <div key={side} className="le-chip-group" role="presentation"><span className="le-chip-side">{SIDE_NAMES[side]}</span>
            {list.map(b => <button key={b.id} role="listitem" className="le-chip" data-selected={selected === b.id} onClick={() => setSelected(selected === b.id ? null : b.id)}
              title={`${SIDE_NAMES[side]} ${b.slot + 1}`}><Icon name={actionIcon(b.action)} /><span>{b.label ?? actionLabel(b.action)}</span></button>)}</div>;
        })}</div>}
      <p className="muted le-hint">Click + to add a button. Drag buttons around the island. Click a button to change it. Drag the corner handle to resize.</p>
      <AnimatePresence initial={false}>{button && <motion.div key={button.id} className="le-selected" data-settled={selectedSettled}
        initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }} exit={{ opacity: 0, height: 0 }} transition={{ duration: .2, ease: [.22, 1, .36, 1] }}
        onAnimationStart={() => setSelectedSettled(false)} onAnimationComplete={() => setSelectedSettled(true)}>
        <div className="settings-row"><span className="label">Button action</span>
          <Dropdown label="Side button action" value={actionKey(button.action)} options={[...ACTION_OPTIONS.map(a => ({ value: a.key, label: a.group === 'Quick actions' ? `⚡ ${a.label}` : a.label })), ...(button.action.type === 'launchDesktop' ? [{ value: actionKey(button.action), label: button.label ?? 'Desktop app' }] : [])]}
            onChange={v => { const a = ACTION_OPTIONS.find(x => x.key === v); if (a) void send({ type: 'edit', id: button.id, action: a.action, label: null }); }} />
          <button className="link" onClick={() => { void send({ type: 'remove', id: button.id }); setSelected(null); }}>Remove</button></div>
        <DesktopAppPicker key={button.id} onPick={app => { void send({ type: 'edit', id: button.id, action: { type: 'launchDesktop', desktopId: app.id }, label: Array.from(app.name).slice(0, 32).join('') }); }} />
      </motion.div>}</AnimatePresence>
      <div className="settings-row"><span className="label">Button size</span><DraftRange aria-label="Side button size" min={28} max={56} step={1} value={layout.buttonDiameter} onCommit={v => { void send({ type: 'diameter', diameter: v }); }} /><output>{Math.round(layout.buttonDiameter)}px</output></div>
      <div className="settings-row"><span className="label">Spacing</span><DraftRange aria-label="Side button spacing" min={4} max={24} step={1} value={layout.gap} onCommit={v => { void send({ type: 'gap', gap: v }); }} /><output>{Math.round(layout.gap)}px</output></div>
      {error && <p role="alert">{error}</p>}
    </section>
    <section className="settings-card" data-setting-id="islandSize" tabIndex={-1} aria-label="Island size"><h3>Size</h3>
      {snapshot.unchangedSizing && <p className="muted" style={{ fontSize: 11 }}>Default sizes per app · pick a size to choose your own.</p>}
      <div className="le-tiles" role="group" aria-label="Size preset">{TILES.map(t =>
        <button key={t.type} className="le-tile" aria-pressed={activeTile === t.type}
          onClick={() => preset(t.type === 'custom' ? { type: 'custom', width: snap(saved.width, CUSTOM.minW, CUSTOM.maxW), maxHeight: snap(saved.maxHeight, CUSTOM.minH, CUSTOM.maxH) } : { type: t.type } as LayoutPreset)}>
          <Icon name={t.icon} /><span>{t.label}</span>{t.type !== 'custom' && <small>{PRESET_SIZES[t.type].width} × {PRESET_SIZES[t.type].maxHeight}</small>}
          {t.type === 'custom' && <small>{layout.preset.type === 'custom' ? `${layout.preset.width} × ${layout.preset.maxHeight}` : 'your own'}</small>}
        </button>)}</div>
      <div className="settings-row"><span className="label">Width</span><DraftRange aria-label="Custom width" min={CUSTOM.minW} max={CUSTOM.maxW} step={10} value={saved.width} live={false}
        onDraft={v => setDraft(v === null ? null : { width: v, maxHeight: saved.maxHeight })} onCommit={(v, final) => { if (final && v !== saved.width) commitCustom({ width: v, maxHeight: saved.maxHeight }); }} /><output>{draft?.width ?? saved.width}px</output></div>
      <div className="settings-row"><span className="label">Maximum height</span><DraftRange aria-label="Custom maximum height" min={CUSTOM.minH} max={CUSTOM.maxH} step={10} value={saved.maxHeight} live={false}
        onDraft={v => setDraft(v === null ? null : { width: saved.width, maxHeight: v })} onCommit={(v, final) => { if (final && v !== saved.maxHeight) commitCustom({ width: saved.width, maxHeight: v }); }} /><output>{draft?.maxHeight ?? saved.maxHeight}px</output></div>
      {H < Math.round(size.maxHeight) && <p className="muted le-hint">The screen fits {W} × {H}; taller islands are clipped to the window.</p>}
    </section>
  </>;
}
