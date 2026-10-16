//! Live video bridge (app side).
//!
//! The browser extension captures frames of a playing `<video>` and sends them through
//! Native Messaging. That host process (`video_host.rs`) forwards them here over a
//! named pipe; each stream becomes a normal PinIt window (placement, resize,
//! relayout, close all work as for images) whose picture is replaced frame by frame.
//!
//! Transport is a Windows named pipe (see `pipe.rs`): no network port, owner-only access,
//! and the app refuses to run a second server under the same name. The first frame is
//! staged as `stream-<sid>.jpg` so the window can be opened through the ordinary image
//! path; that still is never persisted into the session.

use std::fs::File;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pinit_core::image_ref::ImageRef;
use pinit_core::image_size::image_size;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::adapters::tauri_host::label_for;
use crate::app::AppState;
use crate::b64;
use crate::pipe::{Canceller, PipeServer};

pub const STILL_PREFIX: &str = "stream-";
/// Largest accepted frame line (base64 JPEG inside JSON).
pub const MAX_LINE: u64 = 12 * 1024 * 1024;
const MAX_HELLO: u64 = 4 * 1024;
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const WATCH_INTERVAL: Duration = Duration::from_millis(200);

/// Live streams: stream id -> label of its window (None until the first frame opened it).
/// The control connection (below) uses it to find the window whose size it reports.
static STREAMS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn with_streams<T>(f: impl FnOnce(&mut HashMap<String, Option<String>>) -> T) -> T {
    let mut guard = STREAMS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// First line of the host's second ("control") connection: asks for window size hints.
pub fn ctl_hello_line(sid: &str) -> String {
    format!("{}
", json!({ "sid": sid, "ctl": true }))
}

/// The staged first frame of a stream (also used to keep it out of saved sessions).
pub fn is_stream_still(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(STILL_PREFIX))
}

/// Stream ids are chosen by the browser: keep them safe to use in a file name.
pub fn valid_sid(sid: &str) -> bool {
    (1..=40).contains(&sid.len()) && sid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn is_base64(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'-' | b'_'))
}

/// First line a host connection sends: names the stream.
pub fn hello_line(sid: &str) -> String {
    format!("{}\n", json!({ "sid": sid }))
}

