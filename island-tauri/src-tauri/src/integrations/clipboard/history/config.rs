use super::error::Error;
use regex::Regex;

pub const MIN_CAPACITY: usize = 10;
pub const MAX_CAPACITY: usize = 500;
pub const DEFAULT_CAPACITY: usize = 100;
pub const DEFAULT_MAX_TEXT_BYTES: usize = 1024 * 1024; // 1 MiB
pub const DEFAULT_MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024; // 8 MiB
pub const DEFAULT_MAX_IMAGE_SIDE: u32 = 4096;
/// Default total retained-payload budget. Entry count alone allows 100 x 8 MiB images
/// (800 MiB); this caps the sum of every entry's payload bytes (text bytes, raw PNG bytes,
/// file-URI bytes), pinned entries included. Not a user setting.
pub const DEFAULT_MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024; // 64 MiB
pub const DEFAULT_PREVIEW_CHARS: usize = 280;
pub const MAX_DENY_PATTERNS: usize = 32;
pub const MAX_DENY_PATTERN_CHARS: usize = 256;

/// Configuration for the history store.
///
/// * `capacity` — maximum number of entries (pinned entries count toward it).
///   Clamped to `[MIN_CAPACITY, MAX_CAPACITY]` at validation; the default is
///   `DEFAULT_CAPACITY`.
/// * `max_text_bytes` — hard size limit for text entries (1 MiB default).
/// * `max_image_bytes` — hard size limit for the raw PNG bytes (8 MiB default).
/// * `max_image_side` — maximum allowed `width`/`height` pixel dimension.
/// * `max_total_bytes` — budget for the summed payload bytes of all retained entries
///   (64 MiB default). Raised at validation to at least the largest per-item limit so any
///   admissible single entry can fit.
/// * `preview_chars` — maximum number of characters in a generated preview
///   (never splits a Unicode scalar value).
/// * `deny_patterns` — user deny-list regexes. At most
///   `MAX_DENY_PATTERNS` patterns, each at most `MAX_DENY_PATTERN_CHARS`
///   characters, each must compile. Any violation is a hard config error (it
///   is never silently ignored).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryConfig {
    pub capacity: usize,
    pub max_text_bytes: usize,
    pub max_image_bytes: usize,
    pub max_image_side: u32,
    pub max_total_bytes: usize,
    pub preview_chars: usize,
    pub deny_patterns: Vec<String>,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_image_bytes: DEFAULT_MAX_IMAGE_BYTES,
            max_image_side: DEFAULT_MAX_IMAGE_SIDE,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            preview_chars: DEFAULT_PREVIEW_CHARS,
            deny_patterns: Vec::new(),
        }
    }
}

impl HistoryConfig {
    /// Validate and normalize this configuration.
    ///
    /// The capacity is silently clamped into `[MIN_CAPACITY, MAX_CAPACITY]`.
    /// Deny-pattern violations (too many, too long, empty, or uncompilable)
    /// are returned as `Error::InvalidConfig`.
    pub fn validated(mut self) -> Result<Self, Error> {
        if self.deny_patterns.len() > MAX_DENY_PATTERNS {
            return Err(Error::InvalidConfig(format!(
                "too many deny patterns ({} > {MAX_DENY_PATTERNS})",
                self.deny_patterns.len()
            )));
        }
        for pattern in &self.deny_patterns {
            if pattern.is_empty() {
                return Err(Error::InvalidConfig(
                    "deny pattern must not be empty".to_string(),
                ));
            }
            if pattern.chars().count() > MAX_DENY_PATTERN_CHARS {
                return Err(Error::InvalidConfig(format!(
                    "deny pattern exceeds {MAX_DENY_PATTERN_CHARS} chars: {pattern:?}"
                )));
            }
            if let Err(e) = Regex::new(pattern) {
                return Err(Error::InvalidConfig(format!(
                    "deny pattern does not compile: {pattern:?}: {e}"
                )));
            }
        }
        self.capacity = self.capacity.clamp(MIN_CAPACITY, MAX_CAPACITY);
        self.max_total_bytes = self
            .max_total_bytes
            .max(self.max_text_bytes)
            .max(self.max_image_bytes);
        Ok(self)
    }
}
