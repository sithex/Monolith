import { useState } from 'react';
import type { CalendarModel, AgendaRow, AppViewIntents } from '../app-presentations';
import { Empty } from './AppPrimitives';
import { Icon } from './Icon';

/** Where each provider hides its private iCal (ICS) link. */
const PROVIDERS = {
  google: { name: 'Google', steps: [
    'On a computer, open calendar.google.com (the phone app can’t show this link).',
    'Click the gear ⚙ → Settings, then pick your calendar under “Settings for my calendars” on the left.',
    'Scroll to “Integrate calendar” and copy “Secret address in iCal format” (it ends in basic.ics).',
    'Paste it below and press Connect. Repeat “Change link” to switch calendars later.',
  ], example: 'https://calendar.google.com/calendar/ical/…/private-…/basic.ics' },
  outlook: { name: 'Outlook', steps: [
    'Open Outlook on the web → Settings ⚙ → Calendar → Shared calendars.',
    'Under “Publish a calendar”, choose your calendar and “Can view all details”, then Publish.',
    'Copy the ICS link and paste it below.',
  ], example: 'https://outlook.office365.com/owa/calendar/…/calendar.ics' },
  icloud: { name: 'iCloud', steps: [
    'Open iCloud Calendar (icloud.com/calendar) and click the share icon next to a calendar.',
    'Tick “Public Calendar” and copy the webcal:// link (it works as-is).',
    'Paste it below and press Connect.',
  ], example: 'webcal://p…-caldav.icloud.com/published/2/…' },
} as const;
type Provider = keyof typeof PROVIDERS;

/** Rust error text → something a person can act on. */
function friendly(error: unknown) {
  const text = error instanceof Error ? error.message : String(error);
  if (/HTTPS|URL invalid/i.test(text)) return 'That doesn’t look like a calendar link. Paste the full https:// (or webcal://) address.';
  if (/HTTP failure/i.test(text)) return 'The calendar server refused that link. Check it is the private/secret iCal address, not the normal share page.';
  if (/request failed|body failed/i.test(text)) return 'Couldn’t reach the calendar server. Check your connection and try again.';
  if (/Invalid calendar|encoding/i.test(text)) return 'That link didn’t return a calendar (ICS) file.';
  if (/too large/i.test(text)) return 'That calendar is larger than 1 MB, which the island can’t load.';
  return text;
}

export function CalendarSubscription({ configured, connect }: { configured: boolean; connect(url: string | null): Promise<void> }) {
  const [provider, setProvider] = useState<Provider>('google');
  const [url, setUrl] = useState(''), [busy, setBusy] = useState(false), [error, setError] = useState(''), [editing, setEditing] = useState(false), [confirm, setConfirm] = useState(false);
  const run = (value: string | null) => {
    setBusy(true); setError('');
    connect(value).then(() => { setUrl(''); setEditing(false); setConfirm(false); }, e => setError(friendly(e))).finally(() => setBusy(false));
  };
  if (configured && !editing) return <div className="calendar-sub connected" data-testid="calendar-subscription">
    <span className="calendar-sub-dot" aria-hidden="true" />
    <div className="calendar-sub-text"><strong>Calendar subscription connected</strong><small>Phone calendar · read-only · new events appear within ~15 min</small></div>
    <span className="title-spacer" />
    {confirm ? <><button disabled={busy} onClick={() => setConfirm(false)}>Keep</button><button className="danger" disabled={busy} onClick={() => run(null)}>{busy ? 'Removing…' : 'Disconnect'}</button></>
      : <><button disabled={busy} onClick={() => { setEditing(true); setError(''); }}>Change link</button><button disabled={busy} onClick={() => setConfirm(true)}>Disconnect</button></>}
    {error && <p className="calendar-sub-error" role="alert">{error}</p>}
  </div>;
  const p = PROVIDERS[provider];
  return <section className="calendar-sub card" data-testid="calendar-subscription">
    <div className="calendar-sub-head"><Icon name="calendar" /><div><h3>Connect your calendar</h3><small>iCal (ICS) subscription · read-only</small></div></div>
    <p className="muted">Shows your phone’s calendar here. Add an event on your phone and it appears on the island within about 15 minutes. It is one-way: events you create on the island stay on this PC. No password is shared; the private link is stored only on this PC and never shown again.</p>
    <div className="segmented" role="group" aria-label="Calendar provider">{(Object.keys(PROVIDERS) as Provider[]).map(k =>
      <button key={k} aria-pressed={provider === k} onClick={() => setProvider(k)}>{PROVIDERS[k].name}</button>)}</div>
    <ol className="calendar-sub-steps">{p.steps.map((step, i) => <li key={i}>{step}</li>)}</ol>
    <form className="calendar-sub-form" onSubmit={e => { e.preventDefault(); if (url.trim() && !busy) run(url.trim()); }}>
      <input type="url" inputMode="url" spellCheck={false} autoComplete="off" aria-label="Calendar iCal link" placeholder={p.example} value={url} disabled={busy} onChange={e => { setUrl(e.target.value); setError(''); }} />
      <button type="submit" className="primary" disabled={busy || !url.trim()}>{busy ? 'Connecting…' : 'Connect'}</button>
      {editing && <button type="button" disabled={busy} onClick={() => { setEditing(false); setError(''); }}>Cancel</button>}
    </form>
    {error && <p className="calendar-sub-error" role="alert">{error}</p>}
  </section>;
}

