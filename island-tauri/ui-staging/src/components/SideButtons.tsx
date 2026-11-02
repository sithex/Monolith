import { useState, type CSSProperties, type ReactNode } from 'react';
import type { ButtonAction, IconName, IslandIntents, Layout, LayoutGeometry, Rect } from '../models';
import { Icon } from './Icon';
const rectStyle = (rect: Rect): CSSProperties => ({ left: rect.x, top: rect.y, width: rect.w, height: rect.h });
function actionIcon(action: ButtonAction): IconName {
  if (typeof action !== 'string') return 'spark';
  return ({ appGrid: 'grid', settings: 'settings', volume: 'volume', nowPlaying: 'music', timer: 'timer' } as const)[action];
}
export function SideButtons({ layout, geometry, intents, edit = false, children }: { layout: Layout; geometry: LayoutGeometry; intents: IslandIntents; edit?: boolean; children?: ReactNode }) {
  const [moving, setMoving] = useState<string | null>(null);
  return <div className={`geometry-stage ${edit ? 'geometry-edit' : ''}`} style={{ width: geometry.canvas.w, height: geometry.canvas.h }}>
    <div className="geometry-island" style={rectStyle(geometry.island)}>{children}</div>
    {geometry.buttons.map(({ id, rect }) => {
      const button = layout.buttons.find(b => b.id === id);
      if (!button) return null;
      const label = button.label ?? (typeof button.action === 'string' ? button.action : id);
      return <div key={id} className="geometry-button" style={rectStyle(rect)} draggable={edit}
        onDragStart={e => { e.dataTransfer.setData('text/plain', id); setMoving(id); }} onDragEnd={() => setMoving(null)}>
        <button className="round-button" aria-label={`${edit ? 'Edit ' : ''}${label}`} onClick={() => edit ? intents.layout({ type: 'edit', id }) : intents.sideAction(button.action)}><Icon name={actionIcon(button.action)} /></button>
        {edit && <button className="move-side-button" aria-label={`Move ${label}`} aria-pressed={moving === id} onClick={() => setMoving(moving === id ? null : id)}>↗</button>}
      </div>;
    })}
    {edit && geometry.emptySlots.map(({ side, slot, rect }) => <button key={`${side}-${slot}`} className="empty-slot" style={rectStyle(rect)} aria-label={`${moving ? 'Move here' : 'Add button'} ${side} ${slot + 1}`}
      onDragOver={e => e.preventDefault()} onDrop={e => { e.preventDefault(); const id = e.dataTransfer.getData('text/plain'); if (layout.buttons.some(b => b.id === id)) intents.layout({ type: 'move', id, side, slot }); setMoving(null); }}
      onClick={() => { intents.layout(moving ? { type: 'move', id: moving, side, slot } : { type: 'add', side, slot }); setMoving(null); }}>+</button>)}
  </div>;
}
export function LayoutEditor({ layout, geometry, intents }: { layout: Layout; geometry: LayoutGeometry; intents: IslandIntents }) {
  const custom = typeof layout.preset !== 'string' ? layout.preset.custom : { width: 600, maxHeight: 616 };
  const selected = typeof layout.preset === 'string' ? layout.preset : 'custom';
  return <div className="layout-editor">
    <div className="layout-preview"><SideButtons layout={layout} geometry={geometry} intents={intents} edit><div className="mini-home"><Icon name="terminal" /><span>Hermes</span><span className="muted">Your island, your way</span></div></SideButtons></div>
    <p className="muted">Click + to add. Drag a button, or choose its ↗ then a + slot to move. Click the round button to edit.</p>
    <div className="card"><h3>Size</h3><div className="preset-row">{(['compact', 'spacious', 'custom'] as const).map(preset => <button key={preset} aria-pressed={selected === preset} onClick={() => intents.layout({ type: 'preset', preset: preset === 'custom' ? { custom } : preset })}><span className="preset-symbol">{preset === 'compact' ? '↧' : preset === 'spacious' ? '⤢' : '↗'}</span>{preset[0].toUpperCase() + preset.slice(1)}</button>)}</div>
      <label className="slider-row">Width <input aria-label="Custom width" type="range" min={300} max={900} value={custom.width} disabled={selected !== 'custom'} onChange={e => intents.layout({ type: 'preset', preset: { custom: { ...custom, width: Number(e.target.value) } } })}/><output>{selected === 'compact' ? 360 : selected === 'spacious' ? 520 : custom.width}</output></label>
      <label className="slider-row">Maximum height <input aria-label="Custom maximum height" type="range" min={240} max={900} value={custom.maxHeight} disabled={selected !== 'custom'} onChange={e => intents.layout({ type: 'preset', preset: { custom: { ...custom, maxHeight: Number(e.target.value) } } })}/><output>{selected === 'compact' ? 420 : selected === 'spacious' ? 640 : custom.maxHeight}</output></label>
    </div>
  </div>;
}
