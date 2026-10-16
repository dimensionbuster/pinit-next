//! The browser extension is embedded in the executable so a single portable
//! .exe can hand the user a "Load unpacked" folder without any installer.

use std::io;
use std::path::{Path, PathBuf};

const FILES: [(&str, &[u8]); 7] = [
    ("manifest.json", include_bytes!("../../extension/manifest.json")),
    ("background.js", include_bytes!("../../extension/background.js")),
    ("offscreen.html", include_bytes!("../../extension/offscreen.html")),
    ("offscreen.js", include_bytes!("../../extension/offscreen.js")),
    ("icons/icon16.png", include_bytes!("../../extension/icons/icon16.png")),
    ("icons/icon48.png", include_bytes!("../../extension/icons/icon48.png")),
    ("icons/icon128.png", include_bytes!("../../extension/icons/icon128.png")),
];

/// Writes the extension into `dest` (rewriting only changed files) and returns its manifest path.
pub fn extract(dest: &Path) -> io::Result<PathBuf> {
    for (name, bytes) in FILES {
        let path = dest.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::read(&path).ok().as_deref() != Some(bytes) {
            std::fs::write(&path, bytes)?;
        }
    }
    Ok(dest.join("manifest.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracted_files_match_the_repo_extension_and_are_idempotent() {
        let dest = std::env::temp_dir().join(format!("pinit-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        let manifest = extract(&dest).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("extension");
        for (name, _) in FILES {
            assert_eq!(std::fs::read(dest.join(name)).unwrap(), std::fs::read(repo.join(name)).unwrap(), "{name}");
        }
        assert_eq!(manifest, dest.join("manifest.json"));
        extract(&dest).unwrap(); // second run must not fail
    }
}
