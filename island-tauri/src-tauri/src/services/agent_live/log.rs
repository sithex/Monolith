//! Rotation-safe, bounded `logs/agent.log` reader. The opaque cursor belongs to the UI;
//! file identity and bytes always come from the same open descriptor. A skipped oversized
//! line stays discarded until its newline (never return its suffix as a complete line).

use crate::contract::agent_live::AgentLiveLog;
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::Path,
};

/// A first read (null/invalid cursor) starts this far before EOF.
pub const INITIAL_BYTES: u64 = 512 * 1024;
/// Most bytes read per call, including the rotated file and boundary probes.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    dev: u64,
    ino: u64,
    offset: u64,
    discard: bool,
}

impl Cursor {
    fn encode(&self) -> String {
        format!(
            "v2:{}:{}:{}:{}",
            self.dev,
            self.ino,
            self.offset,
            u8::from(self.discard)
        )
    }

    fn decode(s: &str) -> Option<Self> {
        if s.len() > 128 {
            return None;
        }
        let mut it = s.split(':');
        let version = it.next()?;
        if !matches!(version, "v1" | "v2") {
            return None;
        }
        let number = |v: &str| {
            (!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                .then(|| v.parse::<u64>().ok())
                .flatten()
        };
        let mut c = Cursor {
            dev: number(it.next()?)?,
            ino: number(it.next()?)?,
            offset: number(it.next()?)?,
            discard: false,
        };
        if version == "v2" {
            c.discard = match it.next()? {
                "0" => false,
                "1" => true,
                _ => return None,
            };
        }
        it.next().is_none().then_some(c)
    }
}

/// Read complete new lines; missing/unreadable files are data errors, never IPC failures.
pub fn read(base: &Path, cursor: Option<&str>) -> AgentLiveLog {
    let prev = cursor.and_then(Cursor::decode);
    let preserved = || {
        cursor
            .filter(|_| prev.is_some())
            .unwrap_or_default()
            .to_owned()
    };
    let fail = |error: &str| AgentLiveLog {
        cursor: preserved(),
        lines: Vec::new(),
        rotated: false,
        error: Some(error.to_owned()),
    };
    let mut file = match super::open_regular(base) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return fail("agent.log not found"),
        Err(_) => return fail("agent.log unreadable"),
    };
    let meta = match file.metadata() {
        Ok(m) => m,
        Err(_) => return fail("agent.log unreadable"),
    };
    let (dev, ino, size) = (meta.dev(), meta.ino(), meta.len());
    let mut lines = Vec::new();
    let mut budget = MAX_BYTES;
    let rotated = prev.is_some_and(|c| c.dev != dev || c.ino != ino || size < c.offset);
    let (mut offset, mut discard) = match prev {
        None => (size.saturating_sub(INITIAL_BYTES), false),
        Some(c) if rotated => {
            let old = base.with_file_name(format!(
                "{}.1",
                base.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("agent.log")
            ));
            if let Ok(mut old_file) = super::open_regular(&old) {
                if let Ok(m) = old_file.metadata() {
                    if m.dev() == c.dev && m.ino() == c.ino {
                        // Reserve room for the current file; neither file can starve the other.
                        let old_budget = budget / 2;
                        if let Ok((_, _, used)) = read_lines(
                            &mut old_file,
                            c.offset,
                            c.discard,
                            old_budget,
                            true,
                            &mut lines,
                        ) {
                            budget = budget.saturating_sub(used);
                        }
                    }
                }
            }
            (0, false)
        }
        Some(c) => (c.offset, c.discard),
    };
    let skipped = size.saturating_sub(offset) > budget;
    if skipped {
        offset = size.saturating_sub(INITIAL_BYTES.min(budget));
    }
    if (prev.is_none() || skipped) && offset > 0 {
        // A one-byte boundary probe avoids dropping a full line when the window happens
        // to start immediately after a newline. It counts against the total I/O budget.
        let mut byte = [0];
        let probe = file
            .seek(SeekFrom::Start(offset - 1))
            .and_then(|_| file.read_exact(&mut byte));
        if probe.is_err() {
            return fail("agent.log unreadable");
        }
        budget = budget.saturating_sub(1);
        discard = byte[0] != b'\n';
    }
    match read_lines(&mut file, offset, discard, budget, false, &mut lines) {
        Ok((offset, discard, _)) => AgentLiveLog {
            cursor: Cursor {
                dev,
                ino,
                offset,
                discard,
            }
            .encode(),
            lines,
            rotated,
            error: None,
        },
        Err(_) => fail("agent.log unreadable"),
    }
}

