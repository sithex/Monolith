import { useState } from 'react';
import type { AgendaRow, CalendarOp } from '../ipc/client';
import { draftEvent, localDate, saveEvent, deleteEvent, type EventDraft } from './calendar-editor';
export function CalendarEventEditor({ row, date, command, close }: { row: AgendaRow | null; date: Date; command(op: CalendarOp): Promise<void>; close(): void }) {
  const [draft, setDraft] = useState<EventDraft>(() => row ? draftEvent(row) : { uid: crypto.randomUUID(), summary: '', location: '', start: `${localDate(date)}T09:00:00`, end: `${localDate(date)}T10:00:00`, allDay: false });
  const [busy, setBusy] = useState(false), [error, setError] = useState(''), [confirmDelete, setConfirmDelete] = useState(false);
  const readOnly = row !== null && (row.source !== 'local' || row.readOnly);
  async function perform(op: () => CalendarOp) { if (busy || readOnly) return; setError(''); try { const value = op(); setBusy(true); await command(value); close(); } catch (e) { setError(e instanceof Error ? e.message : 'Calendar command failed'); } finally { setBusy(false); } }
  return <section className="card app-stack" data-testid="calendar-event-editor" aria-label={readOnly ? 'Event details' : row ? 'Edit local event' : 'New local event'}>
    <div className="app-toolbar"><h3>{readOnly ? 'Subscription · Read-only' : row ? 'Edit local event' : 'New local event'}</h3><span className="title-spacer"/><button type="button" disabled={busy} onClick={close}>Close event</button></div>
    {readOnly ? <><h4>{row.summary}</h4><p>{row.allDay ? 'All day · ' : ''}{new Date(row.startS*1000).toLocaleString()} → {new Date(row.endS*1000).toLocaleString()}</p>{row.location && <p>{row.location}</p>}</> : <form className="app-stack" onSubmit={e => { e.preventDefault(); void perform(() => saveEvent(draft,row)); }}>
      <fieldset disabled={busy} className="app-stack" style={{ border: 0, padding: 0, minWidth: 0 }}>
        <label>Title<input aria-label="Event title" required maxLength={8192} value={draft.summary} onChange={e => setDraft({ ...draft, summary: e.target.value })}/></label>
        <label>Location<input aria-label="Event location" maxLength={8192} value={draft.location} onChange={e => setDraft({ ...draft, location: e.target.value })}/></label>
        <label><input aria-label="All-day event" type="checkbox" checked={draft.allDay} onChange={e => setDraft({ ...draft, allDay: e.target.checked, start: e.target.checked ? draft.start.slice(0,10) : `${draft.start}T09:00:00`, end: e.target.checked ? draft.end.slice(0,10) : `${draft.end}T10:00:00` })}/>All day</label>
        <label>Start<input aria-label="Event start" type={draft.allDay ? 'date' : 'datetime-local'} step={draft.allDay ? undefined : 1} required value={draft.start} onChange={e => setDraft({ ...draft, start: e.target.value })}/></label>
        <label>End{draft.allDay ? ' (exclusive)' : ''}<input aria-label="Event end" type={draft.allDay ? 'date' : 'datetime-local'} step={draft.allDay ? undefined : 1} required value={draft.end} onChange={e => setDraft({ ...draft, end: e.target.value })}/></label>
        {row && <p className="muted">This replaces the entire local event with this single occurrence. Any recurrence or unsupported fields are removed. Delete removes the entire event.</p>}
        <div className="app-footer"><button type="submit" data-testid="calendar-save-event">{busy ? 'Saving…' : 'Save event'}</button>{row && <button type="button" onClick={() => setConfirmDelete(true)}>Delete event</button>}</div>
        {row && confirmDelete && <div role="group" aria-label="Confirm event deletion"><p>Delete this local event?</p><button type="button" data-testid="calendar-confirm-delete" onClick={() => { void perform(() => deleteEvent(row)); }}>Confirm delete event</button><button type="button" onClick={() => setConfirmDelete(false)}>Keep event</button></div>}
      </fieldset>
    </form>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
