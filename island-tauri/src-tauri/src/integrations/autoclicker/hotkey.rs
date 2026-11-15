//! The autoclicker's global hotkeys, registered with KGlobalAccel (`org.kde.kglobalaccel`,
//! component `monolith-autoclicker`): `toggle-autoclicker` while armed, `stop-autoclicker`
//! while clicking. One host per action. Each is unregistered when not wanted and when the
//! island quits, so a key is never swallowed while nothing would use it. A key another component owns is
//! never taken. KGlobalAccel reports both press and release, which is what hold mode needs.
use crate::integrations::dbus_util;
use crate::keyseq;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use zbus::Connection;

const ACCEL: &str = "org.kde.kglobalaccel";
const ACCEL_PATH: &str = "/kglobalaccel";
const ACCEL_IFACE: &str = "org.kde.KGlobalAccel";
const COMPONENT_IFACE: &str = "org.kde.kglobalaccel.Component";
pub const COMPONENT: &str = "monolith-autoclicker";
const COMPONENT_FRIENDLY: &str = "Monolith Autoclicker";

/// One KGlobalAccel action.
#[derive(Clone, Copy)]
pub struct Action {
    pub id: &'static str,
    pub friendly: &'static str,
}
pub const TOGGLE: Action = Action {
    id: "toggle-autoclicker",
    friendly: "Start / stop clicking",
};
pub const STOP: Action = Action {
    id: "stop-autoclicker",
    friendly: "Stop clicking (emergency)",
};
const SET_PRESENT: u32 = 2;
const NO_AUTOLOADING: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Pressed,
    Released,
}

