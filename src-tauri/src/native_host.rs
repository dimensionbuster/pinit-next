//! Chrome/Edge Native Messaging bridge.
//!
//! The browser launches this same executable with the extension origin as its
//! first argument. In that mode we read one framed JSON message from stdin,
//! hand the work to the running PinIt (single-instance forwards our argv) and
//! answer on stdout. No network port is ever opened.
//!
//! Message (v1): `{"v":1,"type":"open","url":"https://..","referer":"https://.."}`
//!            or `{"v":1,"type":"open","mime":"image/png","data_b64":".."}`

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use pinit_core::image_size::image_size;
use serde_json::{json, Value};

use crate::b64;

// IDs are derived from the public `key` in extension/manifest.json (prod) and
// scripts/dev-extension.json (dev); extension/extension.test.mjs checks both.
pub const PROD_EXTENSION_ID: &str = "ommodopihncklghppcleifpgdbhpkmbg";
pub const DEV_EXTENSION_ID: &str = "ncpmjchiacnkhdfniogljjghbndahaio";
pub const PROD_HOST_NAME: &str = "com.pinit.next";
pub const DEV_HOST_NAME: &str = "com.pinit.next.dev";

/// Debug builds speak to the dev extension under their own host name, so a developer
/// can run the real PinIt and the debug build (each with its own extension) side by side.
pub const EXTENSION_ID: &str = if cfg!(debug_assertions) { DEV_EXTENSION_ID } else { PROD_EXTENSION_ID };
pub const HOST_NAME: &str = if cfg!(debug_assertions) { DEV_HOST_NAME } else { PROD_HOST_NAME };

const MAX_MESSAGE_BYTES: usize = 48 * 1024 * 1024;
const MAX_URL_LEN: usize = 8192;

pub fn is_native_host_launch(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a.starts_with("chrome-extension://"))
}

// ------------------------------------------------------------------ framing

pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

// ----------------------------------------------------------------- handling

/// Side effects of the host, injectable for tests.
pub trait HostEnv {
    /// Start (or signal) the PinIt app with these CLI args.
    fn launch(&self, args: Vec<String>) -> io::Result<()>;
    /// Directory where browser-supplied image bytes are staged.
    fn inbox_dir(&self) -> PathBuf;
}

fn fnv(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn extension_for(data: &[u8]) -> &'static str {
    match data {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [b'G', b'I', b'F', ..] => "gif",
        [0xFF, 0xD8, ..] => "jpg",
        [b'B', b'M', ..] => "bmp",
        _ => "webp",
    }
}

fn open_args(msg: &Value, env: &dyn HostEnv) -> Result<Vec<String>, String> {
    if msg.get("v").and_then(Value::as_u64) != Some(1) || msg.get("type").and_then(Value::as_str) != Some("open") {
        return Err("unsupported message".into());
    }
    if let Some(url) = msg.get("url").and_then(Value::as_str) {
        if url.len() > MAX_URL_LEN || !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("only http(s) image URLs are accepted".into());
        }
        let mut args = vec!["--open-url".to_string(), url.to_string()];
        if let Some(r) = msg.get("referer").and_then(Value::as_str) {
            if r.len() <= MAX_URL_LEN && (r.starts_with("http://") || r.starts_with("https://")) {
                args.extend(["--referer".to_string(), r.to_string()]);
            }
        }
        return Ok(args);
    }
    if let Some(b64_data) = msg.get("data_b64").and_then(Value::as_str) {
        let data = b64::decode(b64_data).ok_or("invalid base64")?;
        image_size(&data).ok_or("not a supported image")?;
        let dir = env.inbox_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{:016x}.{}", fnv(&data), extension_for(&data)));
        std::fs::write(&path, &data).map_err(|e| e.to_string())?;
        return Ok(vec![path.to_string_lossy().into_owned()]);
    }
    Err("message has neither url nor data_b64".into())
}

/// Returns the JSON reply to send back to the extension.
pub fn handle_message(bytes: &[u8], env: &dyn HostEnv) -> Value {
    let result = serde_json::from_slice::<Value>(bytes)
        .map_err(|e| format!("bad json: {e}"))
        .and_then(|msg| open_args(&msg, env))
        .and_then(|args| env.launch(args).map_err(|e| format!("cannot start PinIt: {e}")));
    match result {
        Ok(()) => json!({ "ok": true }),
        Err(error) => json!({ "ok": false, "error": error }),
    }
}

