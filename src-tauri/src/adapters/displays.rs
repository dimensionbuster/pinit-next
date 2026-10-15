//! `DisplayProvider` backed by Tauri monitors (physical pixels).

use pinit_core::geometry::{Point, Rect};
use pinit_core::ports::{Display, DisplayProvider};
use tauri::{AppHandle, Monitor};

pub struct TauriDisplays(pub AppHandle);

/// Monitors have no stable id in Tauri; derive one from name + position.
fn monitor_id(m: &Monitor) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    m.name().map(|n| n.as_bytes()).unwrap_or_default().iter().for_each(|&b| feed(b));
    m.position().x.to_le_bytes().into_iter().chain(m.position().y.to_le_bytes()).for_each(&mut feed);
    h
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
