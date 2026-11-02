//! Shared bounded Controls actor plumbing; domain state and IO stay in each adapter.
use crate::{bus::Latest, error::IslandError};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};
pub trait State: Clone + PartialEq + Send + Sync + 'static {
    fn rev(&mut self) -> &mut u32;
}
impl State for crate::contract::controls::MixerChanged {
    fn rev(&mut self) -> &mut u32 {
        &mut self.rev
    }
}
impl State for crate::contract::controls::TogglesChanged {
    fn rev(&mut self) -> &mut u32 {
        &mut self.rev
    }
}
pub struct Request<T, Op> {
    // Bounded actor queue; operation execution is owned even when a caller disconnects.
    pub op: Op,
    pub reply: oneshot::Sender<Result<T, IslandError>>,
}
pub struct ControlActor<T: State, Op: Send + 'static> {
    pub topic: Latest<T>,
    current: Mutex<T>,
    commands: mpsc::Sender<Request<T, Op>>,
    pub stop: watch::Sender<bool>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}
impl<T: State, Op: Send + 'static> ControlActor<T, Op> {
    pub fn new(initial: T) -> (Arc<Self>, mpsc::Receiver<Request<T, Op>>) {
        let topic = Latest::new();
        topic.publish(initial.clone());
        let (commands, rx) = mpsc::channel(16);
        (
            Arc::new(Self {
                topic,
                current: Mutex::new(initial),
                commands,
                stop: watch::channel(false).0,
                task: Mutex::new(None),
            }),
            rx,
        )
    }
    pub fn own(&self, task: tauri::async_runtime::JoinHandle<()>) {
        match self.task.lock() {
            Ok(mut slot) => *slot = Some(task),
            Err(_) => task.abort(),
        }
    }
    pub fn current(&self) -> T {
        match self.current.lock() {
            Ok(v) => v.clone(),
            Err(v) => v.into_inner().clone(),
        }
    }
    pub fn publish(&self, mut next: T) -> T {
        let mut value = match self.current.lock() {
            Ok(v) => v,
            Err(v) => v.into_inner(),
        };
        *next.rev() = *value.rev();
        if next != *value {
            *next.rev() = next.rev().wrapping_add(1);
            *value = next.clone();
            self.topic.publish(next.clone());
        }
        next
    }
    pub async fn request(&self, op: Op) -> Result<T, IslandError> {
        if *self.stop.borrow() {
            return Err(IslandError::unavailable("Controls module stopped"));
        }
        let (reply, answer) = oneshot::channel();
        self.commands
            .try_send(Request { op, reply })
            .map_err(|_| IslandError::unavailable("Controls queue full or stopped"))?;
        tokio::time::timeout(Duration::from_secs(8), answer)
            .await
            .map_err(|_| IslandError::timeout("Controls command timed out"))?
            .map_err(|_| IslandError::unavailable("Controls worker stopped"))?
    }
    pub async fn shutdown_async(&self) -> Result<(), IslandError> {
        self.stop.send_replace(true);
        let task = self
            .task
            .lock()
            .map_err(|_| IslandError::internal("Controls worker poisoned"))?
            .take();
        if let Some(mut task) = task {
            match tokio::time::timeout(Duration::from_secs(6), &mut task).await {
                Ok(result) => {
                    result.map_err(|_| IslandError::internal("Controls worker panicked"))?
                }
                Err(_) => {
                    task.abort();
                    if task.await.is_err() {
                        tracing::debug!("Controls worker cancelled");
                    }
                    return Err(IslandError::timeout("Controls async shutdown"));
                }
            }
        }
        Ok(())
    }
    pub fn shutdown(&self) -> Result<(), IslandError> {
        self.stop.send_replace(true);
        let task = self
            .task
            .lock()
            .map_err(|_| IslandError::internal("Controls worker poisoned"))?
            .take();
        super::control_process::join(task)
    }
}
pub fn respond<T>(reply: oneshot::Sender<Result<T, IslandError>>, result: Result<T, IslandError>) {
    if reply.send(result).is_err() {
        tracing::debug!("Controls caller disconnected");
    }
}
