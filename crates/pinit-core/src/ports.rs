//! Ports: the only things the core needs from the outside world.
//! Adapters (Tauri, filesystem, network) implement these; the core never
//! names a concrete adapter.

use std::fmt;

use crate::geometry::{Point, Rect};
use crate::image_ref::ImageRef;
use crate::view_state::ViewState;

pub type WindowId = u64;
pub type DisplayId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Display {
    pub id: DisplayId,
    pub bounds: Rect,
    /// Area usable by windows (excludes taskbar etc.), in DIP.
    pub work_area: Rect,
    pub is_primary: bool,
}

pub trait DisplayProvider {
    fn displays(&self) -> Vec<Display>;
    fn cursor(&self) -> Option<Point>;
}

/// An image whose pixel size is known and whose reference is stable
/// (a URL that loads, a file that exists, or cached bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImage {
    pub image: ImageRef,
    pub width: u32,
    pub height: u32,
}

impl ResolvedImage {
    pub fn aspect(&self) -> Option<f64> {
        (self.width > 0 && self.height > 0).then(|| self.width as f64 / self.height as f64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// This resolver does not handle the reference; try the next one.
    Unsupported,
    Failed(String),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Unsupported => write!(f, "unsupported image reference"),
            ResolveError::Failed(m) => write!(f, "resolve failed: {m}"),
        }
    }
}

impl std::error::Error for ResolveError {}

pub trait ImageResolver {
    fn resolve(&self, image: &ImageRef) -> Result<ResolvedImage, ResolveError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowSpec {
    pub image: ResolvedImage,
    pub bounds: Rect,
    pub view: ViewState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError(pub String);

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "window host error: {}", self.0)
    }
}

impl std::error::Error for HostError {}

pub trait WindowHost {
    fn open(&mut self, spec: &WindowSpec) -> Result<WindowId, HostError>;
    fn set_bounds(&mut self, id: WindowId, bounds: Rect);
    fn close(&mut self, id: WindowId);
}

/// A window as persisted between runs.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedWindow {
    pub image: ImageRef,
    pub bounds: Rect,
    pub view: ViewState,
}

pub trait LayoutStore {
    fn load(&self) -> Vec<SavedWindow>;
    fn save(&mut self, windows: &[SavedWindow]);
}
