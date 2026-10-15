//! Application service: opens, tracks, refits and re-lays-out image windows.
//! All collaborators are injected (composition root decides the concretes).

use std::fmt;

use crate::geometry::{Point, Rect};
use crate::image_ref::ImageRef;
use crate::placement::{
    relayout, Anchor, FrontierPlacement, PlacementRequest, PlacementStrategy, SizeRule,
};
use crate::ports::{
    Display, DisplayId, DisplayProvider, HostError, ImageResolver, ResolveError, SavedWindow,
    WindowHost, WindowId, WindowSpec,
};
use crate::rng::{Rng, SplitMix64};
use crate::view_state::ViewState;

const RELAYOUT_SEEDS: u64 = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    Grid { line: i32 },
    Jigsaw { base: i32, min_scale: f64, max_scale: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConfig {
    pub anchor: Anchor,
    pub mode: LayoutMode,
    pub preferred_display: Option<DisplayId>,
}

impl LayoutConfig {
    /// Strategy for a display with `scale` physical px per DIP: configured
    /// sizes are DIP, placement runs in physical pixels.
    pub fn strategy(&self, scale: f64) -> FrontierPlacement {
        let px = |dip: i32| ((dip as f64) * scale).round() as i32;
        let rule = match self.mode {
            LayoutMode::Grid { line } => SizeRule::Grid { line: px(line) },
            LayoutMode::Jigsaw { base, min_scale, max_scale } => {
                SizeRule::Jigsaw { base: px(base), min_scale, max_scale }
            }
        };
        FrontierPlacement::new(self.anchor, rule)
    }
}

/// Restoring a saved window: skip auto placement if the spot is still on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Restore {
    pub bounds: Rect,
    pub view: ViewState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenRequest {
    pub image: ImageRef,
    pub restore: Option<Restore>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    Opened(WindowId),
    AlreadyOpen(WindowId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    Resolve(ResolveError),
    Host(HostError),
    NoDisplay,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::Resolve(e) => e.fmt(f),
            OpenError::Host(e) => e.fmt(f),
            OpenError::NoDisplay => write!(f, "no display available"),
        }
    }
}

impl std::error::Error for OpenError {}

struct Entry {
    id: WindowId,
    key: String,
    image: ImageRef,
    display_id: DisplayId,
    bounds: Rect,
    view: ViewState,
    natural_aspect: f64,
}

impl Entry {
    fn aspect(&self) -> f64 {
        self.view.effective_aspect(self.natural_aspect)
    }
}

pub struct OpenService {
    resolver: Box<dyn ImageResolver + Send>,
    host: Box<dyn WindowHost + Send>,
    displays: Box<dyn DisplayProvider + Send>,
    rng: Box<dyn Rng + Send>,
    config: LayoutConfig,
    windows: Vec<Entry>,
}

impl OpenService {
    pub fn new(
        resolver: Box<dyn ImageResolver + Send>,
        host: Box<dyn WindowHost + Send>,
        displays: Box<dyn DisplayProvider + Send>,
        rng: Box<dyn Rng + Send>,
        config: LayoutConfig,
    ) -> Self {
        Self { resolver, host, displays, rng, config, windows: Vec::new() }
    }

    pub fn set_config(&mut self, config: LayoutConfig) {
        self.config = config;
    }

    pub fn open(&mut self, req: OpenRequest) -> Result<OpenOutcome, OpenError> {
        let key = req.image.key();
        if let Some(e) = self.windows.iter().find(|e| e.key == key) {
            return Ok(OpenOutcome::AlreadyOpen(e.id));
        }

        let resolved = self.resolver.resolve(&req.image).map_err(OpenError::Resolve)?;
        let natural_aspect = resolved
            .aspect()
            .ok_or_else(|| OpenError::Resolve(ResolveError::Failed("image has no size".into())))?;
        let view = req.restore.as_ref().map(|r| r.view).unwrap_or_default();
        let aspect = view.effective_aspect(natural_aspect);

        let displays = self.displays.displays();
        let (display_id, bounds) = match req
            .restore
            .as_ref()
            .and_then(|r| display_for_rect(&displays, &r.bounds).map(|d| (d.id, r.bounds)))
        {
            Some(found) => found,
            None => {
                let d = choose_display(
                    &displays,
                    self.config.preferred_display,
                    self.displays.cursor(),
                )
                .ok_or(OpenError::NoDisplay)?;
                let obstacles: Vec<Rect> = self
                    .windows
                    .iter()
                    .filter(|e| e.display_id == d.id)
                    .map(|e| e.bounds)
                    .collect();
                let p = self.config.strategy(d.scale).place(
                    &PlacementRequest { existing: &obstacles, area: d.work_area, aspect },
                    self.rng.as_mut(),
                );
                (d.id, p.rect)
            }
        };

        let id = self
            .host
            .open(&WindowSpec { image: resolved.clone(), bounds, view })
            .map_err(OpenError::Host)?;
        self.windows.push(Entry {
            id,
            key,
            image: persistable(&resolved),
            display_id,
            bounds,
            view,
            natural_aspect,
        });
        Ok(OpenOutcome::Opened(id))
    }

