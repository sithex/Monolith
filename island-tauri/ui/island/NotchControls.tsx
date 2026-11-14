import type { ReactNode } from 'react';
import type { Settings, SettingsPatch } from '../ipc/client';
import { DraftRange, Dropdown } from './components/AppPrimitives';
import { ColorSwatch } from './components/ColorField';
import { CompanionPicker } from './companionCharacters';
import { monitorChoices, rowSpec, toggleMonitor } from '../views/settings-model';

/** Hook-free shared notch/companion controls; every preference has a stable jump target. */
export function NotchControls({ settings, background, accent, patch, outputs = null, show = () => true, header }: {
  settings: Settings; background: string; accent: string; patch(value: SettingsPatch): void;
  outputs?: string[] | null; show?(rowId: string): boolean; header?: ReactNode;
}) {
  const row = (id: string, control: ReactNode, hint?: string) => {
    const spec = rowSpec(id);
    return show(id) && <div key={id} className="settings-row" data-setting-id={id} data-setting-label={spec.label} tabIndex={-1}>
      <span className="label">{spec.label}{(hint ?? spec.hint) && <small>{hint ?? spec.hint}</small>}</span>{control}</div>;
  };
  const color = (id: string, key: 'panelClockColor' | 'notchBorderColor', fallback: string) => row(id, <div className="notch-color-control">
    <ColorSwatch id={key} label={rowSpec(id).label} value={settings[key] || fallback} onPick={hex => patch({ [key]: hex })}
      onReset={() => patch({ [key]: '' })} resetLabel="Follow skin" />
    <button className="link" type="button" aria-label={`${rowSpec(id).label}: follow skin`} onClick={() => patch({ [key]: '' })}>{settings[key] || 'Follow skin'}</button>
  </div>);
  const flag = (id: string, key: 'notchWaitingPulse' | 'notchHoverPeek' | 'notchCopyProgress' | 'companionReactive' | 'showNotchOnEveryMonitor' | 'companionEnabled' | 'notchBorderEnabled') => row(id,
    <input type="checkbox" className="switch" aria-label={id === 'companion' ? 'Notch companion' : rowSpec(id).label} checked={key === 'showNotchOnEveryMonitor' ? settings[key] === true : settings[key] !== false} onChange={e => patch({ [key]: e.target.checked })} />);
  const saved = settings.notchMonitors ?? [], known = outputs ?? [], choices = monitorChoices(saved, known);
  const companionOptions = [{ value: '', label: 'Every notch' }, ...[...new Set([...known, ...(settings.companionMonitor ? [settings.companionMonitor] : [])])].map(name => ({ value: name, label: name }))];
  return <section className="settings-card" aria-label="Notch appearance">{header ?? <h3>Notch</h3>}
    <p className="muted">Taskbar notch only. The open island keeps its own appearance. Preferences can be edited while a feature is off.</p>
    {flag('everyMonitor', 'showNotchOnEveryMonitor')}
    {row('notchMonitors', <div className="settings-checklist" role="group" aria-label="Notch monitors">
      {choices.length ? choices.map(c => <label key={c.name} data-connected={c.connected} title={c.connected ? undefined : 'Not connected now'}>
        <input type="checkbox" aria-label={`Notch on ${c.name}`} checked={c.checked} onChange={e => patch({ notchMonitors: toggleMonitor(saved, known, c.name, e.target.checked) })} />{c.name}</label>)
        : <span className="muted">{outputs === null ? 'Detecting monitors…' : 'No monitors reported'}</span>}
    </div>, `${saved.length ? `${saved.length} selected` : 'All monitors'}${outputs === null ? ' · detecting…' : ''}`)}
    {flag('waitingPulse', 'notchWaitingPulse')}
    {flag('hoverPeek', 'notchHoverPeek')}
    {flag('copyProgress', 'notchCopyProgress')}
    {color('notchColor', 'panelClockColor', background)}
    {row('notchWidth', <><DraftRange aria-label="Notch width" min={110} max={320} step={2} value={settings.notchWidth ?? 144} onCommit={(value, final) => { if (final) patch({ notchWidth: value }); }} /><output>{settings.notchWidth ?? 144}px</output></>)}
    {flag('notchBorder', 'notchBorderEnabled')}
    {color('notchBorderColor', 'notchBorderColor', accent)}
    {row('notchBorderOpacity', <><DraftRange aria-label="Notch border opacity" min={0} max={100} step={1} value={Math.round((settings.notchBorderOpacity ?? .1) * 100)} onCommit={value => patch({ notchBorderOpacity: value / 100 })} /><output>{Math.round((settings.notchBorderOpacity ?? .1) * 100)}%</output></>)}
    {row('notchBorderWidth', <><DraftRange aria-label="Notch border thickness" min={0} max={4} step={.1} value={settings.notchBorderWidth ?? .8} onCommit={value => patch({ notchBorderWidth: value })} /><output>{settings.notchBorderWidth ?? .8}px</output></>)}
    {flag('companion', 'companionEnabled')}
    {row('companionCharacter', <CompanionPicker value={settings.companionCharacter ?? 'blob'} onPick={id => patch({ companionCharacter: id })} />)}
    {flag('companionReactive', 'companionReactive')}
    {row('companionMonitor', <Dropdown label="Companion monitor" value={settings.companionMonitor ?? ''} options={companionOptions} onChange={value => patch({ companionMonitor: value })} />)}
  </section>;
}
