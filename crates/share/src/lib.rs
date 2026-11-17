//! Shares what is being watched, ported from Taiga `src/share/*`:
//!
//! - [`discord`]: Rich Presence through the Discord desktop app's local IPC
//!   socket (`discord_ipc.cpp`, `discord_rpc.cpp`).
//! - [`webhook`]: JSON events posted to a user-defined URL (`event.cpp`,
//!   `http.cpp`), in the version 1 schema.

pub mod discord;
pub mod webhook;
