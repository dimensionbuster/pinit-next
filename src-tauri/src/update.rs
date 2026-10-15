//! Update notice: asks GitHub for the latest published release and compares versions.
//!
//! Notify-only. The portable build has no sanctioned way to replace its own running exe, so the UI
//! just links to the release page. Draft releases are invisible to `releases/latest`, so a new
//! version only shows up once the maintainer publishes the draft.

use std::time::Duration;

use serde::Serialize;

pub const REPO: &str = "dimensionbuster/pinit-next";
const USER_AGENT: &str = concat!("PinIt/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub url: String,
}

fn parse_version(v: &str) -> Vec<u64> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("");
    core.split('.').map(|n| n.parse().unwrap_or(0)).collect()
}

/// True when `latest` is strictly newer than `current` (missing parts count as 0).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse_version(latest), parse_version(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// Only release pages of this repo may be opened from the UI.
pub fn is_release_url(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/"))
}

fn info_from_release(body: &serde_json::Value, current: &str) -> Option<UpdateInfo> {
    let tag = body.get("tag_name")?.as_str()?;
    let url = body.get("html_url")?.as_str()?;
    if !is_release_url(url) {
        return None;
    }
    let latest = tag.trim_start_matches('v').to_string();
    Some(UpdateInfo { available: is_newer(&latest, current), current: current.to_string(), latest, url: url.to_string() })
}

/// Blocking network call; run it off the UI thread.
pub fn check_latest() -> Result<UpdateInfo, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
    let mut res = agent
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let text = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    info_from_release(&body, env!("CARGO_PKG_VERSION")).ok_or_else(|| "unexpected release response".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("0.2.0-rc1", "0.2.0"));
    }

    #[test]
    fn parses_release_json() {
        let ok = serde_json::json!({"tag_name": "v0.2.0", "html_url": "https://github.com/dimensionbuster/pinit-next/releases/tag/v0.2.0"});
        let info = info_from_release(&ok, "0.1.0").unwrap();
        assert!(info.available);
        assert_eq!(info.latest, "0.2.0");

        let foreign = serde_json::json!({"tag_name": "v9.0.0", "html_url": "https://evil.example/x"});
        assert!(info_from_release(&foreign, "0.1.0").is_none());
    }
}
