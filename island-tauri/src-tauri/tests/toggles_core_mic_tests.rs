use hermes_island_lib::integrations::toggles::core::{
    mic_get_argv, mic_set_argv, parse_mic_mute, MicOp, ParseError,
};

#[test]
fn mic_get_argv_exact() {
    assert_eq!(
        mic_get_argv(),
        vec![
            "pactl".to_string(),
            "get-source-mute".to_string(),
            "@DEFAULT_SOURCE@".to_string()
        ]
    );
}

#[test]
fn mic_set_argv_exact_all_ops() {
    let expected = |arg: &str| {
        vec![
            "pactl".to_string(),
            "set-source-mute".to_string(),
            "@DEFAULT_SOURCE@".to_string(),
            arg.to_string(),
        ]
    };
    assert_eq!(mic_set_argv(MicOp::Mute), expected("1"));
    assert_eq!(mic_set_argv(MicOp::Unmute), expected("0"));
    assert_eq!(mic_set_argv(MicOp::Toggle), expected("toggle"));
}

#[test]
fn parse_accepts_bare_yes_no() {
    assert_eq!(parse_mic_mute("Mute: yes"), Ok(true));
    assert_eq!(parse_mic_mute("Mute: no"), Ok(false));
}

#[test]
fn parse_accepts_trailing_newline_and_crlf() {
    assert_eq!(parse_mic_mute("Mute: yes\n"), Ok(true));
    assert_eq!(parse_mic_mute("Mute: no\r\n"), Ok(false));
}

#[test]
fn parse_rejects_localized_text() {
    assert_eq!(parse_mic_mute("Mute: ja"), Err(ParseError::InvalidOutput));
    assert_eq!(parse_mic_mute("Stumm: ja"), Err(ParseError::InvalidOutput));
}

#[test]
fn parse_rejects_case_insensitive() {
    assert!(parse_mic_mute("mute: yes").is_err());
    assert!(parse_mic_mute("MUTE: YES").is_err());
}

#[test]
fn parse_rejects_extra_lines() {
    assert!(parse_mic_mute("Mute: yes\nMute: no").is_err());
}

#[test]
fn parse_rejects_empty_and_whitespace() {
    assert!(parse_mic_mute("").is_err());
    assert!(parse_mic_mute(" ").is_err());
    assert!(parse_mic_mute("Mute: yes ").is_err());
    assert!(parse_mic_mute("Mute: yes\r").is_err());
}

#[test]
fn parse_rejects_oversize() {
    let big = format!("Mute: yes{}", " ".repeat(248)); // 257 bytes
    assert_eq!(big.len(), 257);
    assert_eq!(parse_mic_mute(&big), Err(ParseError::TooLarge));
}

#[test]
fn parse_size_cap_at_256_bytes() {
    let boundary = format!("Mute: yes{}", " ".repeat(247)); // exactly 256 bytes
    assert_eq!(boundary.len(), 256);
    // Size is within the cap; the content is still invalid.
    assert_eq!(parse_mic_mute(&boundary), Err(ParseError::InvalidOutput));
}