// --------------------------------------------------------------- real world

struct RealEnv;

impl HostEnv for RealEnv {
    fn launch(&self, args: Vec<String>) -> io::Result<()> {
        #[cfg(windows)]
        stop_inheriting_stdio();
        let mut cmd = std::process::Command::new(std::env::current_exe()?);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        }
        // Diagnostics: PINIT_HOST_LOG=<file> captures the launched app's stderr.
        let stderr = match std::env::var_os("PINIT_HOST_LOG").map(std::fs::File::create) {
            Some(Ok(f)) => std::process::Stdio::from(f),
            _ => std::process::Stdio::null(),
        };
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(stderr)
            .spawn()
            .map(|_| ())
    }

    fn inbox_dir(&self) -> PathBuf {
        crate::paths::Paths::detect().inbox
    }
}

/// `Command::spawn` on Windows inherits *every* inheritable handle, including the
/// browser's stdin/stdout pipes. The long-lived app would then keep those pipes
/// open after this host exits. Mark our std handles non-inheritable first.
#[cfg(windows)]
fn stop_inheriting_stdio() {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn SetHandleInformation(h: *mut c_void, mask: u32, flags: u32) -> i32;
    }
    const HANDLE_FLAG_INHERIT: u32 = 1;
    // STD_INPUT_HANDLE = -10, STD_OUTPUT_HANDLE = -11, STD_ERROR_HANDLE = -12
    for n in [-10i32, -11, -12] {
        // SAFETY: plain Win32 calls on handles owned by this process; failures are ignored.
        unsafe {
            let h = GetStdHandle(n as u32);
            if !h.is_null() && h as isize != -1 {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

/// Entry point for host mode. Returns the process exit code.
pub fn run() -> i32 {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let reply = match read_frame(&mut stdin) {
        Ok(Some(bytes)) => handle_message(&bytes, &RealEnv),
        Ok(None) => return 0,
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    };
    match write_frame(&mut stdout, reply.to_string().as_bytes()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

// ------------------------------------------------------------- registration

pub fn manifest_json(exe: &Path) -> String {
    serde_json::to_string_pretty(&json!({
        "name": HOST_NAME,
        "description": "PinIt browser bridge",
        "path": exe.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    }))
    .expect("static json")
}

const REGISTRY_KEYS: [&str; 3] = [
    r"HKCU\Software\Google\Chrome\NativeMessagingHosts",
    r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts",
    r"HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
];

/// Idempotent: writes the host manifest and points each browser's per-user key at it.
/// Runs on every start so a moved/updated executable stays registered.
pub fn register(exe: &Path, manifest_dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(manifest_dir)?;
    let manifest_path = manifest_dir.join(format!("{HOST_NAME}.json"));
    let json = manifest_json(exe);
    if std::fs::read_to_string(&manifest_path).ok().as_deref() != Some(json.as_str()) {
        std::fs::write(&manifest_path, json)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        for base in REGISTRY_KEYS {
            let _ = std::process::Command::new("reg")
                .args(["add", &format!(r"{base}\{HOST_NAME}"), "/ve", "/t", "REG_SZ", "/d"])
                .arg(&manifest_path)
                .args(["/f"])
                .creation_flags(0x0800_0000)
                .output();
        }
    }
    let _ = REGISTRY_KEYS; // referenced on non-Windows builds too
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::Mutex;

    struct FakeEnv {
        dir: PathBuf,
        launched: Mutex<Vec<Vec<String>>>,
        fail: bool,
    }

    impl FakeEnv {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("pinit-nm-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self { dir, launched: Mutex::new(vec![]), fail: false }
        }
    }

    impl HostEnv for FakeEnv {
        fn launch(&self, args: Vec<String>) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("boom"));
            }
            self.launched.lock().unwrap().push(args);
            Ok(())
        }
        fn inbox_dir(&self) -> PathBuf {
            self.dir.clone()
        }
    }

    fn png_b64() -> String {
        // 1x1-ish PNG header is enough for size sniffing: 640x480
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(640u32.to_be_bytes());
        v.extend(480u32.to_be_bytes());
        // tiny base64 encoder for the test
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in v.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= c.len() {
                    out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn frames_round_trip_and_eof_is_none() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"{\"a\":1}").unwrap();
        assert_eq!(&buf[..4], &7u32.to_le_bytes());
        assert_eq!(read_frame(&mut Cursor::new(buf)).unwrap().unwrap(), b"{\"a\":1}");
        assert!(read_frame(&mut Cursor::new(Vec::new())).unwrap().is_none());
    }

    #[test]
    fn oversized_frames_are_rejected_before_allocating() {
        let mut buf = (u32::MAX).to_le_bytes().to_vec();
        buf.extend([0u8; 8]);
        assert!(read_frame(&mut Cursor::new(buf)).is_err());
    }

    #[test]
    fn url_message_launches_app_with_url_and_referer() {
        let env = FakeEnv::new("url");
        let msg = br#"{"v":1,"type":"open","url":"https://e.com/a.jpg","referer":"https://e.com/page"}"#;
        assert_eq!(handle_message(msg, &env), json!({"ok": true}));
        assert_eq!(
            env.launched.lock().unwrap()[0],
            ["--open-url", "https://e.com/a.jpg", "--referer", "https://e.com/page"]
        );
    }

    #[test]
    fn rejects_non_http_urls_and_bad_referers_are_dropped() {
        let env = FakeEnv::new("bad");
        for url in ["file:///C:/secret.png", "javascript:alert(1)", "ftp://x/a.png", ""] {
            let m = json!({"v":1,"type":"open","url":url}).to_string();
            assert_eq!(handle_message(m.as_bytes(), &env)["ok"], false, "{url}");
        }
        let m = json!({"v":1,"type":"open","url":"https://e.com/a.jpg","referer":"javascript:x"}).to_string();
        assert_eq!(handle_message(m.as_bytes(), &env)["ok"], true);
        assert_eq!(env.launched.lock().unwrap().last().unwrap().len(), 2); // no --referer
    }

    #[test]
    fn data_message_is_staged_as_a_file_and_opened() {
        let env = FakeEnv::new("data");
        let m = json!({"v":1,"type":"open","mime":"image/png","data_b64":png_b64()}).to_string();
        assert_eq!(handle_message(m.as_bytes(), &env), json!({"ok": true}));
        let args = env.launched.lock().unwrap()[0].clone();
        assert_eq!(args.len(), 1);
        assert!(args[0].ends_with(".png") && Path::new(&args[0]).is_file());
    }

    #[test]
    fn non_image_data_and_unknown_messages_fail() {
        let env = FakeEnv::new("junk");
        let not_image = json!({"v":1,"type":"open","data_b64":"aGVsbG8="}).to_string();
        assert_eq!(handle_message(not_image.as_bytes(), &env)["ok"], false);
        assert_eq!(handle_message(br#"{"v":2,"type":"open","url":"https://e.com/a.jpg"}"#, &env)["ok"], false);
        assert_eq!(handle_message(b"not json", &env)["ok"], false);
        assert!(env.launched.lock().unwrap().is_empty());
    }

    #[test]
    fn launch_failure_is_reported_to_the_extension() {
        let mut env = FakeEnv::new("fail");
        env.fail = true;
        let r = handle_message(br#"{"v":1,"type":"open","url":"https://e.com/a.jpg"}"#, &env);
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("boom"));
    }

    #[test]
    fn host_launch_detection_and_manifest_shape() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_native_host_launch(&a(&["pinit.exe", "chrome-extension://abc/", "--parent-window=1"])));
        assert!(!is_native_host_launch(&a(&["pinit.exe", "C:/a.png"])));
        let m: Value = serde_json::from_str(&manifest_json(Path::new(r"C:\x\pinit.exe"))).unwrap();
        assert_eq!(m["name"], HOST_NAME);
        assert_eq!(m["type"], "stdio");
        assert_eq!(m["allowed_origins"][0], format!("chrome-extension://{EXTENSION_ID}/"));
    }
}
