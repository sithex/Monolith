//! Slow, counted scratch helpers: no real Hermes home or native GUI required.
use super::*;
use crate::services::scratch;
use std::{fs, os::unix::fs::PermissionsExt, sync::Barrier};

pub(crate) fn fixture(delay: f64) -> (PathBuf, Arc<PaletteService>) {
    let dir = scratch("startup-palette");
    let root = dir.join("app");
    let home = dir.join("home");
    fs::create_dir_all(root.join("island")).unwrap();
    fs::create_dir_all(home.join(".hermes/skins")).unwrap();
    let launcher = home.join(".hermes/hermes-agent/.hermes/bin/hermes");
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    fs::write(&launcher, "#!/bin/sh\nexec /usr/bin/python3 \"$@\"\n").unwrap();
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    let script = format!(
        "import json, pathlib, sys, time\nprofile = pathlib.Path(sys.argv[1]).parent\nwith (profile / 'helper-calls').open('a') as out:\n out.write('call\\n')\ntime.sleep({delay})\nprint(json.dumps(dict(name='Fixture', background='#111111', accent='#abcdef', text='#ffffff', primary='#eeeeee', dim='#999999', border='#444444', surface='#222222', error='#ff8888')))\n"
    );
    fs::write(root.join("island/palette.py"), script).unwrap();
    (dir, Arc::new(PaletteService::new(root, home)))
}

pub(crate) fn calls(dir: &Path) -> usize {
    fs::read_to_string(dir.join("home/.hermes/helper-calls"))
        .unwrap_or_default()
        .lines()
        .count()
}

pub(crate) fn wait_started(dir: &Path) {
    let start = Instant::now();
    while calls(dir) == 0 {
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "scratch helper never started"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn cold_get_is_single_flight_with_concurrent_callers_and_watcher() {
    let (dir, service) = fixture(0.6);
    let barrier = Arc::new(Barrier::new(9));
    let readers: Vec<_> = (0..8)
        .map(|_| {
            let (service, barrier) = (service.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                service.get().unwrap()
            })
        })
        .collect();
    barrier.wait();
    wait_started(&dir);
    let (tx, rx) = mpsc::channel();
    let mut watcher = PaletteWatcher::start(
        service.clone(),
        dir.join("home/.hermes"),
        Duration::from_secs(3600),
        Arc::new(move |palette| {
            tx.send(palette).unwrap();
        }),
    );
    let published = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    for reader in readers {
        assert_eq!(reader.join().unwrap(), published);
    }
    watcher.stop();
    assert_eq!(
        calls(&dir),
        1,
        "watcher startup and cold readers must share one helper"
    );
    assert_eq!(service.cached(), Some(published));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn watcher_publishes_an_already_cached_startup_palette_without_recomputing() {
    let (dir, service) = fixture(0.1);
    let expected = service.get().unwrap();
    let (tx, rx) = mpsc::channel();
    let mut watcher = PaletteWatcher::start(
        service,
        dir.join("home/.hermes"),
        Duration::from_secs(3600),
        Arc::new(move |palette| {
            tx.send(palette).unwrap();
        }),
    );
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), expected);
    watcher.stop();
    assert_eq!(calls(&dir), 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn watcher_winning_the_cold_flight_still_serves_concurrent_gets() {
    let (dir, service) = fixture(0.6);
    let (tx, rx) = mpsc::channel();
    let mut watcher = PaletteWatcher::start(
        service.clone(),
        dir.join("home/.hermes"),
        Duration::from_secs(3600),
        Arc::new(move |palette| {
            tx.send(palette).unwrap();
        }),
    );
    wait_started(&dir);
    let readers: Vec<_> = (0..8)
        .map(|_| {
            let service = service.clone();
            std::thread::spawn(move || service.get().unwrap())
        })
        .collect();
    let published = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    for reader in readers {
        assert_eq!(reader.join().unwrap(), published);
    }
    watcher.stop();
    assert_eq!(calls(&dir), 1);
    fs::remove_dir_all(dir).unwrap();
}
