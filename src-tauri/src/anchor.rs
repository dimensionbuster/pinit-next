//! Native half of "the picture stays put while its window is resized".
//!
//! A pinned window (the user zoomed or rotated it) behaves like a mask over a picture that is
//! fixed on screen. The web layer alone cannot do that without lag: when an edge is dragged the
//! OS moves the window frame at once, but the page only learns of it a few frames later. So the
//! WebView2 surface (wry's container window, a child of the window) is positioned here, inside
//! the very `WM_WINDOWPOSCHANGED` that resizes the window, i.e. before anything is presented:
//!
//! * the surface always reaches [`MARGIN`] px beyond the window to the west and north (the window
//!   clips it). Its origin is its top-left corner, so growing it westwards would shift the page;
//!   with the margin already rendered, an edge drag only *uncovers* pixels that exist;
//! * user drag on a pinned window: the surface keeps its place on screen, so the picture cannot
//!   move; it grows only past the margin (then the page compensates, with some lag);
//! * anything else (window drag, placement by the app, unpinned window): the surface follows the
//!   window rigidly, exactly as before.
//!
//! The page learns where the window sits inside its surface through `win-frame` events and
//! keeps its own picture state consistent (see `ResizeAnchor.ts`).

/// Physical px the page surface extends beyond its window to the west and north.
pub const MARGIN: i32 = 400;

