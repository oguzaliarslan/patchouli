//! Non-secret account details (`accounts.json`). Tokens are kept separately in
//! a [`CredentialStore`](crate::CredentialStore).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use patchouli_core::ServiceId;
use serde::{Deserialize, Serialize};

use crate::Result;

pub const FILE_NAME: &str = "accounts.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub username: String,
    pub user_id: i64,
    /// AniList's score format (e.g. `POINT_10_DECIMAL`). Scores are always
    /// stored on a 0–100 scale; this is for display.
    pub score_format: Option<String>,
    /// Profile picture.
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// Accounts per service, saved to a JSON file on every change.
pub struct AccountsFile {
    path: PathBuf,
    accounts: Mutex<HashMap<ServiceId, Account>>,
}

impl AccountsFile {
    /// Loads the file. A missing or unreadable file means no accounts.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let accounts = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            accounts: Mutex::new(accounts),
        }
    }

    pub fn get(&self, service: ServiceId) -> Option<Account> {
        self.lock().get(&service).cloned()
    }

    pub fn set(&self, service: ServiceId, account: Account) -> Result<()> {
        let mut accounts = self.lock();
        accounts.insert(service, account);
        self.save(&accounts)
    }

    pub fn remove(&self, service: ServiceId) -> Result<()> {
        let mut accounts = self.lock();
        accounts.remove(&service);
        self.save(&accounts)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ServiceId, Account>> {
        self.accounts.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn save(&self, accounts: &HashMap<ServiceId, Account>) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json =
            serde_json::to_vec_pretty(accounts).map_err(|e| crate::Error::Parse(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_reloads() {
        let dir = std::env::temp_dir().join(format!("tracker-accounts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(FILE_NAME);

        let file = AccountsFile::load(&path);
        assert_eq!(file.get(ServiceId::AniList), None);
        let account = Account {
            username: "user".into(),
            user_id: 1,
            score_format: Some("POINT_100".into()),
            avatar_url: Some("https://example.com/a.png".into()),
        };
        file.set(ServiceId::AniList, account.clone()).unwrap();

        let reloaded = AccountsFile::load(&path);
        assert_eq!(reloaded.get(ServiceId::AniList), Some(account));
        reloaded.remove(ServiceId::AniList).unwrap();
        assert_eq!(AccountsFile::load(&path).get(ServiceId::AniList), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
