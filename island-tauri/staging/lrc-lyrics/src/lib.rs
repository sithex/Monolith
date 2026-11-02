//! Pure LRC (Lyrics by LRCLIB) synced-lyrics parser.
//!
//! Parses LRC text as returned by LRCLIB's `syncedLyrics` field and answers
//! "which line is active at playback position t". No IO, no network, no
//! panics on any input.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

/// Inputs larger than 1 MiB are rejected before parsing.
const MAX_INPUT_BYTES: usize = 1024 * 1024;

/// Errors produced by [`parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// The input contained no timed line at all.
    NoTimedLines,
    /// The input exceeded 1 MiB.
    TooLarge,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NoTimedLines => write!(f, "no timed lines found"),
            ParseError::TooLarge => write!(f, "input exceeds 1 MiB"),
        }
    }
}

impl Error for ParseError {}

/// One timed lyric line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Start time in milliseconds, as written in the file (offset not applied).
    pub start_ms: u64,
    /// Lyric text: enhanced word-timing tags removed, whitespace trimmed.
    pub text: String,
}

/// Metadata gathered from `[ti:]`, `[ar:]`, `[al:]`, `[length:]` tags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub length_ms: Option<u64>,
}

/// A parsed LRC document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lyrics {
    /// Timed lines sorted by [`Line::start_ms`] (stable for equal times).
    pub lines: Vec<Line>,
    /// `[offset:+/-N]` value in milliseconds. Positive shifts lyrics earlier.
    pub offset_ms: i64,
    pub metadata: Metadata,
}

impl Lyrics {
    /// Index of the active line at `position_ms`, or `None` when the position is
    /// before the first offset-adjusted line.
    ///
    /// A line is active once its offset-adjusted start time is reached and
    /// remains active until the next line's offset-adjusted start time. A
    /// positive `offset_ms` makes lines active earlier (lyrics shown earlier);
    /// a negative one makes them active later. When several lines share the
    /// same start time, the last one in input order is the active one.
    pub fn active_index(&self, position_ms: u64) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        let bound = position_ms as i128 + self.offset_ms as i128;
        let count = self
            .lines
            .partition_point(|l| (l.start_ms as i128) <= bound);
        if count == 0 {
            None
        } else {
            Some(count - 1)
        }
    }

    /// A contiguous window of lines around the active line: up to `before`
    /// lines before it and up to `after` lines after it, clamped to the edges.
    /// Before the first line (nothing active yet) the first `after` lines are
    /// returned.
    pub fn window(&self, position_ms: u64, before: usize, after: usize) -> &[Line] {
        match self.active_index(position_ms) {
            Some(i) => {
                let lo = i.saturating_sub(before);
                let hi = (i + 1 + after).min(self.lines.len());
                &self.lines[lo..hi]
            }
            None => &self.lines[..after.min(self.lines.len())],
        }
    }
}

/// Parse LRC text into [`Lyrics`].
pub fn parse(input: &str) -> Result<Lyrics, ParseError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(ParseError::TooLarge);
    }
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);

    let mut lines = Vec::new();
    let mut offset_ms = 0i64;
    let mut metadata = Metadata::default();

    for raw in input.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let mut rest = line;
        let mut stamps = Vec::new();
        loop {
            let trimmed = rest.trim_start();
            let Some(inner) = trimmed.strip_prefix('[') else {
                break;
            };
            let Some(close) = inner.find(']') else {
                break;
            };
            let tag = &inner[..close];
            let after = &inner[close + 1..];

            if let Some(ms) = parse_timestamp(tag) {
                stamps.push(ms);
            } else if let Some(v) = parse_offset(tag) {
                offset_ms = v;
            } else if let Some((kind, value)) = parse_metadata_tag(tag) {
                apply_metadata(&mut metadata, kind, value);
            }
            // Any other tag is unknown and ignored.
            rest = after;
        }

        if stamps.is_empty() {
            continue;
        }
        let text = strip_word_timing(rest).trim().to_string();
        for ms in stamps {
            lines.push(Line {
                start_ms: ms,
                text: text.clone(),
            });
        }
    }

    if lines.is_empty() {
        return Err(ParseError::NoTimedLines);
    }
    lines.sort_by_key(|l| l.start_ms);

    Ok(Lyrics {
        lines,
        offset_ms,
        metadata,
    })
}

