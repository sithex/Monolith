//! Pure iCalendar (RFC 5545 subset) core for the Phase 6b read-only Calendar
//! app (SPEC §8 "Phase 6b"; design direction in §0.5 "Apps"). The calendar
//! backend is not chosen yet, but every KDE/online option can export
//! iCalendar text: the Tauri side supplies `.ics` TEXT and this crate parses
//! events and expands an agenda over a time window.
//!
//! Platform-free: no IO, no network, no threads, no clock, no `unsafe`, no
//! `chrono` (civil-date math is implemented here). The crate never panics on
//! any input. Wire types use camelCase via `#[serde(rename_all =
//! "camelCase")]`, no aliases.

#![forbid(unsafe_code)]

mod agenda;
mod civil;
mod error;
mod ics;
mod value;

pub use agenda::{agenda, agenda_checked, agenda_checked_with_warnings, Instance};
pub use civil::NaiveDateTime;
pub use error::{IcsError, Warning};
pub use ics::{parse_calendar, Event, Freq, Rrule, Weekday};
pub use value::{DateTimeKind, DateTimeValue};
