use std::collections::HashMap;

use crate::error::ParseError;
use crate::model::{AppEntry, Warning};

const MAX_INPUT_BYTES: usize = 64 * 1024;

/// A parsed locale `lang_COUNTRY.ENCODING@MODIFIER` (encoding ignored).
struct Locale {
    lang: String,
    country: Option<String>,
    modifier: Option<String>,
}

impl Locale {
    fn parse(s: &str) -> Locale {
        let without_encoding = s.split('.').next().unwrap_or(s);
        let (lang_country, modifier) = match without_encoding.split_once('@') {
            Some((lc, m)) => (lc, Some(m.to_string())),
            None => (without_encoding, None),
        };
        let (lang, country) = match lang_country.split_once('_') {
            Some((l, c)) => (l.to_string(), Some(c.to_string())),
            None => (lang_country.to_string(), None),
        };
        Locale {
            lang,
            country,
            modifier,
        }
    }

    /// Candidate localized-key suffixes, most specific first:
    /// `lang_COUNTRY@MODIFIER`, `lang_COUNTRY`, `lang@MODIFIER`, `lang`.
    fn candidates(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(4);
        if let (Some(c), Some(m)) = (&self.country, &self.modifier) {
            out.push(format!("{}_{}@{}", self.lang, c, m));
        }
        if let Some(c) = &self.country {
            out.push(format!("{}_{}", self.lang, c));
        }
        if let Some(m) = &self.modifier {
            out.push(format!("{}@{}", self.lang, m));
        }
        out.push(self.lang.clone());
        out
    }
}

/// Parse `.desktop` file text into a launcher entry.
pub fn parse_desktop_entry(
    text: &str,
    desktop_id: &str,
    locale: Option<&str>,
) -> Result<(AppEntry, Vec<Warning>), ParseError> {
    if text.len() > MAX_INPUT_BYTES {
        return Err(ParseError::InputTooLarge);
    }
    if text.contains('\0') {
        return Err(ParseError::ContainsNul);
    }
    let locale_info = locale.map(Locale::parse);

    let mut group: HashMap<String, String> = HashMap::new();
    let mut saw_desktop_entry = false;
    let mut active = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = group_header(line) {
            if name == "Desktop Entry" {
                if !saw_desktop_entry {
                    saw_desktop_entry = true;
                    active = true;
                } else {
                    // Only the first [Desktop Entry] group is processed.
                    active = false;
                }
            } else {
                active = false;
            }
            continue;
        }
        if !active {
            continue;
        }
        if let Some((key, value)) = split_key_value(line) {
            // First occurrence of a duplicate key wins.
            group.entry(key).or_insert(value);
        }
    }

    if !saw_desktop_entry {
        return Err(ParseError::NoDesktopEntryGroup);
    }

    let type_value = group.get("Type").ok_or(ParseError::MissingType)?;
    if type_value != "Application" {
        return Err(ParseError::NotApplication(type_value.clone()));
    }
    let name = localized(&group, "Name", locale_info.as_ref()).ok_or(ParseError::MissingName)?;
    let exec = group.get("Exec").ok_or(ParseError::MissingExec)?;

    let mut warnings = Vec::new();
    let terminal = parse_bool(&group, "Terminal", &mut warnings);
    let no_display = parse_bool(&group, "NoDisplay", &mut warnings);
    let hidden = parse_bool(&group, "Hidden", &mut warnings);

    let entry = AppEntry {
        desktop_id: desktop_id.to_string(),
        name: unescape(name),
        generic_name: localized(&group, "GenericName", locale_info.as_ref())
            .map(unescape)
            .unwrap_or_default(),
        comment: localized(&group, "Comment", locale_info.as_ref())
            .map(unescape)
            .unwrap_or_default(),
        icon: localized(&group, "Icon", locale_info.as_ref())
            .map(unescape)
            .unwrap_or_default(),
        exec: exec.clone(),
        try_exec: group
            .get("TryExec")
            .map(|s| unescape(s))
            .unwrap_or_default(),
        terminal,
        no_display,
        hidden,
        only_show_in: parse_list(list_value(&group, "OnlyShowIn")),
        not_show_in: parse_list(list_value(&group, "NotShowIn")),
        categories: parse_list(list_value(&group, "Categories")),
        keywords: parse_list(localized(&group, "Keywords", locale_info.as_ref()).unwrap_or("")),
    };

    Ok((entry, warnings))
}

