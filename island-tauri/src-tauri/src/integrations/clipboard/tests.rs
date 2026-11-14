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
#[cfg(target_os = "linux")]
#[test]
fn clipboard_pause_and_resume_wake_an_idle_watcher_so_the_epoch_fence_precedes_the_next_copy() {
    use std::io::Read;
    let service = service().unwrap();
    let (ours, mut watcher_side) = std::os::unix::net::UnixStream::pair().unwrap();
    watcher_side.set_nonblocking(true).unwrap();
    *lock(&service.shared.watcher_wake).unwrap() = Some(ours);
    let woken = |rx: &mut std::os::unix::net::UnixStream| {
        let mut b = [0u8; 8];
        matches!(rx.read(&mut b), Ok(n) if n > 0)
    };
    assert!(
        !woken(&mut watcher_side),
        "nothing wakes the watcher before a pause"
    );
    service.set_paused(true).unwrap();
    assert!(
        woken(&mut watcher_side),
        "pausing must wake the poll loop to fence the new epoch"
    );
    service.set_paused(false).unwrap();
    assert!(woken(&mut watcher_side), "resuming must wake it too, or the first copy after resume is dropped as queued-while-paused");
    // A watcher that already exited (peer gone) must not turn a pause into an error.
    drop(watcher_side);
    service.set_paused(true).unwrap();
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

/// Records delivered clipboard events; fails the next N `added` emits.
#[derive(Default)]
struct Outlet {
    added: Mutex<Vec<String>>,
    statuses: Mutex<Vec<wire::ClipboardStatus>>,
    fail_next: AtomicU32,
}
impl ClipOutlet for Outlet {
    fn added(&self, item: &wire::ClipSummary) -> Result<(), IslandError> {
        if self
            .fail_next
            .load(Ordering::SeqCst)
            .checked_sub(1)
            .is_some_and(|left| {
                self.fail_next.store(left, Ordering::SeqCst);
                true
            })
        {
            return Err(IslandError::internal("injected emit failure"));
        }
        self.added.lock().unwrap().push(item.preview.clone());
        Ok(())
    }
    fn status(&self, status: &wire::ClipboardStatusChanged) -> Result<(), IslandError> {
        self.statuses.lock().unwrap().push(status.status.clone());
        Ok(())
    }
}
fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready() {
        assert!(std::time::Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn policy(max_restarts: u32) -> RestartPolicy {
    RestartPolicy {
        max_restarts,
        backoff: Backoff::new(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_millis(10),
        ),
        healthy_after: std::time::Duration::from_secs(60),
    }
}
#[test]
fn clipboard_delivery_retries_a_failed_emit_and_keeps_order() {
    let service = service().unwrap();
    let out = Arc::new(Outlet::default());
    out.fail_next.store(2, Ordering::SeqCst);
    service.start_events_with(out.clone(), policy(0)).unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    for text in ["one", "two"] {
        service
            .shared
            .offer(ClipPayload::Text(text.into()), vec![], epoch)
            .unwrap();
    }
    wait_until("both entries delivered", || out.added.lock().unwrap().len() == 2);
    assert_eq!(*out.added.lock().unwrap(), ["one", "two"]);
    assert_eq!(service.status().unwrap().dropped, 0, "a retried entry is not a drop");
    service.shutdown().unwrap();
}
#[test]
fn clipboard_dead_pump_is_restarted_then_reported_stopped() {
    let service = service().unwrap();
    let out = Arc::new(Outlet::default());
    service.start_events_with(out.clone(), policy(1)).unwrap();
    // A broken history lock: every pump run fails at once.
    let shared = service.shared.clone();
    let _ = std::thread::spawn(move || {
        let _guard = shared.inner.lock().unwrap();
        panic!("poison the clipboard history lock");
    })
    .join();
    service.shared.wake.notify_one();
    wait_until("reported stopped", || {
        service.status().unwrap().detail == "delivery-stopped"
    });
    let status = service.status().unwrap();
    assert!(!status.available);
    wait_until("the page was told", || {
        out.statuses
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.detail == "delivery-stopped")
    });
}
#[test]
fn clipboard_stand_in_never_touches_the_saved_history() {
    let dir = std::env::temp_dir().join(format!("hi-clipboard-stand-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("clipboard.json");
    std::fs::write(&file, b"{\"saved\":\"unreadable by this build\"}").unwrap();
    let stand_in = Clipboard::unavailable(dir.clone()).unwrap();
    let status = stand_in.status().unwrap();
    assert_eq!((status.available, status.detail.as_str()), (false, "start-failed"));
    let settings = Settings {
        clipboard_history: true,
        clipboard_persist: true,
        ..Settings::default()
    };
    stand_in.configure(settings).unwrap();
    assert!(lock(&stand_in.worker).unwrap().is_none(), "no capture starts");
    assert!(stand_in.list(None, 10).unwrap().is_empty());
    assert_eq!(stand_in.copy("x").unwrap_err().code, ErrorCode::Invalid);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"{\"saved\":\"unreadable by this build\"}",
        "saved history left as it was"
    );
    stand_in.shutdown().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}
