//! Phase 6b2 agents (ported core; service adapter added separately).
pub mod core;
mod hub;
#[cfg(test)]
mod notification_tests;
pub mod reader;
mod service;
pub use hub::AgentHubSink;
pub use service::{AgentConfig, AgentEvents, AgentService};
