//! Explorer right-click entry for local image files ("PinIt에 고정").
//!
//! Registered per user under `SystemFileAssociations\image`, so it needs no admin rights and
//! covers every image type Windows knows. The verb runs `pinit.exe "<file>"`, which the
//! single-instance plugin forwards to the running app (see `cli.rs`).

use std::path::Path;

/// Debug builds use their own verb so a dev run never overwrites a real install's entry.
const VERB: &str = if cfg!(debug_assertions) { "PinItDev" } else { "PinIt" };
const LABEL: &str = if cfg!(debug_assertions) { "PinIt DEV에 고정" } else { "PinIt에 고정" };

fn verb_key() -> String {
    format!(r"HKCU\Software\Classes\SystemFileAssociations\image\shell\{VERB}")
}

pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// Idempotent; runs on every start so a moved/updated exe stays registered.
pub fn register(exe: &Path) {
    #[cfg(windows)]
    {
        let key = verb_key();
        let icon = format!("{},0", exe.display());
        let command = command_line(exe);
        let entries: [(String, Option<&str>, String); 3] = [
            (key.clone(), None, LABEL.to_string()),
            (key.clone(), Some("Icon"), icon),
            (format!(r"{key}\command"), None, command),
        ];
        for (k, name, data) in entries {
            reg_add(&k, name, &data);
        }
    }
    #[cfg(not(windows))]
    let _ = exe;
}

#[cfg(windows)]
fn reg_add(key: &str, name: Option<&str>, data: &str) {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("reg");
    cmd.args(["add", key]);
    match name {
        Some(n) => cmd.args(["/v", n]),
        None => cmd.arg("/ve"),
    };
    let _ = cmd.args(["/t", "REG_SZ", "/d", data, "/f"]).creation_flags(0x0800_0000).output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_exe_and_placeholder() {
        assert_eq!(command_line(Path::new(r"C:\Program Files\PinIt\pinit.exe")), r#""C:\Program Files\PinIt\pinit.exe" "%1""#);
    }
}
