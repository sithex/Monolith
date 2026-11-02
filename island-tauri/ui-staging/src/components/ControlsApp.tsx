import type { ControlsModel, IconName, IslandIntents, Service, ToggleState } from '../models';
import { Icon } from './Icon';
export const toggleDisplay = <T,>(state: ToggleState<T>): T | null => state.pending ? state.pending.value : state.confirmed;
function Unavailable({ reason }: { reason: string }) { return <p className="unavailable">Unavailable · {reason}</p>; }
function ToggleTile({ title, icon, service, onChange }: { title: string; icon: IconName; service: Service<ToggleState<boolean>>; onChange(on: boolean): void }) {
  const value = service.available ? toggleDisplay(service.value) : null;
  const pending = service.available && service.value.pending !== null;
  return <div className={`card toggle-card ${pending ? 'is-pending' : ''}`}>
    <button className="tile-action" disabled={!service.available || value === null} aria-label={title} aria-pressed={value === true} aria-busy={pending} onClick={() => onChange(!value)}><Icon name={icon}/><strong>{title}</strong><span className="toggle-indicator" aria-hidden="true" data-on={value === true}/></button>
    {!service.available ? <Unavailable reason={service.reason}/> : <p className={pending ? 'pending-label' : 'muted'}>{pending ? 'Pending · ' : ''}{value === null ? 'Unknown' : value ? 'On' : 'Off'}</p>}
  </div>;
}
export function ControlsApp({ model, intents }: { model: ControlsModel; intents: IslandIntents }) {
  const powerValue = toggleDisplay(model.powerToggle);
  const next = model.power.choices[(model.power.choices.indexOf(powerValue!) + 1) % model.power.choices.length];
  const awake = model.keepAwake.available ? model.keepAwake.value : null;
  const backend = model.tailscale.available ? model.tailscale.value.backend : null;
  const backendLabel = typeof backend === 'object' ? (backend ? `Unknown (${backend.unknown})` : 'Unknown') : backend;
  return <div className="controls-app">
    <div className="controls-grid">
      <ToggleTile title="Do Not Disturb" icon="moon" service={model.dnd} onChange={on => intents.control({ type: 'dnd', on })}/>
      <div className={`card toggle-card ${model.powerToggle.pending ? 'is-pending' : ''}`}><button className="tile-action" aria-label="Cycle power profile" aria-busy={!!model.powerToggle.pending} disabled={!model.power.available || !next} onClick={() => intents.control({ type: 'power', profile: next })}><Icon name="power"/><strong>Power profile</strong></button>
        {!model.power.available ? <Unavailable reason={model.power.unavailableReason ?? 'Service not connected'}/> : <><p className={model.powerToggle.pending ? 'pending-label' : 'muted'}>{model.powerToggle.pending ? 'Pending · ' : ''}{powerValue ?? 'Unknown'}</p>{model.power.degraded && <p className="warning">{model.power.degraded}</p>}{model.power.heldBy.map(reason => <small key={reason}>{reason}</small>)}</>}
      </div>
      <ToggleTile title="Mic mute" icon="mic" service={model.mic} onChange={on => intents.control({ type: 'mic', on })}/>
      <div className={`card ${awake?.status === 'requesting' ? 'is-pending' : ''}`}><h3><Icon name="coffee"/>Keep awake</h3>{!model.keepAwake.available ? <Unavailable reason={model.keepAwake.reason}/> : <><p className={awake?.status === 'requesting' ? 'pending-label' : 'muted'}>{awake?.status === 'requesting' ? `Pending${awake.cancelPending ? ' · cancelling' : ''}` : awake?.status === 'on' ? 'On' : 'Off'}{awake?.autoOffInMs != null && ` · ${Math.ceil(awake.autoOffInMs / 60000)} min left`}</p><label className="select-label">Duration<select aria-label="Keep awake duration" value="" onChange={e => { if (e.target.value) intents.control({ type: 'keepAwake', on: e.target.value !== 'off', durationMs: e.target.value === 'untilOff' || e.target.value === 'off' ? null : Number(e.target.value) }); }}><option value="" disabled>Choose…</option><option value="1800000">30 minutes</option><option value="3600000">1 hour</option><option value="7200000">2 hours</option><option value="untilOff">Until turned off</option><option value="off">Turn off</option></select></label></>}
      </div>
    </div>
    <div className="card"><h3><Icon name="volume"/>Audio output</h3>{!model.output.available ? <Unavailable reason={model.output.reason}/> : <label className="select-label">Output device<select aria-label="Audio output" value={model.output.value.current?.name ?? ''} onChange={e => intents.control({ type: 'output', name: e.target.value })}><option value="" disabled>Unknown default output</option>{model.output.value.choices.map(sink => <option key={sink.name} value={sink.name}>{sink.description}{sink.muted ? ' · muted' : ''}</option>)}</select></label>}</div>
    <div className="card network-card"><h3><Icon name="network"/>Network & Tailscale <span className="pill muted">Read only</span></h3>
      {!model.network.available ? <Unavailable reason={model.network.reason ?? 'NetworkManager not connected'}/> : <p>{model.network.kind} · {model.network.state ?? 'Unknown'} · {model.network.connectivity ?? 'Unknown'} connectivity</p>}
      {!model.tailscale.available ? <Unavailable reason={model.tailscale.reason}/> : <><p className="muted">{backendLabel} · {model.tailscale.value.selfName ?? 'Unknown device'} · {model.tailscale.value.onlineCount} online</p><ul className="peer-list">{[...model.tailscale.value.peers].sort((a, b) => Number(b.online) - Number(a.online) || a.hostName.localeCompare(b.hostName)).map(peer => <li key={peer.hostName}><span className={`status-dot ${peer.online ? 'online' : ''}`}/><span>{peer.hostName}<small>{peer.os || 'Unknown OS'}</small></span><span className="muted">{peer.online ? 'Online' : `Last seen: ${peer.lastSeen ?? 'Unknown'}`}</span></li>)}</ul></>}
    </div>
    <div className="capture-row">{([{ key: 'screenshot', title: 'Screenshot', icon: 'camera' }, { key: 'colourPicker', title: 'Colour picker', icon: 'picker' }] as const).map(({ key, title, icon }) => <div className="card" key={key}><button className="tile-action" disabled={!model[key].available} onClick={() => intents.control({ type: key })}><Icon name={icon}/><strong>{title}</strong></button>{!model[key].available && <Unavailable reason={model[key].reason}/>}</div>)}</div>
    <div className="card"><h3><Icon name="spark"/>Hermes voice</h3>{!model.voices.available ? <Unavailable reason={model.voices.reason}/> : <><p className="muted">Active: {model.voices.value.active ?? 'Unknown'}</p><div className="voice-list">{model.voices.value.voices.map(name => <button key={name} aria-pressed={model.voices.available && model.voices.value.active === name} onClick={() => intents.control({ type: 'voice', name })}>{name}</button>)}</div></>}</div>
    <div className="card updates-card"><h3><Icon name="download"/>Package updates <span className="update-badge">{model.updates.count === null ? '?' : model.updates.count}</span></h3><p>{model.updates.count === null ? 'Unknown · no successful check' : `${model.updates.repoCount ?? 'Unknown'} repo · ${model.updates.aurCount ?? 'Unknown'} AUR`}{model.updates.stale && ' · Stale'}{model.updates.needsReboot && ' · Reboot may be needed'}</p>{model.updates.error && <Unavailable reason={model.updates.error}/>}<button onClick={() => intents.control({ type: 'checkUpdates' })}>Check updates</button></div>
  </div>;
}
