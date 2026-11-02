import type { DisplayView, MixerChannel, MixerModel } from '../app-models';
import type { IslandIntents, Service } from '../models';
import { Empty, ServiceCard } from './AppPrimitives';
import { Icon } from './Icon';
function Channel({ channel, target, intents }: { channel: MixerChannel; target: 'sink' | 'stream'; intents: Pick<IslandIntents, 'mixer'> }) {
  const volume = channel.volumePercent;
  return <div className="mixer-channel"><div className="app-toolbar"><Icon name="volume"/><strong>{channel.label}</strong><span className="title-spacer"/><button aria-label={`Mute ${channel.label}`} aria-pressed={channel.muted === null ? 'mixed' : channel.muted} disabled={channel.muted === null} onClick={() => { if (channel.muted !== null) intents.mixer({ type: 'mute', target, index: channel.index, muted: !channel.muted }); }}>{channel.muted === null ? 'Mute unknown' : channel.muted ? 'Muted' : 'Unmuted'}</button></div>
    {volume === null ? <p className="muted">Volume unknown</p> : <label className="slider-row">Volume<input type="range" aria-label={`${channel.label} volume`} min={0} max={100} step={1} value={Math.min(100, volume)} aria-valuetext={`${volume}%`} onChange={e => intents.mixer({ type: 'volume', target, index: channel.index, percent: Number(e.target.value) })}/><output>{volume}%</output></label>}
  </div>;
}
export function VolumeMixer({ model, intents }: { model: Service<MixerModel>; intents: Pick<IslandIntents, 'mixer'> }) {
  return <ServiceCard title="Volume mixer" service={model}>{value => <div className="app-stack">
    <label className="select-label">Output device<select aria-label="Mixer output" value={value.outputName ?? ''} disabled={!value.outputs.length} onChange={e => intents.mixer({ type: 'output', name: e.target.value })}><option value="" disabled>Unknown output</option>{value.outputs.map(o => <option key={o.name} value={o.name}>{o.description}</option>)}</select></label>
    {value.master ? <Channel channel={value.master} target="sink" intents={intents}/> : <p className="unavailable">Master volume unavailable</p>}
    <h3>Apps</h3>{value.streams.length ? value.streams.map(stream => <div className="stream-row" key={stream.index}><Channel channel={stream} target="stream" intents={intents}/>{stream.corked && <small className="muted">Idle stream</small>}</div>) : <Empty icon="volume">No app streams</Empty>}
  </div>}</ServiceCard>;
}
export function Brightness({ displays, intents }: { displays: Service<DisplayView[]>; intents: Pick<IslandIntents, 'brightness'> }) {
  return <ServiceCard title="Brightness" service={displays}>{value => value.length ? <div className="app-stack">{value.map(display => <div key={display.name}><div className="app-toolbar"><Icon name="monitor"/><strong>{display.label || display.name}</strong><small className="muted">{display.isInternal ? 'Built-in' : 'External'}</small></div>{display.percent === null ? <p className="muted">Brightness unknown</p> : <label className="slider-row">Brightness<input aria-label={`${display.label || display.name} brightness`} type="range" min={0} max={100} step={1} value={display.percent} aria-busy={display.dragging} onChange={e => intents.brightness({ name: display.name, percent: Number(e.target.value), phase: 'change' })} onPointerUp={e => intents.brightness({ name: display.name, percent: Number(e.currentTarget.value), phase: 'end' })} onKeyUp={e => { if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) intents.brightness({ name: display.name, percent: Number(e.currentTarget.value), phase: 'end' }); }} onBlur={e => intents.brightness({ name: display.name, percent: Number(e.currentTarget.value), phase: 'end' })}/><output>{display.percent}%</output></label>}</div>)}</div> : <Empty icon="monitor">No supported displays</Empty>}</ServiceCard>;
}
