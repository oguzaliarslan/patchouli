//! Discord Rich Presence.

mod activity;
mod ipc;
mod presence;

pub use activity::{Activity, ActivityType, Button, StatusDisplayType, to_json};
pub use ipc::{Opcode, socket_name};
pub use presence::{Presence, PresenceStatus};
