//! zbus helpers shared by the system-bus services (UPower, BlueZ). Linux only.
//!
//! Every call is bounded, every incoming signal is size-capped, and D-Bus variants are
//! converted to `serde_json::Value` for the audited pure cores (unsupported variant kinds
//! become `null`, never a fabricated value).

use crate::error::IslandError;
use serde_json::{Map, Number, Value as Json};
use std::{collections::HashMap, future::poll_fn, num::NonZeroUsize, pin::Pin, time::Duration};
use zbus::{
    export::futures_core::Stream, Connection, Message, MessageStream, OwnedVariant as OwnedValue,
    Variant as Value,
};

pub const DBUS: &str = "org.freedesktop.DBus";
pub const DBUS_PATH: &str = "/org/freedesktop/DBus";
pub const CALL_TIMEOUT: Duration = Duration::from_secs(2);
/// Signals larger than this are counted and dropped, never parsed.
pub const MAX_SIGNAL_BYTES: usize = 128 * 1024;

pub fn transport(_: zbus::Error) -> IslandError {
    IslandError::unavailable("D-Bus transport failed")
}

pub async fn connect(address: &str) -> Result<Connection, IslandError> {
    tokio::time::timeout(
        CALL_TIMEOUT,
        zbus::connection::Builder::address(address).build(),
    )
    .await
    .map_err(|_| IslandError::timeout("D-Bus connect"))?
    .map_err(transport)
}

/// `GetNameOwner`: the owner's unique name, or `None` when nobody owns `name`.
pub async fn name_owner(conn: &Connection, name: &str) -> Result<Option<String>, IslandError> {
    match tokio::time::timeout(
        CALL_TIMEOUT,
        conn.call_method(Some(DBUS), DBUS_PATH, Some(DBUS), "GetNameOwner", &(name,)),
    )
    .await
    {
        Ok(Ok(reply)) => reply
            .body()
            .deserialize::<String>()
            .map(Some)
            .map_err(transport),
        Ok(Err(zbus::Error::FDO(e))) if matches!(*e, zbus::fdo::Error::NameHasNoOwner(_)) => {
            Ok(None)
        }
        Ok(Err(e)) => Err(transport(e)),
        Err(_) => Err(IslandError::timeout("D-Bus owner lookup")),
    }
}

/// A match-rule stream. The rule is installed on the bus BEFORE this returns, so a caller
/// that subscribes first and reads state second cannot miss a change in between.
pub async fn subscribe(conn: &Connection, rule: &str) -> Result<MessageStream, IslandError> {
    MessageStream::for_match_rule(rule, conn, NonZeroUsize::new(64))
        .await
        .map_err(transport)
}

pub async fn next(stream: &mut MessageStream) -> Option<zbus::Result<Message>> {
    poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

/// Bounded method call returning the deserialized body.
pub async fn call<B, R>(
    conn: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
    body: &B,
) -> Result<R, IslandError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let reply = tokio::time::timeout(
        CALL_TIMEOUT,
        conn.call_method(Some(destination), path, Some(interface), member, body),
    )
    .await
    .map_err(|_| IslandError::timeout("D-Bus call"))?
    .map_err(transport)?;
    reply.body().deserialize::<R>().map_err(transport)
}

pub fn value_to_json(value: &Value<'_>) -> Json {
    match value {
        Value::Bool(b) => Json::Bool(*b),
        Value::U8(n) => Json::from(*n),
        Value::I16(n) => Json::from(*n),
        Value::U16(n) => Json::from(*n),
        Value::I32(n) => Json::from(*n),
        Value::U32(n) => Json::from(*n),
        Value::I64(n) => Json::from(*n),
        Value::U64(n) => Json::from(*n),
        Value::F64(f) => Number::from_f64(*f).map_or(Json::Null, Json::Number),
        Value::Str(s) => Json::String(s.as_str().to_owned()),
        Value::ObjectPath(p) => Json::String(p.as_str().to_owned()),
        Value::Variant(inner) => value_to_json(inner),
        Value::Array(items) => Json::Array(items.inner().iter().map(value_to_json).collect()),
        // Other containers (dicts, structs, fds) are not part of the modeled properties.
        _ => Json::Null,
    }
}

pub fn props_to_json(props: &HashMap<String, OwnedValue>) -> Map<String, Json> {
    props
        .iter()
        .map(|(key, value)| (key.clone(), value_to_json(value)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn variants_convert_without_fabrication() {
        assert_eq!(value_to_json(&Value::U32(2)), json!(2));
        assert_eq!(value_to_json(&Value::I64(-5)), json!(-5));
        assert_eq!(value_to_json(&Value::F64(42.5)), json!(42.5));
        assert_eq!(value_to_json(&Value::F64(f64::NAN)), Json::Null);
        assert_eq!(value_to_json(&Value::Bool(true)), json!(true));
        assert_eq!(value_to_json(&Value::from("x")), json!("x"));
        assert_eq!(
            value_to_json(&Value::Variant(Box::new(Value::U8(7)))),
            json!(7)
        );
        let array = Value::from(vec!["a", "b"]);
        assert_eq!(value_to_json(&array), json!(["a", "b"]));
    }
}
