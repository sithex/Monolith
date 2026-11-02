import { useLayoutEffect, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { AnimatePresence, motion } from 'framer-motion';
import type { SettingsPatch } from '../ipc/client';
import { saveSettings, saveAppearance, useSettings, useAppearance, usePalette } from '../stores';
import { NotchControls } from '../island/NotchControls';
import { LayoutEditor } from '../island/LayoutEditor';
import { SchemePicker } from '../island/SchemePicker';
import { DraftRange } from '../island/components/AppPrimitives';
import { TerminalCommandSettings } from '../island/TerminalCommandSettings';


type Tab = 'general' | 'appearance' | 'layout';
function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return <div className="settings-row"><span className="label">{label}{hint && <small>{hint}</small>}</span>{children}</div>;
}
export function Card({ title, children }: { title?: string; children: ReactNode }) {
  return <section className="settings-card">{title && <h3>{title}</h3>}{children}</section>;
}
function DeniedApps({ value, onSave }: { value: string; onSave(apps: string[]): void }) {
  const [open, setOpen] = useState(false), [draft, setDraft] = useState(value);
  const count = value ? value.split('\n').length : 0;
  return <>
    <Row label="Blocked apps" hint={count ? `${count} app${count === 1 ? '' : 's'} never notify` : 'Every app can notify'}>
      <button className="link" onClick={() => { setDraft(value); setOpen(!open); }}>{open ? 'Close' : 'Edit'}</button></Row>
    {open && <div className="denied-editor">
      <textarea aria-label="Blocked notification apps" rows={4} spellCheck={false} value={draft} placeholder="One app name per line, e.g. Discord" onChange={e => setDraft(e.target.value)} />
      <div className="app-footer"><span className="muted">Exact names, one per line</span><span className="title-spacer" />
        <button className="pill-button primary" onClick={() => { onSave(draft === '' ? [] : draft.split('\n').filter(l => l !== '')); setOpen(false); }}>Save</button></div>
    </div>}
  </>;
}
/**
 * Where the island will land while an offset slider is held. The island itself only moves
 * on release (the slider lives inside it: moving it live made the slider run from the
 * pointer), then glides there. Portaled to <body>: the island clips and transforms its children.
 */
