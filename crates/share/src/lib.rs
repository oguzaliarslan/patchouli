//! Shares what is being watched: Discord IPC rich presence and a JSON webhook.
//!
//! Phase 6: port Taiga `src/share/*`, keeping the webhook event schema (version 1).

/// Version of the webhook JSON document. Bump only on breaking changes.
pub const EVENT_VERSION: u32 = 1;
