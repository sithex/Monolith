import type { Settings, SettingsPatch } from '../ipc/client';
import { DraftRange } from './components/AppPrimitives';
import { ColorSwatch } from './components/ColorField';
import { CompanionPicker } from './companionCharacters';

export function NotchControls({ settings, background, accent, patch }: {
  settings: Settings; background: string; accent: string; patch(value: SettingsPatch): void;
}) {
  const color = (key: 'panelClockColor' | 'notchBorderColor', label: string, fallback: string) =>
    <div className="settings-row"><label htmlFor={key}>{label}</label><div className="notch-color-control">
      <ColorSwatch id={key} label={label} value={settings[key] || fallback} onPick={hex => patch({ [key]: hex })}
        onReset={() => patch({ [key]: '' })} resetLabel="Follow skin" />
      <button className="link" type="button" aria-label={`${label}: follow skin`} onClick={() => patch({ [key]: '' })}>
        {settings[key] || 'Follow skin'}</button>
    </div></div>;
  return <section className="settings-card" aria-label="Notch appearance"><h3>Notch</h3>
    <p className="muted">Taskbar notch only. The open island keeps its own appearance.</p>
    <label className="settings-row"><span>Show notch on every monitor</span><input type="checkbox" className="switch" aria-label="Show notch on every monitor"
      checked={settings.showNotchOnEveryMonitor === true} onChange={e => patch({ showNotchOnEveryMonitor: e.target.checked })} /></label>
    <p className="muted">Plasma: one window follows the clicked notch. Displays without a top panel get a small dedicated notch panel. Requires the updated notch widget.</p>
    <label className="settings-row"><span>Companion</span><input type="checkbox" className="switch" aria-label="Notch companion"
      checked={settings.companionEnabled !== false} onChange={e => patch({ companionEnabled: e.target.checked })} /></label>
    <p className="muted">A small face in the notch whose eyes follow your pointer. Plasma only.</p>
    {settings.companionEnabled !== false && <div className="settings-row"><span>Character</span>
      <CompanionPicker value={settings.companionCharacter ?? 'blob'} onPick={id => patch({ companionCharacter: id })} /></div>}
    {color('panelClockColor', 'Notch color', background)}
    <div className="settings-row"><span>Width</span><DraftRange aria-label="Notch width" min={110} max={320} step={2}
      value={settings.notchWidth ?? 144} onCommit={(value, final) => { if (final) patch({ notchWidth: value }); }} />
      <output>{settings.notchWidth ?? 144}px</output></div>
    <label className="settings-row"><span>Notch border</span><input type="checkbox" className="switch" aria-label="Notch border"
      checked={settings.notchBorderEnabled !== false} onChange={e => patch({ notchBorderEnabled: e.target.checked })} /></label>
    {color('notchBorderColor', 'Notch border color', accent)}
    <div className="settings-row"><span>Border opacity</span><DraftRange aria-label="Notch border opacity" min={0} max={100} step={1}
      value={Math.round((settings.notchBorderOpacity ?? .1) * 100)} onCommit={value => patch({ notchBorderOpacity: value / 100 })} />
      <output>{Math.round((settings.notchBorderOpacity ?? .1) * 100)}%</output></div>
    <div className="settings-row"><span>Border thickness</span><DraftRange aria-label="Notch border thickness" min={0} max={4} step={.1}
      value={settings.notchBorderWidth ?? .8} onCommit={value => patch({ notchBorderWidth: value })} />
      <output>{settings.notchBorderWidth ?? .8}px</output></div>
  </section>;
}
