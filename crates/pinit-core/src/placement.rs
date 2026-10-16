//! Window placement. Pure integer-DIP geometry; no OS, no DPI.
//!
//! Strategy: *frontier filling*. Windows are packed starting from an anchor
//! corner and a flow axis. Candidates are scanned in fill order (nearest the
//! anchor first), so any unavoidable gap migrates toward the open frontier and
//! reads as "not filled yet" rather than as a hole.
//!
//! Implementation works in a canonical frame (top-left origin, rows flowing
//! left-to-right, stacking downward). Other anchors are mirrored/transposed
//! into that frame and back.

use crate::geometry::Rect;
use crate::rng::Rng;

const MIN_DIM: i32 = 80;
const CASCADE_STEP: i32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Fill along a horizontal line, then start the next line below/above.
    Rows,
    /// Fill along a vertical line, then start the next line beside it.
    Columns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub corner: Corner,
    pub flow: Flow,
}

impl Anchor {
    pub const ALL: [Anchor; 8] = [
        Anchor { corner: Corner::TopLeft, flow: Flow::Rows },
        Anchor { corner: Corner::TopRight, flow: Flow::Rows },
        Anchor { corner: Corner::BottomLeft, flow: Flow::Rows },
        Anchor { corner: Corner::BottomRight, flow: Flow::Rows },
        Anchor { corner: Corner::TopLeft, flow: Flow::Columns },
        Anchor { corner: Corner::TopRight, flow: Flow::Columns },
        Anchor { corner: Corner::BottomLeft, flow: Flow::Columns },
        Anchor { corner: Corner::BottomRight, flow: Flow::Columns },
    ];
}

/// How a window's size is chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeRule {
    /// Fixed line thickness (row height for `Rows`, column width for `Columns`);
    /// the other side follows the image aspect ratio.
    Grid { line: i32 },
    /// Visual size varies randomly in `[min_scale, max_scale] * base`
    /// (equal-area normalised across aspects); may shrink down to `min_scale`
    /// to fit a gap.
    Jigsaw { base: i32, min_scale: f64, max_scale: f64 },
}

#[derive(Debug, Clone, Copy)]
pub struct PlacementRequest<'a> {
    /// Windows already on this display (obstacles), real coordinates.
    pub existing: &'a [Rect],
    /// Usable area of the display (work area), real coordinates.
    pub area: Rect,
    /// Effective width / height of the new window's content (after rotation).
    pub aspect: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub rect: Rect,
    /// True when no free spot existed and the window was cascaded on top.
    pub overflowed: bool,
}

pub trait PlacementStrategy {
    fn place(&self, req: &PlacementRequest, rng: &mut dyn Rng) -> Placement;
}

#[derive(Debug, Clone, Copy)]
pub struct FrontierPlacement {
    pub anchor: Anchor,
    pub rule: SizeRule,
}

impl FrontierPlacement {
    pub const fn new(anchor: Anchor, rule: SizeRule) -> Self {
        Self { anchor, rule }
    }
}

/// Maps real rects to the canonical frame and back.
struct Frame {
    area: Rect,
    flip_x: bool,
    flip_y: bool,
    transpose: bool,
}

impl Frame {
    fn new(area: Rect, anchor: Anchor) -> Self {
        let (flip_x, flip_y) = match anchor.corner {
            Corner::TopLeft => (false, false),
            Corner::TopRight => (true, false),
            Corner::BottomLeft => (false, true),
            Corner::BottomRight => (true, true),
        };
        Self { area, flip_x, flip_y, transpose: anchor.flow == Flow::Columns }
    }

    fn canon_area(&self) -> Rect {
        if self.transpose {
            Rect::new(0, 0, self.area.h, self.area.w)
        } else {
            Rect::new(0, 0, self.area.w, self.area.h)
        }
    }

    fn to_canon(&self, r: &Rect) -> Rect {
        let mut x = r.x - self.area.x;
        let mut y = r.y - self.area.y;
        if self.flip_x {
            x = self.area.w - x - r.w;
        }
        if self.flip_y {
            y = self.area.h - y - r.h;
        }
        if self.transpose {
            Rect::new(y, x, r.h, r.w)
        } else {
            Rect::new(x, y, r.w, r.h)
        }
    }

