//! Phase 1 spike (throwaway): N transparent, frameless, always-on-top windows.
//! Prints monitor/DPI info and window geometry, exits after SPIKE_SECS.
//! Run: SPIKE_N=20 cargo run -p pinit-next --bin spike_windows

use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const PAGE: &str = "data:text/html;charset=utf-8,<body style='margin:0;background:transparent'>\
<div style='width:100vw;height:100vh;background:linear-gradient(135deg,%23e66,%2366e);border-radius:12px'></div>";

fn main() {
    let n: u32 = std::env::var("SPIKE_N").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    let url = std::env::var("SPIKE_URL").unwrap_or_else(|_| PAGE.to_string());
    let secs: u64 = std::env::var("SPIKE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(15);

    tauri::Builder::default()
        .setup(move |app| {
            let url = url.clone();
            if let Some(main) = app.get_webview_window("main") {
                main.close()?;
            }
            for m in app.available_monitors()? {
                println!(
                    "monitor {:?} size={}x{} pos=({},{}) scale={}",
                    m.name(),
                    m.size().width,
                    m.size().height,
                    m.position().x,
                    m.position().y,
                    m.scale_factor()
                );
            }
            for i in 0..n {
                let w = WebviewWindowBuilder::new(
                    app,
                    format!("w{i}"),
                    WebviewUrl::External(url.parse().unwrap()),
                )
                .inner_size(300.0, 300.0)
                .position(20.0 + (i % 6) as f64 * 310.0, 20.0 + (i / 6) as f64 * 310.0)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true);
                let w = match std::env::var("SPIKE_ARGS") {
                    Ok(a) => w.additional_browser_args(&a),
                    Err(_) => w,
                }
                .build()?;
                if i < 3 {
                    println!(
                        "w{i} outer_pos={:?} inner_size={:?} scale={:?}",
                        w.outer_position()?,
                        w.inner_size()?,
                        w.scale_factor()?
                    );
                }
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(secs));
                handle.exit(0);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("spike failed");
}
