import type { AgendaRow, CalendarOp } from '../ipc/client';
export type EventDraft = { uid: string; summary: string; location: string; start: string; end: string; allDay: boolean };
export const localDate = (d: Date) => `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
const localTime = (d: Date) => `${localDate(d)}T${String(d.getHours()).padStart(2,'0')}:${String(d.getMinutes()).padStart(2,'0')}:${String(d.getSeconds()).padStart(2,'0')}`;
export function draftEvent(row: AgendaRow): EventDraft {
  return { uid: row.uid, summary: row.summary, location: row.location ?? '', allDay: row.allDay, start: row.allDay ? localDate(new Date(row.startS*1000)) : localTime(new Date(row.startS*1000)), end: row.allDay ? localDate(new Date(row.endS*1000)) : localTime(new Date(row.endS*1000)) };
}
function bounded(s: string, max: number) { if (typeof s !== 'string' || new TextEncoder().encode(s).length > max || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(s)) throw Error('Event field is invalid or too long'); return s; }
const escapeText = (s: string) => s.replaceAll('\\','\\\\').replace(/\r\n|\r|\n/g,'\\n').replaceAll(';','\\;').replaceAll(',','\\,');
function fold(line: string) {
  let out = '', bytes = 0;
  for (const c of line) { const size = new TextEncoder().encode(c).length; if (bytes+size > 75) { out += '\r\n '; bytes = 1; } out += c; bytes += size; }
  return out;
}
function dateValue(value: string, allDay: boolean) {
  const pattern = allDay ? /^\d{4}-\d{2}-\d{2}$/ : /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2})?$/;
  if (!pattern.test(value)) throw Error('Enter valid event dates');
  const date = new Date(allDay ? `${value}T00:00:00` : value);
  // Refuse nonexistent local times, including DST gaps, instead of silently normalizing.
  if (!Number.isFinite(date.getTime()) || (allDay ? localDate(date) : localTime(date).slice(0,value.length)) !== value) throw Error('Event date or local time does not exist');
  return { ms: date.getTime(), wire: allDay ? value.replaceAll('-','') : date.toISOString().replace(/[-:]/g,'').replace(/\.\d{3}Z$/,'Z') };
}
/** Exactly one VEVENT; never accepts a URL or generates an RRULE/secret field. */
export function eventDocument(draft: EventDraft): string {
  const uid = bounded(draft.uid,1024); if (!uid || /[\r\n]/.test(uid)) throw Error('Invalid event identity');
  const summary = bounded(draft.summary,8192); if (!summary.trim()) throw Error('Enter an event title');
  const location = bounded(draft.location,8192), start = dateValue(draft.start,draft.allDay), end = dateValue(draft.end,draft.allDay);
  if (end.ms <= start.ms) throw Error('Event end must be after its start (all-day end is exclusive)');
  return ['BEGIN:VCALENDAR','VERSION:2.0','PRODID:-//Hermes Island//Local Calendar//EN','BEGIN:VEVENT',`UID:${escapeText(uid)}`,`DTSTART${draft.allDay ? ';VALUE=DATE' : ''}:${start.wire}`,`DTEND${draft.allDay ? ';VALUE=DATE' : ''}:${end.wire}`,`SUMMARY:${escapeText(summary)}`, ...(location ? [`LOCATION:${escapeText(location)}`] : []),'END:VEVENT','END:VCALENDAR'].map(fold).join('\r\n')+'\r\n';
}
export function saveEvent(draft: EventDraft, row: AgendaRow | null): CalendarOp {
  if (row && (row.readOnly || row.source !== 'local' || row.uid !== draft.uid)) throw Error('Subscribed events are read-only; local edits must retain their identity');
  const ics = eventDocument(draft);
  return row ? { type: 'edit', source: 'local', uid: row.uid, ics } : { type: 'create', ics };
}
export function deleteEvent(row: AgendaRow): CalendarOp {
  if (row.readOnly || row.source !== 'local') throw Error('Subscribed events are read-only');
  return { type: 'delete', source: 'local', uid: row.uid };
}
