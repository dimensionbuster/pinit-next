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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pinit_core::image_ref::ImageRef;
use pinit_core::image_size::image_size;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, EventTarget, Manager};

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

/// Live streams: stream id -> labels of its windows (empty until the first frame opened one).
/// One capture feeds every window of the stream. The control connection (below) uses it to
/// find the windows whose size it reports.
static STREAMS: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);

fn with_streams<T>(f: impl FnOnce(&mut HashMap<String, Vec<String>>) -> T) -> T {
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

/// The fields of a host line that matter here, borrowed from the line (see `read_stream`).
#[derive(serde::Deserialize)]
struct Line<'a> {
    sid: Option<&'a str>,
    #[serde(rename = "type")]
    kind: Option<&'a str>,
    jpeg_b64: Option<&'a str>,
}

/// The window label an event target names, if any.
fn target_label(t: &EventTarget) -> Option<&str> {
    match t {
        EventTarget::Window { label } | EventTarget::Webview { label } | EventTarget::WebviewWindow { label } => Some(label),
        _ => None,
    }
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
    /// Windows showing this stream (the first is opened by the first frame).
    labels: Mutex<Vec<String>>,
    /// `video-attach` requests waiting for the next frame to open their window with.
    attach_pending: AtomicUsize,
    /// Staged first frames (`stream-<sid>[-n].jpg`), removed when the stream ends.
    stills: Mutex<Vec<PathBuf>>,
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
                // every window of the stream was closed (a stream with one left goes on)
                let windows_gone = {
                    let mut labels = st.labels.lock().unwrap();
                    let had_windows = !labels.is_empty();
                    labels.retain(|l| app.get_webview_window(l).is_some());
                    had_windows && labels.is_empty()
                };
                if windows_gone ||(!st.hello_done.load(Ordering::SeqCst) && started.elapsed() > HELLO_TIMEOUT) {
                    canceller.cancel();
                    break;
                }
            }
        });
        let r = read_stream(app, inbox, &pipe, &st);
        st.done.store(true, Ordering::SeqCst);
        r
    });

    let labels = std::mem::take(&mut *st.labels.lock().unwrap());
    let windows: Vec<_> = labels.iter().filter_map(|l| app.get_webview_window(l)).collect();
    // the user closed the (last) window: the cancelled read is the normal way this ends
    let opened = !st.stills.lock().unwrap().is_empty(); // a window was opened at some point
    let closed_by_user = opened && windows.is_empty();
    for w in windows {
        let _ = w.close();
    }
    if let Some(sid) = st.sid.lock().unwrap().as_deref() {
        with_streams(|m| m.remove(sid));
    }
    for still in st.stills.lock().unwrap().drain(..) {
        let _ = std::fs::remove_file(still);
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
    with_streams(|m| m.insert(sid.clone(), Vec::new()));

    loop {
        if !read_line(&mut reader, &mut line, MAX_LINE)? {
            return Ok(());
        }
        // Borrowed fields only: a frame line is hundreds of KB and arrives dozens of times a second.
        let Ok(msg) = serde_json::from_str::<Line>(line.trim()) else { continue };
        if msg.sid != Some(sid.as_str()) {
            continue;
        }
        match msg.kind {
            Some("video-stop") => return Ok(()),
            // Another window onto the same capture: it opens with the next frame.
            Some("video-attach") => {
                st.attach_pending.fetch_add(1, Ordering::SeqCst);
            }
            Some("video-frame") => {
                let Some(b64_data) = msg.jpeg_b64 else { continue };
                if !is_base64(b64_data) {
                    continue;
                }
                // the first frame opens the first window, each pending attach one more
                let first = st.stills.lock().unwrap().is_empty();
                let mut to_open = st.attach_pending.swap(0, Ordering::SeqCst) + usize::from(first);
                while to_open > 0 {
                    to_open -= 1;
                    let n = st.stills.lock().unwrap().len();
                    let name = if n == 0 { format!("{STILL_PREFIX}{sid}.jpg") } else { format!("{STILL_PREFIX}{sid}-{n}.jpg") };
                    let still = inbox.join(name);
                    st.stills.lock().unwrap().push(still.clone());
                    let l = open_window(app, inbox, &still, b64_data).map_err(io::Error::other)?;
                    with_streams(|m| m.entry(sid.clone()).or_default().push(l.clone()));
                    st.labels.lock().unwrap().push(l);
                }
                // serialised once for all windows of the stream (not once per window)
                let labels = st.labels.lock().unwrap().clone();
                let _ = app.emit_filter("video-frame", b64_data, |t| {
                    target_label(t).is_some_and(|l| labels.iter().any(|x| x == l))
                });
            }
            _ => {}
        }
    }
}

/// Control connection: the host reads, we only write. Reports the pixel size of the biggest
/// window whenever it changes so the browser can capture at the size that is actually shown.
fn send_size_hints(app: &AppHandle, pipe: &File, sid: &str) -> io::Result<()> {
    let mut out = pipe;
    let mut last = (0u32, 0u32);
    let started = Instant::now();
    loop {
        std::thread::sleep(WATCH_INTERVAL);
        let labels = match with_streams(|m| m.get(sid).cloned()) {
            None => return Ok(()), // stream ended
            Some(l) if l.is_empty() && started.elapsed() > Duration::from_secs(60) => return Ok(()),
            Some(l) if l.is_empty() => continue, // window not open yet
            Some(l) => l,
        };
        let windows: Vec<_> = labels.iter().filter_map(|l| app.get_webview_window(l)).collect();
        if windows.is_empty() {
            return Ok(());
        }
        // one capture feeds all windows, so it has to be big enough for the biggest one
        let size = windows
            .iter()
            .filter_map(|w| w.inner_size().ok())
            .fold((0u32, 0u32), |(w, h), s| (w.max(s.width), h.max(s.height)));
        if size != last && size.0 > 0 && size.1 > 0 {
            last = size;
            out.write_all(format!("{}
", json!({ "w": size.0, "h": size.1 })).as_bytes())?;
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
