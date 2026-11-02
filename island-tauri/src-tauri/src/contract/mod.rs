//! Types that cross the IPC boundary (SPEC §5.1). camelCase serde, no 64-bit integers, inputs
//! deny unknown fields, exactly one TypeScript type per Rust type.

pub mod ai;
pub mod apps;
pub mod clipboard;
pub mod controls;
pub mod events;
pub mod frontend;
pub mod island;
pub mod modules;
pub mod notify;
pub mod palette;
pub mod settings;
pub mod system;
pub mod terminal;
pub mod tiles;
pub mod workspace;
