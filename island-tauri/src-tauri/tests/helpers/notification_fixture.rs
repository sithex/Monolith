#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use crate::{
    common::{stop_group, wait_for},
    gui_stack::Stack,
    host,
};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
pub struct Mock {
    pub child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
}
impl Mock {
    pub fn start(s: &Stack) -> Self {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/helpers/notification_mock.py");
        // This private bus has ONLY our mock service descriptor; no real activation dirs.
        fs::write(
            s.root
                .join("services/org.freedesktop.Notifications.service"),
            format!(
                "[D-BUS Service]\nName=org.freedesktop.Notifications\nExec=/usr/bin/python3 {}\n",
                script.display()
            ),
        )
        .unwrap();
        let mut child = Command::new("/usr/bin/python3")
            .arg(script)
            .env_clear()
            .envs(s.env())
            .env("HERMES_ISLAND_SCRATCH", &s.root)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(s.root.join("notification-mock.log")).unwrap())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let ready = output.recv_timeout(Duration::from_secs(5));
        assert!(
            ready.as_ref().is_ok_and(|s| s.starts_with("ready :")),
            "mock startup {ready:?}: {}",
            fs::read_to_string(s.root.join("notification-mock.log")).unwrap()
        );
        Self {
            child,
            input,
            output,
        }
    }
    pub fn send(&mut self, line: &str) {
        writeln!(self.input, "{line}").unwrap();
        assert_eq!(
            self.output.recv_timeout(Duration::from_secs(5)).unwrap(),
            "ok"
        );
    }
    pub fn quit(&mut self) {
        self.send("quit");
        wait_for("mock exits", Duration::from_secs(5), || {
            self.child.try_wait().unwrap().map(|_| ())
        });
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        stop_group(&mut self.child);
    }
}
pub fn notify(s: &Stack, app: &str, title: &str) {
    let result = Command::new("notify-send")
        .args(["-a", app, "-t", "0", title, "Private <b>body</b>"])
        .env_clear()
        .envs(s.env())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "notify-send: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
pub fn list(s: &Stack) -> Value {
    host::eval(s, "window.__TAURI_INTERNALS__.invoke('notifications_list')")
}
pub fn popups(s: &Stack) -> Value {
    host::eval(
        s,
        "window.__TAURI_INTERNALS__.invoke('notification_popup',{op:{type:'list'}}).then(p=>p.items)",
    )
}
pub fn report(s: &Stack) -> Value {
    host::eval(s,"window.__TAURI_INTERNALS__.invoke('island_bootstrap').then(b=>b.capabilities.notificationMonitor)")
}
pub fn ready(s: &Stack) {
    wait_for("monitor available", Duration::from_secs(10), || {
        let r = report(s);
        (r["available"] == true).then_some(())
    });
}
pub fn app(s: &Stack) -> host::Owned {
    host::prepare(s);
    fs::write(
        s.root.join("inst/settings.json"),
        r#"{"uiFont":"openrunde","systemNotificationMirror":true,"toastTtlMs":20000}"#,
    )
    .unwrap();
    let app = host::launch_ui(s, "island");
    wait_for("test control", Duration::from_secs(30), || {
        s.root.join("inst/test-control.sock").exists().then_some(())
    });
    // Notifications are popup cards in the Island renderer (NotificationCards.tsx), not
    // the legacy hover-preview toast.
    host::until(
        s,
        "island bootstrap",
        20,
        "!!document.querySelector('.island')",
    );
    app
}
/// Patch settings through the production command (the legacy `window.island` shim is absent).
pub fn set_settings(s: &Stack, patch: &str) {
    host::eval(
        s,
        &format!("window.__TAURI_INTERNALS__.invoke('settings_patch',{{patch:{patch}}})"),
    );
}
