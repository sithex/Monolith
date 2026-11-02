//! Controls property observer. Subscribe-before-read, unique-owner authorization, bounded calls.
use crate::{
    error::IslandError,
    integrations::{
        control_actor::{respond, ControlActor, Request, State},
        dbus_util::{self as dbus, CALL_TIMEOUT, DBUS, MAX_SIGNAL_BYTES},
    },
};
use serde_json::{Map, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use zbus::{Connection, Message, OwnedVariant, Variant};
pub const PPD: &str = "org.freedesktop.UPower.PowerProfiles";
pub const PPD_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
pub const NM: &str = "org.freedesktop.NetworkManager";
pub const NM_PATH: &str = "/org/freedesktop/NetworkManager";
pub const SCREENSAVER: &str = "org.freedesktop.ScreenSaver";
pub const SCREENSAVER_PATH: &str = "/org/freedesktop/ScreenSaver";
pub const KWIN: &str = "org.kde.KWin";
pub const PICKER_PATH: &str = "/ColorPicker";
pub const PICKER_IFACE: &str = "org.kde.kwin.ColorPicker";
pub const PROPS: &str = "org.freedesktop.DBus.Properties";
#[derive(Clone, Debug, PartialEq)]
pub struct Properties {
    pub rev: u32,
    pub owner: Option<String>,
    pub values: Option<Map<String, Value>>,
    pub detail: String,
}
impl State for Properties {
    fn rev(&mut self) -> &mut u32 {
        &mut self.rev
    }
}
pub struct SetProfile(pub String);
pub type PropertyFeed = ControlActor<Properties, SetProfile>;
fn empty(detail: &str) -> Properties {
    Properties {
        rev: 0,
        owner: None,
        values: None,
        detail: detail.into(),
    }
}
impl PropertyFeed {
    pub fn start(address: Option<String>, name: &'static str) -> Arc<Self> {
        let (service, rx) = Self::new(empty("starting"));
        let owned = service.clone();
        service.own(tauri::async_runtime::spawn(async move {
            worker(owned, address, name, rx).await
        }));
        service
    }
}
fn path(name: &str) -> &'static str {
    if name == PPD {
        PPD_PATH
    } else {
        NM_PATH
    }
}
async fn worker(
    service: Arc<PropertyFeed>,
    address: Option<String>,
    name: &'static str,
    mut rx: mpsc::Receiver<Request<Properties, SetProfile>>,
) {
    let mut stop = service.stop.subscribe();
    let Some(address) = address else {
        service.publish(empty("no-private-system-bus"));
        return;
    };
    let mut delay = 1;
    while !*stop.borrow() {
        let result = tokio::select! {_=stop.changed()=>break,result=session(&service,&address,name,&mut rx)=>result};
        service.publish(empty("bus-unavailable"));
        if let Err(error) = result {
            tracing::debug!(%error,"Controls property observer lost");
        }
        tokio::select! {_=stop.changed()=>break,()=tokio::time::sleep(Duration::from_secs(delay))=>{}}
        delay = (delay * 2).min(60);
    }
    service.publish(empty("stopped"));
}
pub async fn call<B, R>(
    conn: &Connection,
    owner: &str,
    path: &str,
    iface: &str,
    member: &str,
    body: &B,
) -> Result<R, IslandError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let reply = tokio::time::timeout(
        CALL_TIMEOUT,
        conn.call_method(Some(owner), path, Some(iface), member, body),
    )
    .await
    .map_err(|_| IslandError::timeout("Controls D-Bus call"))?
    .map_err(dbus::transport)?;
    if reply.data().len() > MAX_SIGNAL_BYTES {
        return Err(IslandError::unavailable("Controls D-Bus reply limit"));
    }
    reply.body().deserialize().map_err(dbus::transport)
}
fn value(v: &Variant<'_>) -> Value {
    match v {
        Variant::Dict(d) => Value::Object(
            d.iter()
                .filter_map(|(k, v)| match k {
                    Variant::Str(s) => Some((s.to_string(), value(v))),
                    _ => None,
                })
                .collect(),
        ),
        Variant::Array(a) => Value::Array(a.inner().iter().map(value).collect()),
        Variant::Variant(v) => value(v),
        _ => dbus::value_to_json(v),
    }
}
async fn load(conn: &Connection, name: &str, owner: &str) -> Result<Properties, IslandError> {
    if name == SCREENSAVER || name == KWIN {
        return Ok(Properties {
            rev: 0,
            owner: Some(owner.into()),
            values: Some(Map::new()),
            detail: "owner-present".into(),
        });
    }
    let props: HashMap<String, OwnedVariant> =
        call(conn, owner, path(name), PROPS, "GetAll", &(name,)).await?;
    let mut values: Map<String, Value> = props.iter().map(|(k, v)| (k.clone(), value(v))).collect();
    if name == NM {
        let devices = values
            .get("Devices")
            .and_then(Value::as_array)
            .ok_or_else(|| IslandError::unavailable("Network devices unavailable"))?;
        if devices.len() > 256 {
            return Err(IslandError::unavailable("Network device limit"));
        }
        let mut wireless = false;
        for device in devices {
            let device = device
                .as_str()
                .ok_or_else(|| IslandError::unavailable("Network device path invalid"))?;
            let kind: OwnedVariant = call(
                conn,
                owner,
                device,
                PROPS,
                "Get",
                &("org.freedesktop.NetworkManager.Device", "DeviceType"),
            )
            .await?;
            let kind = value(&kind)
                .as_u64()
                .ok_or_else(|| IslandError::unavailable("Network device type unknown"))?;
            wireless |= kind == 2;
        }
        values.insert("HasWirelessDevice".into(), Value::Bool(wireless));
    }
    Ok(Properties {
        rev: 0,
        owner: Some(owner.into()),
        values: Some(values),
        detail: "dbus".into(),
    })
}
fn signal_matches(message: &Message, owner: Option<&str>, name: &str) -> bool {
    // These two feeds consume ONLY NameOwnerChanged, no state/property signals.
    if name != PPD && name != NM {
        return false;
    }
    let header = message.header();
    let sender = header.sender().map(|s| s.as_str());
    let authenticated = owner.is_some() && sender == owner;
    if !authenticated
        || message.data().len() > MAX_SIGNAL_BYTES
        || header.path().map(|p| p.as_str()) != Some(path(name))
        || header.interface().map(|i| i.as_str()) != Some(PROPS)
        || header.member().map(|m| m.as_str()) != Some("PropertiesChanged")
    {
        return false;
    }
    let Ok((iface, _, _)) = message
        .body()
        .deserialize::<(String, HashMap<String, OwnedVariant>, Vec<String>)>()
    else {
        return false;
    };
    iface == name
}
async fn session(
    service: &PropertyFeed,
    address: &str,
    name: &str,
    rx: &mut mpsc::Receiver<Request<Properties, SetProfile>>,
) -> Result<(), IslandError> {
    let conn = dbus::connect(address).await?;
    let mut signals = tokio::time::timeout(
        CALL_TIMEOUT,
        dbus::subscribe(
            &conn,
            "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
        ),
    )
    .await
    .map_err(|_| IslandError::timeout("Controls signal subscription"))??;
    let owner_rule=format!("type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{name}'");
    let mut owners = tokio::time::timeout(CALL_TIMEOUT, dbus::subscribe(&conn, &owner_rule))
        .await
        .map_err(|_| IslandError::timeout("Controls owner subscription"))??;
    let mut owner = dbus::name_owner(&conn, name).await?;
    if let Some(owner) = &owner {
        service.publish(load(&conn, name, owner).await?);
    } else {
        service.publish(empty("no-owner"));
    }
    loop {
        tokio::select! {
            message=dbus::next(&mut signals)=>{
                let message=message.ok_or_else(||IslandError::unavailable("Controls property stream closed"))?.map_err(dbus::transport)?;
                if signal_matches(&message,owner.as_deref(),name){if let Some(owner)=&owner {service.publish(load(&conn,name,owner).await?);}}
            }
            message=dbus::next(&mut owners)=>{
                let message=message.ok_or_else(||IslandError::unavailable("Controls owner stream closed"))?.map_err(dbus::transport)?;
                if message.data().len()>MAX_SIGNAL_BYTES || message.header().sender().map(|s|s.as_str())!=Some(DBUS) {continue;}
                let Ok((changed,_,new))=message.body().deserialize::<(String,String,String)>() else{continue;};
                if changed!=name {continue;} owner=(!new.is_empty()).then_some(new);
                if let Some(owner)=&owner {service.publish(load(&conn,name,owner).await?);}else{service.publish(empty("name-lost"));}
            }
            request=rx.recv()=>{
                let Some(request)=request else{return Ok(());};if request.reply.is_closed(){continue;}
                let result=async {
                    if name!=PPD {return Err(IslandError::invalid("Network is read-only"));}
                    let owner=owner.as_deref().ok_or_else(||IslandError::unavailable("Power profiles unavailable"))?;
                    let profile=request.op.0;
                    if !["power-saver","balanced","performance"].contains(&profile.as_str()){return Err(IslandError::invalid("Unknown power profile"));}
                    call::<_,()>(&conn,owner,path(name),PROPS,"Set",&(name,"ActiveProfile",Variant::from(profile.as_str()))).await?;
                    Ok(service.publish(load(&conn,name,owner).await?))
                }.await;
                respond(request.reply,result);
            }
        }
    }
}
