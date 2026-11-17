use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use serde_json::Value;

use crate::Settings;

/// Called after settings change, with the new settings and the changed keys.
pub type Listener = Box<dyn Fn(&Settings, &BTreeSet<String>) + Send + Sync>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "settings file error: {e}"),
            Self::Json(e) => write!(f, "settings format error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Settings backed by a JSON file. Thread-safe; every change is saved immediately.
pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Settings>,
    listeners: Mutex<Vec<Listener>>,
}

impl SettingsStore {
    /// Loads settings from `path`, or creates the file with defaults.
    ///
    /// Values of the wrong type are dropped one by one, like Taiga, which reads
    /// each key separately. A file that isn't a JSON object at all is renamed
    /// to `<name>.corrupt` and replaced with defaults, so a damaged file
    /// doesn't stop the app from starting.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let settings = match std::fs::read(&path) {
            // Windows editors (and PowerShell) may add a UTF-8 byte order mark.
            Ok(bytes) => match parse_lenient(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes))
            {
                Some(mut settings) => {
                    settings.normalize();
                    settings
                }
                None => {
                    std::fs::rename(&path, corrupt_path(&path))?;
                    Settings::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(e.into()),
        };
        let store = Self {
            path,
            current: RwLock::new(settings),
            listeners: Mutex::new(Vec::new()),
        };
        if !store.path.exists() {
            store.save(&store.get())?;
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A copy of the current settings.
    pub fn get(&self) -> Settings {
        self.current
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Changes settings with `f`, saves them if anything changed, and notifies
    /// listeners. Returns the changed keys.
    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<BTreeSet<String>, Error> {
        let mut new = self.get();
        f(&mut new);
        self.replace(new)
    }

    /// Replaces all settings. Returns the changed keys.
    pub fn replace(&self, mut new: Settings) -> Result<BTreeSet<String>, Error> {
        new.normalize();
        let changed = {
            let mut current = self.current.write().unwrap_or_else(|e| e.into_inner());
            let changed = changed_keys(&current, &new)?;
            if changed.is_empty() {
                return Ok(changed);
            }
            self.save(&new)?;
            *current = new.clone();
            changed
        };
        let listeners = self.listeners.lock().unwrap_or_else(|e| e.into_inner());
        for listener in listeners.iter() {
            listener(&new, &changed);
        }
        Ok(changed)
    }

    /// Sets the given keys (e.g. `track.update.delay`), skipping unknown keys
    /// and values of the wrong type. Returns the changed keys.
    pub fn merge(
        &self,
        values: &serde_json::Map<String, Value>,
    ) -> Result<BTreeSet<String>, Error> {
        let Value::Object(mut current) = serde_json::to_value(self.get())? else {
            return Ok(BTreeSet::new());
        };
        for (key, value) in values {
            if current.contains_key(key) {
                current.insert(key.clone(), value.clone());
            }
        }
        let bytes = serde_json::to_vec(&Value::Object(current))?;
        match parse_lenient(&bytes) {
            Some(new) => self.replace(new),
            None => Ok(BTreeSet::new()),
        }
    }

    /// Registers a function to call after every change.
    pub fn subscribe(
        &self,
        listener: impl Fn(&Settings, &BTreeSet<String>) + Send + Sync + 'static,
    ) {
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Box::new(listener));
    }

    /// Writes to a temporary file and renames it, so a crash can't leave a
    /// half-written file.
    fn save(&self, settings: &Settings) -> Result<(), Error> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(settings)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

/// Parses a settings object, skipping keys whose values have the wrong type.
/// `None` if it isn't a JSON object.
fn parse_lenient(bytes: &[u8]) -> Option<Settings> {
    let Value::Object(file) = serde_json::from_slice::<Value>(bytes).ok()? else {
        return None;
    };
    if let Ok(settings) = serde_json::from_value::<Settings>(Value::Object(file.clone())) {
        return Some(settings);
    }
    let Value::Object(mut accepted) = serde_json::to_value(Settings::default()).ok()? else {
        return None;
    };
    for (key, value) in file {
        let mut candidate = accepted.clone();
        candidate.insert(key.clone(), value.clone());
        if serde_json::from_value::<Settings>(Value::Object(candidate)).is_ok() {
            accepted.insert(key, value);
        }
    }
    serde_json::from_value(Value::Object(accepted)).ok()
}

fn corrupt_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".corrupt");
    path.with_file_name(name)
}

