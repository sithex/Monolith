use super::model::ClipPayload;

/// Build the summary preview string for a payload.
///
/// * Text: whitespace collapsed to single spaces, trimmed, at most `max_chars`
///   characters — never splitting a Unicode scalar value.
/// * Image: `Image W×H`.
/// * Files: `N files: a, b, …` (basenames), truncated at `max_chars` with a
///   trailing `…`.
pub fn preview(payload: &ClipPayload, max_chars: usize) -> String {
    match payload {
        ClipPayload::Text(text) => collapse_whitespace(text, max_chars),
        ClipPayload::Image { width, height, .. } => format!("Image {width}×{height}"),
        ClipPayload::Files(uris) => files_preview(uris, max_chars),
    }
}

/// Collapse runs of whitespace into a single space, trim the edges, then keep
/// at most `max_chars` characters. Operates on `char`s, so a multi-byte
/// character is never split.
fn collapse_whitespace(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut pending_space = false;
    let mut emitted = 0usize;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if emitted > 0 {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            if emitted >= max_chars {
                break;
            }
            out.push(' ');
            emitted += 1;
            pending_space = false;
        }
        if emitted >= max_chars {
            break;
        }
        out.push(ch);
        emitted += 1;
    }
    out
}

fn files_preview(uris: &[String], max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let names: Vec<String> = uris.iter().map(|u| display_name(u)).collect();
    let full = format!("{} files: {}", names.len(), names.join(", "));
    if full.chars().count() <= max_chars {
        return full;
    }
    let mut out: String = full.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Last path segment of a URI; the whole string when there is no `/`.
fn display_name(uri: &str) -> String {
    match uri.rfind('/') {
        Some(i) => uri[i + 1..].to_string(),
        None => uri.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> ClipPayload {
        ClipPayload::Text(s.to_string())
    }

    #[test]
    fn preview_truncates_unicode_safely() {
        // "é" is two bytes; a byte-wise cut would split it.
        let payload = t("héllo");
        assert_eq!(preview(&payload, 2), "hé");
        assert_eq!(preview(&payload, 3), "hél");
        // Multi-byte char straddling the boundary.
        let payload = t("日本語テキスト");
        assert_eq!(preview(&payload, 3), "日本語");
        // max_chars longer than content is a no-op.
        assert_eq!(preview(&t("hi"), 10), "hi");
    }

    #[test]
    fn preview_collapses_whitespace() {
        assert_eq!(preview(&t("  a   b\t\n c  "), 280), "a b c");
        assert_eq!(preview(&t("\n\n"), 280), "");
        assert_eq!(preview(&t("   x"), 280), "x");
    }

    #[test]
    fn preview_zero_chars_returns_empty() {
        assert_eq!(preview(&t("hello"), 0), "");
        assert_eq!(preview(&ClipPayload::Files(vec!["a".into()]), 0), "");
        assert_eq!(
            preview(
                &ClipPayload::Image {
                    png: vec![],
                    width: 10,
                    height: 10
                },
                0
            ),
            "Image 10×10"
        );
    }

    #[test]
    fn preview_image_format() {
        let img = ClipPayload::Image {
            png: vec![],
            width: 640,
            height: 480,
        };
        assert_eq!(preview(&img, 280), "Image 640×480");
    }

    #[test]
    fn preview_files_format() {
        let files = ClipPayload::Files(vec!["file:///a/b.txt".into(), "file:///c.png".into()]);
        assert_eq!(preview(&files, 280), "2 files: b.txt, c.png");
    }

    #[test]
    fn preview_files_truncation() {
        let files = ClipPayload::Files(vec![
            "file:///a/one.txt".into(),
            "file:///a/two.txt".into(),
            "file:///a/three.txt".into(),
        ]);
        let p = preview(&files, 12);
        assert!(p.chars().count() <= 12, "got: {p}");
        assert!(p.ends_with('…'));
        // Never split a char: 12 chars, "…" is one char.
        assert!(p.is_char_boundary(p.len()) || p.ends_with('…'));
    }
}
