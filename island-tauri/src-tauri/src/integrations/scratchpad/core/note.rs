use serde::{Deserialize, Serialize};

/// Maximum note body size in bytes (256 KiB).
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// Maximum title length in characters (80).
pub const TITLE_MAX_CHARS: usize = 80;
/// Minimum valid note id length in characters.
pub const MIN_ID_LEN: usize = 1;
/// Maximum valid note id length in characters.
pub const MAX_ID_LEN: usize = 64;

/// A full scratchpad note. `title` is always derived from `body` (first
/// non-blank line, trimmed, cut at [`TITLE_MAX_CHARS`] on a char boundary,
/// "Untitled" when blank) — it is recomputed on load and never trusted from
/// storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// Unique id: 1..=64 chars of [A-Za-z0-9_-].
    pub id: String,
    /// Derived title (see struct docs).
    pub title: String,
    /// Note body, at most [`MAX_BODY_BYTES`] bytes.
    pub body: String,
    /// Creation time (injected monotonic ms).
    pub created_ms: u64,
    /// Last modification time (injected monotonic ms).
    pub updated_ms: u64,
    /// Whether the note sorts first.
    pub pinned: bool,
}

/// Lightweight list view of a note (no body).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummary {
    /// Note id.
    pub id: String,
    /// Derived title.
    pub title: String,
    /// Creation time (injected monotonic ms).
    pub created_ms: u64,
    /// Last modification time (injected monotonic ms).
    pub updated_ms: u64,
    /// Whether the note sorts first.
    pub pinned: bool,
}

/// Derive a note title from its body: first non-blank line, trimmed, cut at
/// [`TITLE_MAX_CHARS`] on a character boundary (never mid-UTF-8),
/// "Untitled" when the body is blank.
pub(crate) fn derive_title(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return trimmed.chars().take(TITLE_MAX_CHARS).collect();
        }
    }
    "Untitled".to_string()
}

/// True when `id` is 1..=64 chars of [A-Za-z0-9_-].
pub(crate) fn is_valid_id(id: &str) -> bool {
    let mut len = 0usize;
    for c in id.chars() {
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return false;
        }
        len += 1;
        if len > MAX_ID_LEN {
            return false;
        }
    }
    len >= MIN_ID_LEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_body_title_is_untitled() {
        assert_eq!(derive_title(""), "Untitled");
    }

    #[test]
    fn whitespace_only_body_title_is_untitled() {
        assert_eq!(derive_title("   \n\t\n  \n"), "Untitled");
    }

    #[test]
    fn title_uses_first_nonblank_line() {
        let body = "\n   \nHello world\nsecond line\n";
        assert_eq!(derive_title(body), "Hello world");
    }

    #[test]
    fn title_trims_the_picked_line() {
        assert_eq!(derive_title("  padded  \nrest"), "padded");
    }

    #[test]
    fn title_ascii_cut_at_80_chars() {
        let title = derive_title(&"a".repeat(81));
        assert_eq!(title.len(), 80);
        assert_eq!(title, "a".repeat(80));
    }

    #[test]
    fn title_cut_never_splits_multibyte_char() {
        // 79 ASCII chars followed by one 2-byte char = exactly 80 chars.
        let body = format!("{}é", "a".repeat(79));
        let title = derive_title(&body);
        assert_eq!(title.chars().count(), 80);
        assert!(title.ends_with('é'));
        assert!(std::str::from_utf8(title.as_bytes()).is_ok());
    }

    #[test]
    fn title_cut_with_emoji_straddling_boundary() {
        // 79 ASCII chars + a 4-byte emoji: the emoji is char #80, fully kept.
        let body = format!("{}😀", "a".repeat(79));
        let title = derive_title(&body);
        assert_eq!(title.chars().count(), 80);
        assert!(title.ends_with('😀'));
    }

    #[test]
    fn title_cut_skips_emoji_beyond_80_chars() {
        // 80 ASCII chars + emoji: the emoji is char #81, cut cleanly.
        let body = format!("{}😀", "a".repeat(80));
        let title = derive_title(&body);
        assert_eq!(title, "a".repeat(80));
    }

    #[test]
    fn title_always_valid_utf8() {
        for body in ["ééé", "日本語のテキスト", "a😀b", "αβγδ", "🌍"] {
            let title = derive_title(body);
            assert!(std::str::from_utf8(title.as_bytes()).is_ok());
            assert!(title.chars().count() <= TITLE_MAX_CHARS);
        }
    }

    #[test]
    fn id_validation_boundaries() {
        assert!(is_valid_id("a"));
        assert!(is_valid_id("A"));
        assert!(is_valid_id("0"));
        assert!(is_valid_id("-"));
        assert!(is_valid_id("_"));
        assert!(is_valid_id(&"a".repeat(64)));
        assert!(is_valid_id(&format!("{}-{}", "a".repeat(62), "Z")));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id(&"a".repeat(65)));
        assert!(!is_valid_id("a b"));
        assert!(!is_valid_id("a.b"));
        assert!(!is_valid_id("a/b"));
        assert!(!is_valid_id("á"));
        assert!(!is_valid_id("aá"));
        assert!(!is_valid_id("#"));
        assert!(!is_valid_id(" "));
        assert!(!is_valid_id("a\n"));
        assert!(!is_valid_id("\u{0}"));
    }
}
