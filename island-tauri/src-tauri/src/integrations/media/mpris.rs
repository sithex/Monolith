//! MPRIS over zbus (session bus): `org.mpris.MediaPlayer2.*` names, `Player` properties and
//! `Seeked`, with owner tracking.
//!
//! Subscribe first, read second. Every signal is accepted ONLY from the unique name that
//! currently owns the player's well-known name and only from `/org/mpris/MediaPlayer2`; the
//! match rules are deliberately broad (no sender), so these in-code guards are the sole
//! sender/path defense for `PropertiesChanged` and `Seeked`, each pinned by a rogue-sender
//! test. `NameOwnerChanged` is accepted only from the bus driver.

use super::{Call, ControlPlan, MediaService, BUS_PREFIX};
use crate::error::IslandError;
use crate::integrations::dbus_util::{
    call, connect, name_owner, next, subscribe, transport, value_to_json, CALL_TIMEOUT, DBUS,
    DBUS_PATH, MAX_SIGNAL_BYTES,
};
use serde_json::{Map, Value as Json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::watch;
use zbus::{Connection, Message, OwnedVariant as OwnedValue, Variant as Value};

pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// Players tracked at once (the audited core caps at 32 as well).
const MAX_PLAYERS: usize = 32;

pub(super) async fn run(
    service: Arc<MediaService>,
    address: Option<String>,
    mut stop: watch::Receiver<bool>,
) {
    let Some(address) = address else {
        service.status.unavailable("no-session-bus-configured");
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
        service.set_link(None);
        service.lost();
        match result {
            Ok(()) => backoff = 1,
            Err(error) => {
                tracing::warn!(%error, "MPRIS session ended");
                service.status.unavailable("bus-unavailable");
            }
        }
        tokio::select! {
            _ = stop.changed() => break,
            () = tokio::time::sleep(Duration::from_secs(backoff)) => {}
        }
        backoff = (backoff * 2).min(60);
    }
    service.set_link(None);
    service.lost();
    service.status.unavailable("stopped");
}

/// `a{sv}` (or a variant holding one) → JSON object; other kinds are not modeled.
fn dict_to_json(value: &Value<'_>) -> Json {
    match value {
        Value::Variant(inner) => dict_to_json(inner),
        Value::Dict(dict) => {
            let mut map = Map::new();
            for (key, entry) in dict.iter() {
                if let Value::Str(key) = key {
                    map.insert(key.as_str().to_owned(), value_to_json(entry));
                }
            }
            Json::Object(map)
        }
        _ => Json::Null,
    }
}

/// Player properties → the JSON shape the service applies. `Metadata` is a nested dict.
fn props_json(props: &HashMap<String, OwnedValue>) -> Map<String, Json> {
    props
        .iter()
        .map(|(key, value)| {
            let json = if key == "Metadata" {
                dict_to_json(value)
            } else {
                value_to_json(value)
            };
            (key.clone(), json)
        })
        .collect()
}

async fn load_player(
    service: &MediaService,
    conn: &Connection,
    bus: &str,
    owner: &str,
) -> Result<(), IslandError> {
    let props: HashMap<String, OwnedValue> =
        call(conn, bus, OBJECT_PATH, PROPERTIES, "GetAll", &(PLAYER,)).await?;
    let pid = call::<_, u32>(
        conn,
        DBUS,
        DBUS_PATH,
        DBUS,
        "GetConnectionUnixProcessID",
        &(owner,),
    )
    .await
    .ok();
    let mut json = props_json(&props);
    let writable = call::<_, String>(
        conn,
        bus,
        OBJECT_PATH,
        "org.freedesktop.DBus.Introspectable",
        "Introspect",
        &(),
    )
    .await
    .ok()
    .is_some_and(|xml| volume_writable(&xml));
    json.insert("VolumeWritable".into(), Json::Bool(writable));
    service.player_loaded(bus, owner, pid, &json);
    Ok(())
}

async fn fetch_position(service: &MediaService, conn: &Connection, bus: &str) {
    let position = call::<_, OwnedValue>(
        conn,
        bus,
        OBJECT_PATH,
        PROPERTIES,
        "Get",
        &(PLAYER, "Position"),
    )
    .await;
    if let Ok(value) = position {
        if let Some(us) = value_to_json(&value).as_i64() {
            service.player_position(bus, us);
        }
    }
}

async fn session(service: &Arc<MediaService>, address: &str) -> Result<(), IslandError> {
    let conn = connect(address).await?;
    let mut names = subscribe(
        &conn,
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0namespace='org.mpris.MediaPlayer2'",
    )
    .await?;
    let mut props = subscribe(
        &conn,
        "type='signal',path='/org/mpris/MediaPlayer2',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.mpris.MediaPlayer2.Player'",
    )
    .await?;
    let mut seeked = subscribe(
        &conn,
        "type='signal',path='/org/mpris/MediaPlayer2',interface='org.mpris.MediaPlayer2.Player',member='Seeked'",
    )
    .await?;
    let listed: Vec<String> = call(&conn, DBUS, DBUS_PATH, DBUS, "ListNames", &()).await?;
    for bus in listed
        .iter()
        .filter(|n| n.starts_with(BUS_PREFIX))
        .take(MAX_PLAYERS)
    {
        if let Some(owner) = name_owner(&conn, bus).await? {
            if let Err(error) = load_player(service, &conn, bus, &owner).await {
                tracing::debug!(%error, bus, "MPRIS player could not be read");
            }
        }
    }
    service.set_link(Some(conn.clone()));
    service.status.available("mpris");
    service.status.connected();
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
                if !name.starts_with(BUS_PREFIX) {
                    continue;
                }
                if new.is_empty() {
                    service.player_removed(&name);
                } else if let Err(error) = load_player(service, &conn, &name, &new).await {
                    tracing::debug!(%error, bus = %name, "MPRIS player could not be read");
                }
            }
            message = next(&mut props) => {
                let message = message.ok_or_else(|| IslandError::unavailable("properties stream ended"))?.map_err(transport)?;
                if let Some(update) = handle_props(service, &message) {
                    if update.reload {
                        if let Some(owner) = service.owner_of(&update.bus) {
                            if let Err(error) = load_player(service, &conn, &update.bus, &owner).await {
                                tracing::debug!(%error, bus = %update.bus, "MPRIS reload failed");
                            }
                        }
                    } else if update.position_unknown {
                        fetch_position(service, &conn, &update.bus).await;
                    }
                }
            }
            message = next(&mut seeked) => {
                let message = message.ok_or_else(|| IslandError::unavailable("seeked stream ended"))?.map_err(transport)?;
                handle_seeked(service, &message);
            }
        }
    }
}

