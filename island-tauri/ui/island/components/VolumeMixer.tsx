import { useState } from 'react';
import type { DisplayView, MixerChannel, MixerModel, AppViewIntents } from '../app-presentations';
import type { Service } from '../models';
import { DraftRange, Dropdown, Empty, Fader, ServiceCard } from './AppPrimitives';
import { Icon } from './Icon';
import { RollingText } from './RollingText';
import { useReducedMotionAll } from './motion-preference';

function Column({ channel, master, iconPng, intents }: { channel: MixerChannel; master?: boolean; iconPng?: string | null; intents: Pick<AppViewIntents, 'mixer'> }) {
  const volume = channel.volumePercent, muted = channel.muted;
  const commit = (percent: number) => intents.mixer(channel.name !== null ? { type: 'sinkVolume', name: channel.name, percent } : { type: 'streamVolume', index: channel.index, percent });
  // The fader's local value while it is being driven or settling; the backend value otherwise.
  const [live, setLive] = useState<number | null>(null);
  const [failedIcon, setFailedIcon] = useState<string | null>(null);
  const reduced = useReducedMotionAll();
  const shown = live ?? volume ?? 0;
  return <div className={`fader-col ${master ? 'master' : ''}`}>
    <div className="fader-name">{!master && iconPng && failedIcon !== iconPng ? <img className="mixer-app-icon" src={iconPng} alt="" width={15} height={15} onError={() => setFailedIcon(iconPng)} /> : <Icon name={master ? 'volume' : 'music'} />}<span title={channel.label}>{channel.label}</span></div>
    {volume === null ? <p className="muted" style={{ flex: 1 }}>Unknown</p> : <Fader value={Math.min(100, volume)} label={channel.label} muted={muted} onDraft={setLive} onCommit={commit} />}
    <span className="fader-pct">{volume === null ? '—' : <><span className="sr-only">{Math.round(shown)}%</span>
      <span aria-hidden="true">{live !== null ? String(Math.round(shown)) : <RollingText text={String(Math.round(shown))} reduced={reduced} fast />}%</span></>}</span>
    <button className="fader-mute" aria-label={`Mute ${channel.label}`} aria-pressed={muted} onClick={() => intents.mixer(channel.name !== null ? { type: 'sinkMute', name: channel.name, muted: !muted } : { type: 'streamMute', index: channel.index, muted: !muted })}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={muted ? 'M4 9h4l5-5v16l-5-5H4zM17 9l5 6m0-6-5 6' : 'M4 9h4l5-5v16l-5-5H4zM17 8a6 6 0 0 1 0 8'} /></svg>
    </button>
  </div>;
}
export function VolumeMixer({ model, intents }: { model: Service<MixerModel>; intents: Pick<AppViewIntents, 'mixer'> }) {
  return <ServiceCard title="Volume mixer" icon="volume" service={model}>{value => {
    const master = value.sinks.find(s => s.name === value.defaultSink);
    return <div className="mixer">
      <div className="select-label"><span>Output</span><Dropdown label="Mixer output" value={value.defaultSink} placeholder="Unknown output" options={value.sinks.map(o => ({ value: o.name, label: o.description }))} onChange={name => intents.mixer({ type: 'defaultSink', name })} /></div>
      <div className="mixer-strip" data-fit-width="">
        {master ? <Column master channel={{ ...master, label: 'Output' }} intents={intents} /> : <p className="unavailable">Master volume unavailable</p>}
        <div className="mixer-divider" />
        {value.streams.length ? value.streams.map(stream => <Column key={stream.index} iconPng={stream.iconPng} channel={{ ...stream, label: stream.appName, name: null }} intents={intents} />) : <Empty icon="volume">No apps playing</Empty>}
      </div>
    </div>;
  }}</ServiceCard>;
}
export function Brightness({ displays, intents }: { displays: Service<DisplayView[]>; intents: Pick<AppViewIntents, 'brightness'> }) {
  const reduced = useReducedMotionAll();
  return <ServiceCard title="Brightness" icon="sun" service={displays}>{value => value.length ? <div className="app-stack">{value.map(display => <div key={display.name}><div className="app-toolbar"><Icon name="monitor"/><strong>{display.label || display.name}</strong><small className="muted">{display.isInternal ? 'Built-in' : 'External'}</small></div>{display.percent === null ? <p className="muted">Brightness unknown</p> : <label className="slider-row">Brightness<DraftRange aria-label={`${display.label || display.name} brightness`} min={0} max={100} step={1} value={display.percent} aria-busy={display.dragging}
    onCommit={(percent, final) => { intents.brightness({ type: 'drag', name: display.name, percent }); if (final) intents.brightness({ type: 'release', name: display.name }); }} /><output><span className="sr-only">{display.percent}%</span><span aria-hidden="true"><RollingText text={String(display.percent)} reduced={reduced} fast />%</span></output></label>}</div>)}</div> : <Empty icon="monitor">No supported displays</Empty>}</ServiceCard>;
}