pub type KeySink = Arc<dyn Fn(Key) + Send + Sync>;
/// `(state, detail)`, see `AutoclickerChanged::hotkey_state`.
pub type StatusSink = Arc<dyn Fn(&'static str, String) + Send + Sync>;

pub struct HotkeyHost {
    want: watch::Sender<Option<String>>,
    stop: watch::Sender<bool>,
    done: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl HotkeyHost {
    pub fn start(action: Action, on_key: KeySink, on_status: StatusSink) -> Self {
        let (want, want_rx) = watch::channel(None);
        let (stop, stop_rx) = watch::channel(false);
        let (done_tx, done) = std::sync::mpsc::channel();
        tauri::async_runtime::spawn(async move {
            run(action, want_rx, stop_rx, on_key, on_status).await;
            let _ = done_tx.send(());
        });
        Self {
            want,
            stop,
            done: std::sync::Mutex::new(Some(done)),
        }
    }

    /// `None` = disarmed (unregistered); `Some("")` = armed without a key.
    pub fn set(&self, want: Option<String>) {
        self.want.send_if_modified(|w| {
            let changed = *w != want;
            *w = want;
            changed
        });
    }

    /// Unregisters the key and waits for that (bounded).
    pub fn shutdown(&self) {
        self.stop.send_replace(true);
        if let Some(done) = self.done.lock().ok().and_then(|mut d| d.take()) {
            let _ = done.recv_timeout(Duration::from_millis(1500));
        }
    }
}

fn action_id(action: Action) -> Vec<&'static str> {
    vec![COMPONENT, action.id, COMPONENT_FRIENDLY, action.friendly]
}

fn owner_rule(name: &str) -> String {
    format!("type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{name}'")
}

fn key_rule(member: &str) -> String {
    format!("type='signal',interface='{COMPONENT_IFACE}',member='{member}',arg0='{COMPONENT}'")
}

async fn run(
    action: Action,
    mut want: watch::Receiver<Option<String>>,
    mut stop: watch::Receiver<bool>,
    on_key: KeySink,
    on_status: StatusSink,
) {
    let conn = tokio::time::timeout(
        Duration::from_secs(3),
        zbus::connection::Builder::session().build(),
    )
    .await
    .ok()
    .and_then(Result::ok);
    let Some(conn) = conn else {
        on_status("unavailable", "Can't reach the session D-Bus".into());
        let _ = stop.wait_for(|s| *s).await;
        return;
    };
    let streams = async {
        Ok::<_, crate::error::IslandError>((
            dbus_util::subscribe(&conn, &owner_rule(ACCEL)).await?,
            dbus_util::subscribe(&conn, &key_rule("globalShortcutPressed")).await?,
            dbus_util::subscribe(&conn, &key_rule("globalShortcutReleased")).await?,
        ))
    }
    .await;
    let Ok((mut owner, mut pressed, mut released)) = streams else {
        on_status("unavailable", "Can't listen for KDE shortcuts".into());
        let _ = stop.wait_for(|s| *s).await;
        return;
    };
    let mut accel = dbus_util::name_owner(&conn, ACCEL).await.ok().flatten();
    let mut registered = false;
    let mut current = want.borrow_and_update().clone();
    apply(&conn, action, &accel, &current, &mut registered, &on_status).await;
    loop {
        tokio::select! {
            changed = stop.changed() => if changed.is_err() || *stop.borrow() { break },
            changed = want.changed() => {
                if changed.is_err() { break; }
                current = want.borrow_and_update().clone();
                apply(&conn, action, &accel, &current, &mut registered, &on_status).await;
            }
            msg = dbus_util::next(&mut owner) => match msg {
                Some(Ok(msg)) => {
                    if msg.header().sender().map(|s| s.as_str()) == Some(dbus_util::DBUS) {
                        if let Ok((_, _, new)) = msg.body().deserialize::<(String, String, String)>() {
                            accel = (!new.is_empty()).then_some(new);
                            registered = false;
                            apply(&conn, action, &accel, &current, &mut registered, &on_status).await;
                        }
                    }
                }
                _ => break,
            },
            msg = dbus_util::next(&mut pressed) => match msg {
                Some(Ok(msg)) => if is_ours(&msg, action, &accel) { on_key(Key::Pressed) },
                _ => break,
            },
            msg = dbus_util::next(&mut released) => match msg {
                Some(Ok(msg)) => if is_ours(&msg, action, &accel) { on_key(Key::Released) },
                _ => break,
            },
        }
    }
    if registered && accel.is_some() {
        let _ = unregister(&conn, action).await;
    }
}

fn is_ours(msg: &zbus::Message, action: Action, accel: &Option<String>) -> bool {
    let header = msg.header();
    let from_accel =
        matches!((header.sender(), accel), (Some(s), Some(owner)) if s.as_str() == owner);
    from_accel
        && msg.body().len() <= dbus_util::MAX_SIGNAL_BYTES
        && msg
            .body()
            .deserialize::<(String, String, i64)>()
            .is_ok_and(|(c, a, _)| c == COMPONENT && a == action.id)
}

async fn unregister(conn: &Connection, action: Action) -> Result<bool, crate::error::IslandError> {
    dbus_util::call(
        conn,
        ACCEL,
        ACCEL_PATH,
        ACCEL_IFACE,
        "unregister",
        &(COMPONENT, action.id),
    )
    .await
}

type Keys = Vec<(Vec<i32>,)>;
/// KGlobalShortcutInfo: unique, friendly, componentUnique, componentFriendly, contextUnique,
/// contextFriendly, keys, defaultKeys.
type ShortcutInfo = (
    String,
    String,
    String,
    String,
    String,
    String,
    Vec<i32>,
    Vec<i32>,
);

async fn owners(conn: &Connection, code: i32) -> Vec<String> {
    let found: Result<Vec<ShortcutInfo>, _> = dbus_util::call(
        conn,
        ACCEL,
        ACCEL_PATH,
        ACCEL_IFACE,
        "globalShortcutsByKey",
        &((vec![code],), (0i32,)),
    )
    .await;
    found
        .map(|list| {
            list.into_iter()
                .filter(|s| s.2 != COMPONENT)
                .map(|s| format!("{} / {}", s.3, s.1))
                .collect()
        })
        .unwrap_or_default()
}

async fn apply(
    conn: &Connection,
    action: Action,
    accel: &Option<String>,
    want: &Option<String>,
    registered: &mut bool,
    on_status: &StatusSink,
) {
    if accel.is_none() {
        *registered = false;
        on_status("unavailable", "KDE global shortcuts aren't running".into());
        return;
    }
    let text = match want.as_deref().map(str::trim) {
        None | Some("") => {
            if *registered {
                let _ = unregister(conn, action).await;
                *registered = false;
            }
            match want {
                None => on_status("off", String::new()),
                Some(_) => on_status("none", "No hotkey set".into()),
            }
            return;
        }
        Some(text) => text.to_owned(),
    };
    let code = match keyseq::parse(&text) {
        Ok(Some(code)) => code,
        _ => {
            on_status("invalid", format!("KDE can't bind {text}"));
            return;
        }
    };
    let id = action_id(action);
    let result = async {
        let () =
            dbus_util::call(conn, ACCEL, ACCEL_PATH, ACCEL_IFACE, "doRegister", &(&id,)).await?;
        *registered = true;
        let current: Keys = dbus_util::call(
            conn,
            ACCEL,
            ACCEL_PATH,
            ACCEL_IFACE,
            "shortcutKeys",
            &(&id,),
        )
        .await?;
        let ours = current.iter().any(|k| k.0.first() == Some(&code));
        let free: bool = ours
            || dbus_util::call(
                conn,
                ACCEL,
                ACCEL_PATH,
                ACCEL_IFACE,
                "globalShortcutAvailable",
                &((vec![code],), COMPONENT),
            )
            .await?;
        if !free {
            let _: Keys = dbus_util::call(
                conn,
                ACCEL,
                ACCEL_PATH,
                ACCEL_IFACE,
                "setShortcutKeys",
                &(&id, Keys::new(), SET_PRESENT | NO_AUTOLOADING),
            )
            .await?;
            return Ok::<_, crate::error::IslandError>(false);
        }
        let set: Keys = dbus_util::call(
            conn,
            ACCEL,
            ACCEL_PATH,
            ACCEL_IFACE,
            "setShortcutKeys",
            &(&id, vec![(vec![code],)], SET_PRESENT | NO_AUTOLOADING),
        )
        .await?;
        Ok(set.iter().any(|k| k.0.first() == Some(&code)))
    }
    .await;
    match result {
        Ok(true) => on_status("bound", format!("{text} is registered")),
        Ok(false) => {
            let who = owners(conn, code).await;
            let detail = if who.is_empty() {
                format!("{text} is already used by another shortcut")
            } else {
                format!("{text} is already used by {}", who.join(", "))
            };
            tracing::warn!(%detail, "Autoclicker hotkey left unbound");
            on_status("conflict", detail);
        }
        Err(error) => {
            tracing::warn!(%error, "Autoclicker hotkey registration failed");
            on_status("unavailable", "KDE didn't accept the hotkey".into());
        }
    }
}