struct PropsUpdate {
    bus: String,
    /// `invalidated_properties` were reported: re-read everything.
    reload: bool,
    /// Status or metadata changed: the player's `Position` must be read once.
    position_unknown: bool,
}

/// Sender guard + path guard shared by both signal handlers: the bus name whose CURRENT owner
/// sent this message from the MPRIS object path.
fn trusted_player(service: &MediaService, message: &Message) -> Option<String> {
    if message.data().len() > MAX_SIGNAL_BYTES {
        return None;
    }
    let header = message.header();
    if header.path().map(|p| p.as_str()) != Some(OBJECT_PATH) {
        return None;
    }
    service.bus_for_owner(header.sender()?.as_str())
}

fn handle_props(service: &MediaService, message: &Message) -> Option<PropsUpdate> {
    let bus = trusted_player(service, message)?;
    let (interface, changed, invalidated) = message
        .body()
        .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
        .ok()?;
    if interface != PLAYER {
        return None;
    }
    let json = props_json(&changed);
    let status_or_track = json.contains_key("PlaybackStatus") || json.contains_key("Metadata");
    let explicit_position = json.contains_key("Position");
    service.player_props(&bus, &json);
    Some(PropsUpdate {
        bus,
        reload: !invalidated.is_empty(),
        position_unknown: status_or_track && !explicit_position,
    })
}

fn handle_seeked(service: &MediaService, message: &Message) {
    let Some(bus) = trusted_player(service, message) else {
        return;
    };
    if let Ok(position_us) = message.body().deserialize::<i64>() {
        service.player_seeked(&bus, position_us);
    }
}

/// Send the planned call to the player. The destination is the planned well-known name; no
/// other peer is ever addressed.
pub(super) async fn invoke(conn: &Connection, plan: &ControlPlan) -> Result<(), IslandError> {
    let send = async {
        match &plan.call {
            Call::Volume { value } => {
                conn.call_method(
                    Some(plan.bus.as_str()),
                    OBJECT_PATH,
                    Some(PROPERTIES),
                    "Set",
                    &(PLAYER, "Volume", Value::F64(*value)),
                )
                .await
            }
            Call::Method(member) => {
                conn.call_method(
                    Some(plan.bus.as_str()),
                    OBJECT_PATH,
                    Some(PLAYER),
                    *member,
                    &(),
                )
                .await
            }
            Call::SetPosition {
                track_id,
                target_us,
            } => {
                let path = zbus::ObjectPath::try_from(track_id.as_str())
                    .map_err(|_| zbus::Error::Failure("invalid track id".into()))?;
                conn.call_method(
                    Some(plan.bus.as_str()),
                    OBJECT_PATH,
                    Some(PLAYER),
                    "SetPosition",
                    &(path, *target_us),
                )
                .await
            }
            Call::Seek { offset_us } => {
                conn.call_method(
                    Some(plan.bus.as_str()),
                    OBJECT_PATH,
                    Some(PLAYER),
                    "Seek",
                    &(*offset_us,),
                )
                .await
            }
        }
    };
    match tokio::time::timeout(CALL_TIMEOUT, send).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(_)) => Err(IslandError::unavailable("The player rejected the command")),
        Err(_) => Err(IslandError::timeout("Player command")),
    }
}

fn volume_writable(xml: &str) -> bool {
    // Introspection is advisory; Properties.Set remains bounded and reports rejection.
    let Ok(interface) = regex::Regex::new(
        r#"(?s)<interface\s+name=["']org\.mpris\.MediaPlayer2\.Player["'][^>]*>(.*?)</interface>"#,
    ) else {
        return false;
    };
    let Ok(property) = regex::Regex::new(r#"<property\s+[^>]*>"#) else {
        return false;
    };
    let Ok(name) = regex::Regex::new(r#"\bname\s*=\s*["']Volume["']"#) else {
        return false;
    };
    let Ok(access) = regex::Regex::new(r#"\baccess\s*=\s*["']readwrite["']"#) else {
        return false;
    };
    interface.captures(xml).is_some_and(|caps| {
        property
            .find_iter(&caps[1])
            .any(|p| name.is_match(p.as_str()) && access.is_match(p.as_str()))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn volume_requires_writable_property_on_player_interface() {
        assert!(super::volume_writable(
            r#"<interface name="org.mpris.MediaPlayer2.Player"><property access="readwrite" type="d" name="Volume"/></interface>"#
        ));
        assert!(!super::volume_writable(
            r#"<interface name="org.mpris.MediaPlayer2.Player"><property name="Volume" access="read"/></interface>"#
        ));
    }
}
