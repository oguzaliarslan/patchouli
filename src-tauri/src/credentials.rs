//! Tokens in the OS credential store (Windows Credential Manager, macOS
//! Keychain).
//!
//! Values are stored under the app's identifier as `<slug>.access`,
//! `<slug>.refresh` and `<slug>.expires`. Windows stores a secret as UTF-16 and
//! allows 2560 bytes, so at most 1280 characters per entry. Longer values are
//! split into chunks: the first entry holds `<count>:<chunk>`, and the rest are
//! `<name>.2`, `<name>.3`, ...

use patchouli_core::ServiceId;
use patchouli_sync::{CredentialStore, Credentials, Error, Result};

/// Characters per entry, below Windows' limit of 1280.
const CHUNK_CHARS: usize = 1000;

pub struct KeyringStore {
    target: String,
}

impl KeyringStore {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
        }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.target, name).map_err(error)
    }

    fn read(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(error(e)),
        }
    }

    fn remove(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(error(e)),
        }
    }

    /// Reads a value and the number of chunks it's stored in.
    fn get_chunked(&self, name: &str) -> Result<Option<(String, usize)>> {
        let Some(first) = self.read(name)? else {
            return Ok(None);
        };
        let Some((count, chunk)) = first
            .split_once(':')
            .and_then(|(n, rest)| Some((n.parse::<usize>().ok()?, rest)))
        else {
            return Ok(Some((first, 1)));
        };
        let mut value = chunk.to_owned();
        for i in 2..=count {
            let part = self
                .read(&format!("{name}.{i}"))?
                .ok_or_else(|| Error::Credentials(format!("Credential {name} is incomplete.")))?;
            value.push_str(&part);
        }
        Ok(Some((value, count)))
    }

    fn get(&self, service: ServiceId, kind: &str) -> Result<Option<String>> {
        let name = format!("{}.{kind}", service.slug());
        Ok(self.get_chunked(&name)?.map(|(value, _)| value))
    }

    fn set(&self, service: ServiceId, kind: &str, value: Option<&str>) -> Result<()> {
        let name = format!("{}.{kind}", service.slug());
        let old_count = self.get_chunked(&name).ok().flatten().map_or(1, |(_, n)| n);

        let chunks: Vec<String> = match value {
            Some(value) => {
                let chars: Vec<char> = value.chars().collect();
                let mut chunks: Vec<String> = chars
                    .chunks(CHUNK_CHARS)
                    .map(|c| c.iter().collect())
                    .collect();
                if chunks.is_empty() {
                    chunks.push(String::new());
                }
                chunks
            }
            None => Vec::new(),
        };

        for (i, chunk) in chunks.iter().enumerate() {
            let (entry_name, text) = if i == 0 {
                (name.clone(), format!("{}:{chunk}", chunks.len()))
            } else {
                (format!("{name}.{}", i + 1), chunk.clone())
            };
            self.entry(&entry_name)?
                .set_password(&text)
                .map_err(error)?;
        }
        if chunks.is_empty() {
            self.remove(&name)?;
        }
        // Remove chunks the old value used and the new one doesn't.
        for i in chunks.len().max(1) + 1..=old_count {
            self.remove(&format!("{name}.{i}"))?;
        }
        Ok(())
    }
}

fn error(e: keyring::Error) -> Error {
    Error::Credentials(format!("Credential store error: {e}"))
}

impl CredentialStore for KeyringStore {
    fn load(&self, service: ServiceId) -> Result<Option<Credentials>> {
        let Some(access_token) = self.get(service, "access")? else {
            return Ok(None);
        };
        Ok(Some(Credentials {
            access_token,
            refresh_token: self.get(service, "refresh")?,
            expires_at: self.get(service, "expires")?.and_then(|s| s.parse().ok()),
        }))
    }

    fn save(&self, service: ServiceId, credentials: &Credentials) -> Result<()> {
        self.set(service, "access", Some(&credentials.access_token))?;
        self.set(service, "refresh", credentials.refresh_token.as_deref())?;
        let expires = credentials.expires_at.map(|t| t.to_string());
        self.set(service, "expires", expires.as_deref())
    }

    fn delete(&self, service: ServiceId) -> Result<()> {
        for kind in ["access", "refresh", "expires"] {
            self.set(service, kind, None)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses the real OS credential store, so it's opt-in:
    /// `cargo test -p patchouli -- --ignored`
    #[test]
    #[ignore = "writes to the OS credential store"]
    fn round_trips_through_the_os_store() {
        let store = KeyringStore::new(format!("tracker-test-{}", std::process::id()));
        let service = ServiceId::MyAnimeList;
        let credentials = Credentials {
            // Longer than one Windows entry allows (1280 characters).
            access_token: "a".repeat(2500),
            refresh_token: Some("r:".repeat(700)), // contains the separator
            expires_at: Some(1_900_000_000),
        };
        store.save(service, &credentials).unwrap();
        assert_eq!(store.load(service).unwrap(), Some(credentials));

        store
            .save(
                service,
                &Credentials {
                    access_token: "only".into(),
                    ..Credentials::default()
                },
            )
            .unwrap();
        let loaded = store.load(service).unwrap().unwrap();
        assert_eq!(loaded.refresh_token, None);

        store.delete(service).unwrap();
        assert_eq!(store.load(service).unwrap(), None);
        store.delete(service).unwrap(); // deleting twice is fine
    }
}
