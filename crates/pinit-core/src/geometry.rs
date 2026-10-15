#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Axis-aligned rectangle in integer physical pixels of the virtual desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn center(&self) -> Point {
        Point { x: self.x + self.w / 2, y: self.y + self.h / 2 }
    }

    pub const fn contains_point(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// Area of the intersection (0 when disjoint).
    pub fn overlap_area(&self, o: &Rect) -> i64 {
        let w = (self.right().min(o.right()) - self.x.max(o.x)).max(0) as i64;
        let h = (self.bottom().min(o.bottom()) - self.y.max(o.y)).max(0) as i64;
        w * h
    }

    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub const fn area(&self) -> i64 {
        self.w as i64 * self.h as i64
    }

    /// True if the interiors overlap (touching edges do not count).
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    pub fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_edges_do_not_intersect() {
        let a = Rect::new(0, 0, 10, 10);
        assert!(!a.intersects(&Rect::new(10, 0, 5, 5)));
        assert!(a.intersects(&Rect::new(9, 9, 5, 5)));
    }

    #[test]
    fn contains_requires_full_inclusion() {
        let a = Rect::new(0, 0, 10, 10);
        assert!(a.contains(&Rect::new(2, 2, 8, 8)));
        assert!(!a.contains(&Rect::new(2, 2, 9, 8)));
    }
}
