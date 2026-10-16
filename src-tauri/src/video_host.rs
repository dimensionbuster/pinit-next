//! Native Messaging host, stream mode (see `video_bridge.rs` for the app side).
//!
//! Chrome opens a long-lived port (`connectNative`) and sends `video-start`, then
//! `video-frame` messages. We connect to the running PinIt over its named pipe (starting
//! it when needed), forward every message as one JSON line, and tell Chrome to stop
//! when PinIt closes the pipe (its window was closed).

use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::native_host::{read_frame, write_frame, HostEnv, RealEnv};
use crate::pipe;
use crate::video_bridge::{hello_line, valid_sid, MAX_LINE};

const START_TIMEOUT: Duration = Duration::from_secs(15);
const WATCH_INTERVAL: Duration = Duration::from_millis(300);

/// Chrome reads our stdout; the stop notice and the replies must not interleave.
static STDOUT: Mutex<()> = Mutex::new(());

fn send(msg: &Value) {
    let _guard = STDOUT.lock().unwrap_or_else(|e| e.into_inner());
    let _ = write_frame(&mut io::stdout().lock(), msg.to_string().as_bytes());
}

/// Open the app's pipe, or start the app and wait for it to appear.
fn connect_to_app(env: &dyn HostEnv) -> io::Result<File> {
    match pipe::connect() {
        Ok(f) => return Ok(f),
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        Err(_) => {}
    }
    env.launch(vec!["--bridge-hello".to_string()])?;
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        match pipe::connect() {
            Ok(f) => return Ok(f),
            // not up yet, or every instance is busy for a moment
            Err(_) => continue,
        }
    }
    Err(io::Error::other("PinIt did not start in time"))
}

/// Stream mode entry. `first` is the already-read `video-start` message.
pub fn run(first: &Value) -> i32 {
    let Some(sid) = first.get("sid").and_then(Value::as_str).filter(|s| valid_sid(s)).map(str::to_string) else {
        send(&json!({ "ok": false, "error": "bad stream id" }));
        return 1;
    };
    let conn = match connect_to_app(&RealEnv) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            send(&json!({ "ok": false, "error": format!("cannot reach PinIt: {e}") }));
            return 1;
        }
    };
    if (&*conn).write_all(hello_line(&sid).as_bytes()).is_err() {
        send(&json!({ "ok": false, "error": "PinIt closed the connection" }));
        return 1;
    }
    send(&json!({ "ok": true, "type": "video-ready", "sid": sid }));

    // The pipe handle is synchronous, so we must not block in a read while writing. Instead
    // poll for the app having closed its end (the window was closed) and tell Chrome to stop.
    {
        let conn = Arc::clone(&conn);
        let sid = sid.clone();
        std::thread::spawn(move || {
            while !pipe::is_broken(&conn) {
                std::thread::sleep(WATCH_INTERVAL);
            }
            send(&json!({ "v": 1, "type": "stop", "sid": sid }));
            std::process::exit(0);
        });
    }

    // Chrome -> app.
    let mut stdin = io::stdin().lock();
    while let Ok(Some(bytes)) = read_frame(&mut stdin) {
        let Ok(msg) = serde_json::from_slice::<Value>(&bytes) else { continue };
        let kind = msg.get("type").and_then(Value::as_str);
        let wanted = matches!(kind, Some("video-frame" | "video-stop"));
        if !wanted || msg.get("sid").and_then(Value::as_str) != Some(sid.as_str()) {
            continue;
        }
        let mut line = msg.to_string();
        if line.len() as u64 >= MAX_LINE {
            continue;
        }
        line.push('\n');
        if (&*conn).write_all(line.as_bytes()).is_err() || kind == Some("video-stop") {
            break;
        }
    }
    0
}

/// `Some(message)` if the first frame from Chrome starts a video stream.
pub fn parse_start(bytes: &[u8]) -> Option<Value> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    (v.get("v").and_then(Value::as_u64) == Some(1) && v.get("type").and_then(Value::as_str) == Some("video-start"))
        .then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_video_start_selects_stream_mode() {
        assert!(parse_start(br#"{"v":1,"type":"video-start","sid":"a"}"#).is_some());
        assert!(parse_start(br#"{"v":1,"type":"open","url":"https://e.com/a.png"}"#).is_none());
        assert!(parse_start(br#"{"v":2,"type":"video-start"}"#).is_none());
        assert!(parse_start(b"nope").is_none());
    }
}