/// Parse `mm:ss` / `mm:ss.frac` / `mm:ss:frac` into milliseconds. Minutes may
/// exceed 59 and have any number of digits. Fractions are decimal seconds:
/// `.5` = 500 ms, `.05` = 50 ms, `.005` = 5 ms.
fn parse_timestamp(tag: &str) -> Option<u64> {
    let parts: Vec<&str> = tag.split(['.', ':']).collect();
    let (mm, ss, frac) = match parts.as_slice() {
        [m, s] => (*m, *s, ""),
        [m, s, f] => (*m, *s, *f),
        _ => return None,
    };
    if mm.is_empty() || ss.is_empty() {
        return None;
    }
    let minutes = parse_digits(mm)?;
    let seconds = parse_digits(ss)?;
    let mut total = minutes
        .checked_mul(60_000)?
        .checked_add(seconds.checked_mul(1_000)?)?;
    if !frac.is_empty() {
        total = total.checked_add(parse_fraction_ms(frac)?)?;
    }
    Some(total)
}

fn parse_digits(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn parse_fraction_ms(frac: &str) -> Option<u64> {
    if frac.len() > 9 {
        return None;
    }
    let num = parse_digits(frac)?;
    let denom = 10u64.pow(frac.len() as u32);
    num.checked_mul(1_000)?.checked_div(denom)
}

/// Parse the `[offset:+/-N]` tag into a millisecond delta.
fn parse_offset(tag: &str) -> Option<i64> {
    let rest = strip_tag_prefix(tag, "offset:")?;
    let (neg, digits) = match rest.as_bytes().first() {
        Some(b'+') => (false, &rest[1..]),
        Some(b'-') => (true, &rest[1..]),
        _ => (false, rest),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let magnitude: i64 = digits.parse().ok()?;
    Some(if neg { -magnitude } else { magnitude })
}

enum MetadataKind {
    Title,
    Artist,
    Album,
    Length,
}

fn parse_metadata_tag(tag: &str) -> Option<(MetadataKind, &str)> {
    if let Some(v) = strip_tag_prefix(tag, "ti:") {
        return Some((MetadataKind::Title, v));
    }
    if let Some(v) = strip_tag_prefix(tag, "ar:") {
        return Some((MetadataKind::Artist, v));
    }
    if let Some(v) = strip_tag_prefix(tag, "al:") {
        return Some((MetadataKind::Album, v));
    }
    if let Some(v) = strip_tag_prefix(tag, "length:") {
        return Some((MetadataKind::Length, v));
    }
    None
}

fn apply_metadata(metadata: &mut Metadata, kind: MetadataKind, value: &str) {
    let value = value.trim();
    match kind {
        MetadataKind::Title => metadata.title = Some(value.to_string()),
        MetadataKind::Artist => metadata.artist = Some(value.to_string()),
        MetadataKind::Album => metadata.album = Some(value.to_string()),
        MetadataKind::Length => {
            if let Some(ms) = parse_timestamp(value) {
                metadata.length_ms = Some(ms);
            }
        }
    }
}

/// Case-insensitive `[prefix]`-style tag prefix strip (ASCII-aware, never
/// panics on non-ASCII input: slicing only happens at verified char bounds).
fn strip_tag_prefix<'a>(tag: &'a str, prefix: &str) -> Option<&'a str> {
    let head = tag.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &tag[prefix.len()..])
}

/// Remove enhanced word-timing `<mm:ss.xx>` spans from text. Spans whose inner
/// content is not a valid timestamp (e.g. `<3>`) are left untouched.
fn strip_word_timing(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('<') {
            if let Some(end) = inner.find('>') {
                if parse_timestamp(&inner[..end]).is_some() {
                    rest = &inner[end + 1..];
                    continue;
                }
            }
        }
        let ch = rest.chars().next().unwrap_or_default();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}
