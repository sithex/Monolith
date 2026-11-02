use hermes_island_lib::integrations::media::lrc::{parse, Line, Lyrics, Metadata, ParseError};

fn parse_ok(input: &str) -> Lyrics {
    match parse(input) {
        Ok(l) => l,
        Err(e) => panic!("expected Ok, got {e:?}"),
    }
}

fn line(ms: u64, text: &str) -> Line {
    Line {
        start_ms: ms,
        text: text.to_string(),
    }
}

#[test]
fn parses_basic_mm_ss() {
    let l = parse_ok("[00:10.00]hello\n[00:20.00]world");
    assert_eq!(l.lines, vec![line(10_000, "hello"), line(20_000, "world")]);
}

#[test]
fn fraction_scaling() {
    let l = parse_ok("[00:01.5]a\n[00:02.05]b\n[00:03.005]c");
    assert_eq!(
        l.lines,
        vec![line(1_500, "a"), line(2_050, "b"), line(3_005, "c")]
    );
}

#[test]
fn colon_fraction() {
    assert_eq!(parse_ok("[00:10:50]x").lines, vec![line(10_500, "x")]);
    assert_eq!(parse_ok("[01:02:50]x").lines, vec![line(62_500, "x")]);
    assert_eq!(parse_ok("[00:10.50]x").lines, parse_ok("[00:10:50]x").lines);
}

#[test]
fn minutes_may_exceed_59_and_have_many_digits() {
    let l = parse_ok("[75:00]x\n[123:45]y");
    assert_eq!(l.lines, vec![line(4_500_000, "x"), line(7_425_000, "y")]);
}

#[test]
fn multi_timestamp_line_expands() {
    let l = parse_ok("[00:10.00][00:20.00]chorus");
    assert_eq!(
        l.lines,
        vec![line(10_000, "chorus"), line(20_000, "chorus")]
    );
}

#[test]
fn empty_text_timed_lines_kept() {
    let l = parse_ok("[00:10.00]\n[00:20.00]music");
    assert_eq!(l.lines, vec![line(10_000, ""), line(20_000, "music")]);
}

#[test]
fn untimed_non_metadata_lines_ignored() {
    let l = parse_ok("just plain text\n[00:10.00]a");
    assert_eq!(l.lines, vec![line(10_000, "a")]);
    assert_eq!(l.metadata, Metadata::default());
}

#[test]
fn unknown_tags_ignored() {
    let l = parse_ok("[re:some editor]\n[by:someone]\n[ve:1.0]\n[xyz:zzz]\n[00:10.00]a");
    assert_eq!(l.lines, vec![line(10_000, "a")]);
}

#[test]
fn malformed_timestamp_tag_is_ignored() {
    assert_eq!(parse("[10]hi"), Err(ParseError::NoTimedLines));
    assert_eq!(parse("[10:20:30:40]hi"), Err(ParseError::NoTimedLines));
    assert_eq!(parse("[:10]hi"), Err(ParseError::NoTimedLines));
}

#[test]
fn crlf_and_utf8_bom_handled() {
    let l = parse_ok("\u{FEFF}[00:10.00]a\r\n[00:20.00]b\r\n");
    assert_eq!(l.lines, vec![line(10_000, "a"), line(20_000, "b")]);
}

#[test]
fn whitespace_trimmed() {
    let l = parse_ok("[00:10.00]   hello world   \n[00:20.00]\tspaced\t");
    assert_eq!(
        l.lines,
        vec![line(10_000, "hello world"), line(20_000, "spaced")]
    );
}

#[test]
fn enhanced_word_timing_stripped() {
    let l = parse_ok("[00:10.00]hello <00:10.50>world<00:11.20>!");
    assert_eq!(l.lines, vec![line(10_000, "hello world!")]);
}

#[test]
fn non_timestamp_angle_spans_kept() {
    let l = parse_ok("[00:10.00]I <3 you <b>bold</b>");
    assert_eq!(l.lines, vec![line(10_000, "I <3 you <b>bold</b>")]);
}

#[test]
fn metadata_parsed() {
    let l = parse_ok("[ti:My Song]\n[ar:An Artist]\n[al:An Album]\n[length:03:24]\n[00:10.00]a");
    assert_eq!(l.metadata.title.as_deref(), Some("My Song"));
    assert_eq!(l.metadata.artist.as_deref(), Some("An Artist"));
    assert_eq!(l.metadata.album.as_deref(), Some("An Album"));
    assert_eq!(l.metadata.length_ms, Some(204_000));
}

