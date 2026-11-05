//! Game mode worker against a private bus with in-process fakes of KWin scripting and
//! KGlobalAccel. Never touches the real session: no KWin script reaches the live compositor
//! and no real global shortcut is registered.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::private_bus::{wait_until, PrivateBus};
use hermes_island_lib::{
    integrations::plasma::gamemode::{
        Desired, Event, GameModeHost, ShortcutStatus, Status, ACCEL, BUS_NAME, COMPONENT,
        INTERFACE, KWIN, OBJECT_PATH,
    },
    keyseq,
};
use std::{
    collections::BTreeMap,
    os::unix::fs::DirBuilderExt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;
use zbus::Connection;

type Log = Arc<Mutex<Vec<String>>>;

struct FakeScripting {
    log: Log,
    refuse: bool,
}

#[zbus::interface(name = "org.kde.kwin.Scripting")]
impl FakeScripting {
    #[zbus(name = "unloadScript")]
    async fn unload_script(&self, name: String) -> bool {
        self.log.lock().unwrap().push(format!("unload {name}"));
        true
    }
    #[zbus(name = "loadDeclarativeScript")]
    async fn load_declarative_script(&self, path: String, name: String) -> i32 {
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            body.contains("onWindowActivated"),
            "a real script file is passed"
        );
        self.log.lock().unwrap().push(format!("load {name}"));
        if self.refuse {
            -1
        } else {
            7
        }
    }
}

struct FakeScript {
    log: Log,
}

#[zbus::interface(name = "org.kde.kwin.Script")]
impl FakeScript {
    #[zbus(name = "run")]
    async fn run(&self) {
        self.log.lock().unwrap().push("run".into());
    }
}

async fn fake_kwin(address: &str, log: Log, refuse: bool) -> Connection {
    zbus::connection::Builder::address(address)
        .name(KWIN)
        .serve_at(
            "/Scripting",
            FakeScripting {
                log: log.clone(),
                refuse,
            },
        )
        .serve_at("/Scripting/Script7", FakeScript { log })
        .build()
        .await
        .unwrap()
}

type Key = (Vec<i32>,);
type Info = (
    String,
    String,
    String,
    String,
    String,
    String,
    Vec<i32>,
    Vec<i32>,
);

struct FakeAccel {
    log: Log,
    /// key code → (component, friendly component, action friendly)
    taken: BTreeMap<i32, (String, String, String)>,
    ours: Mutex<Vec<Key>>,
}

#[zbus::interface(name = "org.kde.KGlobalAccel")]
impl FakeAccel {
    #[zbus(name = "doRegister")]
    async fn do_register(&self, id: Vec<String>) {
        assert_eq!(id[0], COMPONENT);
        self.log.lock().unwrap().push(format!("register {}", id[1]));
    }
    #[zbus(name = "shortcutKeys")]
    async fn shortcut_keys(&self, _id: Vec<String>) -> Vec<Key> {
        self.ours.lock().unwrap().clone()
    }
    #[zbus(name = "globalShortcutAvailable")]
    async fn global_shortcut_available(&self, key: Key, component: String) -> bool {
        assert_eq!(component, COMPONENT);
        !self.taken.contains_key(&key.0[0])
    }
    #[zbus(name = "globalShortcutsByKey")]
    async fn global_shortcuts_by_key(&self, key: Key, _match: (i32,)) -> Vec<Info> {
        self.taken
            .get(&key.0[0])
            .map(|(c, cf, af)| {
                vec![(
                    "x".into(),
                    af.clone(),
                    c.clone(),
                    cf.clone(),
                    "default".into(),
                    "Default".into(),
                    key.0.clone(),
                    key.0.clone(),
                )]
            })
            .unwrap_or_default()
    }
    #[zbus(name = "setShortcutKeys")]
    async fn set_shortcut_keys(&self, _id: Vec<String>, keys: Vec<Key>, flags: u32) -> Vec<Key> {
        // Like kglobalaccel: keys owned by another component are silently dropped.
        let kept: Vec<Key> = keys
            .into_iter()
            .filter(|k| !self.taken.contains_key(&k.0[0]))
            .collect();
        self.log
            .lock()
            .unwrap()
            .push(format!("set {:?} flags {flags}", kept));
        *self.ours.lock().unwrap() = kept.clone();
        kept
    }
    #[zbus(name = "unregister")]
    async fn unregister(&self, component: String, action: String) -> bool {
        self.log
            .lock()
            .unwrap()
            .push(format!("unregister {component} {action}"));
        self.ours.lock().unwrap().clear();
        true
    }
    #[zbus(name = "setInactive")]
    async fn set_inactive(&self, _id: Vec<String>) {
        self.log.lock().unwrap().push("inactive".into());
    }
}

async fn fake_accel(address: &str, log: Log) -> Connection {
    let meta_backtick = keyseq::parse("Meta+`").unwrap().unwrap();
    let taken = BTreeMap::from([(
        meta_backtick,
        (
            "kwin".to_owned(),
            "KWin".to_owned(),
            "Walk Through Windows of Current Application".to_owned(),
        ),
    )]);
    zbus::connection::Builder::address(address)
        .name(ACCEL)
        .serve_at(
            "/kglobalaccel",
            FakeAccel {
                log,
                taken,
                ours: Mutex::new(Vec::new()),
            },
        )
        .build()
        .await
        .unwrap()
}

