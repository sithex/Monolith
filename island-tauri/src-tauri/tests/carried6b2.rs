//! Phase 6b2 carried items: calendar userinfo rejection, downloads model-membership check and
//! calendar time zones (system tz database, fixed zone fixtures; the host zone is never implied).
#![allow(clippy::unwrap_used)]
use hermes_island_lib::{
    contract::apps::*,
    error::IslandError,
    integrations::{
        calendar::{CalendarConfig, CalendarService, TzConfig},
        downloads::{DownloadsService, Opener},
        private_file,
    },
};
use std::{
    ffi::OsString,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[allow(dead_code)]
#[path = "helpers/calendar_fixture.rs"]
mod calendar_fixture;
use calendar_fixture::CalendarFixture;

static SERIAL: AtomicU64 = AtomicU64::new(0);
const ZONEINFO: &str = "/usr/share/zoneinfo";
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hi-6b2-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}
fn calendar(dir: &Scratch, tz: TzConfig) -> Arc<CalendarService> {
    CalendarService::new(CalendarConfig {
        data: dir.0.clone(),
        loopback_test: true,
        timeout: Duration::from_secs(1),
        tz,
    })
    .unwrap()
}
fn zone(name: &str) -> TzConfig {
    TzConfig {
        local: Some(name.into()),
        dir: Some(ZONEINFO.into()),
    }
}
fn event(uid: &str, start_line: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{uid}\r\n{start_line}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
}
// 2026-10-24T00:00Z .. 2026-11-03T00:00Z
const WINDOW: (i64, i64) = (1_792_800_000, 1_793_664_000);
fn starts(s: &CalendarService, now: f64) -> Vec<(String, f64, f64)> {
    s.current(now, WINDOW.0, WINDOW.1, 0)
        .unwrap()
        .agenda
        .into_iter()
        .map(|r| (r.uid, r.start_s, r.end_s))
        .collect()
}
fn start_of(s: &CalendarService, uid: &str) -> f64 {
    starts(s, 1000.)
        .into_iter()
        .find(|r| r.0 == uid)
        .unwrap_or_else(|| panic!("{uid} missing from agenda"))
        .1
}

#[tokio::test]
async fn calendar_secret_with_userinfo_is_rejected_and_nothing_is_fetched() {
    let server = CalendarFixture::start(event("remote", "DTSTART:20261031T120000Z"));
    for (n, userinfo) in ["user@", "user:pw@", ":pw@"].into_iter().enumerate() {
        let dir = Scratch::new();
        let url = server
            .url()
            .replacen("http://", &format!("http://{userinfo}"), 1);
        private_file::write(&dir.0.join("calendar-secret-url"), url.as_bytes(), 8192).unwrap();
        let s = calendar(&dir, zone("UTC"));
        assert!(s.refresh(1000.).await.is_err(), "case {n}: {userinfo}");
        let v = s.current(2000., WINDOW.0, WINDOW.1, 0).unwrap();
        assert!(!v.configured, "case {n}: userinfo URL must not configure");
        assert!(v.agenda.is_empty());
        assert_eq!(server.hits(), 0, "case {n}: a request was sent");
    }
    // Exercise production HTTPS validation too, against a loopback-only destination. Even if
    // validation regresses, this cannot contact any real calendar or external host.
    for userinfo in ["user@", "user:pw@"] {
        let dir = Scratch::new();
        let url = server
            .url()
            .replacen("http://", &format!("https://{userinfo}"), 1);
        private_file::write(&dir.0.join("calendar-secret-url"), url.as_bytes(), 8192).unwrap();
        let s = CalendarService::new(CalendarConfig {
            data: dir.0.clone(),
            loopback_test: false,
            timeout: Duration::from_millis(200),
            tz: zone("UTC"),
        })
        .unwrap();
        assert!(s.refresh(1000.).await.is_err());
        assert!(!s.current(2000., WINDOW.0, WINDOW.1, 0).unwrap().configured);
        assert_eq!(server.hits(), 0);
    }
    // Control: the same URL without userinfo does fetch (the fixture really counts requests).
    let dir = Scratch::new();
    private_file::write(
        &dir.0.join("calendar-secret-url"),
        server.url().as_bytes(),
        8192,
    )
    .unwrap();
    let s = calendar(&dir, zone("UTC"));
    s.refresh(1000.).await.unwrap();
    assert!(server.hits() >= 1);
}