/// Create the pipe server and serve connections on worker threads.
pub fn start(app: &AppHandle, inbox: &Path) -> io::Result<()> {
    // Leftover stills from a previous run (crash) are of no use.
    if let Ok(dir) = std::fs::read_dir(inbox) {
        for e in dir.flatten() {
            if is_stream_still(&e.path()) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let mut server = PipeServer::bind()?;
    let app = app.clone();
    let inbox = inbox.to_path_buf();
    std::thread::spawn(move || {
        let mut failures = 0;
        while failures < 5 {
            match server.accept() {
                Ok(pipe) => {
                    failures = 0;
                    let (app, inbox) = (app.clone(), inbox.clone());
                    std::thread::spawn(move || {
                        if let Err(e) = serve(&app, &inbox, pipe) {
                            eprintln!("video stream ended: {e}");
                        }
                    });
                }
                Err(e) => {
                    failures += 1;
                    eprintln!("video bridge accept failed: {e}");
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
    });
    Ok(())
}

/// Reads one `\n`-terminated line of at most `limit` bytes into `buf` (cleared first).
/// False on EOF or when the line is too long.
fn read_line(r: &mut impl BufRead, buf: &mut String, limit: u64) -> io::Result<bool> {
    buf.clear();
    let n = r.take(limit).read_line(buf)?;
    Ok(n > 0 && buf.ends_with('\n'))
}

/// Per-connection state shared with the watcher thread.
#[derive(Default)]
struct Stream {
    hello_done: AtomicBool,
    done: AtomicBool,
    sid: Mutex<Option<String>>,
    label: Mutex<Option<String>>,
}

fn serve(app: &AppHandle, inbox: &Path, pipe: File) -> io::Result<()> {
    let canceller = Canceller::new(&pipe);
    let st = Stream::default();

    // A blocking pipe read cannot time out, so a watcher cancels it when the window is
    // closed (also while the video is paused and no frames flow) or the hello never comes.
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            let started = Instant::now();
            while !st.done.load(Ordering::SeqCst) {
                std::thread::sleep(WATCH_INTERVAL);
                let window_gone = st
                    .label
                    .lock()
                    .unwrap()
                    .as_deref()
                    .is_some_and(|l| app.get_webview_window(l).is_none());
                if window_gone || (!st.hello_done.load(Ordering::SeqCst) && started.elapsed() > HELLO_TIMEOUT) {
                    canceller.cancel();
                    break;
                }
            }
        });
        let r = read_stream(app, inbox, &pipe, &st);
        st.done.store(true, Ordering::SeqCst);
        r
    });

    let label = st.label.lock().unwrap().take();
    let window = label.as_deref().and_then(|l| app.get_webview_window(l));
    // the user closed the window: the cancelled read is the normal way this ends
    let closed_by_user = label.is_some() && window.is_none();
    if let Some(w) = window {
        let _ = w.close();
    }
    if let Some(sid) = st.sid.lock().unwrap().as_deref() {
        with_streams(|m| m.remove(sid));
        let _ = std::fs::remove_file(inbox.join(format!("{STILL_PREFIX}{sid}.jpg")));
    }
    // dropping `pipe` tells the host (it polls for a closed pipe) to stop the capture
    if closed_by_user { Ok(()) } else { result }
}

fn read_stream(app: &AppHandle, inbox: &Path, pipe: &File, st: &Stream) -> io::Result<()> {
    let mut reader = BufReader::new(pipe);
    let mut line = String::new();
    if !read_line(&mut reader, &mut line, MAX_HELLO)? {
        return Err(io::Error::other("no hello"));
    }
    let hello: Value = serde_json::from_str(line.trim()).map_err(io::Error::other)?;
    let sid = hello
        .get("sid")
        .and_then(Value::as_str)
        .filter(|s| valid_sid(s))
        .ok_or_else(|| io::Error::other("bad sid"))?
        .to_string();
    st.hello_done.store(true, Ordering::SeqCst);
    if hello.get("ctl").and_then(Value::as_bool) == Some(true) {
        return send_size_hints(app, pipe, &sid);
    }
    *st.sid.lock().unwrap() = Some(sid.clone());
    with_streams(|m| m.insert(sid.clone(), None));

    let still = inbox.join(format!("{STILL_PREFIX}{sid}.jpg"));
    loop {
        if !read_line(&mut reader, &mut line, MAX_LINE)? {
            return Ok(());
        }
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else { continue };
        if msg.get("sid").and_then(Value::as_str) != Some(sid.as_str()) {
            continue;
        }
        match msg.get("type").and_then(Value::as_str) {
            Some("video-stop") => return Ok(()),
            Some("video-frame") => {
                let Some(b64_data) = msg.get("jpeg_b64").and_then(Value::as_str) else { continue };
                let current = st.label.lock().unwrap().clone();
                match current {
                    Some(l) => {
                        if is_base64(b64_data) {
                            let _ = app.emit_to(l, "video-frame", b64_data);
                        }
                    }
                    None => {
                        let l = open_window(app, inbox, &still, b64_data).map_err(io::Error::other)?;
                        with_streams(|m| m.insert(sid.clone(), Some(l.clone())));
                        *st.label.lock().unwrap() = Some(l);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Control connection: the host reads, we only write. Reports the window's pixel size
/// whenever it changes so the browser can capture at the size that is actually shown.
fn send_size_hints(app: &AppHandle, pipe: &File, sid: &str) -> io::Result<()> {
    let mut out = pipe;
    let mut last = (0u32, 0u32);
    let started = Instant::now();
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        match with_streams(|m| m.get(sid).cloned()) {
            None => return Ok(()), // stream ended
            Some(None) if started.elapsed() > Duration::from_secs(60) => return Ok(()),
            Some(None) => continue, // window not open yet
            Some(Some(label)) => {
                let Some(win) = app.get_webview_window(&label) else { return Ok(()) };
                let Ok(size) = win.inner_size() else { continue };
                if (size.width, size.height) != last && size.width > 0 && size.height > 0 {
                    last = (size.width, size.height);
                    out.write_all(format!("{}
", json!({ "w": size.width, "h": size.height })).as_bytes())?;
                }
            }
        }
    }
}

/// Stage the first frame and open it like any image; returns the window label.
fn open_window(app: &AppHandle, inbox: &Path, still: &Path, b64_data: &str) -> Result<String, String> {
    let data = b64::decode(b64_data).ok_or("invalid base64")?;
    if !matches!(data.as_slice(), [0xFF, 0xD8, ..]) || image_size(&data).is_none() {
        return Err("first frame is not a JPEG".into());
    }
    std::fs::create_dir_all(inbox).map_err(|e| e.to_string())?;
    std::fs::write(still, &data).map_err(|e| e.to_string())?;
    let id = app.state::<AppState>().open_one_id(ImageRef::File(still.to_path_buf()))?;
    Ok(label_for(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_ids_are_file_name_safe() {
        assert!(valid_sid("a1-B2"));
        assert!(!valid_sid(""));
        assert!(!valid_sid("../x"));
        assert!(!valid_sid(&"a".repeat(41)));
    }

    #[test]
    fn stills_are_recognised_by_prefix() {
        assert!(is_stream_still(Path::new(r"C:\inbox\stream-abc.jpg")));
        assert!(!is_stream_still(Path::new(r"C:\inbox\abc.jpg")));
    }

    #[test]
    fn lines_are_split_and_length_limited() {
        let mut r = BufReader::new(&b"{\"a\":1}\nrest"[..]);
        let mut buf = String::new();
        assert!(read_line(&mut r, &mut buf, 1024).unwrap());
        assert_eq!(buf, "{\"a\":1}\n");
        assert!(!read_line(&mut r, &mut buf, 1024).unwrap()); // EOF without newline
        let mut r = BufReader::new(&b"0123456789\n"[..]);
        assert!(!read_line(&mut r, &mut buf, 4).unwrap()); // over the limit
    }

    #[test]
    fn hello_names_the_stream() {
        let l = hello_line("sid-1");
        assert!(l.ends_with('\n'));
        let v: Value = serde_json::from_str(l.trim()).unwrap();
        assert_eq!(v["sid"].as_str(), Some("sid-1"));
    }
}
