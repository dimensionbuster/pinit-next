//! Native half of "the picture stays put while its window is resized".
//!
//! A pinned window (the user zoomed or rotated it) behaves like a mask over a picture that is
//! fixed on screen. The web layer alone cannot do that without lag: when an edge is dragged the
//! OS moves the window frame at once, but the page only learns of it a few frames later. So the
//! page's webview is positioned here, from the window's `Moved`/`Resized` events, which Tauri
//! delivers from inside the very window message that resized the window (and `set_bounds` is
//! applied at once when called on the main thread), i.e. before anything is presented:
//!
//! * the webview always reaches [`MARGIN`] px beyond the window to the west and north (the window
//!   clips it). Its origin is its top-left corner, so growing it westwards would shift the page;
//!   with the margin already rendered, an edge drag only *uncovers* pixels that exist;
//! * user drag on a pinned window: the webview keeps its place on screen, so the picture cannot
//!   move; it grows only past the margin (then the page compensates, with some lag);
//! * anything else (window drag, placement by the app, unpinned window): the webview follows the
//!   window rigidly, exactly as before.
//!
//! The page learns where the window sits inside its webview through `win-frame` events and
//! keeps its own picture state consistent (see `ResizeAnchor.ts`).
//!
//! Only public Tauri API is used (the `unstable` feature makes every webview a child webview
//! whose bounds can be set freely; auto resize is switched off for the viewer windows).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use tauri::{Emitter, PhysicalPosition, PhysicalSize, Position, Rect, Size, Webview, WebviewWindow, Window};

/// Physical px the page webview extends beyond its window to the west and north.
pub const MARGIN: i32 = 400;
/// A runaway drag must not allocate an enormous surface.
const MAX_SURFACE: i32 = 16384;
/// How long after the last geometry change a finished drag trims the webview.
const TRIM_DELAY: Duration = Duration::from_millis(250);

/// Diagnostics are on only in debug builds started with `PINIT_TRACE=1` (the printing itself
/// would slow a drag down), e.g. `$env:PINIT_TRACE=1; npm run dev:app`.
pub fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| cfg!(debug_assertions) && std::env::var_os("PINIT_TRACE").is_some())
}

macro_rules! trace {
    ($($t:tt)*) => { if tracing() { eprintln!("[anchor] {}", format!($($t)*)); } };
}

/// A screen rectangle in physical px.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct R {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl R {
    fn width(&self) -> i32 {
        self.right - self.left
    }

    fn height(&self) -> i32 {
        self.bottom - self.top
    }

    fn union(&self, o: &R) -> R {
        R { left: self.left.min(o.left), top: self.top.min(o.top), right: self.right.max(o.right), bottom: self.bottom.max(o.bottom) }
    }

    /// The webview rect for a window: the margin to the west/north, flush with it elsewhere.
    fn with_margin(&self) -> R {
        R { left: self.left - MARGIN, top: self.top - MARGIN, right: self.right, bottom: self.bottom }
    }
}

impl std::fmt::Display for R {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({},{} {}x{})", self.left, self.top, self.width(), self.height())
    }
}

