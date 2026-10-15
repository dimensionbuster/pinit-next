//! `WindowHost` backed by Tauri webview windows. One transparent, frameless,
//! always-on-top window per image; the viewer page pulls its data from the
//! registry via the `viewer_init` command.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pinit_core::geometry::Rect;
use pinit_core::image_ref::ImageRef;
use pinit_core::ports::{HostError, WindowHost, WindowId, WindowSpec};
use pinit_core::view_state::ViewState;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewJson {
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub quarter_turns: u8,
    pub fine_angle_deg: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl From<ViewState> for ViewJson {
    fn from(v: ViewState) -> Self {
        Self {
            zoom: v.zoom,
            pan_x: v.pan_x,
            pan_y: v.pan_y,
            quarter_turns: v.quarter_turns,
            fine_angle_deg: v.fine_angle_deg,
            flip_h: v.flip_h,
            flip_v: v.flip_v,
        }
    }
}

impl From<ViewJson> for ViewState {
    fn from(v: ViewJson) -> Self {
        let mut s = ViewState { zoom: 1.0, ..ViewState::default() };
        s.zoom_by(v.zoom);
        s.pan_x = v.pan_x;
        s.pan_y = v.pan_y;
        s.quarter_turns = v.quarter_turns % 4;
        s.set_fine_angle(v.fine_angle_deg);
        s.flip_h = v.flip_h;
        s.flip_v = v.flip_v;
        s
    }
}

/// What the viewer page needs to render itself.
#[derive(Debug, Clone, Serialize)]
pub struct ViewerInit {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub view: ViewJson,
    /// Original URL (for "open source page"); None for local files.
    pub source_url: Option<String>,
}

pub type Registry = Arc<Mutex<HashMap<String, ViewerInit>>>;

pub fn label_for(id: WindowId) -> String {
    format!("img-{id}")
}

pub fn id_from_label(label: &str) -> Option<WindowId> {
    label.strip_prefix("img-")?.parse().ok()
}

pub struct TauriHost {
    app: AppHandle,
    registry: Registry,
    next_id: WindowId,
    show_in_taskbar: bool,
}

impl TauriHost {
    pub fn new(app: AppHandle, registry: Registry, show_in_taskbar: bool) -> Self {
        Self { app, registry, next_id: 1, show_in_taskbar }
    }
}

/// Moves/resizes a window to exact physical bounds, even across monitors with
/// different DPI scales.
///
/// Moving a window onto a monitor with another scale makes Windows rescale it
/// (WM_DPICHANGED), e.g. a 480x300 window sent from a 150% to a 100% monitor
/// ends up 320x200. So: position first, then size, and re-apply until the
/// window reports what we asked for. Never call this from the UI thread.
fn apply_bounds(w: &tauri::WebviewWindow, b: Rect) {
    let pos = PhysicalPosition::new(b.x, b.y);
    let size = PhysicalSize::new(b.w.max(1) as u32, b.h.max(1) as u32);
    for _ in 0..6 {
        let _ = w.set_position(pos);
        let _ = w.set_size(size);
        std::thread::sleep(std::time::Duration::from_millis(40));
        let settled = matches!(
            (w.outer_position(), w.outer_size()),
            (Ok(p), Ok(s)) if p == pos && s == size
        );
        if settled {
            return;
        }
    }
}

impl WindowHost for TauriHost {
    fn open(&mut self, spec: &WindowSpec) -> Result<WindowId, HostError> {
        let id = self.next_id;
        self.next_id += 1;
        let label = label_for(id);

        // Least privilege: the asset protocol may serve only files we display.
        self.app
            .asset_protocol_scope()
            .allow_file(&spec.image.local_path)
            .map_err(|e| HostError(e.to_string()))?;

        let source_url = match &spec.image.image {
            ImageRef::Url { url, .. } => Some(url.clone()),
            _ => None,
        };
        self.registry.lock().unwrap().insert(
            label.clone(),
            ViewerInit {
                path: spec.image.local_path.clone(),
                width: spec.image.width,
                height: spec.image.height,
                view: spec.view.into(),
                source_url,
            },
        );

        let built = WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::App("viewer.html".into()))
            .title("PinIt")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(!self.show_in_taskbar)
            .resizable(true)
            .visible(false)
            .build();
        match built {
            Ok(w) => {
                apply_bounds(&w, spec.bounds);
                let _ = w.show();
                Ok(id)
            }
            Err(e) => {
                self.registry.lock().unwrap().remove(&label);
                Err(HostError(e.to_string()))
            }
        }
    }

    fn set_bounds(&mut self, id: WindowId, bounds: Rect) {
        if let Some(w) = self.app.get_webview_window(&label_for(id)) {
            apply_bounds(&w, bounds);
        }
    }

    fn close(&mut self, id: WindowId) {
        if let Some(w) = self.app.get_webview_window(&label_for(id)) {
            let _ = w.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip() {
        assert_eq!(id_from_label(&label_for(42)), Some(42));
        assert_eq!(id_from_label("main"), None);
        assert_eq!(id_from_label("img-x"), None);
    }

    #[test]
    fn view_json_round_trips_and_sanitises() {
        let mut v = ViewState::default();
        v.rotate_quarter(true);
        v.set_fine_angle(-33.0);
        v.toggle_flip_v();
        v.zoom_by(2.0);
        let back: ViewState = ViewJson::from(v).into();
        assert_eq!(back, v);
        let hostile = ViewJson { zoom: 1e9, quarter_turns: 7, fine_angle_deg: 725.0, ..ViewJson::from(ViewState::default()) };
        let s: ViewState = hostile.into();
        assert_eq!((s.zoom, s.quarter_turns, s.fine_angle_deg), (10.0, 3, 5.0));
    }
}
