import { useEffect, useRef, useState } from 'react';
import type { DisplayView, MixerChannel, MixerModel, AppViewIntents } from '../app-presentations';
import type { Service } from '../models';
import { DraftRange, Dropdown, Empty, ServiceCard } from './AppPrimitives';
import { Icon } from './Icon';

/**
 * Vertical capsule fader: 28 px track, no knob, filled from the bottom.
 * Dragging follows the pointer directly with a local draft and throttled commits;
 * external changes ease with the shared base duration.
 */
function Fader({ value, label, muted, onCommit }: { value: number; label: string; muted: boolean; onCommit(v: number): void }) {
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
  return <div className="fader" role="slider" tabIndex={0} aria-label={`${label} volume`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={draft} aria-valuetext={`${draft}%`} data-muted={muted}
    onPointerDown={e => { if (e.button !== 0) return; e.currentTarget.setPointerCapture(e.pointerId); setDragging(true); fromPointer(e.clientY); }}
    onPointerMove={e => { if (dragging) fromPointer(e.clientY); }}
    onPointerUp={() => { setDragging(false); flush(); }} onPointerCancel={() => { setDragging(false); flush(); }}
    onWheel={e => { e.stopPropagation(); set(draft - Math.sign(e.deltaY || -e.deltaX) * 2); }}
    onKeyDown={e => {
      const step = e.key === 'ArrowUp' || e.key === 'ArrowRight' ? 2 : e.key === 'ArrowDown' || e.key === 'ArrowLeft' ? -2 : e.key === 'PageUp' ? 10 : e.key === 'PageDown' ? -10 : 0;
      if (step) { e.preventDefault(); e.stopPropagation(); set(draft + step); }
    }}>
    <div className="fader-track" ref={track}>
      <div className="fader-fill" data-direct={dragging || direct} style={{ transform: `scaleY(${draft / 100})` }} />
      <svg className="fader-glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"
        style={{ color: draft > 12 ? 'rgb(0 0 0 / .55)' : 'var(--fader-ink, rgb(255 255 255 / .6))' }}><path d="M4 9h4l5-5v16l-5-5H4z" /></svg>
    </div>
  </div>;
}
function Column({ channel, master, intents }: { channel: MixerChannel; master?: boolean; intents: Pick<AppViewIntents, 'mixer'> }) {
  const volume = channel.volumePercent, muted = channel.muted;
  const commit = (percent: number) => intents.mixer(channel.name !== null ? { type: 'sinkVolume', name: channel.name, percent } : { type: 'streamVolume', index: channel.index, percent });
  const [shown, setShown] = useState(volume ?? 0);
  useEffect(() => { if (volume !== null) setShown(volume); }, [volume]);
  return <div className={`fader-col ${master ? 'master' : ''}`} title={channel.label}>
    <div className="fader-name"><Icon name={master ? 'volume' : 'music'} /><span>{channel.label}</span></div>
    {volume === null ? <p className="muted" style={{ flex: 1 }}>Unknown</p> : <Fader value={Math.min(100, volume)} label={channel.label} muted={muted} onCommit={v => { setShown(v); commit(v); }} />}
    <span className="fader-pct">{volume === null ? '—' : `${Math.round(shown)}%`}</span>
    <button className="fader-mute" aria-label={`Mute ${channel.label}`} aria-pressed={muted} onClick={() => intents.mixer(channel.name !== null ? { type: 'sinkMute', name: channel.name, muted: !muted } : { type: 'streamMute', index: channel.index, muted: !muted })}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={muted ? 'M4 9h4l5-5v16l-5-5H4zM17 9l5 6m0-6-5 6' : 'M4 9h4l5-5v16l-5-5H4zM17 8a6 6 0 0 1 0 8'} /></svg>
    </button>
  </div>;
}
export function VolumeMixer({ model, intents }: { model: Service<MixerModel>; intents: Pick<AppViewIntents, 'mixer'> }) {
  return <ServiceCard title="Volume mixer" service={model}>{value => {
    const master = value.sinks.find(s => s.name === value.defaultSink);
    return <div className="mixer">
      <div className="select-label"><span>Output</span><Dropdown label="Mixer output" value={value.defaultSink} placeholder="Unknown output" options={value.sinks.map(o => ({ value: o.name, label: o.description }))} onChange={name => intents.mixer({ type: 'defaultSink', name })} /></div>
      <div className="mixer-strip">
        {master ? <Column master channel={{ ...master, label: 'Output' }} intents={intents} /> : <p className="unavailable">Master volume unavailable</p>}
        <div className="mixer-divider" />
        {value.streams.length ? value.streams.map(stream => <Column key={stream.index} channel={{ ...stream, label: stream.appName, name: null }} intents={intents} />) : <Empty icon="volume">No apps playing</Empty>}
      </div>
    </div>;
  }}</ServiceCard>;
}
export function Brightness({ displays, intents }: { displays: Service<DisplayView[]>; intents: Pick<AppViewIntents, 'brightness'> }) {
  return <ServiceCard title="Brightness" service={displays}>{value => value.length ? <div className="app-stack">{value.map(display => <div key={display.name}><div className="app-toolbar"><Icon name="monitor"/><strong>{display.label || display.name}</strong><small className="muted">{display.isInternal ? 'Built-in' : 'External'}</small></div>{display.percent === null ? <p className="muted">Brightness unknown</p> : <label className="slider-row">Brightness<DraftRange aria-label={`${display.label || display.name} brightness`} min={0} max={100} step={1} value={display.percent} aria-busy={display.dragging}
    onCommit={(percent, final) => { intents.brightness({ type: 'drag', name: display.name, percent }); if (final) intents.brightness({ type: 'release', name: display.name }); }} /><output>{display.percent}%</output></label>}</div>)}</div> : <Empty icon="monitor">No supported displays</Empty>}</ServiceCard>;
}