    /// Host reports the window was closed (by the user or otherwise).
    pub fn on_closed(&mut self, id: WindowId) {
        self.windows.retain(|e| e.id != id);
    }

    /// Host reports the user moved or resized the window.
    pub fn on_bounds_changed(&mut self, id: WindowId, bounds: Rect) {
        let displays = self.displays.displays();
        if let Some(e) = self.windows.iter_mut().find(|e| e.id == id) {
            e.bounds = bounds;
            if let Some(d) = display_for_rect(&displays, &bounds) {
                e.display_id = d.id;
            }
        }
    }

    /// UI changed the view. If the effective aspect changed (quarter turn),
    /// the window is refit around its centre and the new bounds are returned.
    pub fn set_view(&mut self, id: WindowId, view: ViewState) -> Option<Rect> {
        let displays = self.displays.displays();
        let e = self.windows.iter_mut().find(|e| e.id == id)?;
        let old_aspect = e.aspect();
        e.view = view;
        let new_aspect = e.aspect();
        if (new_aspect - old_aspect).abs() < 1e-9 {
            return None;
        }
        let area = displays
            .iter()
            .find(|d| d.id == e.display_id)
            .map(|d| d.work_area)
            .unwrap_or(e.bounds);
        let refit = refit(e.bounds, new_aspect, area);
        e.bounds = refit;
        self.host.set_bounds(id, refit);
        Some(refit)
    }

    /// Re-pack every window per display using the current config. Order of
    /// opening is the fill order.
    pub fn relayout_all(&mut self) {
        let displays = self.displays.displays();
        let base_seed = self.rng.next_u64();
        for d in &displays {
            let idx: Vec<usize> = (0..self.windows.len())
                .filter(|&i| self.windows[i].display_id == d.id)
                .collect();
            if idx.is_empty() {
                continue;
            }
            let strategy = self.config.strategy(d.scale);
            let aspects: Vec<f64> = idx.iter().map(|&i| self.windows[i].aspect()).collect();
            let placed = relayout(&strategy, d.work_area, &aspects, base_seed..base_seed + RELAYOUT_SEEDS, |s| {
                Box::new(SplitMix64::new(s))
            });
            for (&i, p) in idx.iter().zip(placed) {
                self.windows[i].bounds = p.rect;
                self.host.set_bounds(self.windows[i].id, p.rect);
            }
        }
    }

    pub fn snapshot(&self) -> Vec<SavedWindow> {
        self.windows
            .iter()
            .map(|e| SavedWindow { image: e.image.clone(), bounds: e.bounds, view: e.view })
            .collect()
    }

    pub fn open_count(&self) -> usize {
        self.windows.len()
    }
}

/// What to save for restore: in-memory bytes only survive as their cached file.
fn persistable(r: &crate::ports::ResolvedImage) -> ImageRef {
    match &r.image {
        ImageRef::Bytes { .. } => ImageRef::File(r.local_path.clone()),
        other => other.clone(),
    }
}

/// Preferred display if present, else the one under the cursor, else primary.
pub fn choose_display(
    displays: &[Display],
    preferred: Option<DisplayId>,
    cursor: Option<Point>,
) -> Option<&Display> {
    preferred
        .and_then(|id| displays.iter().find(|d| d.id == id))
        .or_else(|| cursor.and_then(|c| displays.iter().find(|d| d.bounds.contains_point(c))))
        .or_else(|| displays.iter().find(|d| d.is_primary))
        .or_else(|| displays.first())
}

/// Display whose work area overlaps `r` the most; None if fully off-screen.
pub fn display_for_rect<'a>(displays: &'a [Display], r: &Rect) -> Option<&'a Display> {
    displays
        .iter()
        .map(|d| (d, r.overlap_area(&d.work_area)))
        .filter(|(_, a)| *a > 0)
        .max_by_key(|(_, a)| *a)
        .map(|(d, _)| d)
}

