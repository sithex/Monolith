//! File-copy progress decoder and notch cadence (`integrations::jobs::core`): pure, no bus.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use hermes_island_lib::integrations::jobs::core::*;
use std::{
    collections::BTreeMap,
    num::NonZeroU32,
    time::{Duration, Instant},
};
use zbus::{Message, OwnedVariant, Variant as Value};

const OWNER: &str = ":1.16";
const CLIENT: &str = ":1.40";
const VIEW: &str = "/org/kde/notificationmanager/jobs/JobView_1";

fn props(entries: &[(&str, Value<'static>)]) -> BTreeMap<String, OwnedVariant> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), OwnedVariant::try_from(v.clone()).unwrap()))
        .collect()
}

fn request(client: &str, serial: u32, hints: BTreeMap<String, OwnedVariant>) -> Message {
    Message::method_call(SERVER_PATH, "requestView")
        .interface(SERVER_V2)
        .destination(SERVER_NAME)
        .sender(client)
        .serial(NonZeroU32::new(serial).unwrap())
        .build(&("org.kde.dolphin", 3i32, hints))
        .unwrap()
}

fn reply(call: &Message, from: &str, path: &str) -> Message {
    Message::method_return(&call.header())
        .sender(from)
        .build(&zbus::ObjectPath::try_from(path).unwrap())
        .unwrap()
}

fn update(client: &str, path: &str, properties: BTreeMap<String, OwnedVariant>) -> Message {
    Message::method_call(path, "update")
        .interface(VIEW_V3)
        .destination(OWNER)
        .sender(client)
        .serial(NonZeroU32::new(99).unwrap())
        .build(&(properties,))
        .unwrap()
}

fn terminate(client: &str, path: &str) -> Message {
    Message::method_call(path, "terminate")
        .interface(VIEW_V3)
        .destination(OWNER)
        .sender(client)
        .serial(NonZeroU32::new(100).unwrap())
        .build(&(0u32, "", BTreeMap::<String, OwnedVariant>::new()))
        .unwrap()
}

/// A tracker holding one job started by CLIENT on VIEW (as Dolphin's first message carries).
fn started(now: Instant) -> Tracker {
    let mut t = Tracker::default();
    let call = request(
        CLIENT,
        5,
        props(&[
            ("title", Value::from("Copying")),
            ("percent", Value::from(5u32)),
            ("totalBytes", Value::from(25_165_824u64)),
            ("processedBytes", Value::from(1_392_640u64)),
            (
                "destUrl",
                Value::from("file:///data/user/Videos/big%20file.mkv"),
            ),
        ]),
    );
    assert!(!t.observe(&call, OWNER, now), "no row before the reply");
    assert!(t.observe(&reply(&call, OWNER, VIEW), OWNER, now));
    t
}

#[test]
fn request_reply_update_and_terminate_make_one_row_that_ends() {
    let now = Instant::now();
    let mut t = started(now);
    let rows = t.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].app, "org.kde.dolphin");
    assert_eq!(rows[0].title, "Copying");
    assert_eq!(rows[0].percent, 5.);
    assert_eq!(rows[0].total_bytes, 25_165_824.);
    assert_eq!(rows[0].destination, "/data/user/Videos/big file.mkv");
    assert_eq!(t.aggregate(), Some(5));
    // Only changed keys arrive in an update; the rest stays.
    assert!(t.observe(
        &update(CLIENT, VIEW, props(&[("percent", Value::from(47u32))])),
        OWNER,
        now
    ));
    assert_eq!(t.aggregate(), Some(47));
    assert_eq!(t.rows()[0].total_bytes, 25_165_824.);
    // The same value again changes nothing (no publish, no notch work).
    assert!(!t.observe(
        &update(CLIENT, VIEW, props(&[("percent", Value::from(47u32))])),
        OWNER,
        now
    ));
    assert!(t.observe(&terminate(CLIENT, VIEW), OWNER, now));
    assert!(t.is_empty());
    assert_eq!(t.aggregate(), None);
}

