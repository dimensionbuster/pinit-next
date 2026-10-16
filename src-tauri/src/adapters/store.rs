//! JSON persistence for settings and the saved window layout.
//! Writes are atomic (temp file + rename) so a crash never leaves half a file.

use std::fs;
use std::path::{Path, PathBuf};

use pinit_core::geometry::Rect;
use pinit_core::image_ref::ImageRef;
use pinit_core::placement::{Anchor, Corner, Flow};
use pinit_core::ports::{LayoutStore, SavedWindow};
use pinit_core::service::{LayoutConfig, LayoutMode};
use pinit_core::view_state::ViewState;
use serde::{Deserialize, Serialize};

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeKind {
    Grid,
    Jigsaw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub corner: String,
    pub flow: String,
    pub mode: ModeKind,
    /// Grid row/column thickness in DIP (presets S/M/L are 200/300/440).
    pub grid_line: i32,
    pub jigsaw_base: i32,
    pub jigsaw_min_scale: f64,
    pub jigsaw_max_scale: f64,
    pub preferred_display: Option<u64>,
    pub show_in_taskbar: bool,
    /// Colour (#rrggbb) painted behind the image inside every viewer window.
    pub backdrop_color: String,
    /// 0.0 (invisible) ..= 1.0 (opaque).
    pub backdrop_opacity: f64,
    /// "Re-arrange" keeps each window's current size and only moves them.
    pub relayout_keep_sizes: bool,
    /// Set by the app (never by the UI) once the browser extension has talked to it.
    pub bridge_connected: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            corner: "topright".into(),
            flow: "rows".into(),
            mode: ModeKind::Jigsaw,
            grid_line: 300,
            jigsaw_base: 300,
            jigsaw_min_scale: 0.7,
            jigsaw_max_scale: 1.4,
            preferred_display: None,
            show_in_taskbar: false,
            backdrop_color: "#000000".into(),
            backdrop_opacity: 0.4,
            relayout_keep_sizes: false,
            bridge_connected: false,
        }
    }
}

impl Settings {
    /// Clamp/repair values that came from the UI or a hand-edited file.
    pub fn sanitized(mut self) -> Self {
        let c = self.backdrop_color.as_bytes();
        let valid = c.len() == 7 && c[0] == b'#' && c[1..].iter().all(u8::is_ascii_hexdigit);
        if !valid {
            self.backdrop_color = Settings::default().backdrop_color;
        }
        self.backdrop_color.make_ascii_lowercase();
        self.backdrop_opacity =
            if self.backdrop_opacity.is_finite() { self.backdrop_opacity.clamp(0.0, 1.0) } else { 0.4 };
        self
    }

    pub fn layout_config(&self) -> LayoutConfig {
        let corner = match self.corner.as_str() {
            "topleft" => Corner::TopLeft,
            "bottomleft" => Corner::BottomLeft,
            "bottomright" => Corner::BottomRight,
            _ => Corner::TopRight,
        };
        let flow = match self.flow.as_str() {
            "columns" => Flow::Columns,
            "diagonal" => Flow::Diagonal,
            _ => Flow::Rows,
        };
        let mode = match self.mode {
            ModeKind::Grid => LayoutMode::Grid { line: self.grid_line.clamp(80, 1600) },
            ModeKind::Jigsaw => {
                let lo = self.jigsaw_min_scale.clamp(0.2, 1.0);
                let hi = self.jigsaw_max_scale.clamp(lo, 3.0);
                LayoutMode::Jigsaw { base: self.jigsaw_base.clamp(80, 1600), min_scale: lo, max_scale: hi }
            }
        };
        LayoutConfig { anchor: Anchor { corner, flow }, mode, preferred_display: self.preferred_display }
    }
}

pub struct SettingsFile(pub PathBuf);

impl SettingsFile {
    pub fn load(&self) -> Settings {
        read_json::<Settings>(&self.0).unwrap_or_default().sanitized()
    }

    pub fn save(&self, s: &Settings) -> std::io::Result<()> {
        write_atomic(&self.0, &serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?)
    }
}

// ------------------------------------------------------------------ layout

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ImageDto {
    Url { url: String, referer: Option<String> },
    File { path: PathBuf },
}

#[derive(Serialize, Deserialize)]
struct ViewDto {
    zoom: f64,
    pan_x: f64,
    pan_y: f64,
    quarter_turns: u8,
    fine_angle_deg: f64,
    flip_h: bool,
    flip_v: bool,
}

#[derive(Serialize, Deserialize)]
struct SavedDto {
    image: ImageDto,
    bounds: [i32; 4],
    view: ViewDto,
    /// Absent in older session files = not hand-sized.
    #[serde(default)]
    user_sized: bool,
}

#[derive(Serialize, Deserialize)]
struct LayoutFile {
    version: u32,
    windows: Vec<SavedDto>,
}

impl SavedDto {
    fn from_core(w: &SavedWindow) -> Option<Self> {
        let image = match &w.image {
            ImageRef::Url { url, referer } => ImageDto::Url { url: url.clone(), referer: referer.clone() },
            // the staged first frame of a live video: the stream is gone after a restart
            ImageRef::File(path) if crate::video_bridge::is_stream_still(path) => return None,
            ImageRef::File(path) => ImageDto::File { path: path.clone() },
            ImageRef::Bytes { .. } => return None, // service persists these as cached files
        };
        let v = &w.view;
        Some(Self {
            image,
            bounds: [w.bounds.x, w.bounds.y, w.bounds.w, w.bounds.h],
            view: ViewDto {
                zoom: v.zoom,
                pan_x: v.pan_x,
                pan_y: v.pan_y,
                quarter_turns: v.quarter_turns,
                fine_angle_deg: v.fine_angle_deg,
                flip_h: v.flip_h,
                flip_v: v.flip_v,
            },
            user_sized: w.user_sized,
        })
    }

