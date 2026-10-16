//! Application service: opens, tracks, refits and re-lays-out image windows.
//! All collaborators are injected (composition root decides the concretes).

use std::fmt;

use crate::geometry::{Point, Rect};
use crate::image_ref::ImageRef;
use crate::placement::{
    relayout_mixed, Anchor, FrontierPlacement, PlacementRequest, PlacementStrategy, Slot, SizeRule,
};
use crate::ports::{
    Display, DisplayId, DisplayProvider, HostError, ImageResolver, ResolveError, ResolvedImage,
    SavedWindow, WindowHost, WindowId, WindowSpec,
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
    pub user_sized: bool,
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
    /// Last bounds the service itself set (open/refit/relayout/fit); lets
    /// `fit_to_aspect` tell which side the user dragged.
    last_fit: Rect,
    /// The user resized this window by hand (size drifted from `last_fit`).
    user_sized: bool,
    view: ViewState,
    natural_aspect: f64,
}

/// Slack for the OS rounding a size the service asked for (DPI, min size).
const SIZE_TOLERANCE: i32 = 4;

impl Entry {
    fn aspect(&self) -> f64 {
        self.view.effective_aspect(self.natural_aspect)
    }

    /// Hand-resized, zoomed or rotated: "keep current size" leaves it alone.
    fn is_touched(&self) -> bool {
        self.user_sized
            || (self.view.zoom - 1.0).abs() > 1e-6
            || self.view.quarter_turns != 0
            || self.view.fine_angle_deg.abs() > 1e-6
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

    /// Window already showing this image, if any (cheap; call before resolving).
    pub fn find_open(&self, image: &ImageRef) -> Option<WindowId> {
        let key = image.key();
        self.windows.iter().find(|e| e.key == key).map(|e| e.id)
    }

    /// Resolve with the built-in resolver, then open. The resolver may block
    /// (network); callers that cannot hold the service lock that long should
    /// resolve first and call [`open_resolved`](Self::open_resolved).
    pub fn open(&mut self, req: OpenRequest) -> Result<OpenOutcome, OpenError> {
        if let Some(id) = self.find_open(&req.image) {
            return Ok(OpenOutcome::AlreadyOpen(id));
        }
        let resolved = self.resolver.resolve(&req.image).map_err(OpenError::Resolve)?;
        self.open_resolved(resolved, req.restore)
    }

    pub fn open_resolved(
        &mut self,
        resolved: ResolvedImage,
        restore: Option<Restore>,
    ) -> Result<OpenOutcome, OpenError> {
        let key = resolved.image.key();
        if let Some(id) = self.find_open(&resolved.image) {
            return Ok(OpenOutcome::AlreadyOpen(id));
        }
        let natural_aspect = resolved
            .aspect()
            .ok_or_else(|| OpenError::Resolve(ResolveError::Failed("image has no size".into())))?;
        let view = restore.as_ref().map(|r| r.view).unwrap_or_default();
        let user_sized = restore.as_ref().is_some_and(|r| r.user_sized);
        let aspect = view.effective_aspect(natural_aspect);

        let displays = self.displays.displays();
        let (display_id, bounds) = match restore
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
            last_fit: bounds,
            user_sized,
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
            if (bounds.w - e.last_fit.w).abs() > SIZE_TOLERANCE
                || (bounds.h - e.last_fit.h).abs() > SIZE_TOLERANCE
            {
                e.user_sized = true;
            }
            e.bounds = bounds;
            if let Some(d) = display_for_rect(&displays, &bounds) {
                e.display_id = d.id;
            }
        }
    }