#[test]
fn only_the_requesting_client_and_the_real_owner_count() {
    let now = Instant::now();
    let mut t = Tracker::default();
    let call = request(CLIENT, 5, BTreeMap::new());
    t.observe(&call, OWNER, now);
    // A reply from someone other than the server owner is not a job view.
    assert!(!t.observe(&reply(&call, ":1.66", VIEW), OWNER, now));
    assert!(t.is_empty());
    assert!(t.observe(&reply(&call, OWNER, VIEW), OWNER, now));
    // Another client cannot move or end CLIENT's job.
    assert!(!t.observe(
        &update(":1.66", VIEW, props(&[("percent", Value::from(90u32))])),
        OWNER,
        now
    ));
    assert!(!t.observe(&terminate(":1.66", VIEW), OWNER, now));
    assert_eq!(t.rows().len(), 1);
    assert_eq!(t.rows()[0].percent, -1.);
}

#[test]
fn a_client_leaving_the_bus_ends_its_jobs_and_silent_jobs_expire() {
    let now = Instant::now();
    let mut t = started(now);
    assert!(!t.client_gone(":1.99"));
    assert!(t.client_gone(CLIENT));
    assert!(t.is_empty());
    let mut t = started(now);
    assert!(!t.expire(now + JOB_TTL - Duration::from_secs(1)));
    assert_eq!(t.next_expiry(), Some(now + JOB_TTL));
    assert!(t.expire(now + JOB_TTL));
    assert!(t.is_empty());
}

#[test]
fn v2_setters_map_onto_the_same_row() {
    let now = Instant::now();
    let mut t = Tracker::default();
    let call = Message::method_call(SERVER_PATH, "requestView")
        .interface(SERVER_V1)
        .destination(SERVER_NAME)
        .sender(CLIENT)
        .serial(NonZeroU32::new(9).unwrap())
        .build(&("kio", "folder", 0i32))
        .unwrap();
    t.observe(&call, OWNER, now);
    assert!(t.observe(&reply(&call, OWNER, VIEW), OWNER, now));
    let v2 = |member: &'static str| {
        Message::method_call(VIEW, member)
            .interface(VIEW_V2)
            .sender(CLIENT)
            .serial(NonZeroU32::new(11).unwrap())
    };
    assert!(t.observe(
        &v2("setTotalAmount").build(&(400u64, "bytes")).unwrap(),
        OWNER,
        now
    ));
    assert!(t.observe(
        &v2("setProcessedAmount").build(&(100u64, "bytes")).unwrap(),
        OWNER,
        now
    ));
    assert_eq!(t.rows()[0].percent, 25., "bytes give a percent");
    assert!(t.observe(&v2("setPercent").build(&(60u32,)).unwrap(), OWNER, now));
    assert_eq!(t.aggregate(), Some(60), "a reported percent wins");
    assert!(t.observe(&v2("terminate").build(&("",)).unwrap(), OWNER, now));
    assert!(t.is_empty());
}

#[test]
fn caps_bound_jobs_and_text() {
    let now = Instant::now();
    let mut t = Tracker::default();
    for n in 0..(MAX_JOBS as u32 + 2) {
        let call = request(
            CLIENT,
            n + 1,
            props(&[("title", Value::from("x".repeat(MAX_TEXT + 50)))]),
        );
        t.observe(&call, OWNER, now);
        t.observe(&reply(&call, OWNER, &format!("/jobs/J{n}")), OWNER, now);
    }
    assert_eq!(t.rows().len(), MAX_JOBS);
    assert_eq!(t.overflow, 2);
    assert!(t.rows().iter().all(|r| r.title.len() == MAX_TEXT));
}

