use ics_core::{
    agenda, agenda_checked, agenda_checked_with_warnings, parse_calendar, DateTimeKind, Event,
    IcsError, Instance, NaiveDateTime, Warning,
};

const JAN1_2026: i64 = 1_767_225_600; // 2026-01-01T00:00:00Z
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;

fn cal(body: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//x//EN\r\n{body}END:VCALENDAR\r\n")
}

fn evt(uid: &str, props: &str) -> String {
    format!("BEGIN:VEVENT\r\nUID:{uid}\r\n{props}END:VEVENT\r\n")
}

fn parse_full(text: &str) -> (Vec<Event>, Vec<Warning>) {
    match parse_calendar(text) {
        Ok(x) => x,
        Err(e) => panic!("parse failed: {e}"),
    }
}

fn parse_no_warnings(text: &str) -> Vec<Event> {
    let (ev, w) = parse_full(text);
    if !w.is_empty() {
        panic!("unexpected warnings: {w:?}");
    }
    ev
}

fn no_tz(_: &str, _: NaiveDateTime) -> Option<i32> {
    None
}

fn ag(events: &[Event], ws: i64, we: i64, off: i32) -> Vec<Instance> {
    agenda(events, ws, we, off, &no_tz)
}

#[test]
fn unfolding_space_tab_crlf_lf() {
    let c = cal(&evt(
        "u1",
        "SUMMARY:Hello\r\n World\r\nDTSTART:20260101T100000Z\r\n",
    ));
    let ev = parse_no_warnings(&c);
    assert_eq!(ev[0].summary, "HelloWorld");

    let c2 = cal(&evt(
        "u2",
        "SUMMARY:Hello\r\n\tWorld\r\nDTSTART:20260101T100000Z\r\n",
    ));
    let ev2 = parse_no_warnings(&c2);
    assert_eq!(ev2[0].summary, "HelloWorld");

    // bare LF only (no CRLF)
    let lf = "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:u3\nSUMMARY:a\n b\nDTSTART:20260101T100000Z\nEND:VEVENT\nEND:VCALENDAR\n";
    let ev3 = parse_no_warnings(lf);
    assert_eq!(ev3[0].summary, "ab");
}

#[test]
fn text_escapes() {
    let c = cal(&evt(
        "u1",
        "SUMMARY:a\\\\b\\;c\\,d\\ne\\Nf\r\nDTSTART:20260101T100000Z\r\n",
    ));
    let ev = parse_no_warnings(&c);
    assert_eq!(ev[0].summary, "a\\b;c,d\ne\nf");
}

#[test]
fn case_insensitive_names() {
    let txt = "begin:vcalendar\nversion:2.0\nbegin:vevent\nuid:x\nsummary:S\ndtstart:20260101T100000Z\nend:vevent\nend:vcalendar\n";
    let ev = parse_no_warnings(txt);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].summary, "S");
}

#[test]
fn quoted_params_with_colon_semicolon() {
    let c = cal(&evt(
        "u1",
        "DTSTART;TZID=\"America/New_York\";X-P=\"a:b;c\":20261030T140000\r\n",
    ));
    let ev = parse_no_warnings(&c);
    assert_eq!(
        ev[0].dtstart.naive,
        NaiveDateTime::new(2026, 10, 30, 14, 0, 0).unwrap()
    );
    match &ev[0].dtstart.kind {
        DateTimeKind::Zoned { tzid } => assert_eq!(tzid, "America/New_York"),
        other => panic!("expected zoned, got {other:?}"),
    }
}

#[test]
fn all_four_dtstart_forms() {
    let c = cal(&(evt("a", "DTSTART;VALUE=DATE:20261030\r\n")
        + &evt("b", "DTSTART:20261030T140000Z\r\n")
        + &evt("c", "DTSTART:20261030T140000\r\n")
        + &evt("d", "DTSTART;TZID=America/New_York:20261030T140000\r\n")));
    let ev = parse_no_warnings(&c);
    assert_eq!(ev.len(), 4);
    assert_eq!(ev[0].dtstart.kind, DateTimeKind::Date);
    assert_eq!(ev[1].dtstart.kind, DateTimeKind::Utc);
    assert_eq!(ev[2].dtstart.kind, DateTimeKind::Floating);
    assert_eq!(
        ev[3].dtstart.kind,
        DateTimeKind::Zoned {
            tzid: "America/New_York".into()
        }
    );
    let n = NaiveDateTime::new(2026, 10, 30, 14, 0, 0).unwrap();
    assert_eq!(
        ev[0].dtstart.naive,
        NaiveDateTime::date(2026, 10, 30).unwrap()
    );
    assert_eq!(ev[1].dtstart.naive, n);
    assert_eq!(ev[2].dtstart.naive, n);
    assert_eq!(ev[3].dtstart.naive, n);
}