fn group_header(line: &str) -> Option<String> {
    let t = line.trim();
    if !t.starts_with('[') {
        return None;
    }
    let rest = &t[1..];
    let pos = rest.rfind(']')?;
    let inner = &rest[..pos];
    if inner.is_empty() {
        None
    } else {
        Some(inner.trim().to_string())
    }
}

fn split_key_value(line: &str) -> Option<(String, String)> {
    let eq = line.find('=')?;
    let key = line[..eq].trim();
    if key.is_empty() {
        return None;
    }
    let value = line[eq + 1..].trim();
    Some((key.to_string(), value.to_string()))
}

/// Apply the standard string escapes to a non-Exec value.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A boolean key is exactly `true` or `false`; anything else is treated as
/// `false` and counts a warning.
fn parse_bool(group: &HashMap<String, String>, key: &str, warnings: &mut Vec<Warning>) -> bool {
    match group.get(key) {
        Some(v) if v == "true" => true,
        Some(v) if v == "false" => false,
        Some(v) => {
            warnings.push(Warning::new(format!(
                "key {key} has non-boolean value {v:?}; treated as false"
            )));
            false
        }
        None => false,
    }
}

fn list_value<'a>(group: &'a HashMap<String, String>, key: &str) -> &'a str {
    group.get(key).map_or("", |s| s.as_str())
}

/// Resolve a possibly-localized key: try `base[lang_COUNTRY@MODIFIER]`,
/// `base[lang_COUNTRY]`, `base[lang@MODIFIER]`, `base[lang]`, then the
/// unlocalized `base`.
fn localized<'a>(
    group: &'a HashMap<String, String>,
    base: &str,
    locale: Option<&Locale>,
) -> Option<&'a str> {
    if let Some(loc) = locale {
        for cand in loc.candidates() {
            let key = format!("{base}[{cand}]");
            if let Some(v) = group.get(&key) {
                return Some(v);
            }
        }
    }
    group.get(base).map(|s| s.as_str())
}

/// Parse a `;`-separated list: `\;` is a literal semicolon, `\\` a literal
/// backslash, the standard `\s \n \t \r` escapes apply, and a trailing `;`
/// is allowed. Empty (after trimming) items are dropped.
fn parse_list(raw: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(';') => cur.push(';'),
                Some('\\') => cur.push('\\'),
                Some('s') => cur.push(' '),
                Some('n') => cur.push('\n'),
                Some('t') => cur.push('\t'),
                Some('r') => cur.push('\r'),
                Some(other) => {
                    cur.push('\\');
                    cur.push(other);
                }
                None => cur.push('\\'),
            },
            ';' => {
                let item = cur.trim();
                if !item.is_empty() {
                    items.push(item.to_string());
                }
                cur.clear();
            }
            c => cur.push(c),
        }
    }
    let tail = cur.trim();
    if !tail.is_empty() {
        items.push(tail.to_string());
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_candidates_full_chain() {
        let loc = Locale::parse("sr_RS@latin");
        assert_eq!(
            loc.candidates(),
            vec!["sr_RS@latin", "sr_RS", "sr@latin", "sr"]
        );
    }

    #[test]
    fn locale_candidates_de_de_utf8_ignores_encoding() {
        let loc = Locale::parse("de_DE.UTF-8");
        assert_eq!(loc.candidates(), vec!["de_DE", "de"]);
    }

    #[test]
    fn locale_candidates_plain_lang() {
        let loc = Locale::parse("pt");
        assert_eq!(loc.candidates(), vec!["pt"]);
    }

    #[test]
    fn locale_candidates_lang_at_modifier() {
        let loc = Locale::parse("sr@latin");
        assert_eq!(loc.candidates(), vec!["sr@latin", "sr"]);
    }

    #[test]
    fn list_split_with_escaped_semicolon() {
        assert_eq!(parse_list("a;b\\;c;"), vec!["a", "b;c"]);
    }

    #[test]
    fn list_escapes_and_empty_items() {
        assert_eq!(parse_list("x\\syy;;z;"), vec!["x yy", "z"]);
    }

    #[test]
    fn list_backslash_escape() {
        assert_eq!(parse_list("a\\\\b"), vec!["a\\b"]);
    }
}
