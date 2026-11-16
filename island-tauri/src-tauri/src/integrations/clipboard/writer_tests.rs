//! Phase 8: off-lock coalescing persistence writer and the total byte budget.
use super::history::{
    ClipInput, ClipPayload, HistoryConfig, HistoryStore, OfferOutcome, SkipReason,
};
use super::*;
use std::time::{Duration, Instant};
use std::{path::PathBuf, sync::atomic::Ordering, sync::Arc};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hi-clip-p8-{tag}-{}-{}", std::process::id(), now()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn persist_settings() -> Settings {
    Settings {
        clipboard_persist: true,
        clipboard_history: false,
        ..Settings::default()
    }
}
fn started(dir: &std::path::Path) -> (Arc<Clipboard>, u32) {
    let s = Clipboard::new(dir.to_path_buf(), persist_settings()).unwrap();
    s.shared.enabled.store(true, Ordering::Release);
    let epoch = s.shared.epoch.load(Ordering::Acquire);
    (s, epoch)
}
fn copy(s: &Arc<Clipboard>, epoch: u32, text: String) {
    s.shared
        .offer(ClipPayload::Text(text), vec![], epoch)
        .unwrap();
}
fn pin_latest(s: &Arc<Clipboard>) {
    let id = s.list(None, 1).unwrap()[0].id.clone();
    s.pin(&id, true).unwrap();
}
fn input(text: String) -> ClipInput {
    ClipInput {
        content: ClipPayload::Text(text),
        mime_types: vec![],
        password_hint: None,
        source_app: None,
    }
}

