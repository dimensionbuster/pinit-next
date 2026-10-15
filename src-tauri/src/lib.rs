pub mod adapters;
pub mod app;
pub mod b64;
pub mod cli;
pub mod commands;
pub mod extension_bundle;
pub mod native_host;
pub mod paths;

use std::sync::atomic::Ordering;

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

use pinit_core::geometry::Rect;

use crate::adapters::tauri_host::id_from_label;
use crate::app::{open_in_background, restore_saved, set_quitting, AppState, Msg};
use crate::cli::parse_open_args;

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "PinIt 열기").build(app)?;
    let relayout = MenuItemBuilder::with_id("relayout", "다시 정렬").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "종료").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &relayout, &sep, &quit]).build()?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("PinIt")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            // Menu callbacks run on the UI thread: never take the service lock here.
            "relayout" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    state.service.lock().unwrap().relayout_all();
                    state.send(Msg::Dirty);
                });
            }
            "quit" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    set_quitting(&app);
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    let label = window.label();
    if label == "main" {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
        }
        return;
    }
    let Some(id) = id_from_label(label) else { return };
    let state = window.app_handle().state::<AppState>();
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if let (Ok(p), Ok(s)) = (window.outer_position(), window.outer_size()) {
                state.send(Msg::Bounds(id, Rect::new(p.x, p.y, s.width as i32, s.height as i32)));
            }
        }
        WindowEvent::Destroyed => {
            // On app exit keep the layout; only a user close forgets the window.
            if !state.quitting.load(Ordering::SeqCst) {
                state.send(Msg::Closed(id));
            }
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // One shared renderer for all same-origin image windows (see docs/SPIKES.md:
    // 20 windows 1.3 GB -> 0.4 GB). Must be set before any webview exists.
    // SAFETY: called once at startup before other threads are spawned.
    let paths = paths::Paths::detect();
    unsafe {
        if let Some(profile) = &paths.webview {
            // Portable mode: keep the WebView2 profile next to the exe, not in AppData.
            let _ = std::fs::create_dir_all(profile);
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", profile);
        }
        let extra = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", format!("{extra} --process-per-site"));
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let images = parse_open_args(argv.get(1..).unwrap_or_default());
            if images.is_empty() {
                show_main(app);
            } else {
                open_in_background(app, images);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = AppState::build(&handle)?;
            app.manage(state);
            build_tray(&handle)?;

            // Keep the browser bridge registered (idempotent; follows a moved/updated exe).
            // PINIT_NO_REGISTER=1 lets test runs avoid hijacking a real installation's registration.
            if let (Ok(exe), None) = (std::env::current_exe(), std::env::var_os("PINIT_NO_REGISTER")) {
                let dir = paths::Paths::detect().data;
                std::thread::spawn(move || {
                    if let Err(e) = native_host::register(&exe, &dir.join("native-host")) {
                        eprintln!("native host registration failed: {e}");
                    }
                });
            }

            let args: Vec<String> = std::env::args().skip(1).collect();
            let images = parse_open_args(&args);
            restore_saved(&handle);
            if images.is_empty() {
                show_main(&handle);
            } else {
                open_in_background(&handle, images);
            }
            Ok(())
        })
        .on_window_event(on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::viewer_init,
            commands::viewer_set_view,
            commands::viewer_fit,
            commands::viewer_close,
            commands::viewer_open_source,
            commands::viewer_save_as,
            commands::get_settings,
            commands::set_settings,
            commands::list_displays,
            commands::open_count,
            commands::relayout,
            commands::close_all,
            commands::pick_and_open,
            commands::open_paths,
            commands::open_url_text,
            commands::reveal_extension,
            commands::quit,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                // Last window closed: stay alive in the tray.
                api.prevent_exit();
            } else {
                set_quitting(app);
            }
        }
    });
}