    fn to_real(&self, c: &Rect) -> Rect {
        let (mut x, mut y, w, h) =
            if self.transpose { (c.y, c.x, c.h, c.w) } else { (c.x, c.y, c.w, c.h) };
        if self.flip_x {
            x = self.area.w - x - w;
        }
        if self.flip_y {
            y = self.area.h - y - h;
        }
        Rect::new(x + self.area.x, y + self.area.y, w, h)
    }
}

fn fits(area: &Rect, obstacles: &[Rect], r: &Rect) -> bool {
    area.contains(r) && !obstacles.iter().any(|o| o.intersects(r))
}

fn dims_from_height(h: i32, aspect: f64) -> (i32, i32) {
    let h = h.max(1);
    (((h as f64) * aspect).round().max(1.0) as i32, h)
}

/// Shrink (w, h) uniformly so it fits within `max_w` x `max_h`, keeping aspect.
fn clamp_to(w: i32, h: i32, max_w: i32, max_h: i32) -> (i32, i32) {
    let k = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    (((w as f64) * k).floor().max(1.0) as i32, ((h as f64) * k).floor().max(1.0) as i32)
}

impl PlacementStrategy for FrontierPlacement {
    fn place(&self, req: &PlacementRequest, rng: &mut dyn Rng) -> Placement {
        let frame = Frame::new(req.area, self.anchor);
        let area = frame.canon_area();
        let aspect = if frame.transpose { 1.0 / req.aspect } else { req.aspect };
        let obstacles = canon_obstacles(&frame, req);

        // Target size (canonical), chosen once so rng consumption is fixed.
        let (tw, th, min_h) = match self.rule {
            SizeRule::Grid { line } => {
                let (w, h) = dims_from_height(line, aspect);
                let (w, h) = clamp_to(w, h, area.w, area.h);
                (w, h, h)
            }
            SizeRule::Jigsaw { base, min_scale, max_scale } => {
                let t = min_scale + (max_scale - min_scale) * rng.next_f64();
                let side = base as f64 * t;
                let root = aspect.sqrt();
                let (w, h) = (
                    (side * root).round().max(1.0) as i32,
                    (side / root).round().max(1.0) as i32,
                );
                let (w, h) = clamp_to(w, h, area.w, area.h);
                let floor = ((h as f64) * (min_scale / t).min(1.0)).round() as i32;
                (w, h, floor.max(MIN_DIM.min(h)))
            }
        };
        scan(&frame, &area, &obstacles, req.existing.len(), tw, th, min_h, aspect)
    }
}

impl FrontierPlacement {
    /// Place a window of an exact size (no size rule, no shrinking): used when
    /// re-arranging windows whose size the user chose. Oversized windows are
    /// scaled down (aspect kept) to fit the area.
    pub fn place_exact(&self, existing: &[Rect], area: Rect, w: i32, h: i32) -> Placement {
        let frame = Frame::new(area, self.anchor);
        let canon = frame.canon_area();
        let (cw, ch) = if frame.transpose { (h, w) } else { (w, h) };
        let (cw, ch) = clamp_to(cw.max(1), ch.max(1), canon.w, canon.h);
        let aspect = cw as f64 / ch as f64;
        let req = PlacementRequest { existing, area, aspect: 1.0 };
        let obstacles = canon_obstacles(&frame, &req);
        scan(&frame, &canon, &obstacles, existing.len(), cw, ch, ch, aspect)
    }
}

fn canon_obstacles(frame: &Frame, req: &PlacementRequest) -> Vec<Rect> {
    req.existing
        .iter()
        .filter(|r| r.intersects(&req.area))
        .map(|r| frame.to_canon(r))
        .collect()
}

