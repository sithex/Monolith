//! Installed font families for the Settings font picker (fontconfig `fc-list`).
//!
//! Read-only and bounded (3 s, 4 MiB). `fc-list` runs once per island run; a successful list
//! is cached (installing a font needs an island restart to show up). Only families that can be
//! saved as `terminalFontFamily` AND used unquoted in a CSS family list are returned, because
//! that setting refuses quotes (so names such as `Noto Sans Symbols 2` cannot be written).

use super::blocking;
use crate::error::IslandError;
use serde::Serialize;
use std::sync::{Arc, Mutex};

/// One installed family. `monospace`: some face reports fontconfig spacing mono (100) or
/// dual-width (90).
#[derive(Serialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FontFamily {
    pub family: String,
    pub monospace: bool,
}

pub const FC_LIST_FORMAT: &str = "%{family[0]}\\t%{spacing}\\n";
const MAX_FAMILIES: usize = 4096;

static CACHE: Mutex<Option<Arc<Vec<FontFamily>>>> = Mutex::new(None);

#[tauri::command]
#[specta::specta]
pub async fn font_families() -> Result<Vec<FontFamily>, IslandError> {
    if let Some(cached) = CACHE
        .lock()
        .map_err(|_| IslandError::internal("font cache poisoned"))?
        .as_ref()
    {
        return Ok(cached.as_ref().clone());
    }
    let list = blocking(|| {
        let mut command = std::process::Command::new("fc-list");
        command.arg("--format").arg(FC_LIST_FORMAT);
        let raw = crate::services::process::run_bounded(
            command,
            std::time::Duration::from_secs(3),
            4 * 1024 * 1024,
            false,
        )?;
        Ok(parse_fc_list(&String::from_utf8_lossy(&raw)))
    })
    .await?;
    let list = Arc::new(list);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(Arc::clone(&list));
    }
    Ok(list.as_ref().clone())
}

/// `family\tspacing` lines → deduped families sorted case-insensitively. A family is
/// monospace when any of its faces is (Nerd Fonts mix spacings across styles).
pub fn parse_fc_list(text: &str) -> Vec<FontFamily> {
    let mut map: std::collections::BTreeMap<String, (String, bool)> = Default::default();
    for line in text.lines() {
        let (family, spacing) = line.split_once('\t').unwrap_or((line, ""));
        // fontconfig escapes literal separators in %{family[0]}; a trailing `\` never survives
        // `css_family_ok` anyway.
        let family = family.trim();
        if !css_family_ok(family) {
            continue;
        }
        let mono = matches!(spacing.trim(), "100" | "90");
        let entry = map
            .entry(family.to_lowercase())
            .or_insert_with(|| (family.to_owned(), false));
        entry.1 |= mono;
        if map.len() >= MAX_FAMILIES {
            break;
        }
    }
    map.into_values()
        .map(|(family, monospace)| FontFamily { family, monospace })
        .collect()
}

/// Passes `check_font_family` (no quotes, `;{}<>\`, control chars, ≤128 bytes), contains no
/// comma (it would split the family list) and is a run of CSS identifiers, so it works
/// unquoted in `font-family` (each word must not start with a digit or `--`).
pub fn css_family_ok(family: &str) -> bool {
    family.len() <= 128 && !family.is_empty() && family.split(' ').all(css_ident)
}

/// One unquoted CSS identifier: `[-]?[A-Za-z_ or non-ASCII][A-Za-z0-9_- or non-ASCII]*`
/// (no leading digit, no `--`). Everything `check_font_family` refuses is outside this set.
fn css_ident(word: &str) -> bool {
    let start = |c: char| {
        c.is_ascii_alphabetic()
            || c == '_'
            || (!c.is_ascii() && !c.is_control() && !c.is_whitespace())
    };
    let rest = |c: char| start(c) || c.is_ascii_digit() || c == '-';
    let body = word.strip_prefix('-').unwrap_or(word);
    let mut chars = body.chars();
    chars.next().is_some_and(start) && chars.all(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dedupes_sorts_and_flags_monospace() {
        let raw = "DejaVu Sans Mono\t100\nDejaVu Sans\t\nFantasqueSansM Nerd Font\t\n\
                   FantasqueSansM Nerd Font\t100\nadwaita sans\t0\nAdwaita Sans\t\nBitstream Charter\t90\n";
        let got = parse_fc_list(raw);
        let names: Vec<_> = got
            .iter()
            .map(|f| (f.family.as_str(), f.monospace))
            .collect();
        assert_eq!(
            names,
            [
                ("adwaita sans", false),
                ("Bitstream Charter", true),
                ("DejaVu Sans", false),
                ("DejaVu Sans Mono", true),
                ("FantasqueSansM Nerd Font", true),
            ]
        );
    }

    #[test]
    fn families_the_setting_or_css_cannot_hold_are_skipped() {
        for bad in [
            "Noto Sans Symbols 2",
            "cursor.pcf",
            "0xProto",
            "a,b",
            "Bad\"Quote",
            "It's",
            "a;b",
            "x\\y",
            "<b>",
            "Two  Spaces",
            " Lead",
            "--x",
            "-1x",
            "",
        ] {
            assert!(!css_family_ok(bad), "{bad:?} must be skipped");
            // The parser trims surrounding blanks first, so only untrimmed names must vanish.
            if bad.trim() == bad {
                assert!(
                    parse_fc_list(&format!("{bad}\t100\n")).is_empty(),
                    "{bad:?}"
                );
            }
        }
        for ok in [
            "DejaVu Sans Mono",
            "Noto Sans CJK JP",
            "Fira_Code",
            "-x-Font",
            "Ubuntu-Mono",
            "思源黑体",
        ] {
            assert!(css_family_ok(ok), "{ok:?} must be kept");
        }
        assert!(!css_family_ok(&"a".repeat(129)));
    }

    #[test]
    fn every_returned_family_passes_the_settings_rule() {
        let raw = "Hack\t100\nNoto Sans Symbols 2\t\nJetBrains Mono\t100\n";
        for family in parse_fc_list(raw) {
            let patch: crate::contract::settings::SettingsPatch =
                serde_json::from_value(serde_json::json!({ "terminalFontFamily": family.family }))
                    .unwrap();
            assert!(
                patch.apply(&Default::default()).is_ok(),
                "{} refused by check_font_family",
                family.family
            );
        }
    }
}
