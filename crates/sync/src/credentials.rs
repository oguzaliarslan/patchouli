//! Access and refresh tokens. The app stores them in the OS credential store;
//! [`MemoryCredentialStore`] is for tests.

use std::collections::HashMap;
use std::sync::Mutex;

use patchouli_core::ServiceId;
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Seconds since the Unix epoch, if the service said.
    pub expires_at: Option<i64>,
}

pub trait CredentialStore: Send + Sync {
    fn load(&self, service: ServiceId) -> Result<Option<Credentials>>;
    fn save(&self, service: ServiceId, credentials: &Credentials) -> Result<()>;
    fn delete(&self, service: ServiceId) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryCredentialStore(Mutex<HashMap<ServiceId, Credentials>>);

impl CredentialStore for MemoryCredentialStore {
    fn load(&self, service: ServiceId) -> Result<Option<Credentials>> {
        Ok(self.0.lock().unwrap().get(&service).cloned())
    }

    fn save(&self, service: ServiceId, credentials: &Credentials) -> Result<()> {
        self.0.lock().unwrap().insert(service, credentials.clone());
        Ok(())
    }

    fn delete(&self, service: ServiceId) -> Result<()> {
        self.0.lock().unwrap().remove(&service);
        Ok(())
    }
}
