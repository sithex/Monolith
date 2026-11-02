//! Colour helpers for the KWin colour picker (SPEC §0.5 item 6). The KWin
//! `org.kde.kwin.ColorPicker.pick()` reply is an ARGB u32 (`0xAARRGGBB`).

/// Convert a KWin ARGB u32 to a lowercase `#rrggbb` hex string (alpha
/// dropped).
pub fn argb_to_hex(argb: u32) -> String {
    let r = (argb >> 16) & 0xff;
    let g = (argb >> 8) & 0xff;
    let b = argb & 0xff;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Convert a KWin ARGB u32 to a CSS `rgba(r, g, b, a)` string, with alpha as
/// a 0.0–1.0 fraction (`a / 255`).
pub fn argb_to_rgba_css(argb: u32) -> String {
    let r = (argb >> 16) & 0xff;
    let g = (argb >> 8) & 0xff;
    let b = argb & 0xff;
    let a = (argb >> 24) & 0xff;
    format!("rgba({r}, {g}, {b}, {})", a as f32 / 255.0)
}