function OffsetGhost({ preview, current }: { preview: { x: number; y: number } | null; current: { x: number; y: number } }) {
  const [box, setBox] = useState<{ x: number; y: number; w: number; h: number; r: number } | null>(null);
  useLayoutEffect(() => {
    if (!preview) { return; }
    const island = document.querySelector<HTMLElement>('.island');
    if (!island) return;
    const rect = island.getBoundingClientRect(), r = parseFloat(getComputedStyle(island).borderRadius) || 0;
    const x = Math.max(0, Math.min(innerWidth - rect.width, rect.x + preview.x - current.x));
    const y = Math.max(0, Math.min(innerHeight - rect.height, rect.y + preview.y - current.y));
    setBox({ x, y, w: rect.width, h: rect.height, r });
  }, [preview?.x, preview?.y, current.x, current.y]);
  return createPortal(<AnimatePresence>{preview && box && <motion.div key="ghost" className="offset-ghost" aria-hidden="true"
    style={{ left: box.x, top: box.y, width: box.w, height: box.h, borderRadius: box.r }}
    initial={{ opacity: 0, scale: 0.98 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, transition: { duration: 0.25, delay: 0.12 } }} transition={{ duration: 0.14 }}>
    <span>Release to move · {preview.x > 0 ? '+' : ''}{preview.x}, {preview.y > 0 ? '+' : ''}{preview.y}</span></motion.div>}</AnimatePresence>, document.body);
}
export function SettingsView() {
  const settings = useSettings(), appearance = useAppearance(), palette = usePalette();
  const [error, setError] = useState(''), [tab, setTab] = useState<Tab>('general');
  const [offsetPreview, setOffsetPreview] = useState<{ x: number; y: number } | null>(null);
  // Never disable controls while a save is in flight (that froze sliders mid-drag);
  // saves are serialized in the store and the newest value wins.
  const patch = (p: SettingsPatch) => { setError(''); void saveSettings(p).catch(e => setError(String(e))); };
  const toggle = (key: 'showDate' | 'hardwareMonitor' | 'stealth' | 'clickSounds' | 'dnd' | 'swipeNavigation' | 'translucent' | 'borderEnabled' | 'notificationsEnabled' | 'systemNotificationMirror' | 'redactBodies' | 'hermesThemeSync', labelText: string, hint?: string, off = false) =>
    <Row label={labelText} hint={hint}><input className="switch" type="checkbox" aria-label={labelText} checked={off ? settings[key] === true : settings[key] !== false} onChange={e => patch({ [key]: e.target.checked })} /></Row>;
  const range = (key: 'terminalFontSize' | 'hoverDelay' | 'offsetX' | 'offsetY', labelText: string, min: number, max: number, unit: string, step = 1, hint?: string) =>
    <Row label={labelText} hint={hint}><DraftRange aria-label={labelText} min={min} max={max} step={step} value={settings[key]} onCommit={(v, final) => { if (final || !isOffset(key)) patch({ [key]: v }); }}
      {...(isOffset(key) ? { live: false, onDraft: (v: number | null) => setOffsetPreview(v === null ? null : { x: key === 'offsetX' ? v : settings.offsetX, y: key === 'offsetY' ? v : settings.offsetY }) } : {})} />
      <output>{isOffset(key) && offsetPreview ? (key === 'offsetX' ? offsetPreview.x : offsetPreview.y) : settings[key]}{unit}</output></Row>;
  const isOffset = (key: string) => key === 'offsetX' || key === 'offsetY';
  const tabs: [Tab, string][] = [['general', 'General'], ['appearance', 'Appearance'], ['layout', 'Layout']];
  const denied = (settings.notificationDeniedApps ?? []).join('\n');
  // The display-server choice only exists on Linux; the running one is stamped by Rust.
  const isLinux = typeof navigator !== 'undefined' && /Linux/i.test(navigator.userAgent) && !/Android/i.test(navigator.userAgent);
  const runningBackend = document.documentElement.dataset.displayBackend === 'wayland' ? 'wayland' : 'x11';
  return <div className="settings-panel app-stack" aria-label="Island settings" data-own-privacy="">
    <div className="settings-tabs" role="tablist" aria-label="Settings sections">{tabs.map(([id, name]) =>
      <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>
        {tab === id && <motion.span layoutId="settings-tab" className="tab-bg" transition={{ type: 'spring', visualDuration: 0.3, bounce: 0.15 }} />}{name}</button>)}</div>
    {error && <p role="alert">{error}</p>}
    {tab === 'general' && <>
      <Card title="Behavior">
        {toggle('swipeNavigation', 'Swipe between views', 'Horizontal drag or two-finger swipe changes app')}
        {toggle('stealth', 'Auto-hide when idle')}
        {toggle('clickSounds', 'Click sounds')}
      </Card>
      <TerminalCommandSettings value={settings.terminalCommand ?? ''} onSave={command => saveSettings({ terminalCommand: command })} />
      <Card title="Notifications">
        {toggle('notificationsEnabled', 'Hermes notifications', 'Agent and system alerts on the island')}
        {toggle('dnd', 'Do Not Disturb', 'Keep arrivals quietly, no toast')}
        {toggle('systemNotificationMirror', 'Mirror desktop notifications', 'Show other apps\' notifications too')}
        {toggle('redactBodies', 'Hide message text', 'Only titles are kept')}
        <DeniedApps value={denied} onSave={apps => patch({ notificationDeniedApps: apps })} />
      </Card>
      <Card title="Clock">{toggle('showDate', 'Show date under time')}</Card>
      {isLinux && <Card title="Display">
        <Row label="Display server" hint={`Running on ${runningBackend === 'wayland' ? 'native Wayland' : 'X11 (XWayland)'}${(settings.displayBackend ?? 'wayland') !== runningBackend ? ' · applies when the island restarts' : ''}`}>
          <div className="segmented" role="group" aria-label="Display server">
            <button aria-pressed={(settings.displayBackend ?? 'wayland') === 'wayland'} onClick={() => patch({ displayBackend: 'wayland' })}>Wayland</button>
            <button aria-pressed={settings.displayBackend === 'x11'} onClick={() => patch({ displayBackend: 'x11' })}>X11</button></div></Row>
      </Card>}
      <Card title="Apps">
        {toggle('hardwareMonitor', 'Hardware monitor')}
        <Row label="Arrange or hide apps" hint="Hold any app in Apps, then drag it"><span className="muted">In Apps</span></Row>
      </Card>
    </>}
    {tab === 'appearance' && <>
      <Card title="Colour scheme">
        <SchemePicker value={appearance} settings={settings} patch={patch} onPick={id => { void saveAppearance(id).catch(e => setError(String(e))); }} />
        {toggle('hermesThemeSync', 'Use for Hermes too', 'Applies this scheme as your Hermes skin; off restores your previous skin', true)}
        <Row label="Colour overrides" hint="Accent and background come from the scheme unless overridden"><button className="link" onClick={() => patch({ accent: '', background: '', borderColor: '', panelClockColor: '' })}>Clear overrides</button></Row>
      </Card>
      <Card title="Surface">
        {toggle('translucent', 'Translucent background', 'Frosted glass over the desktop (KWin blur)')}
        {settings.translucent !== false && <Row label="Glass strength" hint="How much of the desktop shows through">
          <DraftRange aria-label="Glass strength" min={0} max={100} step={1} value={settings.glassStrength ?? 55} throttleMs={60}
            onDraft={v => { if (v !== null) document.documentElement.style.setProperty('--glass-strength', String(v / 100)); }}
            onCommit={(v, final) => { if (final) patch({ glassStrength: v }); }} />
          <output>{settings.glassStrength ?? 55}%</output></Row>}
        {toggle('borderEnabled', 'Outline')}
      </Card>
      <NotchControls settings={settings} background={settings.background || palette.surface} accent={settings.accent || palette.accent} patch={patch} />
      <Card title="Text">
        <Row label="UI font"><div className="segmented" role="group" aria-label="UI font">
          <button aria-pressed={settings.uiFont === 'system'} onClick={() => patch({ uiFont: 'system' })}>System</button>
          <button aria-pressed={settings.uiFont === 'openrunde'} onClick={() => patch({ uiFont: 'openrunde' })}>OpenRunde</button></div></Row>
        {range('terminalFontSize', 'Terminal font size', 8, 24, 'px')}
      </Card>
    </>}
    {tab === 'layout' && <>
      <LayoutEditor />
      <Card title="Position">
        {range('offsetX', 'Horizontal offset', -400, 400, 'px')}
        {range('offsetY', 'Vertical offset', -200, 200, 'px')}
      </Card>
      <OffsetGhost preview={offsetPreview} current={{ x: settings.offsetX, y: settings.offsetY }} />
    </>}
  </div>;
}