#[test]
fn unknown_tzid_warns_and_treats_as_floating() {
    let c = cal(&evt(
        "u1",
        "DTSTART;TZID=Foo/Bar:20261030T140000\r\nDURATION:PT1H\r\n",
    ));
    let (ev, _) = parse_full(&c);
    let inst0 = ag(&ev, 0, 4 * DAY, 0);
    let (inst, warnings) = agenda_checked_with_warnings(&ev, 0, 4 * DAY, 0, &no_tz).unwrap();
    assert_eq!(inst, inst0);
    assert!(warnings.iter().any(|w| w.message.contains("Foo/Bar")));
}

#[test]
fn duration_forms() {
    let c = cal(&format!(
        "{}{}{}",
        evt("a", "DTSTART:20260101T100000Z\r\nDURATION:P1D\r\n"),
        evt("b", "DTSTART:20260101T100000Z\r\nDURATION:PT1H30M\r\n"),
        evt("c", "DTSTART:20260101T100000Z\r\nDURATION:P2W\r\n"),
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 3);
    assert_eq!(inst[0].end_s - inst[0].start_s, DAY);
    assert_eq!(inst[1].end_s - inst[1].start_s, 5400);
    assert_eq!(inst[2].end_s - inst[2].start_s, 14 * DAY);
}

#[test]
fn negative_and_invalid_duration_skips_event() {
    let c = cal(&format!(
        "{}{}",
        evt("a", "DTSTART:20260101T100000Z\r\nDURATION:-PT1H\r\n"),
        evt("b", "DTSTART:20260101T100000Z\r\nDURATION:P\r\n"),
    ));
    let (ev, warnings) = parse_full(&c);
    assert!(ev.is_empty());
    assert_eq!(warnings.len(), 2);
}

#[test]
fn missing_dtend_defaults() {
    let c = cal(&format!(
        "{}{}",
        evt("a", "DTSTART;VALUE=DATE:20260102\r\n"),
        evt("b", "DTSTART:20260101T100000Z\r\n"),
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 2);
    // timed event starts Jan 1 10:00Z, all-day event starts Jan 2
    assert!(!inst[0].all_day);
    assert_eq!(inst[0].end_s - inst[0].start_s, 0);
    assert!(inst[1].all_day);
    assert_eq!(inst[1].end_s - inst[1].start_s, DAY);
}

#[test]
fn dtend_before_dtstart_skipped() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260102T100000Z\r\nDTEND:20260101T100000Z\r\n",
    ));
    let (ev, warnings) = parse_full(&c);
    assert!(ev.is_empty());
    assert!(warnings
        .iter()
        .any(|w| w.message.contains("DTEND before DTSTART")));
}

#[test]
fn daily_interval2_count5() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;INTERVAL=2;COUNT=5\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 5);
    let base = JAN1_2026 + 10 * HOUR;
    for (i, ins) in inst.iter().enumerate() {
        assert_eq!(ins.start_s, base + (i as i64) * 2 * DAY);
    }
}

#[test]
fn weekly_byday_until() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260105T100000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20260120T000000Z\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    // Jan 5 (Mo), 7 (We), 9 (Fr), 12 (Mo), 14 (We), 16 (Fr), 19 (Mo)
    assert_eq!(inst.len(), 7);
    assert_eq!(inst[0].start_s, JAN1_2026 + 4 * DAY + 10 * HOUR);
    assert_eq!(inst[5].start_s, JAN1_2026 + 15 * DAY + 10 * HOUR);
    assert_eq!(inst[6].start_s, JAN1_2026 + 18 * DAY + 10 * HOUR);
}

#[test]
fn weekly_default_byday_is_dtstart_weekday() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260105T100000Z\r\nRRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=3\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 3);
    assert_eq!(inst[1].start_s, JAN1_2026 + 18 * DAY + 10 * HOUR);
    assert_eq!(inst[2].start_s, JAN1_2026 + 32 * DAY + 10 * HOUR);
}

