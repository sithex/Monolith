//! Service tests are deliberately connection-free. They do not impersonate watcher
//! acceptance: the parent compositor suite must exercise real protocol changes.
use super::*;

fn service() -> Result<Arc<Clipboard>, IslandError> {
    Clipboard::new(
        std::env::temp_dir().join(format!("hi-clipboard-unused-{}", std::process::id())),
        Settings::default(),
    )
}
#[test]
fn clipboard_default_off_construct_configure_pause_open_no_connection_or_watcher() {
    let service = service().unwrap();
    for _ in 0..3 {
        service.configure(Settings::default()).unwrap();
    }
    service.set_paused(true).unwrap();
    let s = service.status().unwrap();
    assert!(!s.enabled && !s.available && !s.watcher_running);
    assert!(s.paused);
    assert_eq!((s.connections_open, s.connection_attempts), (0, 0));
    assert!(service.worker.lock().unwrap().is_none());
    assert!(service.list(None, 500).unwrap().is_empty());
    service.shutdown().unwrap();
}
#[test]
fn clipboard_pause_needs_no_history_lock_and_invalidates_inflight_epoch() {
    let service = service().unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    // Hold the history lock: actor-safe pause must not attempt to take it or do IO.
    let inner = service.shared.inner().unwrap();
    service.set_paused(true).unwrap();
    service.set_paused(false).unwrap();
    drop(inner);
    assert!(!service.shared.active(epoch));
    service
        .shared
        .offer(ClipPayload::Text("stale transfer".into()), vec![], epoch)
        .unwrap();
    assert!(service.list(None, 10).unwrap().is_empty());
    service.shutdown().unwrap();
}
#[test]
fn clipboard_added_queue_is_128_drops_oldest_and_disabled_drops_pending() {
    let service = service().unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    for i in 0..130 {
        service
            .shared
            .offer(ClipPayload::Text(format!("clip-{i}")), vec![], epoch)
            .unwrap();
    }
    let inner = service.shared.inner().unwrap();
    assert_eq!(inner.pending.len(), 128);
    assert_eq!(inner.pending.front().unwrap().preview, "clip-2");
    assert_eq!(inner.pending.back().unwrap().preview, "clip-129");
    drop(inner);
    let s = service.status().unwrap();
    assert_eq!(s.dropped, 2);
    assert_eq!(s.captured, 130);
    service.configure(Settings::default()).unwrap();
    assert!(service.shared.inner().unwrap().pending.is_empty());
    service.shutdown().unwrap();
}
#[test]
fn clipboard_echo_reorders_without_adding_or_queueing_again() {
    let service = service().unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    service
        .shared
        .offer(ClipPayload::Text("first".into()), vec![], epoch)
        .unwrap();
    service
        .shared
        .offer(ClipPayload::Text("second".into()), vec![], epoch)
        .unwrap();
    service.shared.inner().unwrap().pending.clear();
    service
        .shared
        .expect_echo(ClipPayload::Text("first".into()))
        .unwrap();
    service
        .shared
        .offer(ClipPayload::Text("first".into()), vec![], epoch)
        .unwrap();
    assert_eq!(service.list(None, 10).unwrap()[0].preview, "first");
    assert_eq!(service.status().unwrap().echoes, 1);
    assert_eq!(service.status().unwrap().captured, 2);
    assert!(service.shared.inner().unwrap().pending.is_empty());
    service.shutdown().unwrap();
}
#[test]
fn clipboard_policy_change_revalidates_existing_denied_text() {
    let service = service().unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    service
        .shared
        .offer(ClipPayload::Text("private note".into()), vec![], epoch)
        .unwrap();
    let settings = Settings {
        clipboard_deny_patterns: vec!["private".into()],
        ..Settings::default()
    };
    service.configure(settings).unwrap();
    assert!(service.list(None, 10).unwrap().is_empty());
    assert!(service.shared.inner().unwrap().pending.is_empty());
    service.shutdown().unwrap();
}
#[test]
fn clipboard_persistence_is_atomic_private_and_pinned_only() {
    use std::fs;
    let dir = std::env::temp_dir().join(format!(
        "hi-clipboard-persist-{}-{}",
        std::process::id(),
        now()
    ));
    fs::create_dir(&dir).unwrap();
    let service = Clipboard::new(dir.clone(), Settings::default()).unwrap();
    assert!(!dir.join("clipboard.json").exists());
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    service
        .shared
        .offer(ClipPayload::Text("pin me".into()), vec![], epoch)
        .unwrap();
    service
        .shared
        .offer(ClipPayload::Text("do not persist".into()), vec![], epoch)
        .unwrap();
    let id = service.list(Some("pin me"), 10).unwrap()[0].id.clone();
    service.pin(&id, true).unwrap();
    let settings = Settings {
        clipboard_persist: true,
        ..Settings::default()
    };
    service.configure(settings.clone()).unwrap();
    let file = dir.join("clipboard.json");
    let json = fs::read_to_string(&file).unwrap();
    assert!(json.contains("pin me"));
    assert!(!json.contains("do not persist"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    service.shutdown().unwrap();
    let loaded = Clipboard::new(dir.clone(), settings).unwrap();
    assert_eq!(loaded.list(None, 10).unwrap().len(), 1);
    assert!(loaded.list(None, 10).unwrap()[0].pinned);
    loaded.shutdown().unwrap();
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(unix)]
#[test]
fn clipboard_persistence_load_rejects_group_or_other_readable_file() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let dir = std::env::temp_dir().join(format!(
        "hi-clipboard-mode-{}-{}",
        std::process::id(),
        now()
    ));
    fs::create_dir(&dir).unwrap();
    // Removes the scratch dir even when an assertion fails (the mutation belt runs this red).
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let settings = Settings {
        clipboard_persist: true,
        ..Settings::default()
    };
    // Build a genuine private snapshot with one pinned entry.
    let service = Clipboard::new(dir.clone(), Settings::default()).unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    service
        .shared
        .offer(ClipPayload::Text("pinned secret-ish".into()), vec![], epoch)
        .unwrap();
    let id = service.list(None, 10).unwrap()[0].id.clone();
    service.pin(&id, true).unwrap();
    service.configure(settings.clone()).unwrap();
    service.shutdown().unwrap();
    let file = dir.join("clipboard.json");
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // Control: the 0600 file loads and yields its entry.
    let ok = Clipboard::new(dir.clone(), settings.clone()).unwrap();
    assert_eq!(ok.list(None, 10).unwrap().len(), 1);
    ok.shutdown().unwrap();
    // Group- and other-readable (and any extra bit) files are rejected, entries not loaded.
    for mode in [0o644, 0o640, 0o604, 0o660, 0o666] {
        fs::set_permissions(&file, fs::Permissions::from_mode(mode)).unwrap();
        match Clipboard::new(dir.clone(), settings.clone()) {
            Ok(loaded) => {
                let n = loaded.list(None, 10).unwrap().len();
                loaded.shutdown().unwrap();
                panic!("mode {mode:o} was accepted and loaded {n} entries");
            }
            Err(error) => assert!(
                error.message.contains("private"),
                "mode {mode:o}: {error:?}"
            ),
        }
    }
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}
