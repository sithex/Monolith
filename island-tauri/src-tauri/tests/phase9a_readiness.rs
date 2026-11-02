//! Pure policy and scratch persistence/coexistence tests. No launch touches real state.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use hermes_island_lib::{
    cli::Cli,
    contract::{
        island::UiKind,
        settings::{Settings, SettingsPatch},
    },
    paths,
    services::{autostart::Autostart, settings::SettingsService},
    singleton,
};
use serde_json::{json, Value};
use std::{
    fs,
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, PermissionsExt},
            net::UnixListener,
        },
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hi-9a-policy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn parse(args: &[&str]) -> Cli {
    Cli::parse_with_env(args.iter().copied(), None).unwrap()
}

#[test]
fn live_overlay_and_preview_resolve_exact_shared_dirs_without_io() {
    let s = Scratch::new();
    let root = s.0.join("not-created");
    for (args, name) in [
        (&["--overlay"][..], ".island-data-overlay"),
        (&[][..], ".island-data"),
    ] {
        let cli = parse(args);
        assert!(!cli.test_instance);
        assert_eq!(cli.ui, UiKind::Harness);
        let p = paths::resolve(&cli, &root).unwrap();
        assert_eq!(p.data_dir, root.join(name));
        assert_eq!(p.panel_sock, root.join(name).join("panel.sock"));
        assert_eq!(p.status, root.join(name).join("status.json"));
    }
    assert!(!root.exists(), "resolution must be pure");
}
#[test]
fn test_instance_cannot_select_live_paths_or_aliases() {
    let s = Scratch::new();
    let root = s.0.join("repo");
    fs::create_dir(&root).unwrap();
    for name in [".island-data", ".island-data-overlay"] {
        let live = root.join(name);
        fs::create_dir(&live).unwrap();
        let alias = s.0.join(format!("alias-{name}"));
        std::os::unix::fs::symlink(&live, &alias).unwrap();
        for p in [live.clone(), live.join("nested"), alias] {
            let cli = parse(&["--test-instance", "--data-dir", p.to_str().unwrap()]);
            assert!(paths::resolve(&cli, &root).is_err());
            assert_eq!(fs::read_dir(&live).unwrap().count(), 0);
        }
    }
    let p = s.0.join("scratch");
    let cli = parse(&["--test-instance", "--data-dir", p.to_str().unwrap()]);
    assert_eq!(paths::resolve(&cli, &root).unwrap().data_dir, p);
}
#[test]
fn live_fixture_pty_is_refused_even_with_test_environment_enabled() {
    for env in [None, Some("1")] {
        assert!(Cli::parse_with_env(["--overlay", "--fixture-pty"], env).is_err());
        assert!(Cli::parse_with_env(["--overlay", "--data-dir", "/scratch"], env).is_err());
    }
}
#[test]
fn notify_live_directory_override_is_also_refused_before_connecting() {
    let args: Vec<String> = ["notify", "--title", "fixture", "--data-dir", "/scratch"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert!(hermes_island_lib::cli_notify::parse(&args)
        .unwrap()
        .is_err());
}

#[test]
fn runtime_autostart_wiring_selects_tauri_but_test_instances_refuse_writes() {
    use hermes_island_lib::services::{ServiceConfig, Services};
    let s = Scratch::new();
    let cfg = ServiceConfig {
        repo_root: s.0.join("repo"),
        data_dir: s.0.join("inst"),
        home: s.0.join("home"),
        overlay: true,
        test_instance: false,
        fixture_pty: false,
        ui: UiKind::Island,
        test_hooks: false,
    };
    let svc = Services::new(&cfg);
    svc.settings
        .patch(&serde_json::from_value::<SettingsPatch>(json!({"autostart":true})).unwrap())
        .unwrap();
    let desktop =
        fs::read_to_string(cfg.home.join(".config/autostart/hermes-island.desktop")).unwrap();
    assert!(desktop.contains(&format!(
        "Exec=\"{}/island-tauri/run.sh\" --overlay",
        cfg.repo_root.display()
    )));
    svc.shutdown();
    let cfg = ServiceConfig {
        test_instance: true,
        home: s.0.join("refused-home"),
        ..cfg
    };
    let svc = Services::new(&cfg);
    assert!(svc
        .settings
        .patch(&serde_json::from_value::<SettingsPatch>(json!({"autostart":true})).unwrap())
        .is_err());
    assert!(!cfg.home.exists());
    svc.shutdown();
}

#[test]
fn singleton_refuses_legacy_instance_lock_in_scratch_live_dir() {
    let s = Scratch::new();
    let p = paths::resolve_live_dir(&s.0, true).data_dir;
    fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(p.join("instance.lock"))
        .unwrap();
    fs::set_permissions(p.join("instance.lock"), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert!(
        singleton::acquire(&p).is_err(),
        "island-rust's exact lock must exclude Tauri"
    );
}
#[test]
fn singleton_refuses_live_status_without_lock_and_never_unlinks_socket() {
    let s = Scratch::new();
    let mut child = Command::new("/bin/sleep")
        .arg("30")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let path = s.0.join("status.json");
    let bytes = json!({"pid":child.id(),"mode":"idle","runtime":"island-rust"}).to_string();
    fs::write(&path, &bytes).unwrap();
    let socket = UnixListener::bind(s.0.join("panel.sock")).unwrap();
    let refused = singleton::acquire(&s.0).is_err();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(refused);
    assert_eq!(fs::read_to_string(path).unwrap(), bytes);
    assert!(s.0.join("panel.sock").exists());
    drop(socket);
}

fn service(dir: &Path) -> SettingsService {
    SettingsService::new(
        dir.to_owned(),
        Autostart::new(
            dir.join("home"),
            dir.join("repo"),
            dir.join("repo/island-tauri/run.sh"),
            true,
        ),
    )
}
#[test]
fn settings_save_preserves_unknown_nested_keys_without_exposing_them_over_ipc() {
    let s = Scratch::new();
    let unknown = json!({"future":{"array":[null,1,"value"],"flag":true}});
    let mut seed = json!({"accent":"#123456","offsetX":17});
    seed["unknownKey"] = unknown.clone();
    fs::write(s.0.join("settings.json"), seed.to_string()).unwrap();
    let svc = service(&s.0);
    let next = svc
        .patch(&serde_json::from_value::<SettingsPatch>(json!({"showDate":false})).unwrap())
        .unwrap();
    let saved: Value =
        serde_json::from_slice(&fs::read(s.0.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["unknownKey"], unknown);
    assert_eq!(saved["accent"], "#123456");
    assert!(serde_json::to_value(&next)
        .unwrap()
        .get("unknownKey")
        .is_none());
    service(&s.0)
        .patch(&serde_json::from_value::<SettingsPatch>(json!({"offsetY":9})).unwrap())
        .unwrap();
    let saved: Value =
        serde_json::from_slice(&fs::read(s.0.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["unknownKey"], unknown);
}

// Compile the rollback runtime's ACTUAL sources read-only, not a recreated validator.
// Its own unit tests also execute against its scratch fixtures. No runtime is launched.
#[allow(dead_code,clippy::all)]
#[rustfmt::skip]
#[path="../../../island-rust/src/stats.rs"]
mod stats;
#[allow(dead_code, clippy::all)]
mod rollback {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../island-rust/src/services.rs"
    ));
    pub fn validated(raw: &serde_json::Value) -> serde_json::Value {
        let mut result = defaults();
        for (key, value) in raw.as_object().unwrap() {
            if let Ok(valid) = validate(key, value) {
                result[key] = valid;
            }
        }
        result
    }
}
fn roundtrip_seed(seed: &[u8]) {
    let s = Scratch::new();
    let raw: Value = serde_json::from_slice(seed).unwrap();
    assert!(raw.is_object());
    let before = rollback::validated(&raw);
    fs::write(s.0.join("settings.json"), seed).unwrap();
    let svc = service(&s.0);
    assert_eq!(svc.get().unwrap(), Settings::from_file_value(&raw));
    let toggled = !before["showDate"].as_bool().unwrap();
    svc.patch(&serde_json::from_value::<SettingsPatch>(json!({"showDate":toggled})).unwrap())
        .unwrap();
    let saved: Value =
        serde_json::from_slice(&fs::read(s.0.join("settings.json")).unwrap()).unwrap();
    let mut expected = before;
    expected["showDate"] = json!(toggled);
    assert_eq!(rollback::validated(&saved), expected);
}
#[test]
fn tauri_saved_settings_are_accepted_by_actual_island_rust_parser() {
    roundtrip_seed(br##"{"autostart":true,"showInTaskbar":false,"stealth":false,"clickSounds":false,"hoverDelay":180,"offsetX":17,"offsetY":-9,"accent":"","background":"","borderEnabled":true,"borderColor":"","panelClockColor":"","uiFont":"openrunde","showDate":true,"hardwareMonitor":true,"terminalFontSize":14}"##);
}
#[test]
fn copied_user_settings_roundtrip_through_both_actual_parsers() {
    // Supplied only by the 9a runner, after copying once to private scratch. A normal
    // whole-suite run stays independent of any real-user file or environment override.
    if let Some(path) = std::env::var_os("HI_9A_SETTINGS_SEED") {
        let path = PathBuf::from(path);
        assert!(path.starts_with(std::env::temp_dir()));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o400
        );
        roundtrip_seed(&fs::read(path).unwrap());
        println!("PASS: private read-only user settings copy loaded, Tauri saved, actual island-rust parser accepted every legacy value");
    }
}
