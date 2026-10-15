//! Where PinIt keeps its files.
//!
//! Normal install: `%APPDATA%\com.pinit.next` (settings, layout) and
//! `%LOCALAPPDATA%\com.pinit.next` (image cache, inbox, WebView2 profile).
//!
//! Portable: if a file named `portable.flag` sits next to the executable,
//! everything lives in `PinIt-data\` beside it — nothing is written to
//! AppData, so the exe can run from a USB stick or a locked-down PC.

use std::path::{Path, PathBuf};

pub const IDENTIFIER: &str = "com.pinit.next";
pub const PORTABLE_FLAG: &str = "portable.flag";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub portable: bool,
    /// settings.json, layout.json, native-host manifest, extracted extension
    pub data: PathBuf,
    /// downloaded image cache
    pub cache: PathBuf,
    /// images staged by the browser bridge
    pub inbox: PathBuf,
    /// WebView2 profile override (portable only; installed apps use the default)
    pub webview: Option<PathBuf>,
}

impl Paths {
    pub fn resolve(exe: &Path, appdata: Option<PathBuf>, local_appdata: Option<PathBuf>) -> Self {
        let exe_dir = exe.parent().unwrap_or(Path::new("."));
        if exe_dir.join(PORTABLE_FLAG).is_file() {
            let root = exe_dir.join("PinIt-data");
            return Self {
                portable: true,
                data: root.join("data"),
                cache: root.join("cache"),
                inbox: root.join("inbox"),
                webview: Some(root.join("webview")),
            };
        }
        let temp = std::env::temp_dir();
        let roaming = appdata.unwrap_or_else(|| temp.clone()).join(IDENTIFIER);
        let local = local_appdata.unwrap_or(temp).join(IDENTIFIER);
        Self { portable: false, data: roaming, cache: local.join("images"), inbox: local.join("inbox"), webview: None }
    }

    pub fn detect() -> Self {
        let exe = std::env::current_exe().unwrap_or_default();
        Self::resolve(
            &exe,
            std::env::var_os("APPDATA").map(PathBuf::from),
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pinit-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn installed_layout_uses_appdata_and_localappdata() {
        let d = temp("installed");
        let p = Paths::resolve(&d.join("pinit.exe"), Some(PathBuf::from(r"C:\R")), Some(PathBuf::from(r"C:\L")));
        assert!(!p.portable);
        assert_eq!(p.data, PathBuf::from(r"C:\R").join(IDENTIFIER));
        assert_eq!(p.cache, PathBuf::from(r"C:\L").join(IDENTIFIER).join("images"));
        assert_eq!(p.inbox, PathBuf::from(r"C:\L").join(IDENTIFIER).join("inbox"));
        assert!(p.webview.is_none());
    }

    #[test]
    fn flag_file_next_to_exe_makes_everything_local() {
        let d = temp("portable");
        std::fs::write(d.join(PORTABLE_FLAG), b"").unwrap();
        let p = Paths::resolve(&d.join("pinit.exe"), Some(PathBuf::from(r"C:\R")), Some(PathBuf::from(r"C:\L")));
        assert!(p.portable);
        for dir in [&p.data, &p.cache, &p.inbox, p.webview.as_ref().unwrap()] {
            assert!(dir.starts_with(d.join("PinIt-data")), "{dir:?} escapes the portable root");
        }
    }
}
