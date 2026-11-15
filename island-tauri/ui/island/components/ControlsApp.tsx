import type { ControlsModel, AppViewIntents } from '../app-presentations';
import type { IconName } from '../models';
import { Icon } from './Icon';
import { Dropdown, ServiceNotice } from './AppPrimitives';
function Unavailable({ reason }: { reason: string }) { return <ServiceNotice icon="controls" reason={reason} inline/>; }
function ToggleTile({ title, icon, available, detail, value, request, onChange }: { title: string; icon: IconName; available: boolean; detail: string; value: boolean | null; request: string | null; onChange(on: boolean): void }) {
  const pending = request !== null;
  return <div className={`card toggle-card ${pending ? 'is-pending' : ''}`}>
    <button className="tile-action" disabled={!available || value === null} aria-label={title} aria-pressed={value === true} aria-busy={pending} onClick={() => onChange(!value)}><Icon name={icon}/><strong>{title}</strong><span className="toggle-indicator" aria-hidden="true" data-on={value === true}/></button>
    {!available ? <Unavailable reason={detail}/> : <p className={pending ? 'pending-label' : 'muted'}>{pending ? 'Pending · ' : ''}{value === null ? 'Unknown' : value ? 'On' : 'Off'}</p>}
  </div>;
}
/** Controls (restored Nov 15, slimmed): the toggles plus what has no other home — Caffeine (keep awake) with a
 *  duration, audio output, network & Tailscale, Hermes voice. Screenshot, colour picker and package
 *  updates live on side buttons and in the Updates app instead. */
export function ControlsApp({ model, intents }: { model: ControlsModel; intents: Pick<AppViewIntents, 'toggle' | 'tile'> }) {
  const { toggles: t, tiles: s } = model;
  const awake = s?.keepAwake;
  return <div className="controls-app">
    <div className="controls-grid">
      {t ? <>
      <ToggleTile title="Do Not Disturb" icon="moon" available={t.available} detail={t.detail} value={t.dnd} request={null} onChange={on => intents.toggle({ type: 'dnd', on })}/>
      <div className={`card toggle-card ${t.powerRequest !== null ? 'is-pending' : ''}`}><button className="tile-action" aria-label="Cycle power profile" aria-busy={t.powerRequest !== null} disabled={!t.powerAvailable || !t.powerChoices.length} onClick={() => intents.toggle({ type: 'cycleProfile' })}><Icon name="power"/><strong>Power profile</strong></button>
        {!t.powerAvailable ? <Unavailable reason={t.powerDetail}/> : <><p className={t.powerRequest !== null ? 'pending-label' : 'muted'}>{t.powerRequest !== null ? 'Pending · ' : ''}{t.powerProfile ?? 'Unknown'}</p>{t.degraded && <p className="warning">{t.degraded}</p>}{t.heldBy.map(reason => <small key={reason}>{reason}</small>)}</>}
      </div>
      <ToggleTile title="Mic mute" icon="mic" available={t.micAvailable} detail={t.micDetail} value={t.micMuted} request={t.micRequest} onChange={muted => intents.toggle({ type: 'micMute', muted })}/>
      </> : <Unavailable reason="Quick toggles disabled or awaiting snapshot"/>}
      {awake && <div className="card"><h3><Icon name="coffee" active={awake.on === true}/>Caffeine</h3>{!awake.available ? <Unavailable reason={awake.detail}/> : <><p className="muted">{awake.on === null ? 'Unknown' : awake.on ? 'On' : 'Off'}{awake.remainingMs !== null && ` · ${Math.ceil(awake.remainingMs / 60000)} min left`}</p><div className="select-label"><span>Duration</span><Dropdown label="Caffeine duration" value={null} placeholder="Choose…" options={[{ value: '1800000', label: '30 minutes' }, { value: '3600000', label: '1 hour' }, { value: '7200000', label: '2 hours' }, { value: 'untilOff', label: 'Until turned off' }, { value: 'off', label: 'Turn off' }]} onChange={v => intents.tile({ type: 'keepAwake', on: v !== 'off', durationMs: v === 'untilOff' || v === 'off' ? null : Number(v) })} /></div></>}
      </div>}
    </div>
    {s ? <>
    <div className="card"><h3><Icon name="volume"/>Audio output</h3>{!s.output.available ? <Unavailable reason={s.output.detail}/> : <div className="select-label"><span>Output device</span><Dropdown label="Audio output" value={s.output.current} placeholder="Unknown default output" options={s.output.choices.map(sink => ({ value: sink.name, label: `${sink.description}${sink.muted ? ' · muted' : ''}` }))} onChange={name => intents.tile({ type: 'output', name })} /></div>}</div>
    <div className="card network-card"><h3><Icon name="network"/>Network & Tailscale <span className="pill muted">Read only</span></h3>
      {!s.network.available ? <Unavailable reason={s.network.detail}/> : <p>{s.network.kind ?? 'Unknown'} · {s.network.state ?? 'Unknown'} · {s.network.connectivity ?? 'Unknown'} connectivity</p>}
      {!s.tailscale.available ? <Unavailable reason={s.tailscale.detail}/> : <><p className="muted">{s.tailscale.backend ?? 'Unknown'} · {s.tailscale.selfName ?? 'Unknown device'} · {s.tailscale.onlineCount ?? 'Unknown'} online</p><ul className="peer-list">{[...s.tailscale.peers].sort((a, b) => Number(b.online === true) - Number(a.online === true) || a.hostName.localeCompare(b.hostName)).map(peer => <li key={peer.hostName}><span className={`status-dot ${peer.online === true ? 'online' : ''}`}/><span>{peer.hostName}<small>{peer.os || 'Unknown OS'}</small></span><span className="muted">{peer.online === true ? 'Online' : peer.online === false ? `Offline · Last seen: ${peer.lastSeen ?? 'Unknown'}` : `Online status unknown · Last seen: ${peer.lastSeen ?? 'Unknown'}`}</span></li>)}</ul></>}
    </div>
    <div className="card"><h3><Icon name="spark"/>Hermes voice</h3>{!s.voice.available ? <Unavailable reason={s.voice.detail}/> : <><p className="muted">Active: {s.voice.active ?? 'Unknown'}</p><div className="voice-list">{s.voice.choices.map(name => <button key={name} aria-pressed={s.voice.active === name} onClick={() => intents.tile({ type: 'voice', name })}>{name}</button>)}</div></>}</div>
    </> : <Unavailable reason="System tiles disabled or awaiting snapshot"/>}
  </div>;
}