/// Scan candidate spots in fill order (nearest the anchor first).
#[allow(clippy::too_many_arguments)]
fn scan(
    frame: &Frame,
    area: &Rect,
    obstacles: &[Rect],
    existing_count: usize,
    tw: i32,
    th: i32,
    min_h: i32,
    aspect: f64,
) -> Placement {
    let mut xs: Vec<i32> = vec![0];
    let mut ys: Vec<i32> = vec![0];
    for o in obstacles {
        xs.extend([o.x, o.right()]);
        ys.extend([o.y, o.bottom()]);
    }
    xs.retain(|&x| (0..area.w).contains(&x));
    ys.retain(|&y| (0..area.h).contains(&y));
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();

    for &y in &ys {
        for &x in &xs {
            if let Some(r) = fit_at(area, obstacles, x, y, tw, th, min_h, aspect) {
                return Placement { rect: frame.to_real(&r), overflowed: false };
            }
        }
    }

    // Nothing fits: cascade over the existing windows.
    let off = CASCADE_STEP * (existing_count as i32 % 10);
    let (w, h) = clamp_to(tw, th, (area.w - off).max(1), (area.h - off).max(1));
    Placement { rect: frame.to_real(&Rect::new(off, off, w, h)), overflowed: true }
}

#[allow(clippy::too_many_arguments)]
fn fit_at(
    area: &Rect,
    obstacles: &[Rect],
    x: i32,
    y: i32,
    tw: i32,
    th: i32,
    min_h: i32,
    aspect: f64,
) -> Option<Rect> {
    let full = Rect::new(x, y, tw, th);
    if fits(area, obstacles, &full) {
        return Some(full);
    }
    if min_h >= th {
        return None; // fixed size (grid) or no shrink room
    }
    // Largest height in [min_h, th) that fits; fit is monotone in size.
    let at = |h: i32| {
        let (w, h) = dims_from_height(h, aspect);
        Rect::new(x, y, w, h)
    };
    if !fits(area, obstacles, &at(min_h)) {
        return None;
    }
    let (mut lo, mut hi) = (min_h, th); // lo fits, hi does not
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if fits(area, obstacles, &at(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(at(lo))
}

/// Re-pack every item from scratch, trying several seeds and keeping the
/// layout with the least wasted space inside its bounding box.
/// `aspects` are in fill order. Returns placements in the same order.
pub fn relayout(
    strategy: &dyn PlacementStrategy,
    area: Rect,
    aspects: &[f64],
    seeds: impl IntoIterator<Item = u64>,
    make_rng: impl Fn(u64) -> Box<dyn Rng>,
) -> Vec<Placement> {
    let mut best: Option<(i64, Vec<Placement>)> = None;
    for seed in seeds {
        let mut rng = make_rng(seed);
        let mut placed: Vec<Placement> = Vec::with_capacity(aspects.len());
        let mut rects: Vec<Rect> = Vec::with_capacity(aspects.len());
        for &aspect in aspects {
            let p = strategy
                .place(&PlacementRequest { existing: &rects, area, aspect }, rng.as_mut());
            rects.push(p.rect);
            placed.push(p);
        }
        let score = waste(&placed);
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, placed));
        }
    }
    best.map(|(_, p)| p).unwrap_or_default()
}

/// One window in a mixed re-layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot {
    /// Size comes from the size rule for this content aspect.
    Rule(f64),
    /// The user chose this size; it is kept as is.
    Exact { w: i32, h: i32 },
}

/// Like [`relayout`], but some windows keep an exact size while the rest
/// follow the size rule. `slots` are in fill order; placements come back in
/// the same order.
pub fn relayout_mixed(
    strategy: &FrontierPlacement,
    area: Rect,
    slots: &[Slot],
    seeds: impl IntoIterator<Item = u64>,
    make_rng: impl Fn(u64) -> Box<dyn Rng>,
) -> Vec<Placement> {
    let mut best: Option<(i64, Vec<Placement>)> = None;
    for seed in seeds {
        let mut rng = make_rng(seed);
        let mut placed: Vec<Placement> = Vec::with_capacity(slots.len());
        let mut rects: Vec<Rect> = Vec::with_capacity(slots.len());
        for slot in slots {
            let p = match *slot {
                Slot::Rule(aspect) => strategy
                    .place(&PlacementRequest { existing: &rects, area, aspect }, rng.as_mut()),
                Slot::Exact { w, h } => strategy.place_exact(&rects, area, w, h),
            };
            rects.push(p.rect);
            placed.push(p);
        }
        let score = waste(&placed);
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, placed));
        }
    }
    best.map(|(_, p)| p).unwrap_or_default()
}

