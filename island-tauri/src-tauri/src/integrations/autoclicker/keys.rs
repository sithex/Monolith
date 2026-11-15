//! Key text (the same KDE spelling the hotkeys use: `E`, `Space`, `Ctrl+C`) → Linux evdev key
//! codes for the virtual keyboard. Keys are physical (US positions): the desktop's layout
//! decides which character a code types, like a real keyboard.
use crate::keyseq;

pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_LEFTMETA: u16 = 125;

/// A key with the modifiers held around it, outermost first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Vec<u16>,
    pub key: u16,
}

/// Qt key code (no modifier bits) → evdev code.
fn evdev(qt: i32) -> Option<u16> {
    const LETTERS: &[u16; 26] = &[
        30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17,
        45, 21, 44,
    ];
    Some(match qt {
        0x41..=0x5a => LETTERS[(qt - 0x41) as usize],
        0x31..=0x39 => (qt - 0x31 + 2) as u16,
        0x30 => 11,
        0x2d => 12,                                                   // -
        0x3d => 13,                                                   // =
        0x5b => 26,                                                   // [
        0x5d => 27,                                                   // ]
        0x3b => 39,                                                   // ;
        0x27 => 40,                                                   // '
        0x60 => 41,                                                   // `
        0x5c => 43,                                                   // \
        0x2c => 51,                                                   // ,
        0x2e => 52,                                                   // .
        0x2f => 53,                                                   // /
        0x20 => 57,                                                   // Space
        0x0100_0000 => 1,                                             // Esc
        0x0100_0001 => 15,                                            // Tab
        0x0100_0003 => 14,                                            // Backspace
        0x0100_0004 => 28,                                            // Return
        0x0100_0005 => 96,                                            // Enter (keypad)
        0x0100_0006 => 110,                                           // Insert
        0x0100_0007 => 111,                                           // Delete
        0x0100_0008 => 119,                                           // Pause
        0x0100_0009 => 99,                                            // Print
        0x0100_0010 => 102,                                           // Home
        0x0100_0011 => 107,                                           // End
        0x0100_0012 => 105,                                           // Left
        0x0100_0013 => 103,                                           // Up
        0x0100_0014 => 106,                                           // Right
        0x0100_0015 => 108,                                           // Down
        0x0100_0016 => 104,                                           // PgUp
        0x0100_0017 => 109,                                           // PgDown
        0x0100_0055 => 127,                                           // Menu
        0x0100_0030..=0x0100_0039 => (qt - 0x0100_0030 + 59) as u16,  // F1-F10
        0x0100_003a => 87,                                            // F11
        0x0100_003b => 88,                                            // F12
        0x0100_003c..=0x0100_0047 => (qt - 0x0100_003c + 183) as u16, // F13-F24
        _ => return None,
    })
}

/// `None` for text the virtual keyboard can't press (or `""`).
pub fn chord(text: &str) -> Option<Chord> {
    let code = keyseq::parse(text).ok().flatten()?;
    let key = evdev(code & !(keyseq::SHIFT | keyseq::CTRL | keyseq::ALT | keyseq::META))?;
    let modifiers = [
        (keyseq::CTRL, KEY_LEFTCTRL),
        (keyseq::ALT, KEY_LEFTALT),
        (keyseq::SHIFT, KEY_LEFTSHIFT),
        (keyseq::META, KEY_LEFTMETA),
    ]
    .into_iter()
    .filter(|(bit, _)| code & bit != 0)
    .map(|(_, ev)| ev)
    .collect();
    Some(Chord { modifiers, key })
}

/// Every code the virtual device may send, for its key bitmap.
pub fn all_codes() -> Vec<u16> {
    let mut codes: Vec<u16> = (0x20..=0x60)
        .chain(0x0100_0000..=0x0100_0055)
        .filter_map(evdev)
        .collect();
    codes.extend([KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_LEFTALT, KEY_LEFTMETA]);
    codes.sort_unstable();
    codes.dedup();
    codes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_named_keys_map_to_evdev() {
        assert_eq!(chord("E").unwrap().key, 18);
        assert_eq!(chord("q").unwrap().key, 16);
        assert_eq!(chord("1").unwrap().key, 2);
        assert_eq!(chord("0").unwrap().key, 11);
        assert_eq!(chord("Space").unwrap().key, 57);
        assert_eq!(chord("F1").unwrap().key, 59);
        assert_eq!(chord("F12").unwrap().key, 88);
        assert_eq!(chord("F13").unwrap().key, 183);
        assert_eq!(chord("Esc").unwrap().key, 1);
        let c = chord("Ctrl+Shift+C").unwrap();
        assert_eq!(
            (c.modifiers, c.key),
            (vec![KEY_LEFTCTRL, KEY_LEFTSHIFT], 46)
        );
    }

    #[test]
    fn unpressable_text_is_none() {
        assert_eq!(chord(""), None);
        assert_eq!(chord("!"), None);
        assert_eq!(chord("F30"), None);
        assert_eq!(chord("Hyper+E"), None);
    }

    #[test]
    fn device_bitmap_covers_every_mapped_key() {
        let all = all_codes();
        for text in ["A", "Z", "9", "Space", "Return", "F24", "Menu", "`"] {
            assert!(all.contains(&chord(text).unwrap().key), "{text}");
        }
        assert!(all.contains(&KEY_LEFTMETA));
    }
}
