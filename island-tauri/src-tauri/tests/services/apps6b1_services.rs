#![allow(clippy::unwrap_used)]
use crate::calendar_fixture;
use calendar_fixture::{CalendarFixture, Reply};
use hermes_island_lib::{
    contract::apps::*,
    error::IslandError,
    integrations::{
        calendar::{CalendarConfig, CalendarService, TzConfig},
        downloads::{DownloadsService, Opener},
        private_file,
        scratchpad::{core::MAX_BODY_BYTES, ScratchpadService},
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
static SERIAL: AtomicU64 = AtomicU64::new(0);
const BODY_CAP: usize = 1_048_576; // Independent acceptance boundary, not imported production policy.

#[test]
fn scratchpad_saved_file_is_private_even_when_replacing_a_wide_target() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = Scratch::new();
    let s = ScratchpadService::new(dir.0.clone());
    s.apply(
        0,
        ScratchpadOp::Create {
            id: "private".into(),
        },
    )
    .unwrap();
    s.save_due(750, false).unwrap();
    let path = dir.0.join("scratchpad.json");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    s.apply(
        800,
        ScratchpadOp::Edit {
            id: "private".into(),
            body: "private".into(),
        },
    )
    .unwrap();
    s.save_due(1550, false).unwrap();
    assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn calendar_port_rejects_utf8_in_fixed_width_dates_without_panicking() {
    use hermes_island_lib::integrations::calendar::core::parse_calendar;
    let all_day = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:x\nDTSTART;VALUE=DATE:000é000\nEND:VEVENT\nEND:VCALENDAR\n";
    assert!(parse_calendar(all_day).unwrap().0.is_empty());
    for date in ["000é000", "000é000T000000", "000é000T000000Z"] {
        let text = format!(
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:x\nDTSTART:{date}\nEND:VEVENT\nEND:VCALENDAR\n"
        );
        assert!(parse_calendar(&text).unwrap().0.is_empty());
    }
}

#[test]
fn module_registry_maps_flat_keys_and_preserves_explicit_inert_unknowns() {
    use hermes_island_lib::contract::{
        modules::{ModuleConfig, ModuleId, ModuleJsonValue, Registry},
        settings::{Settings, SettingsPatch},
    };
    use serde_json::json;
    let migrated = Settings::from_file_value(
        &json!({"hardwareMonitor":false,"terminalFontSize":18,"clipboardHistory":true}),
    );
    for id in ["timer", "scratchpad", "downloads", "calendar"] {
        assert!(migrated
            .module_registry
            .get(&ModuleId::new(id).unwrap())
            .unwrap()
            .is_active());
    }
    assert!(!migrated
        .module_registry
        .get(&ModuleId::new("monitor").unwrap())
        .unwrap()
        .is_active());
    let mut entries = migrated.module_registry.modules().to_vec();
    entries.push(hermes_island_lib::contract::modules::ModuleEntry {
        id: ModuleId::new("future").unwrap(),
        enabled: true,
        order: 99,
        config: ModuleConfig::Unknown(ModuleJsonValue::Null),
    });
    let registry = Registry::new(2, entries).unwrap();
    assert!(!registry
        .get(&ModuleId::new("future").unwrap())
        .unwrap()
        .is_active());
    let next = SettingsPatch {
        module_registry: Some(registry.clone()),
        ..Default::default()
    }
    .apply(&migrated)
    .unwrap();
    assert_eq!(
        Settings::from_file_value(&json!(next.to_file_map())).module_registry,
        registry
    );
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hi-6b1-{}-{}",
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
fn ics(uid: &str, title: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{title}\r\nDTSTART:20261031T120000Z\r\nDTEND:20261031T130000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
}
fn calendar(dir: &Scratch, timeout: Duration) -> Arc<CalendarService> {
    CalendarService::new(CalendarConfig {
        data: dir.0.clone(),
        loopback_test: true,
        timeout,
        tz: TzConfig {
            local: Some("UTC".into()),
            dir: None,
        },
    })
    .unwrap()
}
fn agenda(s: &CalendarService, now: f64) -> CalendarChanged {
    s.current(now, 1793404800, 1793577600, 0).unwrap()
}
fn secret(dir: &Scratch, url: &str) {
    private_file::write(&dir.0.join("calendar-secret-url"), url.as_bytes(), 8192).unwrap();
}

#[tokio::test]
async fn calendar_subscription_setup_is_private_rolls_back_and_disconnects() {
    use std::os::unix::fs::MetadataExt;
    let dir = Scratch::new();
    let s = calendar(&dir, Duration::from_secs(2));
    let good = CalendarFixture::start(ics("connected", "Fixture subscription"));
    s.set_subscription(Some(good.url()), 1000.).await.unwrap();
    let path = dir.0.join("calendar-secret-url");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    let original = fs::read(&path).unwrap();
    let current = agenda(&s, 1000.);
    assert!(current.configured);
    assert_eq!(current.agenda.len(), 1);
    assert!(current.agenda[0].read_only);
    assert!(!serde_json::to_string(&current)
        .unwrap()
        .contains("private-canary-token"));
    let bad = CalendarFixture::start("not a calendar".into());
    bad.set(Reply {
        status: 403,
        body: "denied".into(),
        slow: false,
        chunked: false,
    });
    let error = s
        .set_subscription(Some(bad.url()), 2000.)
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("private-canary-token"));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(agenda(&s, 2000.).configured);
    assert_eq!(agenda(&s, 2000.).agenda.len(), 1);
    s.set_subscription(None, 3000.).await.unwrap();
    assert!(!path.exists());
    assert!(!dir.0.join("calendar-cache.json").exists());
    let current = agenda(&s, 3000.);
    assert!(!current.configured);
    assert!(current.agenda.is_empty());
}

#[test]
fn scratchpad_debounce_atomic_private_reload_and_caps() {
    use std::os::unix::fs::MetadataExt;
    let dir = Scratch::new();
    let service = ScratchpadService::new(dir.0.clone());
    service
        .apply(0, ScratchpadOp::Create { id: "n".into() })
        .unwrap();
    service
        .apply(
            100,
            ScratchpadOp::Edit {
                id: "n".into(),
                body: "First note".into(),
            },
        )
        .unwrap();
    service.save_due(849, false).unwrap();
    let file = dir.0.join("scratchpad.json");
    assert!(!file.exists());
    service.save_due(850, false).unwrap();
    let before = fs::metadata(&file).unwrap();
    assert_eq!(before.mode() & 0o777, 0o600);
    let old = fs::File::open(&file).unwrap();
    service
        .apply(
            900,
            ScratchpadOp::Edit {
                id: "n".into(),
                body: "Changed".into(),
            },
        )
        .unwrap();
    service.save_due(1650, false).unwrap();
    assert_ne!(
        old.metadata().unwrap().ino(),
        fs::metadata(&file).unwrap().ino(),
        "save must replace the inode, not truncate it"
    );
    let reloaded = ScratchpadService::new(dir.0.clone());
    assert_eq!(reloaded.current().unwrap().notes[0].body, "Changed");
    assert!(service
        .apply(
            1700,
            ScratchpadOp::Edit {
                id: "n".into(),
                body: "x".repeat(MAX_BODY_BYTES + 1)
            }
        )
        .is_err());
    assert!(!service.current().unwrap().dirty);
}
#[test]
fn scratchpad_permission_failure_preserves_the_original_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new();
    let file = dir.0.join("scratchpad.json");
    fs::write(&file, b"private canary").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    let s = ScratchpadService::new(dir.0.clone());
    assert!(!s.current().unwrap().available);
    assert!(s.apply(0, ScratchpadOp::Create { id: "x".into() }).is_err());
    assert_eq!(fs::read(file).unwrap(), b"private canary");
}
#[derive(Default)]
struct Record(Mutex<Vec<Vec<OsString>>>);
impl Opener for Record {
    fn open(&self, args: &[OsString]) -> Result<(), IslandError> {
        self.0.lock().unwrap().push(args.to_vec());
        Ok(())
    }
}
struct DownloadGuard(Arc<DownloadsService>);
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        let _stop = self.0.shutdown();
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
fn downloads_real_inotify_add_rename_delete_and_only_injected_opener() {
    let dir = Scratch::new();
    let recorder = Arc::new(Record::default());
    let s = DownloadsService::start(Some(dir.0.clone()), recorder.clone()).unwrap();
    let _guard = DownloadGuard(s.clone());
    wait("inotify armed", || s.current().unwrap().available);
    fs::write(dir.0.join("one.part"), b"download").unwrap();
    wait("inotify add", || {
        s.current()
            .unwrap()
            .items
            .iter()
            .any(|i| i.name == "one.part")
    });
    fs::rename(dir.0.join("one.part"), dir.0.join("two.pdf")).unwrap();
    wait("inotify rename", || {
        let i = s.current().unwrap().items;
        i.len() == 1 && i[0].name == "two.pdf"
    });
    s.open("two.pdf", OpenDownload::Open).unwrap();
    s.open("two.pdf", OpenDownload::Reveal).unwrap();
    assert_eq!(
        *recorder.0.lock().unwrap(),
        vec![
            vec![
                OsString::from("xdg-open"),
                dir.0.join("two.pdf").into_os_string()
            ],
            hermes_island_lib::integrations::downloads::reveal_argv(&dir.0.join("two.pdf"))
        ]
    );
    assert!(s.open("../not-a-download", OpenDownload::Open).is_err());
    fs::remove_file(dir.0.join("two.pdf")).unwrap();
    wait("inotify delete", || s.current().unwrap().items.is_empty());
}
#[test]
fn downloads_missing_injected_directory_is_unavailable_not_real_home() {
    let s = DownloadsService::start(None, Arc::new(Record::default())).unwrap();
    let _guard = DownloadGuard(s.clone());
    wait("missing directory", || {
        s.current().unwrap().detail == "not-configured"
    });
    assert!(!s.current().unwrap().available);
    assert!(s.current().unwrap().items.is_empty());
}
#[test]
fn calendar_local_create_edit_delete_are_atomic_private_and_reload() {
    use std::os::unix::fs::MetadataExt;
    let dir = Scratch::new();
    let s = calendar(&dir, Duration::from_secs(1));
    s.apply(CalendarOp::Create {
        ics: ics("local", "Before"),
    })
    .unwrap();
    let path = dir.0.join("calendar-local.ics");
    let old = fs::File::open(&path).unwrap();
    assert_eq!(old.metadata().unwrap().mode() & 0o777, 0o600);
    s.apply(CalendarOp::Edit {
        source: CalendarSource::Local,
        uid: "local".into(),
        ics: ics("local", "After"),
    })
    .unwrap();
    assert_ne!(
        old.metadata().unwrap().ino(),
        fs::metadata(&path).unwrap().ino()
    );
    assert_eq!(agenda(&s, 0.).agenda[0].summary, "After");
    let reloaded = calendar(&dir, Duration::from_secs(1));
    assert_eq!(
        agenda(&reloaded, 0.).agenda[0].source,
        CalendarSource::Local
    );
    s.apply(CalendarOp::Delete {
        source: CalendarSource::Local,
        uid: "local".into(),
    })
    .unwrap();
    assert!(agenda(&s, 0.).agenda.is_empty());
    assert!(fs::read_to_string(path).unwrap().contains("END:VCALENDAR"));
}
#[test]
fn calendar_subscribed_events_cannot_be_edited_even_when_uid_collides() {
    let dir = Scratch::new();
    let s = calendar(&dir, Duration::from_secs(1));
    s.apply(CalendarOp::Create {
        ics: ics("same", "Local"),
    })
    .unwrap();
    let original = fs::read(dir.0.join("calendar-local.ics")).unwrap();
    assert!(s
        .apply(CalendarOp::Delete {
            source: CalendarSource::Subscription,
            uid: "same".into()
        })
        .is_err());
    assert!(s
        .apply(CalendarOp::Edit {
            source: CalendarSource::Subscription,
            uid: "same".into(),
            ics: ics("same", "Bad")
        })
        .is_err());
    assert_eq!(
        fs::read(dir.0.join("calendar-local.ics")).unwrap(),
        original
    );
}
#[tokio::test]
async fn calendar_subscription_loopback_merges_and_failure_keeps_stale_with_age() {
    let dir = Scratch::new();
    let server = CalendarFixture::start(ics("remote", "Subscribed"));
    secret(&dir, &server.url());
    let s = calendar(&dir, Duration::from_secs(1));
    s.apply(CalendarOp::Create {
        ics: ics("local", "Local"),
    })
    .unwrap();
    s.refresh(1000.).await.unwrap();
    let value = agenda(&s, 2000.);
    assert!(value.configured);
    assert!(!value.stale);
    assert_eq!(value.agenda.len(), 2);
    assert_eq!(value.age_ms, Some(1000.));
    assert!(
        value
            .agenda
            .iter()
            .find(|r| r.source == CalendarSource::Subscription)
            .unwrap()
            .read_only
    );
    server.set(Reply {
        status: 503,
        body: String::new(),
        slow: false,
        chunked: false,
    });
    assert!(s.refresh(3000.).await.is_err());
    let value = agenda(&s, 4000.);
    assert!(value.stale);
    assert_eq!(value.age_ms, Some(3000.));
    assert_eq!(value.agenda.len(), 2);
    assert_eq!(s.next_refresh_delay().unwrap(), Duration::from_secs(30));
    let loaded = calendar(&dir, Duration::from_secs(1));
    assert!(agenda(&loaded, 5000.).stale);
    assert_eq!(agenda(&loaded, 5000.).agenda.len(), 2);
}
#[tokio::test]
async fn calendar_secret_url_is_never_in_topic_or_error_or_cache() {
    let dir = Scratch::new();
    let server = CalendarFixture::start(ics("remote", "Normal"));
    let url = server.url();
    secret(&dir, &url);
    let s = calendar(&dir, Duration::from_secs(1));
    let rx = s.topic.subscribe();
    s.refresh(1000.).await.unwrap();
    let emitted = serde_json::to_string(&*rx.borrow()).unwrap();
    assert!(!emitted.contains(&url));
    assert!(!serde_json::to_string(&agenda(&s, 1000.))
        .unwrap()
        .contains(&url));
    assert!(!fs::read_to_string(dir.0.join("calendar-cache.json"))
        .unwrap()
        .contains(&url));
    server.set(Reply {
        status: 200,
        body: ics("remote", &url),
        slow: false,
        chunked: false,
    });
    let error = s.refresh(2000.).await.unwrap_err();
    assert!(!format!("{error:?}").contains(&url));
    assert!(!serde_json::to_string(&*rx.borrow()).unwrap().contains(&url));
}
#[tokio::test]
async fn calendar_body_limit_rejects_one_extra_byte_in_chunked_response() {
    let dir = Scratch::new();
    let mut body = ics("remote", "Cap");
    body.push_str(&" ".repeat(BODY_CAP - body.len()));
    let server = CalendarFixture::start(body.clone());
    secret(&dir, &server.url());
    let s = calendar(&dir, Duration::from_secs(2));
    s.refresh(1000.).await.unwrap();
    body.push(' ');
    server.set(Reply {
        status: 200,
        body,
        slow: false,
        chunked: true,
    });
    assert!(s.refresh(2000.).await.is_err());
    assert!(agenda(&s, 2000.).stale);
}
#[tokio::test]
async fn calendar_timeout_is_bounded_and_has_no_fabricated_last_success() {
    let dir = Scratch::new();
    let server = CalendarFixture::start(ics("remote", "Slow"));
    server.set(Reply {
        status: 200,
        body: ics("remote", "Slow"),
        slow: true,
        chunked: false,
    });
    secret(&dir, &server.url());
    let s = calendar(&dir, Duration::from_millis(50));
    let start = Instant::now();
    assert!(s.refresh(1000.).await.is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    let v = agenda(&s, 2000.);
    assert_eq!(v.updated_at, None);
    assert_eq!(v.age_ms, None);
    assert!(v.agenda.is_empty());
}
#[tokio::test]
async fn calendar_group_only_url_file_is_rejected_before_any_request() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = Scratch::new();
    let url = "https://invalid.test/private-secret-canary";
    secret(&dir, url);
    assert_eq!(
        fs::metadata(dir.0.join("calendar-secret-url"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
    fs::set_permissions(
        dir.0.join("calendar-secret-url"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    let s = calendar(&dir, Duration::from_millis(50));
    assert!(s.refresh(1000.).await.is_err());
    assert!(!agenda(&s, 1000.).configured);
}