#[test]
fn monthly_31st_skips_short_months() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260131T100000Z\r\nRRULE:FREQ=MONTHLY;COUNT=7\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 7);
    // Jan, Mar, May, Jul, Aug, Oct, Dec 2026 (Feb/Apr/Jun/Sep/Nov lack day 31)
    let offsets = [30i64, 89, 150, 211, 242, 303, 364];
    for (ins, off) in inst.iter().zip(offsets.iter()) {
        assert_eq!(ins.start_s, JAN1_2026 + off * DAY + 10 * HOUR);
    }
}

#[test]
fn yearly_feb29_skips_non_leap_years() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20240229T100000Z\r\nRRULE:FREQ=YEARLY;COUNT=3\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let b2024 = 1_704_067_200; // 2024-01-01T00:00Z

    // Leap 2024: the Feb 29 instance appears.
    let inst = ag(&ev, b2024 + 58 * DAY, b2024 + 61 * DAY, 0);
    assert_eq!(inst.len(), 1);
    assert_eq!(inst[0].start_s, b2024 + 59 * DAY + 10 * HOUR);

    // Non-leap 2025 and 2026: nothing around Feb 28 / Mar 1.
    let b2025 = b2024 + 366 * DAY;
    assert!(ag(&ev, b2025 + 58 * DAY, b2025 + 61 * DAY, 0).is_empty());
    let b2026 = b2025 + 365 * DAY;
    assert!(ag(&ev, b2026 + 58 * DAY, b2026 + 61 * DAY, 0).is_empty());

    // Leap 2028: next Feb 29 instance appears.
    let b2028 = b2024 + 1461 * DAY;
    let inst2 = ag(&ev, b2028 + 58 * DAY, b2028 + 61 * DAY, 0);
    assert_eq!(inst2.len(), 1);
    assert_eq!(inst2[0].start_s, b2028 + 59 * DAY + 10 * HOUR);
}

#[test]
fn count_plus_until_invalid() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=3;UNTIL=20260201T000000Z\r\n",
    ));
    let (ev, warnings) = parse_full(&c);
    assert_eq!(ev.len(), 1);
    assert!(ev[0].rrule.is_none());
    assert!(warnings
        .iter()
        .any(|w| w.message.contains("COUNT and UNTIL")));
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 1);
}

#[test]
fn unsupported_rrule_parts() {
    let c1 = cal(&evt(
        "u1",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=MONTHLY;BYMONTHDAY=15\r\n",
    ));
    let (ev1, w1) = parse_full(&c1);
    assert!(ev1[0].rrule.is_none());
    assert!(w1
        .iter()
        .any(|w| w.message.contains("unsupported RRULE part")));

    let c2 = cal(&evt(
        "u2",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=2MO\r\n",
    ));
    let (ev2, w2) = parse_full(&c2);
    assert!(ev2[0].rrule.is_none());
    assert!(w2.iter().any(|w| w.message.contains("BYDAY")));

    let c3 = cal(&evt(
        "u3",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=MONTHLY;BYDAY=MO\r\n",
    ));
    let (ev3, w3) = parse_full(&c3);
    assert!(ev3[0].rrule.is_none());
    assert!(w3
        .iter()
        .any(|w| w.message.contains("only supported for WEEKLY")));
}

#[test]
fn exdate_comma_list_and_repeat() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=5\r\nEXDATE:20260102T100000Z,20260103T100000Z\r\nEXDATE:20260104T100000Z\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 2);
    assert_eq!(inst[0].start_s, JAN1_2026 + 10 * HOUR);
    assert_eq!(inst[1].start_s, JAN1_2026 + 4 * DAY + 10 * HOUR);
}

