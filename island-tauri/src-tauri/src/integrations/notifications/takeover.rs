//! Plasma's inhibitor is DND, not exclusive popup delivery. It preserves KDE history.
//! Only our dedicated connection/cookie is released; other inhibitors are never changed.
use super::{
    hub::Hub,
    linux_monitor::{NAME, PATH},
};
use crate::error::IslandError;
use std::{
    collections::BTreeMap,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::sync::watch;
use zbus::{Connection, OwnedVariant};

pub async fn acquire(connection: &Connection) -> Result<u32, IslandError> {
    let hints: BTreeMap<String, OwnedVariant> = BTreeMap::new();
    let reply = tokio::time::timeout(
        Duration::from_secs(2),
        connection.call_method(
            Some(NAME),
            PATH,
            Some(NAME),
            "Inhibit",
            &("hermes-island", "Monolith notification cards", hints),
        ),
    )
    .await
    .map_err(|_| IslandError::timeout("KDE notification inhibition"))?
    .map_err(|_| IslandError::unavailable("KDE notification inhibition is unavailable"))?;
    reply
        .body()
        .deserialize()
        .map_err(|_| IslandError::unavailable("Invalid KDE inhibition cookie"))
}
pub async fn release(connection: &Connection, cookie: u32) {
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        connection.call_method(Some(NAME), PATH, Some(NAME), "UnInhibit", &(cookie,)),
    )
    .await;
}
async fn connect(address: &str) -> Result<(Connection, zbus::MessageStream, u32), IslandError> {
    let connection = tokio::time::timeout(
        Duration::from_secs(2),
        zbus::connection::Builder::address(address).build(),
    )
    .await
    .map_err(|_| IslandError::timeout("Notification bus connect"))?
    .map_err(|_| IslandError::unavailable("Notification bus unavailable"))?;
    let owners = zbus::MessageStream::for_match_rule("type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.Notifications'", &connection, std::num::NonZeroUsize::new(8)).await.map_err(|_| IslandError::unavailable("Notification owner watch failed"))?;
    let cookie = acquire(&connection).await?;
    Ok((connection, owners, cookie))
}
pub async fn run(
    hub: Arc<Hub>,
    address: String,
    mut enabled: watch::Receiver<bool>,
    mut stopped: watch::Receiver<bool>,
) -> Result<(), IslandError> {
    loop {
        if *stopped.borrow() {
            break;
        }
        if !*enabled.borrow_and_update() {
            tokio::select! { _=stopped.changed()=>break, r=enabled.changed()=>{if r.is_err(){break;}} }
            continue;
        }
        let connected = tokio::select! { _=stopped.changed()=>break, _=enabled.changed()=>continue, result=connect(&address)=>result };
        match connected {
            Ok((connection, mut owners, cookie)) => {
                hub.inhibited.store(true, Ordering::Relaxed);
                tracing::info!("KDE inhibitor held: DND, history preserved; critical/whitelisted KDE popups may bypass");
                tokio::select! { _=stopped.changed()=>{}, _=enabled.changed()=>{}, _=super::linux_monitor::next(&mut owners)=>{} }
                hub.inhibited.store(false, Ordering::Relaxed);
                release(&connection, cookie).await;
                // Drop the connection as a crash-safe fallback if UnInhibit failed.
            }
            Err(error) => {
                tracing::warn!(%error, "KDE popup silencing failed; leaving KDE in control")
            }
        }
        if *stopped.borrow() {
            break;
        }
        if *enabled.borrow() {
            tokio::select! { _=stopped.changed()=>break, _=enabled.changed()=>{}, _=tokio::time::sleep(Duration::from_secs(5))=>{} }
        }
    }
    hub.inhibited.store(false, Ordering::Relaxed);
    Ok(())
}