async fn press(conn: &Connection, action: &str) {
    conn.emit_signal(
        None::<&str>,
        "/component/hermes_island",
        "org.kde.kglobalaccel.Component",
        "globalShortcutPressed",
        &(COMPONENT, action, 1234i64),
    )
    .await
    .unwrap();
}

fn wait_status(host: &GameModeHost, what: &str, ok: impl Fn(&Status) -> bool) -> Status {
    let status = host.status();
    wait_until(what, Duration::from_secs(5), || {
        let s = status.borrow().clone();
        ok(&s).then_some(s)
    })
}

fn wait_event(events: &Arc<Mutex<Vec<Event>>>, what: &str, event: Event) {
    wait_until(what, Duration::from_secs(5), || {
        events.lock().unwrap().contains(&event).then_some(())
    });
}

fn count(log: &Log, prefix: &str) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .filter(|l| l.starts_with(prefix))
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gamemode_detection_shortcut_conflict_toggle_and_kwin_restart() {
    let bus = PrivateBus::start("gamemode");
    let address = bus.address();
    let dir = bus.root.join("run/monolith");
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let kwin_log: Log = Default::default();
    let accel_log: Log = Default::default();
    let kwin = fake_kwin(&address, kwin_log.clone(), false).await;
    let accel = fake_accel(&address, accel_log.clone()).await;

    let events: Arc<Mutex<Vec<Event>>> = Default::default();
    let sink_events = events.clone();
    let (settings, rx) = watch::channel(Desired {
        auto: true,
        shortcut: "Meta+`".into(),
    });
    let host = GameModeHost::start(
        Some(&address),
        dir.clone(),
        rx,
        Arc::new(move |e| sink_events.lock().unwrap().push(e)),
    )
    .await
    .unwrap();

    // The focus script is loaded and started; Meta+` belongs to KWin, so it is NOT taken.
    let status = wait_status(&host, "script + shortcut applied", |s| {
        s.script_loaded && s.shortcut != ShortcutStatus::Pending
    });
    assert_eq!(
        status.shortcut,
        ShortcutStatus::Conflict {
            key: "Meta+`".into(),
            owners: vec!["KWin / Walk Through Windows of Current Application".into()],
        }
    );
    assert_eq!(
        *kwin_log.lock().unwrap(),
        vec![
            "unload monolith-gamemode-focus",
            "load monolith-gamemode-focus",
            "run"
        ]
    );
    assert!(accel_log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l == "set [] flags 2"));

    // The script's call reaches the island as a fullscreen event.
    let client = zbus::connection::Builder::address(address.as_str())
        .build()
        .await
        .unwrap();
    client
        .call_method(
            Some(BUS_NAME),
            OBJECT_PATH,
            Some(INTERFACE),
            "Fullscreen",
            &(true,),
        )
        .await
        .unwrap();
    wait_event(&events, "fullscreen report", Event::Fullscreen(true));

    // A free key is bound (no autoloading: the setting is the authority) and a press toggles.
    settings.send_replace(Desired {
        auto: true,
        shortcut: "Ctrl+Alt+H".into(),
    });
    wait_status(&host, "free key bound", |s| {
        s.shortcut == ShortcutStatus::Bound("Ctrl+Alt+H".into())
    });
    let code = keyseq::parse("Ctrl+Alt+H").unwrap().unwrap();
    assert!(accel_log
        .lock()
        .unwrap()
        .iter()
        .any(|l| *l == format!("set [([{code}],)] flags 6")));
    // A spoofed press from another bus client is ignored; the real one toggles.
    press(&client, "toggle-island").await;
    press(&accel, "some-other-action").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!events.lock().unwrap().contains(&Event::Toggle));
    press(&accel, "toggle-island").await;
    wait_event(&events, "toggle press", Event::Toggle);

    // KWin crashes: the fullscreen flag is cleared; when it is back the script is reloaded once.
    events.lock().unwrap().clear();
    drop(kwin);
    wait_event(
        &events,
        "flag cleared on KWin loss",
        Event::Fullscreen(false),
    );
    let kwin = fake_kwin(&address, kwin_log.clone(), false).await;
    wait_until("script reloaded", Duration::from_secs(5), || {
        (count(&kwin_log, "load") == 2 && count(&kwin_log, "run") == 2).then_some(())
    });
    drop(kwin);
    // A KWin that refuses the script: one attempt, logged, no retry loop.
    let refusing = fake_kwin(&address, kwin_log.clone(), true).await;
    wait_status(&host, "refused load reported", |s| !s.script_loaded);
    wait_until("one refused attempt", Duration::from_secs(5), || {
        (count(&kwin_log, "load") == 3).then_some(())
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(count(&kwin_log, "load"), 3, "no retry spin after a failure");
    drop(refusing);
    let kwin = fake_kwin(&address, kwin_log.clone(), false).await;
    wait_status(&host, "loaded again", |s| s.script_loaded);

    // Auto off: the script is unloaded. Empty shortcut: the action is unregistered.
    let unloads = count(&kwin_log, "unload");
    settings.send_replace(Desired {
        auto: false,
        shortcut: String::new(),
    });
    wait_status(&host, "auto off + unbound", |s| {
        !s.script_loaded && s.shortcut == ShortcutStatus::Unbound
    });
    assert_eq!(count(&kwin_log, "unload"), unloads + 1);
    assert!(accel_log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l == "unregister hermes-island toggle-island"));

    host.shutdown().await;
    drop((kwin, accel, client));
}