#[tokio::test]
async fn calendar_recurrences_recompute_local_midnights_and_end_offsets_per_occurrence() {
    let dir = Scratch::new();
    let s = calendar(&dir, zone("America/New_York"));
    for (uid, line) in [
        ("day", "DTSTART;VALUE=DATE:20261031\r\nRRULE:FREQ=DAILY;COUNT=3"),
        ("explicit", "DTSTART;VALUE=DATE:20261031\r\nDTEND;VALUE=DATE:20261101\r\nRRULE:FREQ=DAILY;COUNT=3"),
        ("night", "DTSTART;TZID=America/New_York:20261031T003000\r\nDTEND;TZID=America/New_York:20261031T023000\r\nRRULE:FREQ=DAILY;COUNT=3"),
    ] {
        s.apply(CalendarOp::Create { ics: event(uid, line) }).unwrap();
    }
    let rows = starts(&s, 1000.);
    for uid in ["day", "explicit", "night"] {
        let durations: Vec<_> = rows
            .iter()
            .filter(|r| r.0 == uid)
            .map(|r| r.2 - r.1)
            .collect();
        assert_eq!(
            durations,
            if uid == "night" {
                vec![7200., 10800., 7200.]
            } else {
                vec![86400., 90000., 86400.]
            },
            "{uid}"
        );
    }
}

#[tokio::test]
async fn calendar_dst_gap_advances_and_fold_chooses_the_first_instant() {
    let dir = Scratch::new();
    let s = calendar(&dir, zone("America/New_York"));
    s.apply(CalendarOp::Create {
        ics: event("gap", "DTSTART;TZID=America/New_York:20260308T023000"),
    })
    .unwrap();
    s.apply(CalendarOp::Create {
        ics: event("fold", "DTSTART;TZID=America/New_York:20261101T013000"),
    })
    .unwrap();
    // Explicit UTC literals are independent of the resolver under test.
    for (uid, expected, lo, hi) in [
        (
            "gap",
            "2026-03-08T07:30:00Z",
            "2026-03-08T00:00:00Z",
            "2026-03-09T00:00:00Z",
        ),
        (
            "fold",
            "2026-11-01T05:30:00Z",
            "2026-11-01T00:00:00Z",
            "2026-11-02T00:00:00Z",
        ),
    ] {
        let epoch = |v: &str| v.parse::<jiff::Timestamp>().unwrap().as_second();
        let rows = s.current(1000., epoch(lo), epoch(hi), 0).unwrap().agenda;
        assert_eq!(
            rows.iter().find(|r| r.uid == uid).unwrap().start_s,
            epoch(expected) as f64
        );
    }
}

#[test]
fn calendar_invalid_zone_configuration_is_unavailable_not_silently_utc() {
    let dir = Scratch::new();
    for tz in [
        zone("Mars/Olympus_Mons"),
        TzConfig {
            local: Some("America/New_York".into()),
            dir: Some(dir.0.join("missing")),
        },
    ] {
        assert!(CalendarService::new(CalendarConfig {
            data: dir.0.clone(),
            loopback_test: true,
            timeout: Duration::from_secs(1),
            tz
        })
        .is_err());
    }
}

