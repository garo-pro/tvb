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

/// Where updates come from, and the minisign key their zips must be signed
/// with. The matching secret key lives only in the GitHub "release" environment.
pub const UPDATE_REPO: &str = "garo-pro/tvb";
pub const UPDATE_PUBLIC_KEY: &str = "RWRfw3J3Qfvn4DSdkahuDb3Jh3trSY7/96z2BX92ji3P7hMumzlcLt+t";

const KEYRING_SERVICE: &str = "tvb-thingiverse";
const KEYRING_USER: &str = "api-token";

pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "tvb")
}

/// Which builds the updater offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    /// Tagged releases.
    #[default]
    Stable,
    /// A build of every change to the main branch.
    Development,
}

impl UpdateChannel {
    pub const ALL: [Self; 2] = [Self::Stable, Self::Development];

    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "Stable releases",
            Self::Development => "Development builds (newest changes, may be unstable)",
        }
    }
}

/// Result order for searches, as offered by the Thingiverse search API.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchSort {
    #[default]
    Relevant,
    Popular,
    Makes,
    Newest,
}

impl SearchSort {
    pub const ALL: [Self; 4] = [Self::Relevant, Self::Popular, Self::Makes, Self::Newest];

    pub fn label(self) -> &'static str {
        match self {
            Self::Relevant => "Relevance",
            Self::Popular => "Most popular",
            Self::Makes => "Most makes",
            Self::Newest => "Newest",
        }
    }

    /// The `sort` query value the API expects.
    pub fn api_value(self) -> &'static str {
        match self {
            Self::Relevant => "relevant",
            Self::Popular => "popular",
            Self::Makes => "makes",
            Self::Newest => "newest",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where downloads go; each thing gets its own subfolder.
    pub download_dir: Option<PathBuf>,
    pub update_channel: UpdateChannel,
    pub check_updates_on_startup: bool,
    /// Last sort order chosen in the main window.
    pub search_sort: SearchSort,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: None,
            update_channel: UpdateChannel::default(),
            check_updates_on_startup: true,
            search_sort: SearchSort::default(),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_files_get_update_defaults() {
        let s: Settings = serde_json::from_str(r#"{"download_dir": "D:/x"}"#).unwrap();
        assert_eq!(s.update_channel, UpdateChannel::Stable);
        assert!(s.check_updates_on_startup);
        assert_eq!(s.search_sort, SearchSort::Relevant);
        let s: Settings = serde_json::from_str(r#"{"update_channel": "development"}"#).unwrap();
        assert_eq!(s.update_channel, UpdateChannel::Development);
    }

    #[test]
    fn search_sort_round_trips_and_matches_api_values() {
        for sort in SearchSort::ALL {
            let json = serde_json::to_string(&sort).unwrap();
            assert_eq!(json, format!("\"{}\"", sort.api_value()));
            assert_eq!(serde_json::from_str::<SearchSort>(&json).unwrap(), sort);
        }
    }
}