/// Unused area inside the bounding box of the layout; overflow is heavily penalised.
pub fn waste(placed: &[Placement]) -> i64 {
    if placed.is_empty() {
        return 0;
    }
    let (mut l, mut t, mut r, mut b) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let mut used = 0i64;
    let mut over = 0i64;
    for p in placed {
        l = l.min(p.rect.x);
        t = t.min(p.rect.y);
        r = r.max(p.rect.right());
        b = b.max(p.rect.bottom());
        used += p.rect.area();
        if p.overflowed {
            over += 1;
        }
    }
    (r - l) as i64 * (b - t) as i64 - used + over * 1_000_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    const AREA: Rect = Rect::new(0, 40, 1920, 1000);

    fn aspects(n: usize, seed: u64) -> Vec<f64> {
        let mut r = SplitMix64::new(seed ^ 0xABCD);
        (0..n).map(|_| 0.45 + r.next_f64() * 2.0).collect()
    }

    fn strategies(anchor: Anchor) -> Vec<(&'static str, FrontierPlacement)> {
        vec![
            ("grid", FrontierPlacement::new(anchor, SizeRule::Grid { line: 300 })),
            (
                "jigsaw",
                FrontierPlacement::new(
                    anchor,
                    SizeRule::Jigsaw { base: 300, min_scale: 0.7, max_scale: 1.4 },
                ),
            ),
        ]
    }

    fn fill(s: &FrontierPlacement, area: Rect, asp: &[f64], seed: u64) -> Vec<Placement> {
        let mut rng = SplitMix64::new(seed);
        let mut rects: Vec<Rect> = vec![];
        let mut out = vec![];
        for &a in asp {
            let p = s.place(&PlacementRequest { existing: &rects, area, aspect: a }, &mut rng);
            rects.push(p.rect);
            out.push(p);
        }
        out
    }

    #[test]
    fn no_overlap_no_escape_all_anchors_modes_seeds() {
        for anchor in Anchor::ALL {
            for (name, s) in strategies(anchor) {
                for seed in 0..20 {
                    let asp = aspects(14, seed);
                    let placed = fill(&s, AREA, &asp, seed);
                    for (i, p) in placed.iter().enumerate() {
                        if p.overflowed {
                            continue;
                        }
                        assert!(AREA.contains(&p.rect), "{name} {anchor:?} seed {seed} #{i} out of area");
                        for q in &placed[..i] {
                            assert!(!p.rect.intersects(&q.rect), "{name} {anchor:?} seed {seed} #{i} overlaps");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn aspect_is_preserved_within_rounding() {
        for anchor in Anchor::ALL {
            for (name, s) in strategies(anchor) {
                let asp = aspects(10, 3);
                for (p, a) in fill(&s, AREA, &asp, 3).iter().zip(&asp) {
                    if p.overflowed {
                        continue;
                    }
                    let got = p.rect.w as f64 / p.rect.h as f64;
                    assert!((got - a).abs() / a < 0.03, "{name} {anchor:?} want {a} got {got}");
                }
            }
        }
    }

    #[test]
    fn first_window_touches_anchor_corner() {
        for anchor in Anchor::ALL {
            for (_, s) in strategies(anchor) {
                let r = fill(&s, AREA, &[1.5], 1)[0].rect;
                let (want_x, want_y) = match anchor.corner {
                    Corner::TopLeft => (AREA.x, AREA.y),
                    Corner::TopRight => (AREA.right() - r.w, AREA.y),
                    Corner::BottomLeft => (AREA.x, AREA.bottom() - r.h),
                    Corner::BottomRight => (AREA.right() - r.w, AREA.bottom() - r.h),
                };
                assert_eq!((r.x, r.y), (want_x, want_y), "{anchor:?}");
            }
        }
    }

    #[test]
    fn grid_rows_share_thickness_and_flow_from_anchor() {
        let s = FrontierPlacement::new(
            Anchor { corner: Corner::TopLeft, flow: Flow::Rows },
            SizeRule::Grid { line: 300 },
        );
        let placed = fill(&s, AREA, &[1.0, 1.0, 1.0], 0);
        assert!(placed.iter().all(|p| p.rect.h == 300));
        assert_eq!(placed[1].rect.x, placed[0].rect.right());
        assert_eq!(placed[1].rect.y, placed[0].rect.y);
    }

    #[test]
    fn grid_wraps_to_next_row_when_line_is_full() {
        let s = FrontierPlacement::new(
            Anchor { corner: Corner::TopLeft, flow: Flow::Rows },
            SizeRule::Grid { line: 300 },
        );
        // 1920 / 300 = 6 squares per row, 7th wraps
        let placed = fill(&s, AREA, &[1.0; 7], 0);
        assert_eq!(placed[6].rect.x, AREA.x);
        assert_eq!(placed[6].rect.y, AREA.y + 300);
    }

    #[test]
    fn full_screen_cascades_instead_of_panicking() {
        let s = FrontierPlacement::new(
            Anchor { corner: Corner::TopLeft, flow: Flow::Rows },
            SizeRule::Grid { line: 500 },
        );
        let placed = fill(&s, AREA, &[1.0; 12], 0);
        assert!(placed.iter().any(|p| p.overflowed));
    }

    #[test]
    fn deterministic_for_same_seed() {
        let s = strategies(Anchor::ALL[0]).remove(1).1;
        let asp = aspects(12, 5);
        assert_eq!(fill(&s, AREA, &asp, 9), fill(&s, AREA, &asp, 9));
    }

    #[test]
    fn jigsaw_gaps_stay_bounded() {
        // Fraction of the occupied bounding box left empty must stay small:
        // gaps should live at the open frontier, not scattered inside.
        let s = FrontierPlacement::new(
            Anchor::ALL[0],
            SizeRule::Jigsaw { base: 300, min_scale: 0.7, max_scale: 1.4 },
        );
        for seed in 0..10 {
            let asp = aspects(8, seed);
            let placed = fill(&s, AREA, &asp, seed);
            assert!(placed.iter().all(|p| !p.overflowed));
            let (mut l, mut t, mut r, mut b) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for p in &placed {
                l = l.min(p.rect.x);
                t = t.min(p.rect.y);
                r = r.max(p.rect.right());
                b = b.max(p.rect.bottom());
            }
            let bbox = ((r - l) as i64 * (b - t) as i64) as f64;
            let ratio = waste(&placed) as f64 / bbox;
            println!("seed {seed}: waste ratio {ratio:.3}");
            assert!(ratio < 0.45, "seed {seed}: waste ratio {ratio:.3}");
        }
    }

    #[test]
    fn place_exact_keeps_size_and_never_overlaps_for_all_anchors() {
        let sizes = [(500, 200), (200, 500), (333, 333), (640, 360), (180, 240), (400, 400)];
        for anchor in Anchor::ALL {
            let s = FrontierPlacement::new(anchor, SizeRule::Grid { line: 300 });
            let mut rects: Vec<Rect> = vec![];
            for &(w, h) in &sizes {
                let p = s.place_exact(&rects, AREA, w, h);
                assert!(!p.overflowed, "{anchor:?}");
                assert_eq!((p.rect.w, p.rect.h), (w, h), "{anchor:?} size changed");
                assert!(AREA.contains(&p.rect), "{anchor:?}");
                assert!(rects.iter().all(|r| !r.intersects(&p.rect)), "{anchor:?} overlap");
                rects.push(p.rect);
            }
        }
    }

    #[test]
    fn place_exact_scales_down_only_when_larger_than_the_area() {
        let s = FrontierPlacement::new(Anchor::ALL[0], SizeRule::Grid { line: 300 });
        let p = s.place_exact(&[], AREA, 4000, 2000);
        assert!(AREA.contains(&p.rect));
        assert!((p.rect.w as f64 / p.rect.h as f64 - 2.0).abs() < 0.02);
    }

    #[test]
    fn relayout_is_no_worse_than_single_pass() {
        let s = FrontierPlacement::new(
            Anchor::ALL[0],
            SizeRule::Jigsaw { base: 300, min_scale: 0.7, max_scale: 1.4 },
        );
        let asp = aspects(12, 2);
        let single = fill(&s, AREA, &asp, 0);
        let best = relayout(&s, AREA, &asp, 0..32, |seed| Box::new(SplitMix64::new(seed)));
        assert_eq!(best.len(), asp.len());
        assert!(waste(&best) <= waste(&single));
    }
}