/// Resize to `aspect` keeping (approximately) the same area and the same
/// centre, then clamp into `area`.
pub fn refit(bounds: Rect, aspect: f64, area: Rect) -> Rect {
    let s = bounds.area() as f64;
    let (mut w, mut h) = ((s * aspect).sqrt(), (s / aspect).sqrt());
    let k = (area.w as f64 / w).min(area.h as f64 / h).min(1.0);
    w *= k;
    h *= k;
    let (w, h) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32);
    let c = bounds.center();
    let x = (c.x - w / 2).clamp(area.x, (area.right() - w).max(area.x));
    let y = (c.y - h / 2).clamp(area.y, (area.bottom() - h).max(area.y));
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placement::{Corner, Flow};
    use crate::ports::ResolvedImage;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct HostLog {
        opened: Vec<WindowSpec>,
        bounds: Vec<(WindowId, Rect)>,
        next: WindowId,
    }

    struct FakeHost(Arc<Mutex<HostLog>>);

    impl WindowHost for FakeHost {
        fn open(&mut self, spec: &WindowSpec) -> Result<WindowId, HostError> {
            let mut l = self.0.lock().unwrap();
            l.next += 1;
            l.opened.push(spec.clone());
            Ok(l.next)
        }
        fn set_bounds(&mut self, id: WindowId, b: Rect) {
            self.0.lock().unwrap().bounds.push((id, b));
        }
        fn close(&mut self, _id: WindowId) {}
    }

    struct FakeDisplays(Vec<Display>, Option<Point>);

    impl DisplayProvider for FakeDisplays {
        fn displays(&self) -> Vec<Display> {
            self.0.clone()
        }
        fn cursor(&self) -> Option<Point> {
            self.1
        }
    }

    /// Resolves "wide" -> 2:1, "tall" -> 1:2, anything else 1:1; "bad" fails.
    struct FakeResolver;

    impl ImageResolver for FakeResolver {
        fn resolve(&self, image: &ImageRef) -> Result<ResolvedImage, ResolveError> {
            let name = match image {
                ImageRef::Url { url, .. } => url.clone(),
                _ => String::new(),
            };
            if name.contains("bad") {
                return Err(ResolveError::Failed("nope".into()));
            }
            let (width, height) = if name.contains("wide") {
                (2000, 1000)
            } else if name.contains("tall") {
                (1000, 2000)
            } else {
                (1000, 1000)
            };
            Ok(ResolvedImage { image: image.clone(), local_path: "fake.png".into(), width, height })
        }
    }

    fn url(u: &str) -> ImageRef {
        ImageRef::Url { url: format!("https://e.com/{u}.jpg"), referer: None }
    }

    fn req(u: &str) -> OpenRequest {
        OpenRequest { image: url(u), restore: None }
    }

    fn two_displays() -> Vec<Display> {
        vec![
            Display {
                id: 1,
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1040),
                scale: 1.0,
                is_primary: true,
            },
            Display {
                id: 2,
                bounds: Rect::new(1920, 0, 1920, 1080),
                work_area: Rect::new(1920, 0, 1920, 1080),
                scale: 1.0,
                is_primary: false,
            },
        ]
    }

    fn service(cursor: Option<Point>, preferred: Option<DisplayId>) -> (OpenService, Arc<Mutex<HostLog>>) {
        let log = Arc::new(Mutex::new(HostLog::default()));
        let svc = OpenService::new(
            Box::new(FakeResolver),
            Box::new(FakeHost(log.clone())),
            Box::new(FakeDisplays(two_displays(), cursor)),
            Box::new(SplitMix64::new(1)),
            LayoutConfig {
                anchor: Anchor { corner: Corner::TopLeft, flow: Flow::Rows },
                mode: LayoutMode::Grid { line: 300 },
                preferred_display: preferred,
            },
        );
        (svc, log)
    }

    #[test]
    fn opens_at_anchor_with_image_aspect() {
        let (mut svc, log) = service(None, None);
        assert_eq!(svc.open(req("wide")), Ok(OpenOutcome::Opened(1)));
        let spec = &log.lock().unwrap().opened[0];
        assert_eq!(spec.bounds, Rect::new(0, 0, 600, 300));
    }

    #[test]
    fn same_image_is_not_opened_twice() {
        let (mut svc, log) = service(None, None);
        svc.open(req("a")).unwrap();
        let again = OpenRequest { image: url("a"), restore: None };
        assert_eq!(svc.open(again), Ok(OpenOutcome::AlreadyOpen(1)));
        assert_eq!(log.lock().unwrap().opened.len(), 1);
        assert_eq!(svc.open_count(), 1);
    }

    #[test]
    fn resolve_failure_opens_nothing() {
        let (mut svc, log) = service(None, None);
        assert!(matches!(svc.open(req("bad")), Err(OpenError::Resolve(_))));
        assert!(log.lock().unwrap().opened.is_empty());
        assert_eq!(svc.open_count(), 0);
    }

    #[test]
    fn display_choice_prefers_setting_then_cursor_then_primary() {
        let d = two_displays();
        assert_eq!(choose_display(&d, Some(2), Some(Point { x: 10, y: 10 })).unwrap().id, 2);
        assert_eq!(choose_display(&d, None, Some(Point { x: 2000, y: 10 })).unwrap().id, 2);
        assert_eq!(choose_display(&d, Some(99), None).unwrap().id, 1);
        assert!(choose_display(&[], None, None).is_none());
    }

    #[test]
    fn windows_land_on_cursor_display_and_fill_it_independently() {
        let (mut svc, log) = service(Some(Point { x: 2500, y: 100 }), None);
        svc.open(req("a")).unwrap();
        svc.open(req("b")).unwrap();
        let l = log.lock().unwrap();
        assert_eq!(l.opened[0].bounds, Rect::new(1920, 0, 300, 300));
        assert_eq!(l.opened[1].bounds, Rect::new(2220, 0, 300, 300));
    }

    #[test]
    fn grid_line_is_dip_scaled_per_display() {
        let log = Arc::new(Mutex::new(HostLog::default()));
        let mut displays = two_displays();
        displays[1].scale = 1.5;
        let mut svc = OpenService::new(
            Box::new(FakeResolver),
            Box::new(FakeHost(log.clone())),
            Box::new(FakeDisplays(displays, Some(Point { x: 2500, y: 100 }))),
            Box::new(SplitMix64::new(1)),
            LayoutConfig {
                anchor: Anchor { corner: Corner::TopLeft, flow: Flow::Rows },
                mode: LayoutMode::Grid { line: 300 },
                preferred_display: None,
            },
        );
        svc.open(req("a")).unwrap();
        // 300 DIP * 1.5 = 450 physical px on the HiDPI display
        assert_eq!(log.lock().unwrap().opened[0].bounds, Rect::new(1920, 0, 450, 450));
    }

    #[test]
    fn closed_window_frees_its_slot_only_after_relayout() {
        let (mut svc, log) = service(None, None);
        svc.open(req("a")).unwrap();
        svc.open(req("b")).unwrap();
        svc.open(req("c")).unwrap();
        svc.on_closed(1);
        assert_eq!(svc.open_count(), 2);
        svc.relayout_all();
        let moved: Vec<(WindowId, Rect)> = log.lock().unwrap().bounds.clone();
        assert_eq!(moved, vec![(2, Rect::new(0, 0, 300, 300)), (3, Rect::new(300, 0, 300, 300))]);
    }

    #[test]
    fn quarter_turn_refits_window_around_its_centre() {
        let (mut svc, log) = service(None, None);
        svc.open(req("wide")).unwrap(); // 600x300 at (0,0)
        let mut v = ViewState::default();
        v.rotate_quarter(true);
        let b = svc.set_view(1, v).expect("aspect changed");
        assert_eq!((b.w, b.h), (300, 600));
        assert_eq!(log.lock().unwrap().bounds.last(), Some(&(1, b)));
        // free-angle rotation never resizes
        v.set_fine_angle(20.0);
        assert!(svc.set_view(1, v).is_none());
    }

    #[test]
    fn refit_stays_inside_area_and_keeps_area_roughly() {
        let area = Rect::new(0, 0, 800, 600);
        let r = refit(Rect::new(700, 500, 400, 100), 0.25, area);
        assert!(area.contains(&r));
        let r2 = refit(Rect::new(100, 100, 400, 200), 0.5, area);
        assert_eq!((r2.w, r2.h), (200, 400));
    }

    #[test]
    fn restore_keeps_saved_bounds_when_still_on_screen_else_replaces() {
        let (mut svc, log) = service(None, None);
        let on = Restore { bounds: Rect::new(100, 100, 250, 250), view: ViewState::default() };
        svc.open(OpenRequest { image: url("a"), restore: Some(on) }).unwrap();
        let off = Restore { bounds: Rect::new(9000, 9000, 250, 250), view: ViewState::default() };
        svc.open(OpenRequest { image: url("b"), restore: Some(off) }).unwrap();
        let l = log.lock().unwrap();
        assert_eq!(l.opened[0].bounds, Rect::new(100, 100, 250, 250));
        // The restored window blocks the first slots, so the off-screen one is
        // re-placed at the first free spot in anchor order.
        assert_eq!(l.opened[1].bounds, Rect::new(350, 0, 300, 300));
    }

    #[test]
    fn snapshot_round_trips_image_bounds_and_view() {
        let (mut svc, _) = service(None, None);
        svc.open(req("a")).unwrap();
        let mut v = ViewState::default();
        v.toggle_flip_h();
        svc.set_view(1, v);
        let snap = svc.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(snap[0].view.flip_h);
        assert_eq!(snap[0].bounds, Rect::new(0, 0, 300, 300));
    }
}
