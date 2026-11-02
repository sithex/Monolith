//! Tracker transitions feed the existing actor and hub, never the session notification bus.
use super::{core::Event, AgentEvents};
use crate::{
    error::IslandError,
    integrations::notifications::{hub::Hub, NotificationInput, NotificationSource, Urgency},
    state::{
        actor::Handle,
        machine::{Activity, Clock, Intent},
    },
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

pub struct AgentHubSink {
    hub: Arc<Hub>,
    actor: Handle,
    clock: Arc<dyn Clock + Send + Sync>,
    owned: Mutex<BTreeSet<String>>,
}
impl AgentHubSink {
    pub fn new(hub: Arc<Hub>, actor: Handle, clock: Arc<dyn Clock + Send + Sync>) -> Self {
        Self {
            hub,
            actor,
            clock,
            owned: Mutex::new(BTreeSet::new()),
        }
    }
    fn remove(&self, id: &str) -> Result<(), IslandError> {
        self.actor.try_dispatch(Intent::RemoveActivity(id.into()))?;
        self.owned
            .lock()
            .map_err(|_| IslandError::internal("Agent activity state poisoned"))?
            .remove(id);
        Ok(())
    }
    fn activity(&self, id: String, priority: i32) -> Result<(), IslandError> {
        self.actor.try_dispatch(Intent::UpsertActivity(Activity {
            id: id.clone(),
            source: "agent-tracker".into(),
            priority,
            compact: true,
            expanded: true,
            started_ms: self.clock.now_ms(),
            expires_ms: None,
        }))?;
        self.owned
            .lock()
            .map_err(|_| IslandError::internal("Agent activity state poisoned"))?
            .insert(id);
        Ok(())
    }
    fn notify(&self, title: &str) -> Result<(), IslandError> {
        self.hub.insert(NotificationInput {
            source: NotificationSource::Alert,
            app_name: "Agents".into(),
            title: title.into(),
            body: None,
            icon_png: None,
            urgency: Urgency::Normal,
            actions: vec![],
            progress: None,
            dedupe_key: None,
            server_id: None,
            replaces_id: 0,
            transient: false,
            ttl_ms: None,
        })?;
        Ok(())
    }
}
impl AgentEvents for AgentHubSink {
    fn deliver(&self, event: &Event) -> Result<(), IslandError> {
        match event {
            Event::Started { id } => {
                self.activity(format!("agent:{id}"), 50)?;
                self.notify("Agent run started")
            }
            Event::Finished { id, .. } => {
                self.remove(&format!("agent:{id}"))?;
                self.activity(format!("agent-finished:{id}"), 60)?;
                self.notify("Agent run finished")
            }
            Event::Stalled { id } | Event::Disappeared { id } => {
                self.remove(&format!("agent:{id}"))
            }
        }
    }
    fn clear(&self) -> Result<(), IslandError> {
        let ids = self
            .owned
            .lock()
            .map_err(|_| IslandError::internal("Agent activity state poisoned"))?
            .clone();
        for id in ids {
            self.remove(&id)?;
        }
        Ok(())
    }
}