export function CalendarApp({ model, intents }: { model: CalendarModel; intents: Pick<AppViewIntents, 'calendar'> & { subscription?(url: string | null): Promise<void> } }) {
  const events = [...model.agenda].sort((a, b) => a.startS - b.startS || a.instanceKey.localeCompare(b.instanceKey));
  const event = (row: AgendaRow, compact = false) => <button className={`calendar-event ${row.allDay ? 'all-day' : ''}`} onClick={() => intents.calendar({ type: 'select', instanceKey: row.instanceKey })}>
    <strong>{row.summary}</strong><small>{row.allDay ? 'All day' : row.timeLabel}{row.recurring === true && ' · Recurring'}</small>
    {!compact && <><small>{row.dateLabel}{row.spanLabel && ` · ${row.spanLabel}`}</small>{row.location && <small>{row.location}</small>}<small>{row.readOnly ? 'Subscription · Read-only' : 'Local event'}</small></>}
  </button>;
  return <div className="calendar-app app-stack">
    <div className="app-toolbar"><button aria-label="Previous month" onClick={() => intents.calendar({ type: 'previous' })}>‹</button><h2>{model.monthLabel}</h2><button aria-label="Next month" onClick={() => intents.calendar({ type: 'next' })}>›</button><span className="title-spacer"/>{(['month', 'agenda'] as const).map(view => <button key={view} aria-pressed={model.view === view} onClick={() => intents.calendar({ type: 'view', view })}>{view === 'month' ? 'Month' : 'Agenda'}</button>)}</div>
    {intents.subscription ? <CalendarSubscription configured={model.configured} connect={intents.subscription} />
      : !model.configured && <div className="app-banner">Subscription not configured <button onClick={() => intents.calendar({ type: 'configure' })}>Set up subscription</button></div>}
    <div className="app-toolbar"><button data-testid="calendar-new-event" onClick={() => intents.calendar({ type: 'create' })}>New local event</button></div>
    {(model.fetchFailed || model.stale) && <p role="status" className="app-banner warning">{model.fetchFailed ? 'Fetch failed' : 'Last good shown'}{model.stale && ` · Stale since ${model.staleSinceLabel ?? 'unknown'}`}</p>}
    <div className="calendar-body">{model.view === 'month' ? <div className="month-grid" aria-label="Month calendar">
      {model.weekdayLabels.map((label, n) => <span className="weekday" key={n}>{label}</span>)}
      {model.days.map(day => <div className="month-day" key={day.key} data-outside={!day.inMonth} data-today={day.today}><span>{day.label}</span>{events.filter(row => row.dayKeys.includes(day.key)).map(row => <div key={row.instanceKey}>{event(row, true)}</div>)}</div>)}
    </div> : events.length ? <ol className="app-list agenda-list">{events.map(row => <li key={row.instanceKey}>{event(row)}</li>)}</ol> : <Empty icon="calendar">No events in this window</Empty>}</div>
    <div className="app-footer"><span className="muted">Local dates supplied by calendar service</span><button onClick={() => intents.calendar({ type: 'refresh' })}>Refresh</button></div>
  </div>;
}