#[test]
fn bad_length_value_leaves_none() {
    let l = parse_ok("[length:oops]\n[00:10.00]a");
    assert_eq!(l.metadata.length_ms, None);
}

#[test]
fn no_timed_lines_errors() {
    assert_eq!(
        parse("just metadata\n[ti:title]"),
        Err(ParseError::NoTimedLines)
    );
    assert_eq!(parse(""), Err(ParseError::NoTimedLines));
}

#[test]
fn too_large_errors() {
    let mut s = String::with_capacity(1_048_577);
    s.push_str("[00:10.00]x");
    while s.len() <= 1_048_576 {
        s.push(' ');
    }
    assert_eq!(parse(&s), Err(ParseError::TooLarge));
}

#[test]
fn exactly_one_mib_accepted() {
    let mut s = String::with_capacity(1_048_576);
    s.push_str("[00:10.00]x");
    while s.len() < 1_048_576 {
        s.push(' ');
    }
    assert!(parse(&s).is_ok());
}

#[test]
fn active_index_boundaries() {
    let l = parse_ok("[00:10.00]a\n[00:20.00]b\n[00:30.00]c");
    assert_eq!(l.active_index(0), None);
    assert_eq!(l.active_index(9_999), None);
    assert_eq!(l.active_index(10_000), Some(0));
    assert_eq!(l.active_index(19_999), Some(0));
    assert_eq!(l.active_index(20_000), Some(1));
    assert_eq!(l.active_index(29_999), Some(1));
    assert_eq!(l.active_index(30_000), Some(2));
    assert_eq!(l.active_index(u64::MAX), Some(2));
}

#[test]
fn active_index_offset_positive_shows_earlier() {
    let l = parse_ok("[offset:+500]\n[00:10.00]a\n[00:20.00]b");
    assert_eq!(l.offset_ms, 500);
    assert_eq!(l.active_index(9_499), None);
    assert_eq!(l.active_index(9_500), Some(0));
    assert_eq!(l.active_index(19_500), Some(1));
}

#[test]
fn active_index_offset_negative_shows_later() {
    let l = parse_ok("[offset:-500]\n[00:10.00]a");
    assert_eq!(l.offset_ms, -500);
    assert_eq!(l.active_index(10_499), None);
    assert_eq!(l.active_index(10_500), Some(0));
}

#[test]
fn active_index_offset_can_push_first_line_before_zero() {
    let l = parse_ok("[offset:+20000]\n[00:10.00]a");
    assert_eq!(l.active_index(0), Some(0));
}

#[test]
fn window_clamps_and_behaves_before_first_line() {
    let l = parse_ok("[00:10.00]a\n[00:20.00]b\n[00:30.00]c\n[00:40.00]d");
    assert_eq!(l.window(20_000, 1, 1), &l.lines[0..3]);
    assert_eq!(l.window(20_000, 10, 1), &l.lines[0..3]);
    assert_eq!(l.window(20_000, 0, 10), &l.lines[1..4]);
    assert_eq!(l.window(0, 2, 2), &l.lines[0..2]);
    assert_eq!(l.window(0, 0, 0), &l.lines[0..0]);
    assert_eq!(l.window(40_000, 1, 5), &l.lines[2..4]);
}

#[test]
fn stable_sort_preserves_equal_time_input_order() {
    let l = parse_ok("[00:10.00]second\n[00:05.00]first\n[00:10.00]third");
    assert_eq!(
        l.lines,
        vec![
            line(5_000, "first"),
            line(10_000, "second"),
            line(10_000, "third")
        ]
    );
}

#[test]
fn no_panic_on_pseudorandom_input() {
    let mut state: u64 = 0x243F_6A88_85A3_08D3;
    for _ in 0..500 {
        let n = (state % 512) as usize;
        let mut s = String::with_capacity(n + 1);
        for _ in 0..n {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r = (state >> 33) % 100;
            if r < 2 {
                s.push('\n');
            } else if r < 4 {
                s.push('\u{FEFF}');
            } else if r < 6 {
                s.push('é');
            } else {
                s.push((b'!' + (r % 94) as u8) as char);
            }
        }
        let _ = parse(&s);
    }
}
