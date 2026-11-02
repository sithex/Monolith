//! BlueZ over zbus: `GetManagedObjects` + ObjectManager and Properties signals.
//!
//! Subscribe first, read second. Signals are accepted ONLY from the current `org.bluez`
//! owner's unique name, ObjectManager signals only from the root object `/`, and every object
//! path must live under `/org/bluez`. The match rules are deliberately broad (interface/arg0
//! only) so these in-code guards are the sole sender/path defense; each is pinned by a
//! rogue-sender / wrong-path test.

use super::BluetoothService;
use crate::error::IslandError;
use crate::integrations::dbus_util::{
    call, connect, name_owner, next, props_to_json, subscribe, transport, DBUS, MAX_SIGNAL_BYTES,
};
use serde_json::{Map, Value as Json};
use std::{collections::HashMap, future::poll_fn, pin::Pin, sync::Arc, task::Poll, time::Duration};
use tokio::sync::watch;
use zbus::{
    export::futures_core::Stream, Connection, Message, MessageStream, OwnedObjectPath,
    OwnedVariant as OwnedValue,
};

pub const NAME: &str = "org.bluez";
const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

type Interfaces = HashMap<String, HashMap<String, OwnedValue>>;

/// Object-path guard: BlueZ objects live at or below `/org/bluez`.
fn bluez_path(path: &str) -> bool {
    path == "/org/bluez" || path.starts_with("/org/bluez/")
}

fn interfaces_json(interfaces: &Interfaces) -> Json {
    Json::Object(
        interfaces
            .iter()
            .map(|(name, props)| (name.clone(), Json::Object(props_to_json(props))))
            .collect(),
    )
}

/// Poll several streams fairly (rotating start index); `None` when any stream ended.
async fn next_of(
    streams: &mut [MessageStream],
    cursor: &mut usize,
) -> Option<zbus::Result<Message>> {
    poll_fn(|cx| {
        let count = streams.len();
        for offset in 0..count {
            let index = (*cursor + offset) % count;
            match Pin::new(&mut streams[index]).poll_next(cx) {
                Poll::Ready(item) => {
                    *cursor = (index + 1) % count;
                    return Poll::Ready(item);
                }
                Poll::Pending => {}
            }
        }
        Poll::Pending
    })
    .await
}

pub(super) async fn run(
    service: Arc<BluetoothService>,
    address: Option<String>,
    mut stop: watch::Receiver<bool>,
) {
    let Some(address) = address else {
        service.status.unavailable("no-system-bus-configured");
        return;
    };
    let mut backoff = 1u64;
    loop {
        if *stop.borrow() {
            break;
        }
        let result = tokio::select! {
            _ = stop.changed() => break,
            result = session(&service, &address) => result,
        };
        service.lost();
        match result {
            Ok(()) => backoff = 1,
            Err(error) => {
                tracing::warn!(%error, "BlueZ session ended");
                service.status.unavailable("bus-unavailable");
            }
        }
        tokio::select! {
            _ = stop.changed() => break,
            () = tokio::time::sleep(Duration::from_secs(backoff)) => {}
        }
        backoff = (backoff * 2).min(60);
    }
    service.lost();
    service.status.unavailable("stopped");
}

async fn load(
    service: &BluetoothService,
    conn: &Connection,
    owner: &str,
) -> Result<(), IslandError> {
    let objects: HashMap<OwnedObjectPath, Interfaces> = call(
        conn,
        owner,
        "/",
        IFACE_OBJECT_MANAGER,
        "GetManagedObjects",
        &(),
    )
    .await?;
    let mut json = Map::new();
    for (path, interfaces) in &objects {
        if bluez_path(path.as_str()) {
            json.insert(path.as_str().to_owned(), interfaces_json(interfaces));
        }
    }
    service.load(&Json::Object(json));
    service.status.connected();
    Ok(())
}

async fn session(service: &BluetoothService, address: &str) -> Result<(), IslandError> {
    let conn = connect(address).await?;
    let mut names = subscribe(
        &conn,
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.bluez'",
    )
    .await?;
    let mut signals = vec![
        subscribe(
            &conn,
            "type='signal',interface='org.freedesktop.DBus.ObjectManager'",
        )
        .await?,
    ];
    for interface in [
        "org.bluez.Device1",
        "org.bluez.Battery1",
        "org.bluez.Adapter1",
    ] {
        signals.push(
            subscribe(
                &conn,
                &format!(
                    "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='{interface}'"
                ),
            )
            .await?,
        );
    }
    let mut owner = name_owner(&conn, NAME).await?;
    match owner.as_deref() {
        Some(unique) => load(service, &conn, unique).await?,
        None => service.status.unavailable("no-owner"),
    }
    let mut cursor = 0usize;
    loop {
        tokio::select! {
            message = next(&mut names) => {
                let message = message.ok_or_else(|| IslandError::unavailable("owner stream ended"))?.map_err(transport)?;
                if message.header().sender().map(|s| s.as_str()) != Some(DBUS) {
                    continue;
                }
                let Ok((name, _old, new)) = message.body().deserialize::<(String, String, String)>() else {
                    continue;
                };
                if name != NAME {
                    continue;
                }
                if new.is_empty() {
                    owner = None;
                    service.lost();
                    service.status.unavailable("name-lost");
                } else {
                    owner = Some(new.clone());
                    load(service, &conn, &new).await?;
                }
            }
            message = next_of(&mut signals, &mut cursor) => {
                let message = message.ok_or_else(|| IslandError::unavailable("signal stream ended"))?.map_err(transport)?;
                handle(service, owner.as_deref(), &message);
            }
        }
    }
}

fn handle(service: &BluetoothService, owner: Option<&str>, message: &Message) {
    let Some(owner) = owner else {
        return;
    };
    if message.data().len() > MAX_SIGNAL_BYTES {
        return;
    }
    let header = message.header();
    // Sender guard: only the current org.bluez owner may change the model.
    if header.sender().map(|s| s.as_str()) != Some(owner) {
        return;
    }
    let path = header.path().map(|p| p.as_str()).unwrap_or("");
    let interface = header.interface().map(|i| i.as_str()).unwrap_or("");
    let member = header.member().map(|m| m.as_str()).unwrap_or("");
    match (interface, member) {
        (IFACE_OBJECT_MANAGER, "InterfacesAdded") => {
            // Root-object guard: the ObjectManager lives at `/`.
            if path != "/" {
                return;
            }
            let Ok((object, interfaces)) = message
                .body()
                .deserialize::<(OwnedObjectPath, Interfaces)>()
            else {
                return;
            };
            if bluez_path(object.as_str()) {
                service.interfaces_added(object.as_str(), &interfaces_json(&interfaces));
            }
        }
        (IFACE_OBJECT_MANAGER, "InterfacesRemoved") => {
            if path != "/" {
                return;
            }
            let Ok((object, names)) = message
                .body()
                .deserialize::<(OwnedObjectPath, Vec<String>)>()
            else {
                return;
            };
            if bluez_path(object.as_str()) {
                service.interfaces_removed(object.as_str(), &names);
            }
        }
        (PROPERTIES, "PropertiesChanged") => {
            let Ok((target, changed, invalidated)) =
                message
                    .body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                return;
            };
            // Object-path guard: the signal's path IS the object.
            if bluez_path(path) {
                service.properties_changed(path, &target, &props_to_json(&changed), &invalidated);
            }
        }
        _ => {}
    }
}