fn left_button_down() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
        // SAFETY: plain key-state query.
        unsafe { GetAsyncKeyState(VK_LBUTTON as i32) < 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

struct Anchor {
    webview: Webview,
    /// Screen rect of the page webview. It always contains `win`.
    dom: R,
    /// Screen rect of the window as last seen.
    win: R,
    pinned: bool,
    /// Placement last applied (relative x, y, width, height).
    applied: (i32, i32, i32, i32),
    trim_pending: bool,
}

static ANCHORS: Mutex<Option<HashMap<String, Anchor>>> = Mutex::new(None);

fn with_anchor<T>(label: &str, f: impl FnOnce(&mut Anchor) -> T) -> Option<T> {
    ANCHORS.lock().unwrap_or_else(|e| e.into_inner()).as_mut()?.get_mut(label).map(f)
}

impl Anchor {
    /// Put the webview at `dom`, relative to the window.
    fn apply(&mut self) {
        let target = (self.dom.left - self.win.left, self.dom.top - self.win.top, self.dom.width(), self.dom.height());
        if self.applied == target {
            return;
        }
        self.applied = target;
        let _ = self.webview.set_bounds(Rect {
            position: Position::Physical(PhysicalPosition::new(target.0, target.1)),
            size: Size::Physical(PhysicalSize::new(target.2.max(1) as u32, target.3.max(1) as u32)),
        });
        trace!("apply  webview -> rel=({},{}) size={}x{}", target.0, target.1, target.2, target.3);
    }

    /// Tell the page where its window sits in the webview. `keep` = the webview held its place on
    /// screen, so the page must keep its picture where it is; `(sx, sy)` is how far the webview
    /// origin moved on screen (non-zero only when it grew past the margin).
    fn emit(&self, window: &Window, keep: bool, sx: i32, sy: i32) {
        let (x, y) = (self.win.left - self.dom.left, self.win.top - self.dom.top);
        trace!("emit   frame=({x},{y} {}x{}) keep={keep} origin=({sx},{sy})", self.win.width(), self.win.height());
        let _ = window.emit_to(
            window.label(),
            "win-frame",
            json!({ "x": x, "y": y, "w": self.win.width(), "h": self.win.height(), "keep": keep, "sx": sx, "sy": sy }),
        );
    }

    fn on_pos(&mut self, window: &Window, new: R) {
        if new == self.win {
            return;
        }
        let resized = new.width() != self.win.width() || new.height() != self.win.height();
        let grown = self.dom.union(&new);
        let lmb = left_button_down();
        let keep = resized && self.pinned && lmb && grown.width() <= MAX_SURFACE && grown.height() <= MAX_SURFACE;
        trace!("on_pos win {} -> {}  resized={resized} pinned={} lmb={lmb} keep={keep}  dom {}", self.win, new, self.pinned, self.dom);
        let old_origin = (self.dom.left, self.dom.top);
        if keep {
            self.dom = grown;
        } else {
            // follow the window rigidly, keeping the page's offset inside the webview
            let (fx, fy) = (self.win.left - self.dom.left, self.win.top - self.dom.top);
            self.dom = R { left: new.left - fx, top: new.top - fy, right: new.right, bottom: new.bottom };
        }
        self.win = new;
        self.apply();
        trace!("on_pos dom -> {}  origin moved ({},{})", self.dom, self.dom.left - old_origin.0, self.dom.top - old_origin.1);
        self.emit(window, keep, self.dom.left - old_origin.0, self.dom.top - old_origin.1);
    }

    /// A drag ended: the webview no longer needs to reach past the window's far edges.
    fn trim(&mut self) {
        if self.dom.right != self.win.right || self.dom.bottom != self.win.bottom {
            self.dom.right = self.win.right;
            self.dom.bottom = self.win.bottom;
            trace!("trim   dom -> {}", self.dom);
            self.apply();
        }
    }
}

fn outer_rect(window: &Window) -> Option<R> {
    let (p, s) = (window.outer_position().ok()?, window.outer_size().ok()?);
    Some(R { left: p.x, top: p.y, right: p.x + s.width as i32, bottom: p.y + s.height as i32 })
}

/// Hook a freshly built viewer window. Safe to call from any thread.
pub fn attach(window: &WebviewWindow) {
    let webview: &Webview = window.as_ref();
    // We place the webview ourselves: Tauri must not scale it with the window.
    let _ = webview.set_auto_resize(false);
    let Some(win) = outer_rect(&window.as_ref().window()) else { return };
    let mut anchor = Anchor { webview: webview.clone(), dom: win.with_margin(), win, pinned: false, applied: (0, 0, 0, 0), trim_pending: false };
    anchor.apply();
    trace!("attach {}  win {} dom {}", window.label(), anchor.win, anchor.dom);
    ANCHORS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(window.label().to_string(), anchor);
}

/// Window `Moved`/`Resized` event: keep the page webview where it belongs. Runs inside the
/// window message that moved the window (main thread).
pub fn on_geometry(window: &Window) {
    let Some(new) = outer_rect(window) else { return };
    if new.left <= -30000 {
        return; // minimised
    }
    let label = window.label().to_string();
    let schedule_trim = with_anchor(&label, |a| {
        a.on_pos(window, new);
        !std::mem::replace(&mut a.trim_pending, true)
    });
    if schedule_trim == Some(true) {
        let window = window.clone();
        std::thread::spawn(move || {
            // wait for the drag to end (button up), then trim on the main thread
            loop {
                std::thread::sleep(TRIM_DELAY);
                if !left_button_down() {
                    break;
                }
            }
            let label = window.label().to_string();
            let _ = window.run_on_main_thread(move || {
                with_anchor(&label, |a| {
                    a.trim_pending = false;
                    a.trim();
                });
            });
        });
    }
}

/// The page says whether the view is zoomed/rotated (pinned) or untouched.
pub fn set_pinned(window: &WebviewWindow, pinned: bool) {
    let label = window.label().to_string();
    let w: Window = window.as_ref().window();
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || {
        with_anchor(&label, |a| {
            trace!("set_pinned {pinned}  win {} dom {}", a.win, a.dom);
            a.pinned = pinned;
            // An unpinned window (view reset) gets its full margin back; the page re-lays itself
            // out at the same time, and the picture is being reset anyway.
            let fresh = a.win.with_margin();
            if !pinned && (a.dom.left != fresh.left || a.dom.top != fresh.top) {
                a.dom = fresh;
                a.apply();
                a.emit(&w2, false, 0, 0);
            }
        });
    });
}

/// Forget a closed window.
pub fn detach(label: &str) {
    if let Some(m) = ANCHORS.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.remove(label);
    }
}
