//! IPC commands exposed to the web UI.
//!
//! DEADLOCK RULE: every command that touches `AppState` locks must be `async`.
//! Sync commands run on the UI thread; `OpenService::open*` holds the service
//! lock while creating a window, which needs the UI thread. A sync command
//! waiting for that lock would block the UI thread forever.

use pinit_core::image_ref::ImageRef;
use pinit_core::ports::Display;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::adapters::store::Settings;
use crate::adapters::tauri_host::{id_from_label, ViewJson, ViewerInit};
use crate::adapters::store::{decode_layout, encode_layout};
use crate::app::{open_in_background, open_saved_in_background, AppState, Msg};
use crate::cli::is_image_path;

type Res<T> = Result<T, String>;

// ------------------------------------------------------------------ viewer

#[tauri::command]
pub async fn viewer_init(window: WebviewWindow, state: State<'_, AppState>) -> Res<ViewerInit> {
    state
        .registry
        .lock()
        .unwrap()
        .get(window.label())
        .cloned()
        .ok_or_else(|| "unknown viewer window".to_string())
}

#[tauri::command]
pub async fn viewer_set_view(window: WebviewWindow, view: ViewJson, state: State<'_, AppState>) -> Res<()> {
    let id = id_from_label(window.label()).ok_or("not a viewer window")?;
    state.service.lock().unwrap().set_view(id, view.into());
    state.send(Msg::Dirty);
    Ok(())
}

#[tauri::command]
pub async fn viewer_fit(window: WebviewWindow, state: State<'_, AppState>) -> Res<()> {
    let id = id_from_label(window.label()).ok_or("not a viewer window")?;
    state.service.lock().unwrap().fit_to_aspect(id);
    Ok(())
}

#[tauri::command]
pub fn viewer_close(window: WebviewWindow) {
    let _ = window.close();
}

#[tauri::command]
pub async fn viewer_open_source(window: WebviewWindow, app: AppHandle, state: State<'_, AppState>) -> Res<()> {
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
pub async fn get_settings(state: State<'_, AppState>) -> Res<Settings> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub async fn set_settings(settings: Settings, app: AppHandle, state: State<'_, AppState>) -> Res<Settings> {
    let mut settings = settings.sanitized();
    // owned by the app: a stale UI copy must not reset it
    settings.bridge_connected = state.settings.lock().unwrap().bridge_connected;
    state.settings_file.save(&settings).map_err(|e| e.to_string())?;
    state.service.lock().unwrap().set_config(settings.layout_config());
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings); // open viewers re-apply the backdrop live
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
pub async fn list_displays(app: AppHandle) -> Vec<DisplayInfo> {
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
pub async fn open_count(state: State<'_, AppState>) -> Res<usize> {
    Ok(state.service.lock().unwrap().open_count())
}

#[tauri::command]
pub async fn relayout(state: State<'_, AppState>) -> Res<()> {
    let keep = state.settings.lock().unwrap().relayout_keep_sizes;
    state.service.lock().unwrap().relayout_all(keep);
    state.send(Msg::Dirty);
    Ok(())
}

#[tauri::command]
pub async fn close_all(app: AppHandle) {
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
pub async fn open_paths(paths: Vec<String>, app: AppHandle) -> usize {
    let images: Vec<ImageRef> = paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .filter(|p| is_image_path(p) && p.is_file())
        .map(ImageRef::File)
        .collect();
    let n = images.len();
    open_in_background(&app, images);
    n
}

#[tauri::command]
pub async fn open_url_text(text: String, app: AppHandle) -> Res<()> {
    let t = text.trim().to_string();
    if !(t.starts_with("http://") || t.starts_with("https://")) {
        return Err("http(s) 이미지 주소를 입력하세요".into());
    }
    open_in_background(&app, vec![ImageRef::Url { url: t, referer: None }]);
    Ok(())
}

#[tauri::command]
pub async fn quit(app: AppHandle) {
    crate::app::set_quitting(&app);
    app.exit(0);
}

/// Shows the bundled browser extension folder so the user can "Load unpacked".
#[tauri::command]
pub async fn reveal_extension(app: AppHandle) -> Res<()> {
    let dev_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("extension-dev");
    let manifest = if cfg!(debug_assertions) && dev_dir.join("manifest.json").is_file() {
        dev_dir.join("manifest.json") // generated by `npm run dev:app`
    } else {
        let dest = crate::paths::Paths::detect().data.join("extension");
        crate::extension_bundle::extract(&dest).map_err(|e| e.to_string())?
    };
    app.opener().reveal_item_in_dir(manifest).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ update

/// Asks GitHub for the latest published release. Errors (offline, rate limit) are for the caller to ignore.
#[tauri::command]
pub async fn check_update() -> Res<crate::update::UpdateInfo> {
    tauri::async_runtime::spawn_blocking(crate::update::check_latest).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_release_page(app: AppHandle, url: String) -> Res<()> {
    if !crate::update::is_release_url(&url) {
        return Err("not a PinIt release url".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// ------------------------------------------------------------ .pinit layouts

/// Saves the currently open windows (positions, sizes, views) to a `.pinit` file.
/// Returns the saved path, or None if the user cancelled.
#[tauri::command]
pub async fn export_layout(app: AppHandle, state: State<'_, AppState>) -> Res<Option<String>> {
    let snapshot = state.service.lock().unwrap().snapshot();
    if snapshot.is_empty() {
        return Err("열려 있는 이미지가 없습니다".into());
    }
    let name = format!("pinit-layout-{}.pinit", crate::date::today_iso());
    let target = app
        .dialog()
        .file()
        .set_file_name(name)
        .add_filter("PinIt 레이아웃", &["pinit"])
        .add_filter("JSON", &["json"])
        .blocking_save_file();
    let Some(path) = target.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    std::fs::write(&path, encode_layout(&snapshot)).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Opens every window from a `.pinit` file (added to the ones already open).
/// Returns how many windows the file contained, or None if the user cancelled.
#[tauri::command]
pub async fn import_layout(app: AppHandle) -> Res<Option<usize>> {
    let picked = app
        .dialog()
        .file()
        .add_filter("PinIt 레이아웃", &["pinit", "json"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let saved = decode_layout(&bytes).ok_or("올바른 PinIt 레이아웃 파일이 아닙니다")?;
    let n = saved.len();
    open_saved_in_background(&app, saved);
    Ok(Some(n))
}