    /// UI changed the view. If the effective aspect changed (quarter turn), the
    /// window's width and height are swapped around its centre (so a window the
    /// user shaped by hand keeps that shape) and the new bounds are returned.
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
        let refit = refit(e.bounds, e.bounds.h as f64 / e.bounds.w as f64, area);
        e.bounds = refit;
        e.last_fit = refit;
        self.host.set_bounds(id, refit);
        Some(refit)
    }

    /// After a user resize: snap back to the image aspect, keeping the side the
    /// user changed most. Returns the new bounds, or None if already fitting.
    pub fn fit_to_aspect(&mut self, id: WindowId) -> Option<Rect> {
        let displays = self.displays.displays();
        let e = self.windows.iter_mut().find(|e| e.id == id)?;
        let aspect = e.aspect();
        let b = e.bounds;
        if ((b.w as f64 / b.h as f64) - aspect).abs() * (b.h as f64) < 1.5 {
            e.last_fit = b;
            return None;
        }
        let keep_width = (b.w - e.last_fit.w).abs() >= (b.h - e.last_fit.h).abs();
        let (mut w, mut h) = if keep_width {
            (b.w, ((b.w as f64) / aspect).round() as i32)
        } else {
            (((b.h as f64) * aspect).round() as i32, b.h)
        };
        let area = displays
            .iter()
            .find(|d| d.id == e.display_id)
            .map(|d| d.work_area)
            .unwrap_or(b);
        let (cw, ch) = clamp_aspect(w, h, area.w, area.h);
        w = cw;
        h = ch;
        let x = b.x.clamp(area.x, (area.right() - w).max(area.x));
        let y = b.y.clamp(area.y, (area.bottom() - h).max(area.y));
        let fitted = Rect::new(x, y, w.max(1), h.max(1));
        e.bounds = fitted;
        e.last_fit = fitted;
        self.host.set_bounds(id, fitted);
        Some(fitted)
    }

    /// Re-pack every window per display from the anchor. Opening order is the
    /// fill order. With `keep_sizes` only the windows the user touched (resized
    /// by hand, zoomed or rotated) keep their current width and height; the
    /// rest follow the size rule. Without it every size is recomputed from the
    /// image aspect and the configured size rule (best of several seeds).
    pub fn relayout_all(&mut self, keep_sizes: bool) {
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
            let slots: Vec<Slot> = idx
                .iter()
                .map(|&i| {
                    let e = &self.windows[i];
                    if keep_sizes && e.is_touched() {
                        Slot::Exact { w: e.bounds.w, h: e.bounds.h }
                    } else {
                        Slot::Rule(e.aspect())
                    }
                })
                .collect();
            let rects: Vec<Rect> = relayout_mixed(
                &strategy,
                d.work_area,
                &slots,
                base_seed..base_seed + RELAYOUT_SEEDS,
                |s| Box::new(SplitMix64::new(s)),
            )
            .into_iter()
            .map(|p| p.rect)
            .collect();
            for (&i, rect) in idx.iter().zip(rects) {
                let e = &mut self.windows[i];
                if !(keep_sizes && e.is_touched()) {
                    e.user_sized = false;
                }
                e.bounds = rect;
                e.last_fit = rect;
                self.host.set_bounds(e.id, rect);
            }
        }
    }

    pub fn snapshot(&self) -> Vec<SavedWindow> {
        self.windows
            .iter()
            .map(|e| SavedWindow { image: e.image.clone(), bounds: e.bounds, view: e.view, user_sized: e.user_sized })
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

fn clamp_aspect(w: i32, h: i32, max_w: i32, max_h: i32) -> (i32, i32) {
    let k = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    (((w as f64) * k).round() as i32, ((h as f64) * k).round() as i32)
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
    fn relayout_can_keep_hand_chosen_sizes() {
        let (mut svc, log) = service(None, None);
        svc.open(req("a")).unwrap();
        svc.open(req("b")).unwrap();
        svc.open(req("c")).unwrap();
        // user reshaped windows by hand (not the grid line, not the image aspect)
        svc.on_bounds_changed(1, Rect::new(500, 500, 420, 180));
        svc.on_bounds_changed(2, Rect::new(900, 100, 250, 640));
        svc.on_bounds_changed(3, Rect::new(10, 700, 333, 333));
        svc.relayout_all(true);
        {
            let l = log.lock().unwrap();
            let last: Vec<(WindowId, Rect)> = l.bounds.iter().rev().take(3).rev().cloned().collect();
            assert_eq!(last[0], (1, Rect::new(0, 0, 420, 180)));
            assert_eq!(last[1], (2, Rect::new(420, 0, 250, 640)));
            assert_eq!(last[2], (3, Rect::new(670, 0, 333, 333)));
        }
        // default relayout still normalises sizes to the grid line
        svc.relayout_all(false);
        let l = log.lock().unwrap();
        assert!(l.bounds.iter().rev().take(3).all(|(_, r)| r.h == 300));
    }

    #[test]
    fn keep_sizes_only_pins_windows_the_user_touched() {
        let (mut svc, log) = service(None, None);
        svc.open(req("a")).unwrap(); // 300x300 grid cell
        svc.open(req("b")).unwrap();
        svc.open(req("c")).unwrap();
        // b resized by hand; c zoomed; a untouched. The host echoing our own size is not a user resize.
        svc.on_bounds_changed(1, Rect::new(0, 0, 301, 299));
        svc.on_bounds_changed(2, Rect::new(900, 100, 250, 640));
        let mut v = ViewState::default();
        v.zoom = 2.0;
        svc.set_view(3, v);
        svc.relayout_all(true);
        let l = log.lock().unwrap();
        let last: Vec<(WindowId, Rect)> = l.bounds.iter().rev().take(3).rev().cloned().collect();
        assert_eq!(last[0], (1, Rect::new(0, 0, 300, 300)));
        assert_eq!(last[1], (2, Rect::new(300, 0, 250, 640)));
        assert_eq!((last[2].1.w, last[2].1.h), (300, 300));
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
        svc.relayout_all(false);
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
        // a hand-shaped (non image-aspect) window keeps its shape, just swapped
        svc.on_bounds_changed(1, Rect::new(10, 10, 500, 200));
        v.rotate_quarter(true);
        let b = svc.set_view(1, v).expect("aspect changed");
        assert_eq!((b.w, b.h), (200, 500));
        assert_eq!(log.lock().unwrap().bounds.last(), Some(&(1, b)));
        // free-angle rotation never resizes
        v.set_fine_angle(20.0);
        assert!(svc.set_view(1, v).is_none());
    }

    #[test]
    fn fit_to_aspect_follows_the_side_the_user_changed() {
        let (mut svc, log) = service(None, None);
        svc.open(req("wide")).unwrap(); // 600x300
        // user widened the window to 800x300 -> keep width, height follows (400)
        svc.on_bounds_changed(1, Rect::new(0, 0, 800, 300));
        assert_eq!(svc.fit_to_aspect(1), Some(Rect::new(0, 0, 800, 400)));
        assert_eq!(log.lock().unwrap().bounds.last().map(|b| b.1), Some(Rect::new(0, 0, 800, 400)));
        // user then made it taller: 800x600 -> keep height, width follows (1200)
        svc.on_bounds_changed(1, Rect::new(0, 0, 800, 600));
        assert_eq!(svc.fit_to_aspect(1), Some(Rect::new(0, 0, 1200, 600)));
        // already fitting: no-op
        svc.on_bounds_changed(1, Rect::new(0, 0, 1200, 600));
        assert_eq!(svc.fit_to_aspect(1), None);
    }

    #[test]
    fn fit_to_aspect_never_leaves_the_work_area() {
        let (mut svc, _) = service(None, None);
        svc.open(req("wide")).unwrap();
        svc.on_bounds_changed(1, Rect::new(1200, 700, 700, 200));
        let r = svc.fit_to_aspect(1).unwrap();
        assert!(Rect::new(0, 0, 1920, 1040).contains(&r), "{r:?}");
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
        let on = Restore { bounds: Rect::new(100, 100, 250, 250), view: ViewState::default(), user_sized: false };
        svc.open(OpenRequest { image: url("a"), restore: Some(on) }).unwrap();
        let off = Restore { bounds: Rect::new(9000, 9000, 250, 250), view: ViewState::default(), user_sized: false };
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
