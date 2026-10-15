pub mod adapters;
pub mod app;
pub mod b64;
pub mod cli;
pub mod commands;
pub mod date;
pub mod extension_bundle;
pub mod native_host;
pub mod paths;
pub mod shell_menu;
pub mod update;

use std::sync::atomic::Ordering;

use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

use pinit_core::geometry::Rect;

use crate::adapters::tauri_host::id_from_label;
use crate::app::{mark_bridge_connected, open_in_background, restore_saved, set_quitting, AppState, Msg};
use crate::cli::{from_browser, has_flag, parse_open_args};

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Handles argv from this launch or from a second instance (browser bridge, file association).
fn handle_launch_args(app: &AppHandle, args: &[String]) {
    if from_browser(args) {
        mark_bridge_connected(app, has_flag(args, "--bridge-hello"));
    }
    let images = parse_open_args(args);
    if images.is_empty() {
        // a bare launch or an extension hello: show the window (hello makes the "connected" status visible)
        show_main(app);
    } else {
        open_in_background(app, images);
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "PinIt 열기").build(app)?;
    let relayout = MenuItemBuilder::with_id("relayout", "다시 정렬").build(app)?;
    let save_all = MenuItemBuilder::with_id("save_all", "열린 창 모두 다운로드").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "종료").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &relayout, &save_all, &sep, &quit]).build()?;
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
                    let keep = state.settings.lock().unwrap().relayout_keep_sizes;
                    state.service.lock().unwrap().relayout_all(keep);
                    state.send(Msg::Dirty);
                });
            }
            "save_all" => {
                // The folder dialog blocks: keep it off the UI thread.
                let app = app.clone();
                std::thread::spawn(move || {
                    if let Err(e) = commands::save_all_open(&app) {
                        eprintln!("save all failed: {e}");
                    }
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
            handle_launch_args(app, argv.get(1..).unwrap_or_default());
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = AppState::build(&handle)?;
            app.manage(state);
            build_tray(&handle)?;
            if cfg!(debug_assertions) {
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.set_title("PinIt (DEV)");
                }
            }

            // Keep the browser bridge registered (idempotent; follows a moved/updated exe).
            // Debug builds register under the dev host name (see native_host.rs), so they never
            // touch a real install's registration. PINIT_NO_REGISTER=1 skips registration entirely.
            let register = std::env::var_os("PINIT_NO_REGISTER").is_none();
            if let (Ok(exe), true) = (std::env::current_exe(), register) {
                let dir = paths::Paths::detect().data;
                std::thread::spawn(move || {
                    shell_menu::register(&exe);
                    if let Err(e) = native_host::register(&exe, &dir.join("native-host")) {
                        eprintln!("native host registration failed: {e}");
                    }
                });
            }

            let args: Vec<String> = std::env::args().skip(1).collect();
            restore_saved(&handle);
            handle_launch_args(&handle, &args);
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
            commands::save_all,
            commands::export_layout,
            commands::import_layout,
            commands::reveal_extension,
            commands::check_update,
            commands::open_release_page,
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
