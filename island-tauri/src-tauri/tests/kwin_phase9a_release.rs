//! The production launcher and binary on nested KWin. Every launch is test-only.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "helpers/gui_stack.rs"]
#[allow(dead_code)]
mod gui_stack;
#[path = "helpers/phase3_host.rs"]
mod host;
use common::*;
use gui_stack::Stack;
use host::{actor_state, client, eval, prepare, status, until, Owned, Wheel};
use serde_json::json;
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection, protocol::xproto::ConnectionExt as _, rust_connection::RustConnection,
};

fn settled(s: &Stack, c: &RustConnection, w: u32, canonical: &str, legacy: &str) {
    let mut last = None;
    let mut since = Instant::now();
    wait_for(
        &format!("9a {canonical}: actor DOM status and stable native region"),
        Duration::from_secs(15),
        || {
            let shape = input_region(c, w);
            if last.as_ref() != Some(&shape) {
                last = Some(shape.clone());
                since = Instant::now();
                return None;
            }
            let state = eval(s, "document.querySelector('.island')?.dataset.state");
            let file = status(s);
            if canonical == "still" {
                if !shape.is_empty() {
                    return None;
                }
            } else if shape.is_empty() {
                return None;
            }
            (state == canonical
                && file["mode"] == legacy
                && actor_state(s)["mode"] == canonical
                && since.elapsed() >= Duration::from_millis(250))
            .then_some(())
        },
    );
}
fn click(s: &Stack, e: &mut Wheel, origin: (i32, i32), selector: &str) {
    let point=eval(s,&format!("(()=>{{const e=document.querySelector({}),r=e.getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()",json!(selector)));
    e.click_at(
        origin.0 + point[0].as_f64().unwrap() as i32,
        origin.1 + point[1].as_f64().unwrap() as i32,
    );
}
#[test]
fn anchor_client_complete_status_embedded_island_and_sigterm_use_real_eis() {
    let mut s = Stack::new("9a-release");
    prepare(&s);
    let seed = std::env::var_os("HI_9A_SETTINGS_SEED").map(|p| {
        let p = std::path::PathBuf::from(p);
        assert!(p.starts_with(std::env::temp_dir()));
        let bytes = fs::read(p).unwrap();
        fs::write(s.root.join("inst/settings.json"), &bytes).unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
    });
    // The REAL script's launch operation must see an online Tauri, never start a rollback app.
    fs::create_dir(s.root.join("replica/island-rust")).unwrap();
    let trap = s.root.join("replica/island-rust/run.sh");
    fs::write(
        &trap,
        format!(
            "#!/bin/sh\nprintf wrong > '{}'\n",
            s.root.join("unexpected-launch").display()
        ),
    )
    .unwrap();
    fs::set_permissions(trap, fs::Permissions::from_mode(0o700)).unwrap();
    let release =
        std::env::var_os("HI_9A_RELEASE_SMOKE").as_deref() == Some(std::ffi::OsStr::new("1"));
    let program = if release {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../run.sh")
    } else {
        Path::new(env!("CARGO_BIN_EXE_hermes-island")).to_path_buf()
    };
    let log = fs::File::create(s.root.join("app.log")).unwrap();
    let mut command = Command::new(program);
    command
        .args([
            "--overlay",
            "--test-instance",
            "--fixture-pty",
            "--data-dir",
        ])
        .arg(s.root.join("inst"));
    if !release {
        command.args(["--ui", "island"]);
    }
    let mut app = Owned(
        command
            .env_clear()
            .envs(s.env())
            .env("DISPLAY", &s.display)
            .env("GDK_BACKEND", "x11")
            .env("HERMES_ISLAND_TEST", "1")
            .env("NO_AT_BRIDGE", "1")
            .env("GTK_USE_PORTAL", "0")
            .env("GIO_USE_PORTAL", "0")
            .env("HERMES_ISLAND_WEATHER_FORECAST_BASE", "http://127.0.0.1:1")
            .env("HERMES_ISLAND_WEATHER_GEOCODE_BASE", "http://127.0.0.1:1")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", s.root.join("bin").display()),
            )
            .process_group(0)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let pid = app.0.id();
    let c = s.connect();
    let root = c.setup().roots[0].root;
    let w = wait_for("9a mapped window", Duration::from_secs(40), || {
        find_window_sized(&c, root, pid, host::overlay_size())
    });
    wait_for("9a private probe", Duration::from_secs(20), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    until(
        &s,
        "9a embedded island mounted",
        20,
        "!!document.querySelector('.island')",
    );
    assert_eq!(eval(&s, "typeof window.island"), "undefined");
    if let Some(seed) = seed {
        let expected = hermes_island_lib::contract::settings::Settings::from_file_value(&seed);
        let loaded = eval(&s, "window.__TAURI_INTERNALS__.invoke('settings_get')");
        // JS has one number type: 180.0 crosses the probe as 180. Compare the
        // generated contract's typed values, not serde_json's integer/float tags.
        let loaded: hermes_island_lib::contract::settings::Settings =
            serde_json::from_value(loaded).unwrap();
        assert_eq!(loaded, expected);
        // Load EXACT copied settings first, then use deterministic geometry for the
        // private physical-input scenario. This patch touches only the scratch instance.
        eval(&s, "window.__TAURI_INTERNALS__.invoke('settings_patch',{patch:{hoverDelay:180,offsetX:0,offsetY:0,showDate:false}})");
        let saved = fs::read(s.root.join("inst/settings.json")).unwrap();
        if let Some(path) = std::env::var_os("HI_9A_SAVED_COPY") {
            let path = std::path::PathBuf::from(path);
            assert!(path.starts_with(std::env::temp_dir()));
            fs::write(&path, saved).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o400)).unwrap();
        }
        println!(
            "PASS: release test instance loaded exact user settings COPY and saved via real IPC"
        );
    }
    let url = eval(&s, "location.href");
    assert!(url.as_str().unwrap().contains("island.html"));
    assert!(
        !url.as_str().unwrap().starts_with("http://localhost:5173"),
        "devUrl rather than embedded assets: {url}"
    );
    if release {
        assert_eq!(
            fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/hermes-island")
        );
    }
    assert_identity(&c, w, pid, true);
    settled(&s, &c, w, "still", "idle");
    let file = status(&s);
    let reply = client(&s, "status");
    for v in [&file, &reply] {
        for key in [
            "pid",
            "updated",
            "mode",
            "bounds",
            "rectangles",
            "anchor",
            "windowId",
            "pty",
            "online",
        ] {
            assert!(v.get(key).is_some(), "anchor protocol key missing: {key}");
        }
        assert_eq!(v["pid"], pid);
        assert_eq!(v["online"], true);
        assert_eq!(v["anchor"]["native"], true);
    }
    let p = c
        .translate_coordinates(w, root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    let origin = (i32::from(p.dst_x), i32::from(p.dst_y));
    let mut e = Wheel::new(&s);
    e.move_to(100, 900);
    client(&s, "enter");
    settled(&s, &c, w, "hover", "preview");
    // Physical hover crossing: no synthetic DOM events and no renderer timers.
    let r=eval(&s,"(()=>{const r=document.querySelector('.island').getBoundingClientRect();return [r.x+100,r.y+80]})()");
    e.move_to(
        origin.0 + r[0].as_f64().unwrap() as i32,
        origin.1 + r[1].as_f64().unwrap() as i32,
    );
    client(&s, "leave");
    e.move_to(100, 900);
    settled(&s, &c, w, "still", "idle");
    client(&s, "enter");
    settled(&s, &c, w, "hover", "preview");
    click(&s, &mut e, origin, "[data-testid=open]");
    settled(&s, &c, w, "large", "pinned");
    until(
        &s,
        "9a fixture bash, never Hermes",
        20,
        "window.__islandTerminalTest?.text().includes('island-test>')===true",
    );
    let pty = status(&s)["pty"].as_u64().unwrap();
    assert!(pty > 0);
    click(&s, &mut e, origin, "[data-testid=collapse]");
    settled(&s, &c, w, "still", "idle");
    client(&s, "launch");
    assert!(
        !s.root.join("unexpected-launch").exists(),
        "real anchor-client online() rejected Tauri status"
    );
    settled(&s, &c, w, "large", "pinned");
    assert_eq!(status(&s)["pty"], pty);
    click(&s, &mut e, origin, "[data-testid=collapse]");
    settled(&s, &c, w, "still", "idle");
    // Signal only our unreaped child. A graceful code 0 and cleaned artifacts are mandatory.
    assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
    let exit = wait_for("9a clean SIGTERM exit", Duration::from_secs(12), || {
        app.0.try_wait().unwrap()
    });
    assert!(exit.success(), "{exit}");
    for name in [
        "panel.sock",
        "status.json",
        "ingest.sock",
        "test-control.sock",
    ] {
        assert!(!s.root.join("inst").join(name).exists(), "leftover {name}");
    }
    wait_for(
        "9a owned fixture PTY reaped",
        Duration::from_secs(5),
        || (!Path::new(&format!("/proc/{pty}")).exists()).then_some(()),
    );
    fs::write(Evidence::new("phase9a").path(if release {"release-smoke.json"} else {"debug-smoke.json"}),json!({"release":release,"url":url,"pid":pid,"pty":pty,"windowId":w,"realEis":true,"anchorClient":"byte-identical","sigtermCode":exit.code(),"clean":true}).to_string()).unwrap();
    drop(e);
    drop(app);
    s.shutdown();
    drop(s);
}