#[derive(Default)]
struct Record(Mutex<Vec<Vec<OsString>>>);
impl Opener for Record {
    fn open(&self, args: &[OsString]) -> Result<(), IslandError> {
        self.0.lock().unwrap().push(args.to_vec());
        Ok(())
    }
}
fn wait(label: &str, ready: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < end, "timed out waiting for: {label}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn downloads_open_refuses_a_regular_file_that_is_not_a_visible_model_item() {
    let dir = Scratch::new();
    let recorder = Arc::new(Record::default());
    let s = DownloadsService::start(Some(dir.0.clone()), recorder.clone()).unwrap();
    wait("inotify armed", || s.current().unwrap().available);
    // Hidden (leading dot) regular file exists on disk but is not a visible model item.
    fs::write(dir.0.join(".hidden.pdf"), b"secret").unwrap();
    fs::write(dir.0.join("visible.pdf"), b"ok").unwrap();
    wait("visible item scanned", || {
        s.current()
            .unwrap()
            .items
            .iter()
            .any(|i| i.name == "visible.pdf")
    });
    assert!(!s
        .current()
        .unwrap()
        .items
        .iter()
        .any(|i| i.name == ".hidden.pdf"));
    for op in [OpenDownload::Open, OpenDownload::Reveal] {
        assert!(s.open(".hidden.pdf", op).is_err());
    }
    assert!(recorder.0.lock().unwrap().is_empty(), "opener was called");
    s.open("visible.pdf", OpenDownload::Open).unwrap();
    assert_eq!(recorder.0.lock().unwrap().len(), 1);
    s.shutdown().unwrap();
}

#[tokio::test]
async fn calendar_floating_zoned_and_all_day_resolve_dst_from_the_tz_database() {
    let dir = Scratch::new();
    let s = calendar(&dir, zone("America/New_York"));
    for (uid, line) in [
        ("float-before", "DTSTART:20261031T090000"),
        ("float-after", "DTSTART:20261101T090000"),
        ("ny-before", "DTSTART;TZID=America/New_York:20261031T090000"),
        ("ny-after", "DTSTART;TZID=America/New_York:20261101T090000"),
        ("berlin-cest", "DTSTART;TZID=Europe/Berlin:20261024T090000"),
        ("berlin-cet", "DTSTART;TZID=Europe/Berlin:20261101T090000"),
        ("utc", "DTSTART:20261101T090000Z"),
    ] {
        s.apply(CalendarOp::Create {
            ics: event(uid, line),
        })
        .unwrap();
    }
    // 2026-10-31T00:00Z = 1793404800; Nov 1 00:00Z = 1793491200; Oct 24 00:00Z = 1792800000.
    assert_eq!(start_of(&s, "float-before"), 1_793_404_800. + 13. * 3600.); // EDT -4
    assert_eq!(start_of(&s, "float-after"), 1_793_491_200. + 14. * 3600.); // EST -5
    assert_eq!(start_of(&s, "ny-before"), 1_793_404_800. + 13. * 3600.);
    assert_eq!(start_of(&s, "ny-after"), 1_793_491_200. + 14. * 3600.);
    assert_eq!(start_of(&s, "berlin-cest"), 1_792_800_000. + 7. * 3600.); // +2
    assert_eq!(start_of(&s, "berlin-cet"), 1_793_491_200. + 8. * 3600.); // +1
    assert_eq!(start_of(&s, "utc"), 1_793_491_200. + 9. * 3600.);
}

#[tokio::test]
async fn calendar_local_zone_follows_the_injected_zone_not_the_client_offset() {
    let dir = Scratch::new();
    let s = calendar(&dir, zone("Europe/Berlin"));
    s.apply(CalendarOp::Create {
        ics: event("float", "DTSTART:20261101T090000"),
    })
    .unwrap();
    // The caller's (UTC) offset argument is only a fallback; Berlin is +1 on Nov 1.
    let v = s.current(1000., WINDOW.0, WINDOW.1, 0).unwrap();
    assert_eq!(v.agenda[0].start_s, 1_793_491_200. + 8. * 3600.);
}

