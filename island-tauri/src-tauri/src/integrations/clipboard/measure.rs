//! Isolated clipboard persistence measurements (temp dir only, never the live history).
//! Run: cargo test --lib clipboard::measure -- --nocapture --test-threads=1
use super::*;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hi-clip-measure-{tag}-{}-{}",
        std::process::id(),
        now()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn pct(v: &mut [Duration], p: f64) -> Duration {
    v.sort();
    v[((v.len() as f64 * p) as usize).min(v.len() - 1)]
}
fn persist_settings() -> Settings {
    Settings {
        clipboard_persist: true,
        clipboard_history: false,
        ..Settings::default()
    }
}
// SHIM-BEGIN (differs between the baseline and the new code)
fn settle(s: &Arc<Clipboard>) {
    s.shared.writer.flush().unwrap();
}
fn writes(s: &Arc<Clipboard>) -> u64 {
    s.shared.writer.writes()
}
fn set_delay(s: &Arc<Clipboard>, ms: u64) {
    s.shared.writer.set_delay_ms(ms)
}
// SHIM-END

/// 100 distinct copies with one pinned entry: how many durable writes?
#[test]
fn measure_burst_100_copies() {
    let dir = tmp("burst");
    let service = Clipboard::new(dir.clone(), persist_settings()).unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    service
        .shared
        .offer(ClipPayload::Text("pinned".into()), vec![], epoch)
        .unwrap();
    let id = service.list(None, 10).unwrap()[0].id.clone();
    service.pin(&id, true).unwrap();
    settle(&service);
    let base = writes(&service);
    let t = Instant::now();
    for i in 0..100 {
        service
            .shared
            .offer(ClipPayload::Text(format!("copy {i}")), vec![], epoch)
            .unwrap();
    }
    let burst = t.elapsed();
    service.shutdown().unwrap();
    println!(
        "MEASURE burst: 100 copies in {burst:?}; durable writes during burst+shutdown flush = {}",
        writes(&service) - base
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Slow disk (injected write delay) with a large pinned history: how long do copies and
/// concurrent history readers wait on the history mutex?
#[test]
fn measure_lock_hold_with_slow_writer() {
    let dir = tmp("slow");
    let service = Clipboard::new(dir.clone(), persist_settings()).unwrap();
    service.shared.enabled.store(true, Ordering::Release);
    let epoch = service.shared.epoch.load(Ordering::Acquire);
    // 20 pinned 1 MiB texts = 20 MiB of persisted payload.
    for i in 0..20 {
        let mut text = format!("pinned-{i}-");
        text.push_str(&"x".repeat(1024 * 1024 - 20));
        service
            .shared
            .offer(ClipPayload::Text(text), vec![], epoch)
            .unwrap();
        let id = service.list(None, 1).unwrap()[0].id.clone();
        service.pin(&id, true).unwrap();
    }
    settle(&service);
    set_delay(&service, 50);
    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let (s, stop) = (service.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut lat = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                let t = Instant::now();
                s.list(None, 5).unwrap();
                lat.push(t.elapsed());
                std::thread::sleep(Duration::from_millis(1));
            }
            lat
        })
    };
    let mut offers = Vec::new();
    for i in 0..40 {
        let t = Instant::now();
        service
            .shared
            .offer(ClipPayload::Text(format!("copy {i}")), vec![], epoch)
            .unwrap();
        offers.push(t.elapsed());
        std::thread::sleep(Duration::from_millis(20));
    }
    stop.store(true, Ordering::Relaxed);
    let mut reads = reader.join().unwrap();
    set_delay(&service, 0);
    println!(
        "MEASURE offer() call (holds history lock): p50 {:?} p95 {:?} max {:?}",
        pct(&mut offers, 0.5),
        pct(&mut offers, 0.95),
        offers.iter().max().unwrap()
    );
    println!(
        "MEASURE list() latency during burst ({} samples): p50 {:?} p95 {:?} max {:?}",
        reads.len(),
        pct(&mut reads, 0.5),
        pct(&mut reads, 0.95),
        reads.iter().max().unwrap()
    );
    service.shutdown().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}