#[test]
fn burst_of_plain_copies_coalesces_into_few_durable_writes() {
    let dir = tmp("burst");
    let (s, epoch) = started(&dir);
    copy(&s, epoch, "pinned".into());
    pin_latest(&s);
    s.shared.writer.flush().unwrap();
    let base = s.shared.writer.writes();
    for i in 0..100 {
        copy(&s, epoch, format!("copy {i}"));
    }
    s.shutdown().unwrap();
    let n = s.shared.writer.writes() - base;
    assert!((1..=3).contains(&n), "expected 1..=3 writes, got {n}");
    // The id counter reached the disk: a reload does not reuse ids.
    let loaded = Clipboard::new(dir.clone(), persist_settings()).unwrap();
    assert_eq!(loaded.list(None, 10).unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pin_change_is_written_without_waiting_for_the_coalescing_delay() {
    let dir = tmp("pin");
    let (s, epoch) = started(&dir);
    copy(&s, epoch, "keep me".into());
    pin_latest(&s);
    let file = dir.join("clipboard.json");
    let t = Instant::now();
    while !std::fs::read_to_string(&file)
        .map(|j| j.contains("keep me"))
        .unwrap_or(false)
    {
        assert!(
            t.elapsed() < Duration::from_millis(80),
            "pinned change was delayed"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    s.shutdown().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn history_lock_is_not_held_across_a_slow_write() {
    let dir = tmp("slow");
    let (s, epoch) = started(&dir);
    for i in 0..5 {
        copy(&s, epoch, format!("pinned {i}"));
        pin_latest(&s);
    }
    s.shared.writer.flush().unwrap();
    s.shared.writer.set_delay_ms(300);
    let mut worst = Duration::ZERO;
    for i in 0..10 {
        let t = Instant::now();
        copy(&s, epoch, format!("copy {i}"));
        let _ = s.list(None, 5).unwrap();
        worst = worst.max(t.elapsed());
        std::thread::sleep(Duration::from_millis(30));
    }
    s.shared.writer.set_delay_ms(0);
    assert!(
        worst < Duration::from_millis(100),
        "history lock stalled {worst:?}"
    );
    s.shutdown().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn turning_persistence_off_wins_over_a_queued_save() {
    let dir = tmp("off");
    let (s, epoch) = started(&dir);
    copy(&s, epoch, "gone soon".into());
    pin_latest(&s);
    s.shared.writer.set_delay_ms(100);
    copy(&s, epoch, "newer".into());
    pin_latest(&s);
    s.configure(Settings {
        clipboard_persist: false,
        clipboard_history: false,
        ..Settings::default()
    })
    .unwrap();
    s.shutdown().unwrap();
    assert!(!dir.join("clipboard.json").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unchanged_state_is_not_written_again() {
    let dir = tmp("noop");
    let (s, epoch) = started(&dir);
    copy(&s, epoch, "p".into());
    pin_latest(&s);
    s.shared.writer.flush().unwrap();
    let base = s.shared.writer.writes();
    s.configure(persist_settings()).unwrap();
    s.configure(persist_settings()).unwrap();
    assert_eq!(s.shared.writer.writes(), base);
    s.shutdown().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn snapshot_json_matches_to_persisted_for_all_payload_kinds() {
    let mut store = HistoryStore::new(HistoryConfig::default()).unwrap();
    let items = [
        ClipPayload::Text("héllo \"quoted\"\n".into()),
        ClipPayload::Image {
            png: vec![1, 2, 3, 255],
            width: 2,
            height: 3,
        },
        ClipPayload::Files(vec!["file:///a b".into(), "file:///c".into()]),
    ];
    for (i, content) in items.into_iter().enumerate() {
        let out = store.offer(
            ClipInput {
                content,
                mime_types: vec![],
                password_hint: None,
                source_app: Some(format!("app{i}")),
            },
            1000 + i as u64,
        );
        let OfferOutcome::Added { id } = out else {
            panic!("not added: {out:?}")
        };
        store.pin(&id, true).unwrap();
    }
    store.offer(input("unpinned".into()), 5000);
    assert_eq!(
        store.persist_snapshot().to_json().unwrap(),
        store.to_persisted(false).unwrap()
    );
}

fn budget_store(capacity: usize, max_total_bytes: usize) -> HistoryStore {
    HistoryStore::new(HistoryConfig {
        capacity,
        max_total_bytes,
        max_text_bytes: 1024 * 1024,
        max_image_bytes: 1024 * 1024,
        ..HistoryConfig::default()
    })
    .unwrap()
}

#[test]
fn retained_bytes_plateau_at_the_budget_not_at_capacity_times_item_size() {
    // 100 entries x 1 MiB would be 100 MiB; the budget keeps it at 8 MiB.
    let mut store = budget_store(100, 8 * 1024 * 1024);
    let mut peak = 0;
    for i in 0..300 {
        let mut text = format!("{i}-");
        text.push_str(&"x".repeat(1024 * 1024 - 8));
        store.offer(input(text), 1000 + i);
        peak = peak.max(store.retained_bytes());
    }
    assert!(peak <= 8 * 1024 * 1024, "peak {peak}");
    assert!(
        store.retained_bytes() > 6 * 1024 * 1024,
        "plateau should be near the budget"
    );
    assert!(store.len() <= 8);
}

#[test]
fn byte_budget_never_evicts_pinned_and_rejection_costs_nothing() {
    let mut store = budget_store(100, 2 * 1024 * 1024);
    let big = |c: char| c.to_string().repeat(1024 * 1024 - 100);
    let OfferOutcome::Added { id: a } = store.offer(input(big('a')), 1) else {
        panic!()
    };
    store.pin(&a, true).unwrap();
    let OfferOutcome::Added { id: _ } = store.offer(input("small".into()), 2) else {
        panic!()
    };
    // Needs ~1 MiB; only ~1 MiB minus the small entry is left after the pinned one: evict
    // the unpinned small entry and fit.
    let OfferOutcome::Added { id: b } = store.offer(input(big('b')), 3) else {
        panic!("should fit")
    };
    store.pin(&b, true).unwrap();
    // Two pinned ~1 MiB entries now fill the budget: a third cannot fit and nothing is lost.
    let before = store.len();
    assert_eq!(
        store.offer(input(big('c')), 4),
        OfferOutcome::Skipped(SkipReason::OverBudget)
    );
    assert_eq!(store.len(), before);
    assert!(store.get(&a).is_some() && store.get(&b).is_some());
}

#[test]
fn rejected_offer_keeps_existing_unpinned_entries() {
    let mut store = budget_store(100, 1024 * 1024 + 10);
    let OfferOutcome::Added { id: p } = store.offer(input("x".repeat(1024 * 1024)), 1) else {
        panic!()
    };
    store.pin(&p, true).unwrap();
    // Unpinned entry that fits in the 10 spare bytes.
    let OfferOutcome::Added { id: small } = store.offer(input("tiny".into()), 2) else {
        panic!()
    };
    // A 1 KiB entry cannot fit even after evicting `tiny`: it is refused and `tiny` stays.
    assert_eq!(
        store.offer(input("y".repeat(1024)), 3),
        OfferOutcome::Skipped(SkipReason::OverBudget)
    );
    assert!(store.get(&small).is_some());
}

#[test]
fn loading_an_oversized_file_trims_unpinned_first_and_keeps_pinned() {
    let mut big = budget_store(100, 64 * 1024 * 1024);
    for i in 0..4u64 {
        let OfferOutcome::Added { id } = big.offer(input(format!("{i}").repeat(300 * 1024)), i)
        else {
            panic!()
        };
        big.pin(&id, true).unwrap();
    }
    let json = big.to_persisted(false).unwrap();
    let small = HistoryStore::from_persisted(
        HistoryConfig {
            max_total_bytes: 1,
            ..HistoryConfig::default()
        },
        &json,
    )
    .unwrap();
    // The budget is floored at the largest per-item limit, and pinned entries are never dropped.
    assert_eq!(small.len(), 4);
}

#[test]
fn steady_stream_of_plain_copies_is_rate_limited_by_the_lazy_delay() {
    let dir = tmp("stream");
    let (s, epoch) = started(&dir);
    copy(&s, epoch, "pinned".into());
    pin_latest(&s);
    s.shared.writer.flush().unwrap();
    let base = s.shared.writer.writes();
    // ~200 ms of copies, one every 4 ms, each far apart enough for a fast disk to keep up.
    for i in 0..50 {
        copy(&s, epoch, format!("copy {i}"));
        std::thread::sleep(Duration::from_millis(4));
    }
    s.shutdown().unwrap();
    let n = s.shared.writer.writes() - base;
    assert!(
        (1..=6).contains(&n),
        "expected at most ~1 write per 100 ms, got {n}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn block_file(dir: &std::path::Path) -> PathBuf {
    // A non-empty directory where the file belongs: the atomic rename fails.
    let blocker = dir.join("clipboard.json");
    let _ = std::fs::remove_file(&blocker);
    std::fs::create_dir_all(blocker.join("x")).unwrap();
    blocker
}

#[test]
fn pin_reports_a_failed_write_and_the_next_change_retries() {
    let dir = tmp("pin-fail");
    let (s, epoch) = started(&dir);
    copy(&s, epoch, "keep me".into());
    let blocker = block_file(&dir);
    let id = s.list(None, 1).unwrap()[0].id.clone();
    assert!(s.pin(&id, true).is_err(), "a pin that did not reach the disk is reported");
    std::fs::remove_dir_all(&blocker).unwrap();
    // Nothing persisted changes here, yet the failed state must be written again.
    s.configure(persist_settings()).unwrap();
    let loaded = Clipboard::new(dir.clone(), persist_settings()).unwrap();
    assert_eq!(loaded.list(None, 10).unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pin_is_on_disk_when_it_returns() {
    let dir = tmp("pin-durable");
    let (s, epoch) = started(&dir);
    s.shared.writer.set_delay_ms(60);
    copy(&s, epoch, "durable".into());
    pin_latest(&s);
    // No flush, no shutdown: a second instance already sees the pin.
    let loaded = Clipboard::new(dir.clone(), persist_settings()).unwrap();
    assert_eq!(loaded.list(None, 10).unwrap().len(), 1);
    s.shutdown().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_background_write_failure_that_later_recovers_is_not_reported_stale() {
    let dir = tmp("stale-error");
    let (s, epoch) = started(&dir);
    copy(&s, epoch, "pinned".into());
    pin_latest(&s);
    let blocker = block_file(&dir);
    let before = s.shared.writer.writes();
    copy(&s, epoch, "plain copy while the disk is failing".into());
    let deadline = Instant::now() + Duration::from_secs(5);
    while s.shared.writer.writes() == before {
        assert!(Instant::now() < deadline, "lazy write never ran");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(50));
    std::fs::remove_dir_all(&blocker).unwrap();
    // The retry succeeds, so this settings change must not fail with the old error.
    s.configure(persist_settings()).unwrap();
    assert!(dir.join("clipboard.json").is_file());
    s.shutdown().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