#[test]
fn exdate_still_counts_toward_count() {
    let c = cal(&evt(
        "u1",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=5\r\nEXDATE:20260102T100000Z,20260103T100000Z\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    // removed instances still consumed COUNT slots -> only 3 remain
    assert_eq!(inst.len(), 3);
    assert_eq!(inst[0].start_s, JAN1_2026 + 10 * HOUR);
    assert_eq!(inst[1].start_s, JAN1_2026 + 3 * DAY + 10 * HOUR);
    assert_eq!(inst[2].start_s, JAN1_2026 + 4 * DAY + 10 * HOUR);
}

#[test]
fn recurrence_id_override_moves_one_instance() {
    let master = evt(
        "m",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=5\r\n",
    );
    let ov = evt(
        "m",
        "RECURRENCE-ID:20260103T100000Z\r\nDTSTART:20260104T090000Z\r\nSUMMARY:Override\r\n",
    );
    let c = cal(&format!("{master}{ov}"));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    // COUNT=5 -> Jan 1..5; the override replaces Jan 3 with a Feb 4 09:00
    // instance, so all five slots still appear.
    assert_eq!(inst.len(), 5);
    assert_eq!(inst[0].start_s, JAN1_2026 + 10 * HOUR);
    assert_eq!(inst[1].start_s, JAN1_2026 + DAY + 10 * HOUR);
    assert_eq!(inst[2].start_s, JAN1_2026 + 3 * DAY + 9 * HOUR);
    assert_eq!(inst[2].summary, "Override");
    assert_eq!(inst[2].recurrence_id, Some(JAN1_2026 + 2 * DAY + 10 * HOUR));
    assert_eq!(inst[3].start_s, JAN1_2026 + 3 * DAY + 10 * HOUR);
    assert_eq!(inst[4].start_s, JAN1_2026 + 4 * DAY + 10 * HOUR);
}

#[test]
fn cancelled_events_and_overrides_skipped() {
    let master = evt(
        "m",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=3\r\nSTATUS:CANCELLED\r\n",
    );
    let c = cal(&master);
    let ev = parse_no_warnings(&c);
    assert!(ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0).is_empty());

    let master2 = evt(
        "m",
        "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=3\r\n",
    );
    let ov = evt(
        "m",
        "RECURRENCE-ID:20260102T100000Z\r\nDTSTART:20260102T090000Z\r\nSTATUS:CANCELLED\r\n",
    );
    let c2 = cal(&format!("{master2}{ov}"));
    let ev2 = parse_no_warnings(&c2);
    let inst2 = ag(&ev2, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst2.len(), 2);
    assert_eq!(inst2[0].start_s, JAN1_2026 + 10 * HOUR);
    assert_eq!(inst2[1].start_s, JAN1_2026 + 2 * DAY + 10 * HOUR);
}

#[test]
fn window_overlap_edges() {
    let c = cal(&format!(
        "{}{}{}{}",
        evt("in", "DTSTART:20260101T100000Z\r\nDURATION:PT1H\r\n"),
        evt(
            "ends_at_start",
            "DTSTART:20251231T220000Z\r\nDURATION:PT2H\r\n"
        ),
        evt("starts_at_end", "DTSTART:20260103T000000Z\r\n"),
        evt("allday", "DTSTART;VALUE=DATE:20260102\r\n"),
    ));
    let ev = parse_no_warnings(&c);
    let ws = JAN1_2026;
    let we = ws + 2 * DAY;
    let inst = ag(&ev, ws, we, 0);
    assert_eq!(inst.len(), 2);
    assert_eq!(inst[0].start_s, ws + 10 * HOUR);
    assert!(inst[1].all_day);
    assert_eq!(inst[1].start_s, ws + DAY);
}

#[test]
fn sort_order_ties() {
    let c = cal(&format!(
        "{}{}{}",
        evt("A", "DTSTART;VALUE=DATE:20260101\r\nSUMMARY:b\r\n"),
        evt("B", "DTSTART:20260101T000000Z\r\nSUMMARY:a\r\n"),
        evt("C", "DTSTART:20260101T000000Z\r\nSUMMARY:b\r\n"),
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026 - DAY, JAN1_2026 + 2 * DAY, 0);
    assert_eq!(inst.len(), 3);
    assert_eq!(inst[0].uid, "A"); // all-day first at equal start
    assert_eq!(inst[1].uid, "B"); // then summary ascending
    assert_eq!(inst[2].uid, "C");
}

#[test]
fn cap_2000_instances() {
    let mut body = String::new();
    for k in 0..8 {
        body.push_str(&evt(
            &format!("u{k}"),
            "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=1000\r\n",
        ));
    }
    let c = cal(&body);
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    assert_eq!(inst.len(), 2000);
    assert!(inst.windows(2).all(|w| {
        let a = &w[0];
        let b = &w[1];
        (a.start_s, !a.all_day, a.summary.as_str(), a.uid.as_str())
            <= (b.start_s, !b.all_day, b.summary.as_str(), b.uid.as_str())
    }));
}

#[test]
fn candidate_guard_10000() {
    // Far-past DTSTART: >10k candidates would be generated before reaching
    // the window, so the guard stops the rule and nothing overlaps.
    let c = cal(&evt(
        "u1",
        "DTSTART:19980101T000000Z\r\nRRULE:FREQ=DAILY\r\n",
    ));
    let ev = parse_no_warnings(&c);
    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 200 * DAY, 0);
    assert_eq!(inst.len(), 0);

    // Control: DTSTART one year before the window is reached within the guard.
    let c2 = cal(&evt(
        "u2",
        "DTSTART:20250101T000000Z\r\nRRULE:FREQ=DAILY\r\n",
    ));
    let ev2 = parse_no_warnings(&c2);
    let inst2 = ag(&ev2, JAN1_2026, JAN1_2026 + 200 * DAY, 0);
    assert!(!inst2.is_empty());
    assert!(inst2.len() <= 2000);
}

#[test]
fn window_longer_than_400_days_is_error() {
    let ev: Vec<Event> = vec![];
    assert!(agenda_checked(&ev, 0, 400 * DAY, 0, &no_tz).is_ok());
    match agenda_checked(&ev, 0, 400 * DAY + 1, 0, &no_tz) {
        Err(IcsError::WindowTooLong) => {}
        other => panic!("expected WindowTooLong, got {other:?}"),
    }
}

#[test]
fn local_offset_minus4_affects_all_day_and_floating() {
    let c = cal(&format!(
        "{}{}",
        evt("ad", "DTSTART;VALUE=DATE:20260101\r\n"),
        evt("fl", "DTSTART:20260101T120000\r\n"),
    ));
    let ev = parse_no_warnings(&c);
    let inst0 = ag(&ev, JAN1_2026, JAN1_2026 + 3 * DAY, 0);
    let instm4 = ag(&ev, JAN1_2026, JAN1_2026 + 3 * DAY, -4 * 3600);
    assert_eq!(instm4[0].start_s, inst0[0].start_s + 4 * HOUR);
    assert_eq!(instm4[1].start_s, inst0[1].start_s + 4 * HOUR);
    assert_eq!(inst0[1].start_s, JAN1_2026 + 12 * HOUR);
}

#[test]
fn serde_camelcase_roundtrip() {
    let c = cal(&format!(
        "{}{}",
        evt(
            "m",
            "DTSTART:20260101T100000Z\r\nRRULE:FREQ=DAILY;COUNT=5\r\nEXDATE:20260102T100000Z\r\nLOCATION:Room 1\r\nDURATION:PT30M\r\n",
        ),
        evt(
            "m",
            "RECURRENCE-ID:20260103T100000Z\r\nDTSTART:20260104T090000Z\r\n",
        ),
    ));
    let (ev, _) = parse_full(&c);
    let j = serde_json::to_string(&ev).unwrap();
    assert!(j.contains("\"durationS\""));
    assert!(j.contains("\"recurrenceId\""));
    let back: Vec<Event> = serde_json::from_str(&j).unwrap();
    assert_eq!(back, ev);

    let inst = ag(&ev, JAN1_2026, JAN1_2026 + 400 * DAY, 0);
    let ji = serde_json::to_string(&inst).unwrap();
    assert!(ji.contains("\"startS\""));
    assert!(ji.contains("\"allDay\""));
    assert!(ji.contains("\"recurrenceId\""));
    let back_i: Vec<Instance> = serde_json::from_str(&ji).unwrap();
    assert_eq!(back_i, inst);
}

#[test]
fn broken_event_skipped_rest_kept() {
    let c = cal(&format!(
        "{}{}",
        evt("good", "DTSTART:20260101T100000Z\r\n"),
        evt("bad", "SUMMARY:no dtstart\r\n"),
    ));
    let (ev, warnings) = parse_full(&c);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].uid, "good");
    assert!(warnings.iter().any(|w| w.message.contains("DTSTART")));
}

#[test]
fn top_level_errors() {
    assert_eq!(
        parse_calendar("hello world").unwrap_err(),
        IcsError::MissingCalendar
    );
    let nul = "BEGIN:VCALENDAR\r\n\x00END:VCALENDAR\r\n";
    assert_eq!(parse_calendar(nul).unwrap_err(), IcsError::ContainsNul);
    let big = "A".repeat(8 * 1024 * 1024 + 1);
    assert_eq!(parse_calendar(&big).unwrap_err(), IcsError::TooLarge);
}

#[test]
fn vtodo_valarm_vtimezone_ignored() {
    let c = "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nUID:x\r\nDTSTART:20260101T100000Z\r\nEND:VTODO\r\nBEGIN:VTIMEZONE\r\nTZID:America/New_York\r\nBEGIN:STANDARD\r\nDTSTART:19701101T020000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:real\r\nDTSTART:20260101T100000Z\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let ev = parse_no_warnings(c);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].uid, "real");
    assert_eq!(ev[0].description, None);
}
