import type { ReactNode } from 'react';
import type { Settings, SettingsPatch } from '../ipc/client';
import { DraftRange, Dropdown } from './components/AppPrimitives';
import { ColorSwatch } from './components/ColorField';
import { CompanionPicker } from './companionCharacters';
import { monitorChoices, toggleMonitor } from '../views/settings-model';

/**
 * Settings → Notch. Hook-free (tests call it as a function). `show(rowId)` filters rows for
 * Settings search (ids from settings-model's `notch` card); `header` replaces the plain title
 * (Settings passes the title + Reset link); `outputs` = connected monitor names, or null
 * while unknown (the checklist then lists only saved names).
 */
export function NotchControls({ settings, background, accent, patch, outputs = null, show = () => true, header }: {
  settings: Settings; background: string; accent: string; patch(value: SettingsPatch): void;
  outputs?: string[] | null; show?(rowId: string): boolean; header?: ReactNode;
}) {
  const color = (key: 'panelClockColor' | 'notchBorderColor', label: string, fallback: string) =>
    <div className="settings-row"><label htmlFor={key}>{label}</label><div className="notch-color-control">
      <ColorSwatch id={key} label={label} value={settings[key] || fallback} onPick={hex => patch({ [key]: hex })}
        onReset={() => patch({ [key]: '' })} resetLabel="Follow skin" />
      <button className="link" type="button" aria-label={`${label}: follow skin`} onClick={() => patch({ [key]: '' })}>
        {settings[key] || 'Follow skin'}</button>
    </div></div>;
  const flag = (key: 'notchWaitingPulse' | 'notchHoverPeek' | 'companionReactive', label: string, hint: string) =>
    <label className="settings-row"><span className="label">{label}<small>{hint}</small></span><input type="checkbox" className="switch" aria-label={label}
      checked={settings[key] !== false} onChange={e => patch({ [key]: e.target.checked })} /></label>;
  const saved = settings.notchMonitors ?? [], known = outputs ?? [];
  const choices = monitorChoices(saved, known);
  const companionOptions = [{ value: '', label: 'Every notch' },
    ...[...new Set([...known, ...(settings.companionMonitor ? [settings.companionMonitor] : [])])].map(name => ({ value: name, label: name }))];
  return <section className="settings-card" aria-label="Notch appearance">{header ?? <h3>Notch</h3>}
    <p className="muted">Taskbar notch only. The open island keeps its own appearance.</p>
    {show('everyMonitor') && <>
      <label className="settings-row"><span>Show notch on every monitor</span><input type="checkbox" className="switch" aria-label="Show notch on every monitor"
        checked={settings.showNotchOnEveryMonitor === true} onChange={e => patch({ showNotchOnEveryMonitor: e.target.checked })} /></label>
      <p className="muted">Plasma: one window follows the clicked notch. Displays without a top panel get a small dedicated notch panel. Requires the updated notch widget.</p>
    </>}
    {show('notchMonitors') && settings.showNotchOnEveryMonitor === true && <div className="settings-row">
      <span className="label">Monitors<small>{saved.length ? `${saved.length} selected` : 'All monitors'}{outputs === null ? ' · detecting…' : ''}</small></span>
      <div className="settings-checklist" role="group" aria-label="Notch monitors">
        {choices.length ? choices.map(c => <label key={c.name} data-connected={c.connected} title={c.connected ? undefined : 'Not connected now'}>
          <input type="checkbox" aria-label={`Notch on ${c.name}`} checked={c.checked}
            onChange={e => patch({ notchMonitors: toggleMonitor(saved, known, c.name, e.target.checked) })} />{c.name}</label>)
          : <span className="muted">{outputs === null ? 'Detecting monitors…' : 'No monitors reported'}</span>}
      </div></div>}
    {show('waitingPulse') && flag('notchWaitingPulse', 'Pulse while waiting', 'The notch breathes while Hermes waits for you')}
    {show('hoverPeek') && flag('notchHoverPeek', 'Hover peek', 'Hovering the notch previews the island')}
    {show('companion') && <>
      <label className="settings-row"><span>Companion</span><input type="checkbox" className="switch" aria-label="Notch companion"
        checked={settings.companionEnabled !== false} onChange={e => patch({ companionEnabled: e.target.checked })} /></label>
      <p className="muted">A small face in the notch whose eyes follow your pointer. Plasma only.</p>
    </>}
    {show('companionCharacter') && settings.companionEnabled !== false && <div className="settings-row"><span>Character</span>
      <CompanionPicker value={settings.companionCharacter ?? 'blob'} onPick={id => patch({ companionCharacter: id })} /></div>}
    {show('companionReactive') && settings.companionEnabled !== false && flag('companionReactive', 'Reactive companion', 'Reacts to Hermes working, waiting and finishing')}
    {show('companionMonitor') && settings.companionEnabled !== false && <div className="settings-row">
      <span className="label">Companion monitor<small>Which notch shows the companion</small></span>
      <Dropdown label="Companion monitor" value={settings.companionMonitor ?? ''} options={companionOptions}
        onChange={value => patch({ companionMonitor: value })} /></div>}
    {show('notchColor') && color('panelClockColor', 'Notch color', background)}
    {show('notchWidth') && <div className="settings-row"><span>Width</span><DraftRange aria-label="Notch width" min={110} max={320} step={2}
      value={settings.notchWidth ?? 144} onCommit={(value, final) => { if (final) patch({ notchWidth: value }); }} />
      <output>{settings.notchWidth ?? 144}px</output></div>}
    {show('notchBorder') && <label className="settings-row"><span>Notch border</span><input type="checkbox" className="switch" aria-label="Notch border"
      checked={settings.notchBorderEnabled !== false} onChange={e => patch({ notchBorderEnabled: e.target.checked })} /></label>}
    {show('notchBorderColor') && color('notchBorderColor', 'Notch border color', accent)}
    {show('notchBorderOpacity') && <div className="settings-row"><span>Border opacity</span><DraftRange aria-label="Notch border opacity" min={0} max={100} step={1}
      value={Math.round((settings.notchBorderOpacity ?? .1) * 100)} onCommit={value => patch({ notchBorderOpacity: value / 100 })} />
      <output>{Math.round((settings.notchBorderOpacity ?? .1) * 100)}%</output></div>}
    {show('notchBorderWidth') && <div className="settings-row"><span>Border thickness</span><DraftRange aria-label="Notch border thickness" min={0} max={4} step={.1}
      value={settings.notchBorderWidth ?? .8} onCommit={value => patch({ notchBorderWidth: value })} />
      <output>{settings.notchBorderWidth ?? .8}px</output></div>}
  </section>;
}
