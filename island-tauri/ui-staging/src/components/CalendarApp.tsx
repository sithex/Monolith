import type { CalendarModel, AgendaRow } from '../app-models';
import type { IslandIntents } from '../models';
import { Empty } from './AppPrimitives';
export function CalendarApp({ model, intents }: { model: CalendarModel; intents: Pick<IslandIntents, 'calendar'> }) {
  const events = [...model.agenda].sort((a, b) => a.startS - b.startS || a.instanceKey.localeCompare(b.instanceKey));
  const event = (row: AgendaRow, compact = false) => <button className={`calendar-event ${row.allDay ? 'all-day' : ''}`} onClick={() => intents.calendar({ type: 'select', instanceKey: row.instanceKey })}>
    <strong>{row.summary}</strong><small>{row.allDay ? 'All day' : row.timeLabel}{row.recurring && ' · Recurring'}</small>
    {!compact && <><small>{row.dateLabel}{row.spanLabel && ` · ${row.spanLabel}`}</small>{row.location && <small>{row.location}</small>}<small>{row.readOnly ? 'Subscription · Read-only' : 'Local event'}</small></>}
  </button>;
  return <div className="calendar-app app-stack">
    <div className="app-toolbar"><button aria-label="Previous month" onClick={() => intents.calendar({ type: 'previous' })}>‹</button><h2>{model.monthLabel}</h2><button aria-label="Next month" onClick={() => intents.calendar({ type: 'next' })}>›</button><span className="title-spacer"/>{(['month', 'agenda'] as const).map(view => <button key={view} aria-pressed={model.view === view} onClick={() => intents.calendar({ type: 'view', view })}>{view === 'month' ? 'Month' : 'Agenda'}</button>)}</div>
    {!model.configured && <div className="app-banner">Subscription not configured <button onClick={() => intents.calendar({ type: 'configure' })}>Set up subscription</button></div>}
    {(model.fetchFailed || model.stale) && <p role="status" className="app-banner warning">{model.fetchFailed ? 'Fetch failed · Last good shown' : 'Last good shown'} · Stale since {model.staleSinceLabel ?? 'unknown'}</p>}
    <div className="calendar-body">{model.view === 'month' ? <div className="month-grid" aria-label="Month calendar">
      {model.weekdayLabels.map((label, n) => <span className="weekday" key={n}>{label}</span>)}
      {model.days.map(day => <div className="month-day" key={day.key} data-outside={!day.inMonth} data-today={day.today}><span>{day.label}</span>{events.filter(row => row.dayKeys.includes(day.key)).map(row => <div key={row.instanceKey}>{event(row, true)}</div>)}</div>)}
    </div> : events.length ? <ol className="app-list agenda-list">{events.map(row => <li key={row.instanceKey}>{event(row)}</li>)}</ol> : <Empty icon="calendar">No events in this window</Empty>}</div>
    <div className="app-footer"><span className="muted">Local dates supplied by calendar service</span><button onClick={() => intents.calendar({ type: 'refresh' })}>Refresh</button></div>
  </div>;
}