#[test]
fn control_characters_never_reach_the_card() {
    let now = Instant::now();
    let mut t = Tracker::default();
    let call = request(
        CLIENT,
        1,
        props(&[
            ("title", Value::from("Copying\nnow")),
            ("destUrl", Value::from("file:///tmp/a%0Ab")),
        ]),
    );
    t.observe(&call, OWNER, now);
    t.observe(&reply(&call, OWNER, VIEW), OWNER, now);
    let row = &t.rows()[0];
    assert_eq!(row.title, "Copying now");
    assert_eq!(row.destination, "/tmp/a b");
    // The real path (for Show in folder) keeps its newline.
    assert_eq!(
        local_path(t.dest_url(VIEW).unwrap()).unwrap(),
        std::path::PathBuf::from("/tmp/a\nb")
    );
}

#[test]
fn several_jobs_aggregate_by_bytes() {
    let now = Instant::now();
    let mut t = Tracker::default();
    for (n, (done, total)) in [(100u64, 100u64), (0, 300)].into_iter().enumerate() {
        let call = request(
            CLIENT,
            n as u32 + 1,
            props(&[
                ("processedBytes", Value::from(done)),
                ("totalBytes", Value::from(total)),
            ]),
        );
        t.observe(&call, OWNER, now);
        t.observe(&reply(&call, OWNER, &format!("/jobs/J{n}")), OWNER, now);
    }
    // 100 of 400 bytes, not the average of 100 % and 0 %.
    assert_eq!(t.aggregate(), Some(25));
}

#[test]
fn destination_urls_decode_to_local_paths_only() {
    assert_eq!(
        local_path("file:///data/user/a%20b").unwrap(),
        std::path::PathBuf::from("/data/user/a b")
    );
    assert!(local_path("file://localhost/tmp").is_some());
    assert!(local_path("smb://server/share").is_none());
    assert!(local_path("file:///tmp/%00x").is_none());
    assert!(local_path("file:///tmp/../etc").is_none());
    assert!(local_path("file:///tmp/%zz").is_none());
}

#[test]
fn notch_steps_are_whole_tens_at_most_every_three_seconds_with_start_and_end_at_once() {
    let t0 = Instant::now();
    let mut s = NotchSteps::default();
    assert_eq!(s.decide(None, t0), Step::Hold, "idle stays silent");
    assert_eq!(s.decide(Some(7), t0), Step::Send(0), "start at once");
    s.sent(0, t0);
    assert_eq!(s.decide(Some(9), t0), Step::Hold, "same step");
    let one = t0 + Duration::from_secs(1);
    assert_eq!(
        s.decide(Some(34), one),
        Step::Later(t0 + MIN_GAP),
        "a new step waits out the gap"
    );
    let three = t0 + MIN_GAP;
    assert_eq!(s.decide(Some(34), three), Step::Send(30));
    s.sent(30, three);
    assert_eq!(
        s.decide(None, three + Duration::from_millis(10)),
        Step::Send(IDLE),
        "end at once"
    );
    assert_eq!(wire(Some(100)), 100);
    assert_eq!(wire(Some(255)), 100);
    assert_eq!(wire(None), -1);
}

#[test]
fn a_one_minute_copy_sends_at_most_a_dozen_steps() {
    let t0 = Instant::now();
    let mut s = NotchSteps::default();
    let mut sent = 0;
    // Updates every 200 ms, percent rising linearly over 60 s.
    for tick in 0..=300u32 {
        let now = t0 + Duration::from_millis(u64::from(tick) * 200);
        let p = (tick * 100 / 300) as u8;
        if let Step::Send(v) = s.decide(Some(p), now) {
            s.sent(v, now);
            sent += 1;
        }
    }
    if let Step::Send(v) = s.decide(None, t0 + Duration::from_secs(61)) {
        s.sent(v, t0);
        sent += 1;
    }
    assert!(sent <= 12, "{sent} notch updates for one copy");
    assert!(sent >= 10);
}
