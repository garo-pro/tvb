//! Settings file and the API token in Windows Credential Manager.

use std::fs;
use std::path::PathBuf;

use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};

/// Who makes the app and where to get help (API terms 3e and 3h).
pub const AUTHOR_URL: &str = "https://github.com/garo-pro";
pub const PROJECT_URL: &str = "https://github.com/garo-pro/tvb";
pub const SUPPORT_URL: &str = "https://github.com/garo-pro/tvb/issues";
pub const PRIVACY_URL: &str = "https://github.com/garo-pro/tvb/blob/main/docs/privacy-policy.md";
pub const EULA_URL: &str = "https://github.com/garo-pro/tvb/blob/main/docs/eula.md";

/// Cached Content must be deleted after at most 30 days (API terms 4b).
pub const CACHE_MAX_AGE: std::time::Duration = std::time::Duration::from_hours(720);

const KEYRING_SERVICE: &str = "tvb-thingiverse";
const KEYRING_USER: &str = "api-token";

pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "tvb")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where downloads go; each thing gets its own subfolder.
    pub download_dir: Option<PathBuf>,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        project_dirs().map(|d| d.config_dir().join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else { return Ok(()) };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)
    }

    pub fn download_dir(&self) -> PathBuf {
        self.download_dir.clone().unwrap_or_else(|| {
            UserDirs::new()
                .and_then(|u| u.download_dir().map(std::path::Path::to_path_buf))
                .unwrap_or_else(std::env::temp_dir)
                .join("Thingiverse")
        })
    }
}

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
}

/// The stored token, or `None` if none is saved or the store is unavailable.
pub fn load_token() -> Option<String> {
    entry().ok()?.get_password().ok().filter(|t| !t.trim().is_empty())
}

pub fn save_token(token: &str) -> Result<(), String> {
    entry().and_then(|e| e.set_password(token.trim())).map_err(|e| e.to_string())
}

pub fn delete_token() -> Result<(), String> {
    match entry().and_then(|e| e.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