#[cfg(windows)]
mod imp {
    use super::MARGIN;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    use serde_json::json;
    use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Controller, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, KillTimer, SetTimer, SetWindowPos, SIZE_MINIMIZED, SWP_NOACTIVATE, SWP_NOZORDER, WINDOWPOS,
        SendMessageW, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVE, WM_MOVING, WM_NCDESTROY, WM_SETFOCUS, WM_SIZE, WM_TIMER, WM_USER,
        WM_WINDOWPOSCHANGED,
    };

    /// Diagnostics for `npm run dev:app` (debug builds only; silent in release).
    macro_rules! trace {
        ($($t:tt)*) => { if cfg!(debug_assertions) { eprintln!("[anchor] {}", format!($($t)*)); } };
    }

    fn fmt(r: &RECT) -> String {
        format!("({},{} {}x{})", r.left, r.top, r.right - r.left, r.bottom - r.top)
    }

    const SUBCLASS_ID: usize = 0x5049_4e41; // "PINA"
    const SETTLE_TIMER: usize = 0x5049_4e42;
    /// wry's own window subclass resizes the surface to the window on every WM_SIZE, which would
    /// fight ours (a small frame flashes between its reset and our override). It removes itself
    /// when sent this message (`PARENT_DESTROY_MESSAGE` in wry's webview2 module); `subclass_proc`
    /// then does the few jobs that subclass had.
    const WRY_DETACH_MESSAGE: u32 = WM_USER + 0x65;
    /// A runaway drag must not allocate an enormous surface.
    const MAX_SURFACE: i32 = 16384;

    fn width(r: &RECT) -> i32 {
        r.right - r.left
    }

    fn height(r: &RECT) -> i32 {
        r.bottom - r.top
    }

    fn union(a: &RECT, b: &RECT) -> RECT {
        RECT { left: a.left.min(b.left), top: a.top.min(b.top), right: a.right.max(b.right), bottom: a.bottom.max(b.bottom) }
    }

    fn left_button_down() -> bool {
        // SAFETY: plain key-state query.
        unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
    }

    struct Anchor {
        app: AppHandle,
        label: String,
        controller: ICoreWebView2Controller,
        /// wry's container window: parent of the WebView2 surface, child of the top-level window.
        container: HWND,
        /// Screen rect of the page surface. It always contains `win`.
        dom: RECT,
        /// Screen rect of the window as last seen.
        win: RECT,
        pinned: bool,
        /// The top-level window (for the diagnostics timer).
        top: HWND,
        /// Container placement last applied (relative x, y, width, height).
        applied: Cell<(i32, i32, i32, i32)>,
    }

    thread_local! {
        /// Only the UI thread (window procedures, `run_on_main_thread`) touches this.
        static ANCHORS: RefCell<HashMap<isize, Anchor>> = RefCell::new(HashMap::new());
    }

    fn with_anchor<R>(hwnd: HWND, f: impl FnOnce(&mut Anchor) -> R) -> Option<R> {
        ANCHORS.with(|m| m.borrow_mut().get_mut(&(hwnd.0 as isize)).map(f))
    }

    impl Anchor {
        /// Put the container and the WebView2 surface at `dom`, relative to the window.
        fn apply(&self) {
            let (w, h) = (width(&self.dom), height(&self.dom));
            let target = (self.dom.left - self.win.left, self.dom.top - self.win.top, w, h);
            if self.applied.replace(target) == target {
                return; // nothing else touches the surface now, so it is still as we left it
            }
            // SAFETY: both windows belong to this (UI) thread.
            unsafe {
                let _ = SetWindowPos(self.container, None, target.0, target.1, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
                let _ = self.controller.SetBounds(RECT { left: 0, top: 0, right: w, bottom: h });
            }
            if cfg!(debug_assertions) {
                // diagnostics: what is left once the queued repositioning has run?
                // SAFETY: timer on this window, handled in `subclass_proc`.
                unsafe { SetTimer(Some(self.top), SETTLE_TIMER, 200, None) };
            }
            trace!("apply  container -> rel=({},{}) size={}x{}", self.dom.left - self.win.left, self.dom.top - self.win.top, w, h);
        }

        /// Where the container really is relative to the window, and the controller's own bounds
        /// right now (diagnostics).
        fn actual(&self) -> String {
            let mut r = RECT::default();
            let mut b = RECT::default();
            // SAFETY: plain queries on objects of this thread.
            unsafe {
                if GetWindowRect(self.container, &mut r).is_err() {
                    return "?".into();
                }
                let _ = self.controller.Bounds(&mut b);
            }
            format!(
                "container rel=({},{}) size={}x{}  controller bounds={}",
                r.left - self.win.left,
                r.top - self.win.top,
                width(&r),
                height(&r),
                fmt(&b)
            )
        }

        /// Surface rect for a window: the margin to the west/north, flush with it elsewhere.
        fn with_margin(win: &RECT) -> RECT {
            RECT { left: win.left - MARGIN, top: win.top - MARGIN, right: win.right, bottom: win.bottom }
        }

        /// Tell the page where its window sits in the surface. `keep` = the surface held its
        /// place on screen, so the page must keep its picture where it is; `(sx, sy)` is how far
        /// the surface origin moved on screen (non-zero only when it grew past the margin).
        fn emit(&self, keep: bool, sx: i32, sy: i32) {
            trace!("emit   frame=({},{} {}x{}) keep={keep} origin=({sx},{sy})", self.win.left - self.dom.left, self.win.top - self.dom.top, width(&self.win), height(&self.win));
            let _ = self.app.emit_to(
                self.label.as_str(),
                "win-frame",
                json!({
                    "x": self.win.left - self.dom.left,
                    "y": self.win.top - self.dom.top,
                    "w": width(&self.win),
                    "h": height(&self.win),
                    "keep": keep,
                    "sx": sx,
                    "sy": sy,
                }),
            );
        }

        fn on_pos(&mut self, wp: &WINDOWPOS) {
            let new = RECT { left: wp.x, top: wp.y, right: wp.x + wp.cx, bottom: wp.y + wp.cy };
            if new.left <= -30000 {
                return; // minimised
            }
            if new.left == self.win.left && new.top == self.win.top && width(&new) == width(&self.win) && height(&new) == height(&self.win) {
                // Nothing changed, but wry still reset the surface on the WM_SIZE that came with
                // this message (repeated same-size placements do that): put ours back.
                self.apply();
                return;
            }
            let resized = width(&new) != width(&self.win) || height(&new) != height(&self.win);
            let grown = union(&self.dom, &new);
            let keep = resized
                && self.pinned
                && left_button_down()
                && width(&grown) <= MAX_SURFACE
                && height(&grown) <= MAX_SURFACE;
            trace!("on_pos state before: {}", self.actual());
            trace!(
                "on_pos win {} -> {}  resized={resized} pinned={} lmb={} keep={keep}  dom {}",
                fmt(&self.win),
                fmt(&new),
                self.pinned,
                left_button_down(),
                fmt(&self.dom)
            );
            let old_origin = (self.dom.left, self.dom.top);
            if keep {
                self.dom = grown;
            } else {
                // follow the window rigidly, keeping the page's offset inside the surface
                let (fx, fy) = (self.win.left - self.dom.left, self.win.top - self.dom.top);
                self.dom = RECT { left: new.left - fx, top: new.top - fy, right: new.right, bottom: new.bottom };
            }
            self.win = new;
            self.apply();
            trace!("on_pos dom -> {}  origin moved ({},{})", fmt(&self.dom), self.dom.left - old_origin.0, self.dom.top - old_origin.1);
            self.emit(keep, self.dom.left - old_origin.0, self.dom.top - old_origin.1);
        }

        /// A drag ended: the surface no longer needs to reach past the window's far edges.
        fn trim(&mut self) {
            // by now wry's queued repositioning has run: does our override still stand?
            trace!("drag end: win {} dom {}  container actual {}", fmt(&self.win), fmt(&self.dom), self.actual());
            if self.dom.right != self.win.right || self.dom.bottom != self.win.bottom {
                self.dom.right = self.win.right;
                self.dom.bottom = self.win.bottom;
                self.apply();
            }
        }

        fn set_pinned(&mut self, pinned: bool) {
            trace!("set_pinned {pinned}  win {} dom {}", fmt(&self.win), fmt(&self.dom));
            self.pinned = pinned;
            // An unpinned window (view reset) gets its full margin back; the page re-lays itself
            // out at the same time, and the picture is being reset anyway.
            let fresh = Self::with_margin(&self.win);
            if !pinned && (self.dom.left != fresh.left || self.dom.top != fresh.top) {
                self.dom = fresh;
                self.apply();
                self.emit(false, 0, 0);
            }
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        match msg {
            // Run wry's and tao's handlers first (they set the surface to the window size),
            // then override the surface within the same message.
            WM_WINDOWPOSCHANGED => {
                // SAFETY: forwards the original message arguments.
                let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
                // SAFETY: for this message `lparam` points to a WINDOWPOS.
                let wp = unsafe { &*(lparam.0 as *const WINDOWPOS) };
                with_anchor(hwnd, |a| a.on_pos(wp));
                result
            }
            WM_TIMER if wparam.0 == SETTLE_TIMER => {
                // SAFETY: kills the timer set in `on_pos`.
                unsafe { let _ = KillTimer(Some(hwnd), SETTLE_TIMER); }
                with_anchor(hwnd, |a| {
                    trace!("settled 200ms after on_pos: {}  | expected rel=({},{}) size={}x{}", a.actual(), a.dom.left - a.win.left, a.dom.top - a.win.top, width(&a.dom), height(&a.dom));
                });
                LRESULT(0)
            }
            // The surface follows the window: sized by us alone (wry's subclass is detached).
            WM_SIZE => {
                // SAFETY: forwards the original message arguments.
                let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
                if wparam.0 != SIZE_MINIMIZED as usize {
                    with_anchor(hwnd, |a| a.apply());
                }
                result
            }
            // Jobs of wry's detached subclass: keep focus in the page, tell it the window moved.
            WM_SETFOCUS | WM_ENTERSIZEMOVE => {
                with_anchor(hwnd, |a| {
                    // SAFETY: controller of this thread.
                    unsafe { let _ = a.controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC); }
                });
                // SAFETY: forwards the original message arguments.
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_MOVE | WM_MOVING => {
                with_anchor(hwnd, |a| {
                    // SAFETY: controller of this thread.
                    unsafe { let _ = a.controller.NotifyParentWindowPositionChanged(); }
                });
                // SAFETY: forwards the original message arguments.
                unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
            }
            WM_EXITSIZEMOVE => {
                // SAFETY: forwards the original message arguments.
                let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
                with_anchor(hwnd, Anchor::trim);
                result
            }
            WM_NCDESTROY => {
                ANCHORS.with(|m| m.borrow_mut().remove(&(hwnd.0 as isize)));
                // SAFETY: removes the subclass installed by `attach`, then forwards.
                unsafe {
                    let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
            }
            // SAFETY: forwards the original message arguments.
            _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        }
    }

    /// Hook a freshly built viewer window. Safe to call from any thread.
    pub fn attach(window: &WebviewWindow) {
        let Ok(hwnd) = window.hwnd() else { return };
        let key = hwnd.0 as isize;
        let (app, label) = (window.app_handle().clone(), window.label().to_string());
        let _ = window.with_webview(move |webview| {
            let controller = webview.controller();
            // SAFETY: runs on the UI thread that owns the window and the controller.
            unsafe {
                let mut container = HWND::default();
                if controller.ParentWindow(&mut container).is_err() {
                    return;
                }
                let hwnd = HWND(key as *mut _);
                let mut win = RECT::default();
                if GetWindowRect(hwnd, &mut win).is_err() {
                    return;
                }
                let dom = Anchor::with_margin(&win);
                let anchor = Anchor { app, label, controller, container, dom, win, pinned: false, top: hwnd, applied: Cell::new((0, 0, 0, 0)) };
                anchor.apply();
                trace!("attach {}  win {} dom {}", anchor.label, fmt(&anchor.win), fmt(&anchor.dom));
                ANCHORS.with(|m| m.borrow_mut().insert(key, anchor));
                let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
                // wry's subclass sits behind ours in the chain: this message makes it remove itself.
                SendMessageW(hwnd, WRY_DETACH_MESSAGE, None, None);
            }
        });
    }

    /// The page says whether the view is zoomed/rotated (pinned) or untouched.
    pub fn set_pinned(window: &WebviewWindow, pinned: bool) {
        let Ok(hwnd) = window.hwnd() else { return };
        let key = hwnd.0 as isize;
        let _ = window.run_on_main_thread(move || {
            with_anchor(HWND(key as *mut _), |a| a.set_pinned(pinned));
        });
    }
}

#[cfg(windows)]
pub use imp::{attach, set_pinned};

#[cfg(not(windows))]
pub fn attach(_: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn set_pinned(_: &tauri::WebviewWindow, _: bool) {}
