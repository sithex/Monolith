//! UPower over zbus: `DisplayDevice` properties (+ root `OnBattery`) with owner tracking.
//!
//! Subscribe first, read second, then process queued signals in order. Signals are accepted
//! ONLY from the current owner's unique name AND only for the two modeled object paths: the
//! match rules are deliberately broad (interface/member/arg0) so these in-code guards are the
//! sole sender/path defense, and each is pinned by a rogue-sender / wrong-path test.

use super::PowerService;
use crate::error::IslandError;
use crate::integrations::dbus_util::{
    call, connect, name_owner, next, props_to_json, subscribe, transport, value_to_json, DBUS,
    MAX_SIGNAL_BYTES,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::watch;
use zbus::{Connection, Message, OwnedVariant as OwnedValue};

pub const NAME: &str = "org.freedesktop.UPower";
pub const ROOT_PATH: &str = "/org/freedesktop/UPower";
pub const DISPLAY_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const IFACE_ROOT: &str = "org.freedesktop.UPower";
const IFACE_DEVICE: &str = "org.freedesktop.UPower.Device";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

pub(super) async fn run(
    service: Arc<PowerService>,
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
                tracing::warn!(%error, "UPower session ended");
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

async fn load(service: &PowerService, conn: &Connection, owner: &str) -> Result<(), IslandError> {
    let device: HashMap<String, OwnedValue> = call(
        conn,
        owner,
        DISPLAY_PATH,
        PROPERTIES,
        "GetAll",
        &(IFACE_DEVICE,),
    )
    .await?;
    let on_battery: OwnedValue = call(
        conn,
        owner,
        ROOT_PATH,
        PROPERTIES,
        "Get",
        &(IFACE_ROOT, "OnBattery"),
    )
    .await?;
    let on_battery = value_to_json(&on_battery);
    service.load(&props_to_json(&device), Some(&on_battery));
    service.status.available("upower");
    service.status.connected();
    Ok(())
}

async fn session(service: &PowerService, address: &str) -> Result<(), IslandError> {
    let conn = connect(address).await?;
    // Subscribe BEFORE any read; queued signals are processed in order afterwards.
    let mut names = subscribe(
        &conn,
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.UPower'",
    )
    .await?;
    let mut device = subscribe(
        &conn,
        "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.freedesktop.UPower.Device'",
    )
    .await?;
    let mut root = subscribe(
        &conn,
        "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.freedesktop.UPower'",
    )
    .await?;
    let mut owner = name_owner(&conn, NAME).await?;
    match owner.as_deref() {
        Some(unique) => load(service, &conn, unique).await?,
        None => service.status.unavailable("no-owner"),
    }
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
            message = next(&mut device) => {
                let message = message.ok_or_else(|| IslandError::unavailable("device stream ended"))?.map_err(transport)?;
                handle(service, owner.as_deref(), &message);
            }
            message = next(&mut root) => {
                let message = message.ok_or_else(|| IslandError::unavailable("root stream ended"))?.map_err(transport)?;
                handle(service, owner.as_deref(), &message);
            }
        }
    }
}

fn handle(service: &PowerService, owner: Option<&str>, message: &Message) {
    let Some(owner) = owner else {
        return;
    };
    if message.data().len() > MAX_SIGNAL_BYTES {
        return;
    }
    let header = message.header();
    // Sender guard: only the current UPower owner may change our state.
    if header.sender().map(|s| s.as_str()) != Some(owner) {
        return;
    }
    let path = header.path().map(|p| p.as_str()).unwrap_or("");
    let Ok((interface, changed, _invalidated)) =
        message
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
    else {
        return;
    };
    let changed = props_to_json(&changed);
    // Path guard: only DisplayDevice device properties and the root's OnBattery.
    match (interface.as_str(), path) {
        (IFACE_DEVICE, DISPLAY_PATH) => service.apply_device(&changed),
        (IFACE_ROOT, ROOT_PATH) => {
            if let Some(value) = changed.get("OnBattery") {
                service.apply_on_battery(value);
            }
        }
        _ => {}
    }
}
