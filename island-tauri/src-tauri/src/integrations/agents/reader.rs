//! In-process read of the `sessions` metadata for the agent chips. Same query, limits and value
//! tolerance as the former Python reader; the open is the shared read-only one.
use super::core::SessionRow;
use crate::{error::IslandError, services::read_only_sqlite};
use rusqlite::{types::ValueRef, Connection, Row};
use std::{path::Path, time::Duration};

/// SQLite busy wait (the Python reader's `timeout=0.5`).
pub const BUSY_TIMEOUT: Duration = Duration::from_millis(500);
/// Rows started at or after `now - LOOKBACK_SECS` are read.
pub const LOOKBACK_SECS: f64 = 86400.;
/// More rows than this is an error, never a silently truncated list.
pub const MAX_ROWS: usize = 10_000;
/// Any text value longer than this (UTF-8 bytes) is an error.
pub const MAX_FIELD_BYTES: usize = 4096;
/// Bound on the text read in one poll; the former helper's 4 MiB output cap.
pub const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;

const QUERY: &str = "SELECT id, source, parent_session_id, model, billing_provider, started_at, \
     ended_at, end_reason, last_activity_at, api_call_count, title FROM sessions \
     WHERE started_at >= ?1 ORDER BY started_at DESC LIMIT 10001";

fn invalid() -> IslandError {
    IslandError::unavailable("Agent metadata invalid")
}

/// Opens `path` through the shared read-only open and confirms `query_only` took effect.
pub fn open(path: &Path) -> Result<Connection, IslandError> {
    let db = read_only_sqlite::open(path, BUSY_TIMEOUT)
        .map_err(|_| IslandError::unavailable("Agent metadata unavailable"))?;
    let query_only: i64 = db
        .query_row("PRAGMA query_only", [], |r| r.get(0))
        .map_err(|_| IslandError::unavailable("Agent metadata unavailable"))?;
    if query_only != 1 {
        return Err(IslandError::unavailable("Agent metadata not read-only"));
    }
    Ok(db)
}

/// Sessions started within the last day, newest first.
pub fn read_sessions(path: &Path, now: f64) -> Result<Vec<SessionRow>, IslandError> {
    let db = open(path)?;
    let mut statement = db
        .prepare(QUERY)
        .map_err(|_| IslandError::unavailable("Agent metadata unavailable"))?;
    let mut rows = statement
        .query([now - LOOKBACK_SECS])
        .map_err(|_| IslandError::unavailable("Agent metadata unavailable"))?;
    let mut out = Vec::new();
    let mut total = 0usize;
    while let Some(row) = rows
        .next()
        .map_err(|_| IslandError::unavailable("Agent metadata unavailable"))?
    {
        if out.len() == MAX_ROWS {
            return Err(IslandError::unavailable("Agent metadata row limit"));
        }
        out.push(session_row(row, &mut total)?);
    }
    Ok(out)
}

fn session_row(row: &Row<'_>, total: &mut usize) -> Result<SessionRow, IslandError> {
    let mut text = |i: usize| -> Result<Option<String>, IslandError> {
        match row.get_ref(i).map_err(|_| invalid())? {
            ValueRef::Null => Ok(None),
            ValueRef::Text(bytes) => {
                if bytes.len() > MAX_FIELD_BYTES {
                    return Err(invalid());
                }
                *total = total.saturating_add(bytes.len());
                if *total > MAX_TOTAL_BYTES {
                    return Err(invalid());
                }
                Ok(Some(
                    std::str::from_utf8(bytes)
                        .map_err(|_| invalid())?
                        .to_owned(),
                ))
            }
            ValueRef::Integer(_) | ValueRef::Real(_) | ValueRef::Blob(_) => Err(invalid()),
        }
    };
    let id = text(0)?.ok_or_else(invalid)?;
    let source = text(1)?.ok_or_else(invalid)?;
    let parent_session_id = text(2)?;
    let model = text(3)?;
    let billing_provider = text(4)?;
    let end_reason = text(7)?;
    let title = text(10)?;
    let number = |i: usize| -> Result<Option<f64>, IslandError> {
        match row.get_ref(i).map_err(|_| invalid())? {
            ValueRef::Null => Ok(None),
            ValueRef::Integer(n) => Ok(Some(n as f64)),
            // JSON had no NaN or infinity, so neither is accepted here.
            ValueRef::Real(x) if x.is_finite() => Ok(Some(x)),
            _ => Err(invalid()),
        }
    };
    let started_at = number(5)?.ok_or_else(invalid)?;
    let ended_at = number(6)?;
    let last_activity_at = number(8)?;
    let api_call_count = match row.get_ref(9).map_err(|_| invalid())? {
        ValueRef::Null => None,
        ValueRef::Integer(n) => Some(u64::try_from(n).map_err(|_| invalid())?),
        _ => return Err(invalid()),
    };
    Ok(SessionRow {
        id,
        source,
        parent_session_id,
        model,
        billing_provider,
        started_at,
        ended_at,
        end_reason,
        last_activity_at,
        api_call_count,
        title,
    })
}
