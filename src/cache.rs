//! Plain-JSON response cache under the user's local cache directory.
//!
//! Each entry is one file, `<kind>/<key>.json`, holding the save time and the
//! payload. Failures are never fatal: a broken or unreadable entry is a miss.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    saved_at: u64,
    data: T,
}

#[derive(Clone)]
pub struct Cache {
    root: Option<PathBuf>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Cache {
    /// Cache in the per-user cache directory, or a no-op cache if there is none.
    pub fn open() -> Self {
        let root = crate::config::project_dirs().map(|d| d.cache_dir().to_path_buf());
        Self { root }
    }

    #[cfg(test)]
    pub fn at(root: PathBuf) -> Self {
        Self { root: Some(root) }
    }

    fn path(&self, kind: &str, key: &str) -> Option<PathBuf> {
        let key = crate::api::safe_file_name(key);
        self.root.as_ref().map(|r| r.join(kind).join(format!("{key}.json")))
    }

    /// Returns the entry if present, parseable, and younger than `max_age`.
    pub fn get<T: DeserializeOwned>(&self, kind: &str, key: &str, max_age: Duration) -> Option<T> {
        let text = fs::read_to_string(self.path(kind, key)?).ok()?;
        let env: Envelope<T> = serde_json::from_str(&text).ok()?;
        (now_secs().saturating_sub(env.saved_at) <= max_age.as_secs()).then_some(env.data)
    }

    /// Stores an entry, writing a temp file first so readers never see half a file.
    pub fn put<T: Serialize>(&self, kind: &str, key: &str, data: &T) {
        let Some(path) = self.path(kind, key) else { return };
        let Some(dir) = path.parent() else { return };
        if fs::create_dir_all(dir).is_err() {
            return;
        }
        let env = Envelope { saved_at: now_secs(), data };
        if let Ok(text) = serde_json::to_string(&env) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, &path).is_err() {
                let _ = fs::remove_file(&tmp);
            }
        }
    }

    /// Deletes entries (and stray temp files) last written more than
    /// `max_age` ago. The API terms require cached Content to be deleted
    /// after at most 30 days unused. Returns how many files were removed.
    pub fn prune(&self, max_age: Duration) -> usize {
        let Some(root) = &self.root else { return 0 };
        let Ok(kinds) = fs::read_dir(root) else { return 0 };
        let mut removed = 0;
        for kind in kinds.flatten().filter(|e| e.path().is_dir()) {
            let Ok(entries) = fs::read_dir(kind.path()) else { continue };
            for entry in entries.flatten() {
                let expired = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    // Unreadable timestamps count as expired: when in doubt, delete.
                    .is_none_or(|age| age > max_age);
                if expired && fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        removed
    }

    /// Deletes every cached entry.
    pub fn clear(&self) -> std::io::Result<()> {
        match &self.root {
            Some(r) if r.exists() => fs::remove_dir_all(r),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_expiry() {
        let dir = std::env::temp_dir().join(format!("tvb-cache-test-{}", std::process::id()));
        let c = Cache::at(dir.clone());
        c.put("things", "42", &vec![1, 2, 3]);
        let got: Option<Vec<i32>> = c.get("things", "42", Duration::from_mins(1));
        assert_eq!(got, Some(vec![1, 2, 3]));
        assert!(c.get::<Vec<i32>>("things", "43", Duration::from_mins(1)).is_none());
        // Wrong type is a miss, not a panic.
        assert!(c.get::<String>("things", "42", Duration::from_mins(1)).is_none());
        c.clear().unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn prune_removes_only_old_entries() {
        let dir = std::env::temp_dir().join(format!("tvb-prune-test-{}", std::process::id()));
        let c = Cache::at(dir.clone());
        c.put("things", "old", &1);
        c.put("things", "new", &2);
        let old = dir.join("things").join("old.json");
        let month_ago = SystemTime::now() - Duration::from_hours(744);
        fs::File::options().write(true).open(&old).unwrap().set_modified(month_ago).unwrap();

        assert_eq!(c.prune(Duration::from_hours(720)), 1);
        assert!(!old.exists());
        assert_eq!(c.get::<i32>("things", "new", Duration::from_mins(1)), Some(2));
        c.clear().unwrap();
    }
}
