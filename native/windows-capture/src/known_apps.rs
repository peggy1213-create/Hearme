//! Meeting apps the helper can detect and capture, matched by **executable name** (the
//! Windows analogue of the macOS helper's bundle IDs). Edit `known-apps.json`, not this
//! file, to add apps; the JSON is embedded as the fallback and may also be overridden by
//! a `known-apps.json` placed next to the binary.

use std::path::PathBuf;

use serde::Deserialize;

/// A meeting app and the executable names that belong to it. Matching is
/// case-insensitive (Windows file names are case-insensitive). Only the app's root
/// process needs to match — process-loopback capture with
/// `INCLUDE_TARGET_PROCESS_TREE` then covers its children (Teams' WebView2 processes,
/// browser renderers).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct KnownApp {
    pub id: String,
    pub name: String,
    pub kind: String, // "app" | "browser"
    #[serde(rename = "exeNames")]
    pub exe_names: Vec<String>,
}

impl KnownApp {
    /// True if `exe_name` (e.g. `"chrome.exe"`) belongs to this app.
    pub fn matches(&self, exe_name: &str) -> bool {
        self.exe_names
            .iter()
            .any(|e| e.eq_ignore_ascii_case(exe_name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct KnownAppsConfig {
    pub version: u32,
    pub apps: Vec<KnownApp>,
}

const EMBEDDED: &str = include_str!("known-apps.json");

impl KnownAppsConfig {
    /// Load the config, preferring a `known-apps.json` next to the executable (the
    /// bare-sidecar case), falling back to the embedded default.
    pub fn load() -> KnownAppsConfig {
        if let Some(path) = sidecar_json_path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<KnownAppsConfig>(&text) {
                    return cfg;
                }
            }
        }
        Self::embedded()
    }

    /// The embedded default, also the fallback when a sidecar file is missing or bad.
    pub fn embedded() -> KnownAppsConfig {
        serde_json::from_str(EMBEDDED).expect("embedded known-apps.json is valid")
    }

    /// Find the known app an executable name belongs to, if any.
    pub fn match_exe(&self, exe_name: &str) -> Option<&KnownApp> {
        self.apps.iter().find(|a| a.matches(exe_name))
    }
}

fn sidecar_json_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join("known-apps.json");
    candidate.exists().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_config_parses() {
        let cfg = KnownAppsConfig::embedded();
        assert_eq!(cfg.version, 1);
        assert!(cfg.apps.iter().any(|a| a.id == "zoom"));
        assert!(cfg.apps.iter().any(|a| a.id == "teams"));
        assert!(cfg.apps.iter().any(|a| a.id == "meet-chrome"));
    }

    #[test]
    fn matches_exe_case_insensitively() {
        let cfg = KnownAppsConfig::embedded();
        let chrome = cfg.match_exe("CHROME.EXE").unwrap();
        assert_eq!(chrome.id, "meet-chrome");
        let teams = cfg.match_exe("ms-teams.exe").unwrap();
        assert_eq!(teams.id, "teams");
    }

    #[test]
    fn unknown_exe_does_not_match() {
        let cfg = KnownAppsConfig::embedded();
        assert!(cfg.match_exe("notepad.exe").is_none());
    }
}