    fn into_core(self) -> SavedWindow {
        let image = match self.image {
            ImageDto::Url { url, referer } => ImageRef::Url { url, referer },
            ImageDto::File { path } => ImageRef::File(path),
        };
        let v = self.view;
        let [x, y, w, h] = self.bounds;
        SavedWindow {
            image,
            bounds: Rect::new(x, y, w, h),
            view: ViewState {
                zoom: v.zoom,
                pan_x: v.pan_x,
                pan_y: v.pan_y,
                quarter_turns: v.quarter_turns % 4,
                fine_angle_deg: v.fine_angle_deg,
                flip_h: v.flip_h,
                flip_v: v.flip_v,
            },
            user_sized: self.user_sized,
        }
    }
}

/// Serialises a layout. This is both the on-disk session format and the `.pinit` export format.
pub fn encode_layout(windows: &[SavedWindow]) -> Vec<u8> {
    let file = LayoutFile { version: 1, windows: windows.iter().filter_map(SavedDto::from_core).collect() };
    serde_json::to_vec_pretty(&file).unwrap_or_default()
}

/// Parses a layout; None for anything that is not a version-1 layout file.
pub fn decode_layout(bytes: &[u8]) -> Option<Vec<SavedWindow>> {
    let file: LayoutFile = serde_json::from_slice(bytes).ok()?;
    (file.version == 1).then(|| file.windows.into_iter().map(SavedDto::into_core).collect())
}

pub struct JsonLayoutStore {
    pub path: PathBuf,
}

impl LayoutStore for JsonLayoutStore {
    fn load(&self) -> Vec<SavedWindow> {
        fs::read(&self.path).ok().and_then(|b| decode_layout(&b)).unwrap_or_default()
    }

    fn save(&mut self, windows: &[SavedWindow]) {
        if let Err(e) = write_atomic(&self.path, &encode_layout(windows)) {
            eprintln!("layout save failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pinit-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn settings_default_when_missing_or_corrupt_and_tolerate_partial_files() {
        let d = dir("settings");
        let f = SettingsFile(d.join("settings.json"));
        assert_eq!(f.load(), Settings::default());
        fs::create_dir_all(&d).unwrap();
        fs::write(&f.0, b"{ not json").unwrap();
        assert_eq!(f.load(), Settings::default());
        fs::write(&f.0, br#"{"theme":"dark","grid_line":440}"#).unwrap();
        let s = f.load();
        assert_eq!((s.theme, s.grid_line), (Theme::Dark, 440));
        assert_eq!(s.jigsaw_base, 300); // untouched fields fall back to defaults
    }

    #[test]
    fn backdrop_values_are_sanitised() {
        let mut s = Settings::default();
        s.backdrop_color = "red; background:url(x)".into();
        s.backdrop_opacity = 7.0;
        let s = s.sanitized();
        assert_eq!((s.backdrop_color.as_str(), s.backdrop_opacity), ("#000000", 1.0));
        let mut s = Settings::default();
        s.backdrop_color = "#FFAA00".into();
        s.backdrop_opacity = f64::NAN;
        let s = s.sanitized();
        assert_eq!((s.backdrop_color.as_str(), s.backdrop_opacity), ("#ffaa00", 0.4));
    }

    #[test]
    fn settings_round_trip_and_clamp_into_a_valid_layout_config() {
        let d = dir("settings2");
        let f = SettingsFile(d.join("s.json"));
        let mut s = Settings::default();
        s.mode = ModeKind::Grid;
        s.grid_line = 99999;
        f.save(&s).unwrap();
        let loaded = f.load();
        assert_eq!(loaded, s);
        assert_eq!(loaded.layout_config().mode, LayoutMode::Grid { line: 1600 });
        assert!(!d.join("s.tmp").exists());
    }

    #[test]
    fn layout_round_trips_urls_files_and_view_state() {
        let d = dir("layout");
        let mut store = JsonLayoutStore { path: d.join("layout.json") };
        assert!(store.load().is_empty());
        let mut view = ViewState::default();
        view.rotate_quarter(true);
        view.set_fine_angle(12.5);
        view.toggle_flip_h();
        let saved = vec![
            SavedWindow {
                image: ImageRef::Url { url: "https://e.com/a.jpg".into(), referer: Some("https://e.com/".into()) },
                bounds: Rect::new(-1920, 40, 600, 300),
                view,
                user_sized: true,
            },
            SavedWindow {
                image: ImageRef::File(PathBuf::from(r"C:\pics\b.png")),
                bounds: Rect::new(0, 0, 300, 300),
                view: ViewState::default(),
                user_sized: false,
            },
        ];
        store.save(&saved);
        assert_eq!(store.load(), saved);
    }

    #[test]
    fn export_format_round_trips_and_rejects_foreign_files() {
        let saved = vec![SavedWindow {
            image: ImageRef::Url { url: "https://e.com/a.jpg".into(), referer: None },
            bounds: Rect::new(10, 20, 300, 200),
            view: ViewState::default(),
            user_sized: false,
        }];
        assert_eq!(decode_layout(&encode_layout(&saved)), Some(saved));
        assert_eq!(decode_layout(b"{}"), None);
        assert_eq!(decode_layout(br#"{"version":2,"windows":[]}"#), None);
        assert_eq!(decode_layout(b"not json"), None);
        assert_eq!(decode_layout(br#"{"version":1,"windows":[]}"#), Some(vec![]));
    }

    #[test]
    fn corrupt_layout_loads_as_empty() {
        let d = dir("layout-bad");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("layout.json"), b"[[[").unwrap();
        assert!(JsonLayoutStore { path: d.join("layout.json") }.load().is_empty());
    }
}
