//! KGlobalAccel / QKeySequence portable text (`Meta+\``, `Ctrl+Alt+T`) → Qt key code.
//!
//! Pure: used by `check_shortcut` (settings validation) and by the global-shortcut registrar,
//! which sends the code to `org.kde.kglobalaccel` as a one-key `QKeySequence`. Only single
//! chords are accepted; sequences (`Ctrl+K, Ctrl+C`) are rejected upstream by the comma rule.

pub const SHIFT: i32 = 0x0200_0000;
pub const CTRL: i32 = 0x0400_0000;
pub const ALT: i32 = 0x0800_0000;
pub const META: i32 = 0x1000_0000;

/// Qt key codes for the named keys KDE writes in `kglobalshortcutsrc`.
const NAMED: &[(&str, i32)] = &[
    ("esc", 0x0100_0000),
    ("escape", 0x0100_0000),
    ("tab", 0x0100_0001),
    ("backspace", 0x0100_0003),
    ("return", 0x0100_0004),
    ("enter", 0x0100_0005),
    ("ins", 0x0100_0006),
    ("insert", 0x0100_0006),
    ("del", 0x0100_0007),
    ("delete", 0x0100_0007),
    ("pause", 0x0100_0008),
    ("print", 0x0100_0009),
    ("home", 0x0100_0010),
    ("end", 0x0100_0011),
    ("left", 0x0100_0012),
    ("up", 0x0100_0013),
    ("right", 0x0100_0014),
    ("down", 0x0100_0015),
    ("pgup", 0x0100_0016),
    ("pageup", 0x0100_0016),
    ("pgdown", 0x0100_0017),
    ("pagedown", 0x0100_0017),
    ("space", 0x20),
    ("menu", 0x0100_0055),
];

/// `Ok(None)` for `""` (no shortcut); `Err` for anything KGlobalAccel could not bind.
pub fn parse(text: &str) -> Result<Option<i32>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.len() > 64 || !text.is_ascii() || text.contains([',', '\t']) {
        return Err("unsupported characters");
    }
    // `Ctrl++` means Ctrl and the plus key: a trailing empty token is the `+` key.
    let (mods, key) = match text.strip_suffix("++") {
        Some(rest) => (rest, "+"),
        None if text == "+" => ("", "+"),
        None => match text.rsplit_once('+') {
            Some((mods, key)) => (mods, key),
            None => ("", text),
        },
    };
    let mut code = 0;
    if !mods.is_empty() {
        for m in mods.split('+') {
            let bit = match m.trim().to_ascii_lowercase().as_str() {
                "meta" | "super" | "win" => META,
                "ctrl" | "control" => CTRL,
                "alt" => ALT,
                "shift" => SHIFT,
                _ => return Err("unknown modifier"),
            };
            if code & bit != 0 {
                return Err("repeated modifier");
            }
            code |= bit;
        }
    }
    Ok(Some(code | key_code(key.trim())?))
}

fn key_code(key: &str) -> Result<i32, &'static str> {
    let lower = key.to_ascii_lowercase();
    if let Some((_, code)) = NAMED.iter().find(|(name, _)| *name == lower) {
        return Ok(*code);
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<i32>().ok()) {
        if (1..=35).contains(&n) && !lower.starts_with("f0") {
            return Ok(0x0100_0030 + n - 1);
        }
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        // Qt uses the upper-case code for letters; punctuation keeps its ASCII code.
        (Some(c), None) if c.is_ascii_graphic() => Ok(c.to_ascii_uppercase() as i32),
        _ => Err("unknown key"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kde_spellings_map_to_qt_codes() {
        assert_eq!(parse("Meta+`"), Ok(Some(META | 0x60)));
        assert_eq!(parse("Ctrl+Alt+T"), Ok(Some(CTRL | ALT | 0x54)));
        assert_eq!(parse("ctrl+alt+t"), parse("Ctrl+Alt+T"));
        assert_eq!(
            parse("Meta+Shift+F12"),
            Ok(Some(META | SHIFT | 0x0100_003b))
        );
        assert_eq!(parse("Meta+Space"), Ok(Some(META | 0x20)));
        assert_eq!(parse("Ctrl++"), Ok(Some(CTRL | 0x2b)));
        assert_eq!(parse("F1"), Ok(Some(0x0100_0030)));
        assert_eq!(parse("  "), Ok(None));
        assert_eq!(parse(""), Ok(None));
    }

    #[test]
    fn unbindable_text_is_rejected() {
        for bad in [
            "Meta+",
            "Meta",
            "Hyper+X",
            "Ctrl+Ctrl+X",
            "Meta+Foo",
            "F0",
            "F36",
            "F01",
            "Ctrl+K, Ctrl+C",
            "Meta+é",
        ] {
            assert!(parse(bad).is_err(), "{bad} must be rejected");
        }
    }
}
