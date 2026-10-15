//! `DisplayProvider` backed by Tauri monitors (physical pixels).

use pinit_core::geometry::{Point, Rect};
use pinit_core::ports::{Display, DisplayProvider};
use tauri::{AppHandle, Monitor};

pub struct TauriDisplays(pub AppHandle);

/// Monitors have no stable id in Tauri; derive one from name + position.
/// Masked to 52 bits: ids travel through JSON to the web UI, and JavaScript
/// numbers are exact only up to 2^53 (a full 64-bit id arrived corrupted, so
/// the "preferred display" setting never matched any monitor).
pub fn display_id(name: Option<&str>, x: i32, y: i32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    name.unwrap_or_default().bytes().for_each(&mut feed);
    x.to_le_bytes().into_iter().chain(y.to_le_bytes()).for_each(&mut feed);
    h & ((1u64 << 52) - 1)
}

fn monitor_id(m: &Monitor) -> u64 {
    display_id(m.name().map(String::as_str), m.position().x, m.position().y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_survive_a_round_trip_through_a_javascript_number() {
        for (name, x, y) in [("\\\\.\\DISPLAY1", 3840, 483), ("\\\\.\\DISPLAY2", 0, 0), ("", -1920, 0)] {
            let id = display_id(Some(name), x, y);
            assert_eq!(id as f64 as u64, id, "{name} lost precision as an f64");
            assert!(id < (1u64 << 53));
        }
    }

    #[test]
    fn different_monitors_get_different_ids() {
        assert_ne!(display_id(Some("A"), 0, 0), display_id(Some("A"), 1920, 0));
        assert_ne!(display_id(Some("A"), 0, 0), display_id(Some("B"), 0, 0));
    }
}

impl DisplayProvider for TauriDisplays {
    fn displays(&self) -> Vec<Display> {
        let primary = self.0.primary_monitor().ok().flatten().map(|m| monitor_id(&m));
        self.0
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(|m| {
                let id = monitor_id(m);
                let (p, s) = (m.position(), m.size());
                let wa = m.work_area();
                Display {
                    id,
                    bounds: Rect::new(p.x, p.y, s.width as i32, s.height as i32),
                    work_area: Rect::new(
                        wa.position.x,
                        wa.position.y,
                        wa.size.width as i32,
                        wa.size.height as i32,
                    ),
                    scale: m.scale_factor(),
                    is_primary: Some(id) == primary,
                }
            })
            .collect()
    }

    fn cursor(&self) -> Option<Point> {
        self.0.cursor_position().ok().map(|p| Point { x: p.x as i32, y: p.y as i32 })
    }
}
