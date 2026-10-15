//! IPC commands exposed to the web UI.

use pinit_core::image_ref::ImageRef;
use pinit_core::ports::Display;
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::adapters::store::Settings;
use crate::adapters::tauri_host::{id_from_label, ViewJson, ViewerInit};
use crate::app::{open_in_background, AppState, Msg};
use crate::cli::is_image_path;

type Res<T> = Result<T, String>;

// ------------------------------------------------------------------ viewer

#[tauri::command]
pub fn viewer_init(window: WebviewWindow, state: State<AppState>) -> Res<ViewerInit> {
    state
        .registry
        .lock()
        .unwrap()
        .get(window.label())
        .cloned()
        .ok_or_else(|| "unknown viewer window".to_string())
}

#[tauri::command]
pub fn viewer_set_view(window: WebviewWindow, view: ViewJson, state: State<AppState>) -> Res<()> {
    let id = id_from_label(window.label()).ok_or("not a viewer window")?;
    state.service.lock().unwrap().set_view(id, view.into());
    state.send(Msg::Dirty);
    Ok(())
}

#[tauri::command]
pub fn viewer_close(window: WebviewWindow) {
    let _ = window.close();
}

#[tauri::command]
pub fn viewer_open_source(window: WebviewWindow, app: AppHandle, state: State<AppState>) -> Res<()> {
    let init = state.registry.lock().unwrap().get(window.label()).cloned().ok_or("unknown window")?;
    match init.source_url {
        Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
            app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
        }
        _ => app.opener().reveal_item_in_dir(init.path).map_err(|e| e.to_string()),
    }
}

#[tauri::command]
pub async fn viewer_save_as(window: WebviewWindow, app: AppHandle, state: State<'_, AppState>) -> Res<bool> {
    let init = state.registry.lock().unwrap().get(window.label()).cloned().ok_or("unknown window")?;
    let name = init.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let target = app
        .dialog()
        .file()
        .set_file_name(name)
        .add_filter("Image", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
        .blocking_save_file();
    match target.and_then(|p| p.into_path().ok()) {
        Some(path) => std::fs::copy(&init.path, path).map(|_| true).map_err(|e| e.to_string()),
        None => Ok(false),
    }
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(settings: Settings, state: State<AppState>) -> Res<Settings> {
    state.settings_file.save(&settings).map_err(|e| e.to_string())?;
    state.service.lock().unwrap().set_config(settings.layout_config());
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

#[derive(Serialize)]
pub struct DisplayInfo {
    id: u64,
    width: i32,
    height: i32,
    scale: f64,
    primary: bool,
}

#[tauri::command]
pub fn list_displays(app: AppHandle) -> Vec<DisplayInfo> {
    use pinit_core::ports::DisplayProvider;
    crate::adapters::displays::TauriDisplays(app)
        .displays()
        .into_iter()
        .map(|d: Display| DisplayInfo {
            id: d.id,
            width: d.bounds.w,
            height: d.bounds.h,
            scale: d.scale,
            primary: d.is_primary,
        })
        .collect()
}

// ----------------------------------------------------------------- actions

#[tauri::command]
pub fn open_count(state: State<AppState>) -> usize {
    state.service.lock().unwrap().open_count()
}

#[tauri::command]
pub fn relayout(state: State<AppState>) {
    state.service.lock().unwrap().relayout_all();
    state.send(Msg::Dirty);
}

#[tauri::command]
pub fn close_all(app: AppHandle) {
    for (label, w) in app.webview_windows() {
        if id_from_label(&label).is_some() {
            let _ = w.close();
        }
    }
}

#[tauri::command]
pub async fn pick_and_open(app: AppHandle) -> Res<usize> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Image", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
        .blocking_pick_files()
        .unwrap_or_default();
    let images: Vec<ImageRef> = picked
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .filter(|p| is_image_path(p))
        .map(ImageRef::File)
        .collect();
    let n = images.len();
    open_in_background(&app, images);
    Ok(n)
}

#[tauri::command]
pub fn open_url_text(text: String, app: AppHandle) -> Res<()> {
    let t = text.trim().to_string();
    if !(t.starts_with("http://") || t.starts_with("https://")) {
        return Err("http(s) 이미지 주소를 입력하세요".into());
    }
    open_in_background(&app, vec![ImageRef::Url { url: t, referer: None }]);
    Ok(())
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    crate::app::set_quitting(&app);
    app.exit(0);
}
