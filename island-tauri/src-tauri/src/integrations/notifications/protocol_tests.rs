//! Wire tests only on a fresh private daemon. No ambient session address is consulted.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::{
    linux_monitor::{invoke_on, NAME, PATH},
    takeover,
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};
use zbus::{Connection, OwnedVariant};
type Log = Arc<Mutex<Vec<String>>>;
struct Bus(Child, String);
impl Bus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        assert!(address.starts_with("unix:"));
        Self(child, address.trim().into())
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Server(Log);
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn inhibit(&self, app: &str, reason: &str, hints: BTreeMap<String, OwnedVariant>) -> u32 {
        assert_eq!(app, "hermes-island");
        assert!(!reason.is_empty());
        assert!(hints.is_empty());
        self.0.lock().unwrap().push("inhibit".into());
        47
    }
    #[zbus(name = "UnInhibit")]
    fn un_inhibit(&self, cookie: u32) {
        self.0.lock().unwrap().push(format!("release {cookie}"));
    }
}
struct Manager(Log);
#[zbus::interface(name = "org.kde.NotificationManager")]
impl Manager {
    fn invoke_action(&self, id: u32, key: &str) {
        self.0.lock().unwrap().push(format!("invoke {id} {key}"));
    }
}
#[tokio::test]
async fn private_bus_cookie_lifetime_and_action_wire_match_plasma() {
    let bus = Bus::new();
    let log = Log::default();
    let server = zbus::connection::Builder::address(bus.1.as_str())
        .name(NAME)
        .serve_at(PATH, Server(log.clone()))
        .serve_at(PATH, Manager(log.clone()))
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(bus.1.as_str())
        .build()
        .await
        .unwrap();
    let cookie = takeover::acquire(&client).await.unwrap();
    assert_eq!(cookie, 47);
    // A unique destination prevents a replacement server receiving an old notification id.
    let owner = server.unique_name().unwrap().as_str();
    invoke_on(&client, owner, 123, "open-mail").await.unwrap();
    takeover::release(&client, cookie).await;
    assert_eq!(
        *log.lock().unwrap(),
        vec!["inhibit", "invoke 123 open-mail", "release 47"]
    );
    assert!(invoke_on(&client, ":1.99999", 123, "open-mail")
        .await
        .is_err());
}
#[tokio::test]
async fn unsupported_inhibitor_fails_without_changing_the_server() {
    let bus = Bus::new();
    let log = Log::default();
    let _server: Connection = zbus::connection::Builder::address(bus.1.as_str())
        .name(NAME)
        .serve_at(PATH, Manager(log.clone()))
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(bus.1.as_str())
        .build()
        .await
        .unwrap();
    assert!(takeover::acquire(&client).await.is_err());
    assert!(log.lock().unwrap().is_empty());
}