#[tokio::test]
async fn calendar_published_default_window_uses_the_local_zone_not_utc_offset_zero() {
    // The published (event-pushed) window has no client offset. Put a floating event tomorrow
    // 09:00 local in Europe/Berlin (computed with jiff from the real clock) and require the
    // published row to equal that Berlin instant, not the same wall time read as UTC.
    let dir = Scratch::new();
    let db = jiff::tz::TimeZoneDatabase::from_dir(ZONEINFO).unwrap();
    let berlin = db.get("Europe/Berlin").unwrap();
    let tomorrow = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_hours(36))
        .unwrap()
        .to_zoned(berlin.clone())
        .date();
    let wall = tomorrow.at(9, 0, 0, 0);
    let expected = berlin
        .to_ambiguous_timestamp(wall)
        .compatible()
        .unwrap()
        .as_second() as f64;
    let utc_reading = jiff::tz::TimeZone::UTC
        .to_ambiguous_timestamp(wall)
        .compatible()
        .unwrap()
        .as_second() as f64;
    assert_ne!(expected, utc_reading);
    let s = calendar(&dir, zone("Europe/Berlin"));
    let line = format!(
        "DTSTART:{:04}{:02}{:02}T090000",
        tomorrow.year(),
        tomorrow.month(),
        tomorrow.day()
    );
    s.apply(CalendarOp::Create {
        ics: event("tomorrow", &line),
    })
    .unwrap();
    let published = s.topic.subscribe().borrow().clone().unwrap();
    assert_eq!(published.agenda.len(), 1);
    assert_eq!(published.agenda[0].start_s, expected);
}

#[tokio::test]
async fn calendar_unknown_tzid_is_rejected_without_fabricating_a_local_instant() {
    let dir = Scratch::new();
    let s = calendar(&dir, zone("America/New_York"));
    assert!(s
        .apply(CalendarOp::Create {
            ics: event("mars", "DTSTART;TZID=Mars/Olympus_Mons:20261101T090000"),
        })
        .is_err());
    assert!(!dir.0.join("calendar-local.ics").exists());
    assert!(starts(&s, 1000.).is_empty());
    let server = CalendarFixture::start(event(
        "remote",
        "DTSTART;TZID=Mars/Olympus_Mons:20261101T090000",
    ));
    private_file::write(
        &dir.0.join("calendar-secret-url"),
        server.url().as_bytes(),
        8192,
    )
    .unwrap();
    assert!(s.refresh(1000.).await.is_err());
    let current = s.current(2000., WINDOW.0, WINDOW.1, 0).unwrap();
    assert!(!current.available);
    assert!(current.agenda.is_empty());
    assert!(!dir.0.join("calendar-cache.json").exists());
}

#[tokio::test]
async fn calendar_default_config_uses_the_system_zone_from_tz_env_in_a_child() {
    if std::env::var("HERMES_6B2_TZ_CHILD").is_ok() {
        let dir = Scratch::new();
        let s = calendar(&dir, TzConfig::default());
        s.apply(CalendarOp::Create {
            ics: event("float", "DTSTART:20261101T090000"),
        })
        .unwrap();
        assert_eq!(start_of(&s, "float"), 1_793_491_200. + 14. * 3600.);
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "calendar_default_config_uses_the_system_zone_from_tz_env_in_a_child",
            "--nocapture",
        ])
        .env("HERMES_6B2_TZ_CHILD", "1")
        .env("TZ", "America/New_York")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
}

#[tokio::test]
async fn calendar_all_day_events_span_local_midnights_across_the_dst_change() {
    let dir = Scratch::new();
    let s = calendar(&dir, zone("America/New_York"));
    for (uid, line) in [
        ("day-before", "DTSTART;VALUE=DATE:20261101"),
        ("day-after", "DTSTART;VALUE=DATE:20261102"),
    ] {
        s.apply(CalendarOp::Create {
            ics: event(uid, line),
        })
        .unwrap();
    }
    let rows = starts(&s, 1000.);
    let find = |uid: &str| rows.iter().find(|r| r.0 == uid).unwrap().clone();
    // Midnight Nov 1 is still EDT (-4), midnight Nov 2 is EST (-5): a 25 hour day.
    assert_eq!(find("day-before").1, 1_793_491_200. + 4. * 3600.);
    assert_eq!(find("day-before").2, 1_793_577_600. + 5. * 3600.);
    assert_eq!(find("day-after").1, 1_793_577_600. + 5. * 3600.);
    assert_eq!(find("day-after").2, 1_793_664_000. + 5. * 3600.);
}