/// Top-level keys whose values differ, e.g. `track.update.delay`.
fn changed_keys(old: &Settings, new: &Settings) -> Result<BTreeSet<String>, Error> {
    let (Value::Object(old), Value::Object(new)) =
        (serde_json::to_value(old)?, serde_json::to_value(new)?)
    else {
        return Ok(BTreeSet::new());
    };
    Ok(old
        .keys()
        .chain(new.keys())
        .filter(|key| old.get(*key) != new.get(*key))
        .cloned()
        .collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn merge_skips_unknown_keys_and_bad_values() {
        let dir =
            std::env::temp_dir().join(format!("patchouli-settings-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SettingsStore::load(dir.join("settings.json")).unwrap();
        let values = serde_json::json!({
            "track.update.delay": 45,
            "sync.service": "myanimelist",
            "track.detection.interval": "fast",
            "no.such.key": true,
        });
        let changed = store.merge(values.as_object().unwrap()).unwrap();
        assert!(changed.contains("track.update.delay"));
        assert!(!changed.contains("track.detection.interval"));
        assert!(!changed.contains("no.such.key"));
        assert_eq!(
            store.get().sync_service,
            patchouli_core::ServiceId::MyAnimeList
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use crate::{FILE_NAME, UPDATE_DELAY_MAX};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("patchouli-settings-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn creates_file_with_defaults() {
        let dir = TempDir::new("create");
        let path = dir.0.join(FILE_NAME);
        let store = SettingsStore::load(&path).unwrap();
        assert!(path.exists());
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn update_saves_and_reports_changed_keys() {
        let dir = TempDir::new("update");
        let path = dir.0.join(FILE_NAME);
        let store = SettingsStore::load(&path).unwrap();

        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        store.subscribe(move |settings, keys| {
            assert_eq!(settings.update_delay_secs, UPDATE_DELAY_MAX);
            assert!(keys.contains("track.update.delay"));
            seen.fetch_add(1, Ordering::SeqCst);
        });

        let changed = store
            .update(|s| {
                s.update_delay_secs = 1_000_000; // clamped
                s.app_hide_nsfw = false;
            })
            .unwrap();
        assert_eq!(
            changed.into_iter().collect::<Vec<_>>(),
            ["app.hideNsfw", "track.update.delay"]
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // No change: nothing saved, no notification.
        assert!(store.update(|_| {}).unwrap().is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let reloaded = SettingsStore::load(&path).unwrap();
        assert_eq!(reloaded.get().update_delay_secs, UPDATE_DELAY_MAX);
        assert!(!reloaded.get().app_hide_nsfw);
    }

    #[test]
    fn keeps_unknown_keys_on_save() {
        let dir = TempDir::new("unknown");
        let path = dir.0.join(FILE_NAME);
        std::fs::write(&path, r#"{"app.style": "fusion", "sync.service": "kitsu"}"#).unwrap();

        let store = SettingsStore::load(&path).unwrap();
        store.update(|s| s.discord_enabled = true).unwrap();

        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["app.style"], "fusion");
        assert_eq!(saved["sync.service"], "kitsu");
        assert_eq!(saved["sharing.discord.enabled"], true);
    }

    #[test]
    fn accepts_byte_order_mark() {
        let dir = TempDir::new("bom");
        let path = dir.0.join(FILE_NAME);
        std::fs::write(&path, b"\xEF\xBB\xBF{\"track.update.delay\": 30}").unwrap();
        let store = SettingsStore::load(&path).unwrap();
        assert_eq!(store.get().update_delay_secs, 30);
        assert!(!dir.0.join("settings.json.corrupt").exists());
    }

    #[test]
    fn drops_only_values_of_the_wrong_type() {
        let dir = TempDir::new("lenient");
        let path = dir.0.join(FILE_NAME);
        std::fs::write(
            &path,
            r#"{"track.update.delay": 30, "track.detection.disabledPlayers": [{"x": 1}], "app.style": "fusion"}"#,
        )
        .unwrap();
        let store = SettingsStore::load(&path).unwrap();
        let settings = store.get();
        assert_eq!(settings.update_delay_secs, 30);
        assert!(settings.disabled_players.is_empty());
        assert_eq!(settings.extra["app.style"], "fusion");
        assert!(!dir.0.join("settings.json.corrupt").exists());
    }

    #[test]
    fn replaces_corrupt_file_with_defaults() {
        let dir = TempDir::new("corrupt");
        let path = dir.0.join(FILE_NAME);
        std::fs::write(&path, "{ not json").unwrap();

        let store = SettingsStore::load(&path).unwrap();
        assert_eq!(store.get(), Settings::default());
        assert!(dir.0.join("settings.json.corrupt").exists());
        assert!(path.exists());
    }
}
