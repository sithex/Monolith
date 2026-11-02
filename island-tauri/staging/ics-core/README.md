# ics-core

Pure iCalendar (RFC 5545 subset) core for the Phase 6b read-only Calendar app
(SPEC §8 "Phase 6b"). Backend-neutral: the Tauri side supplies `.ics` TEXT
(any KDE/online calendar can export it) and this crate parses events and
expands an agenda over a time window.

Platform-free: no IO, no network, no threads, no clock, no `unsafe`, no
`chrono` (civil-date math is implemented here). Dependencies: serde (derive)
+ serde_json only. Wire types use camelCase via
`#[serde(rename_all = "camelCase")]`, no aliases.

## API

- `parse_calendar(text) -> Result<(Vec<Event>, Vec<Warning>), IcsError>`
- `agenda(events, window_start_s, window_end_s, local_offset_s, tz) -> Vec<Instance>`
- `agenda_checked(...) -> Result<Vec<Instance>, IcsError>` (typed 400-day
  window error)
- `agenda_checked_with_warnings(...) -> Result<(Vec<Instance>, Vec<Warning>), IcsError>`
  (also surfaces runtime warnings such as unknown TZIDs)

`tz` is `&dyn Fn(&str, NaiveDateTime) -> Option<i32>` returning an offset in
seconds for `(tzid, local naive time)`; `None` means unknown TZID (treated as
floating with a warning). `NaiveDateTime` is this crate's own tiny struct.

## Supported subset

Content lines CRLF or bare LF, unfolding (space/tab), case-insensitive
property/param names, quoted params, TEXT escapes. Components: VCALENDAR /
VEVENT (VTODO, VTIMEZONE, VALARM ignored). VEVENT properties: UID, SUMMARY,
LOCATION, DESCRIPTION, DTSTART, DTEND, DURATION, RRULE (FREQ DAILY/WEEKLY/
MONTHLY/YEARLY, INTERVAL, COUNT, UNTIL, BYDAY for WEEKLY, WKST ignored),
EXDATE (repeat/comma list), STATUS CANCELLED, RECURRENCE-ID overrides.

Limits: input ≤ 8 MiB, no NUL, windows ≤ 400 days, ≤ 2000 returned instances,
≤ 10 000 generated candidates per rule.

## Gate

```
nice -n 10 cargo fmt --check
nice -n 10 cargo clippy --all-targets --locked -- -D warnings
nice -n 10 cargo test --locked
```

See `VERIFICATION.md` for gate output and mutation checks.
