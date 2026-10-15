//! Per-window view transform: zoom, pan, rotation, flips.
//!
//! Transform order (renderer must match): flip in image space, then rotate by
//! `total_angle_deg()` about the image centre, then zoom, then pan.

pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewState {
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    /// Clockwise quarter turns, 0..=3. Changes the window's aspect ratio.
    pub quarter_turns: u8,
    /// Free rotation on top of the quarter turns, degrees in (-180, 180].
    /// Rotates the image inside a fixed window.
    pub fine_angle_deg: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            quarter_turns: 0,
            fine_angle_deg: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }
}

impl ViewState {
    pub fn total_angle_deg(&self) -> f64 {
        self.quarter_turns as f64 * 90.0 + self.fine_angle_deg
    }

    /// Window content aspect (w/h) given the image's natural aspect. Only
    /// quarter turns change it; the free angle rotates inside the window.
    pub fn effective_aspect(&self, natural_aspect: f64) -> f64 {
        if self.quarter_turns % 2 == 1 { 1.0 / natural_aspect } else { natural_aspect }
    }

    pub fn rotate_quarter(&mut self, clockwise: bool) {
        self.quarter_turns =
            if clockwise { (self.quarter_turns + 1) % 4 } else { (self.quarter_turns + 3) % 4 };
    }

    pub fn set_fine_angle(&mut self, deg: f64) {
        let mut d = deg % 360.0;
        if d > 180.0 {
            d -= 360.0;
        } else if d <= -180.0 {
            d += 360.0;
        }
        self.fine_angle_deg = d;
    }

    pub fn toggle_flip_h(&mut self) {
        self.flip_h = !self.flip_h;
    }

    pub fn toggle_flip_v(&mut self) {
        self.flip_v = !self.flip_v;
    }

    /// Multiplicative zoom: equal steps feel equal at every zoom level.
    pub fn zoom_by(&mut self, factor: f64) {
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    pub fn reset_transform(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turns_wrap_both_ways() {
        let mut v = ViewState::default();
        v.rotate_quarter(false);
        assert_eq!(v.quarter_turns, 3);
        v.rotate_quarter(true);
        v.rotate_quarter(true);
        assert_eq!(v.quarter_turns, 1);
    }

    #[test]
    fn only_odd_quarter_turns_swap_aspect() {
        let mut v = ViewState::default();
        assert_eq!(v.effective_aspect(2.0), 2.0);
        v.rotate_quarter(true);
        assert_eq!(v.effective_aspect(2.0), 0.5);
        v.rotate_quarter(true);
        assert_eq!(v.effective_aspect(2.0), 2.0);
        v.set_fine_angle(33.0);
        assert_eq!(v.effective_aspect(2.0), 2.0);
    }

    #[test]
    fn fine_angle_normalises_into_half_open_range() {
        let mut v = ViewState::default();
        v.set_fine_angle(190.0);
        assert_eq!(v.fine_angle_deg, -170.0);
        v.set_fine_angle(-180.0);
        assert_eq!(v.fine_angle_deg, 180.0);
        v.set_fine_angle(725.0);
        assert_eq!(v.fine_angle_deg, 5.0);
    }

    #[test]
    fn zoom_is_clamped_and_multiplicative() {
        let mut v = ViewState::default();
        for _ in 0..100 {
            v.zoom_by(1.25);
        }
        assert_eq!(v.zoom, MAX_ZOOM);
        for _ in 0..200 {
            v.zoom_by(0.8);
        }
        assert_eq!(v.zoom, MIN_ZOOM);
    }

    #[test]
    fn total_angle_combines_quarter_and_fine() {
        let mut v = ViewState::default();
        v.rotate_quarter(true);
        v.set_fine_angle(-10.0);
        assert_eq!(v.total_angle_deg(), 80.0);
    }
}