/// Return (next offset, discarding an oversized line, bytes read). Only an old file's
/// actual EOF may flush an unterminated line; hitting the budget is not EOF.
fn read_lines(
    file: &mut File,
    from: u64,
    mut discard: bool,
    budget: u64,
    flush_tail: bool,
    out: &mut Vec<String>,
) -> io::Result<(u64, bool, u64)> {
    file.seek(SeekFrom::Start(from))?;
    let available = file.metadata()?.len().saturating_sub(from);
    let mut buf = Vec::new();
    file.take(budget).read_to_end(&mut buf)?;
    let n = buf.len() as u64;
    if n == 0 {
        return Ok((from, discard, 0));
    }
    let begin = if discard {
        match buf.iter().position(|&b| b == b'\n') {
            Some(nl) => {
                discard = false;
                nl + 1
            }
            None => return Ok((from + n, true, n)),
        }
    } else {
        0
    };
    let take = match buf[begin..].iter().rposition(|&b| b == b'\n') {
        _ if flush_tail && n >= available => buf.len(),
        Some(nl) => begin + nl + 1,
        None if begin == buf.len() => begin,
        // A single line filled the entire budget. Advance, but keep discarding its tail.
        None if n >= budget => return Ok((from + n, true, n)),
        None => begin,
    };
    for raw in buf[begin..take].split(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(raw);
        let line = line.strip_suffix('\r').unwrap_or(&line);
        if !line.is_empty() {
            out.push(line.to_owned());
        }
    }
    Ok((from + take as u64, discard, n))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::services::scratch;
    use std::{fs, io::Write};

    fn append(path: &Path, s: &str) {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }

    #[test]
    fn missing_file_fails_soft_and_keeps_only_a_valid_cursor() {
        let dir = scratch("alog-missing");
        let r = read(&dir.join("agent.log"), Some("v1:1:2:3"));
        assert_eq!(r.error.as_deref(), Some("agent.log not found"));
        assert_eq!(r.cursor, "v1:1:2:3");
        assert!(r.lines.is_empty());
        assert!(read(&dir.join("agent.log"), Some(&"x".repeat(1000)))
            .cursor
            .is_empty());
        assert!(Cursor::decode("v1:+1:2:3").is_none());
        assert!(Cursor::decode("v2:1:2:3:2").is_none());
        assert!(Cursor::decode("v2:1:2:3:0:extra").is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_lines_wait_for_their_newline() {
        let dir = scratch("alog-partial");
        let log = dir.join("agent.log");
        append(&log, "a\nb\npart");
        let r1 = read(&log, None);
        assert_eq!(r1.lines, ["a", "b"]);
        let r2 = read(&log, Some(&r1.cursor));
        assert!(r2.lines.is_empty());
        append(&log, "ial\r\nc\n");
        let r3 = read(&log, Some(&r2.cursor));
        assert_eq!(r3.lines, ["partial", "c"]);
        assert!(!r3.rotated);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn first_read_is_the_last_512_kib_without_the_partial_first_line() {
        let dir = scratch("alog-start");
        let log = dir.join("agent.log");
        let line = format!("{}\n", "x".repeat(99));
        let mut body = line.repeat(10_000);
        body.push_str("last\n");
        append(&log, &body);
        let r = read(&log, None);
        assert_eq!(r.lines.last().map(String::as_str), Some("last"));
        assert!(r.lines[..r.lines.len() - 1].iter().all(|l| l.len() == 99));
        let bytes: usize = r.lines.iter().map(|l| l.len() + 1).sum();
        assert!(bytes as u64 <= INITIAL_BYTES && bytes as u64 > INITIAL_BYTES - 200);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rotation_drains_the_old_inode_then_reads_the_new_file_from_zero() {
        let dir = scratch("alog-rotate");
        let log = dir.join("agent.log");
        append(&log, "one\n");
        let r1 = read(&log, None);
        append(&log, "two\nthree-partial");
        fs::rename(&log, dir.join("agent.log.1")).unwrap();
        append(&log, "new1\n");
        let r2 = read(&log, Some(&r1.cursor));
        assert!(r2.rotated);
        assert_eq!(r2.lines, ["two", "three-partial", "new1"]);
        fs::write(&log, "n\n").unwrap();
        let r3 = read(&log, Some(&r2.cursor));
        assert!(r3.rotated);
        assert_eq!(r3.lines, ["n"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_huge_backlog_is_capped_and_garbage_cursors_restart() {
        let dir = scratch("alog-cap");
        let log = dir.join("agent.log");
        append(&log, "start\n");
        let r1 = read(&log, None);
        let line = format!("{}\n", "y".repeat(1023));
        append(&log, &line.repeat(3 * 1024));
        let r2 = read(&log, Some(&r1.cursor));
        let bytes: u64 = r2.lines.iter().map(|l| l.len() as u64 + 1).sum();
        assert!(bytes <= MAX_BYTES, "{bytes}");
        assert!(!r2.lines.iter().any(|l| l == "start"));
        let r3 = read(&log, Some("nonsense"));
        assert!(!r3.lines.is_empty() && r3.error.is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_line_never_returns_its_suffix_as_a_complete_line() {
        let dir = scratch("alog-long-line");
        let log = dir.join("agent.log");
        append(&log, &"x".repeat(INITIAL_BYTES as usize + 70_000));
        let first = read(&log, None);
        assert!(first.lines.is_empty());
        assert!(Cursor::decode(&first.cursor).unwrap().discard);
        append(&log, "not-a-line\nreal\n");
        let next = read(&log, Some(&first.cursor));
        assert_eq!(next.lines, ["real"]);
        assert!(!Cursor::decode(&next.cursor).unwrap().discard);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn old_file_budget_is_not_treated_as_eof_and_new_file_is_not_starved() {
        let dir = scratch("alog-rotation-budget");
        let log = dir.join("agent.log");
        append(&log, "start\n");
        let first = read(&log, None);
        append(&log, &"x".repeat(MAX_BYTES as usize + 1));
        fs::rename(&log, dir.join("agent.log.1")).unwrap();
        append(&log, "current\n");
        let next = read(&log, Some(&first.cursor));
        assert_eq!(next.lines, ["current"]);
        assert!(next.rotated);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nonregular_logs_fail_soft_without_blocking() {
        let dir = scratch("alog-nonregular");
        let log = dir.join("agent.log");
        fs::create_dir(&log).unwrap();
        assert_eq!(
            read(&log, None).error.as_deref(),
            Some("agent.log unreadable")
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
