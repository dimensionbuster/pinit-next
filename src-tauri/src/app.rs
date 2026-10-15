//! Composition root: the only place that names concrete adapters.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pinit_core::geometry::Rect;
use pinit_core::image_ref::ImageRef;
use pinit_core::ports::{ImageResolver, LayoutStore, WindowId};
use pinit_core::resolver::ChainResolver;
use pinit_core::rng::SplitMix64;
use pinit_core::service::{OpenOutcome, OpenService, Restore};
use tauri::{AppHandle, Emitter, Manager};

use crate::adapters::displays::TauriDisplays;
use crate::adapters::resolvers::{BytesResolver, FileResolver, UrlResolver};
use crate::adapters::store::{JsonLayoutStore, Settings, SettingsFile};
use crate::adapters::tauri_host::{Registry, TauriHost};

/// Events from window callbacks. Handled on a worker thread so that window
/// creation (which holds the service lock and waits on the UI thread) can
/// never deadlock against the UI thread's own event callbacks.
pub enum Msg {
    Bounds(WindowId, Rect),
    Closed(WindowId),
    Dirty,
}

pub struct AppState {
    pub service: Arc<Mutex<OpenService>>,
    pub resolver: Arc<ChainResolver>,
    pub settings: Mutex<Settings>,
    pub settings_file: SettingsFile,
    pub layout: Mutex<JsonLayoutStore>,
    pub registry: Registry,
    pub tx: Mutex<Sender<Msg>>,
    pub quitting: AtomicBool,
}

fn make_resolver(cache: &std::path::Path) -> ChainResolver {
    ChainResolver::new(vec![
        Box::new(FileResolver),
        Box::new(BytesResolver { cache_dir: cache.to_path_buf() }),
        Box::new(UrlResolver { cache_dir: cache.to_path_buf() }),
    ])
}

impl AppState {
    pub fn build(app: &AppHandle) -> tauri::Result<Self> {
        let data = app.path().app_data_dir()?;
        let cache = app.path().app_cache_dir()?.join("images");
        let settings_file = SettingsFile(data.join("settings.json"));
        let settings = settings_file.load();
        let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
        let service = OpenService::new(
            Box::new(make_resolver(&cache)),
            Box::new(TauriHost::new(app.clone(), registry.clone(), settings.show_in_taskbar)),
            Box::new(TauriDisplays(app.clone())),
            Box::new(SplitMix64::new(seed())),
            settings.layout_config(),
        );
        let (tx, rx) = mpsc::channel();
        let state = Self {
            service: Arc::new(Mutex::new(service)),
            resolver: Arc::new(make_resolver(&cache)),
            settings: Mutex::new(settings),
            settings_file,
            layout: Mutex::new(JsonLayoutStore { path: data.join("layout.json") }),
            registry,
            tx: Mutex::new(tx),
            quitting: AtomicBool::new(false),
        };
        spawn_pump(app.clone(), rx);
        Ok(state)
    }

    pub fn send(&self, m: Msg) {
        let _ = self.tx.lock().unwrap().send(m);
    }

    /// Write the current windows to disk.
    pub fn persist(&self) {
        let snap = self.service.lock().unwrap().snapshot();
        self.layout.lock().unwrap().save(&snap);
    }

    /// Resolve (possibly slow, lock-free) then open. Never call on the UI thread.
    pub fn open_one(&self, image: ImageRef, restore: Option<Restore>) -> Result<(), String> {
        if self.service.lock().unwrap().find_open(&image).is_some() {
            return Ok(());
        }
        let resolved = self.resolver.resolve(&image).map_err(|e| e.to_string())?;
        let outcome = self
            .service
            .lock()
            .unwrap()
            .open_resolved(resolved, restore)
            .map_err(|e| e.to_string())?;
        if matches!(outcome, OpenOutcome::Opened(_)) {
            self.send(Msg::Dirty);
        }
        Ok(())
    }
}

fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}

fn spawn_pump(app: AppHandle, rx: mpsc::Receiver<Msg>) {
    std::thread::spawn(move || {
        let mut dirty = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(msg) => {
                    let state = app.state::<AppState>();
                    let mut svc = state.service.lock().unwrap();
                    match msg {
                        Msg::Bounds(id, r) => svc.on_bounds_changed(id, r),
                        Msg::Closed(id) => {
                            svc.on_closed(id);
                            state.registry.lock().unwrap().remove(&crate::adapters::tauri_host::label_for(id));
                        }
                        Msg::Dirty => {}
                    }
                    dirty = true;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if dirty {
                        app.state::<AppState>().persist();
                        dirty = false;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

/// Open images from any input source without blocking the caller.
pub fn open_in_background(app: &AppHandle, images: Vec<ImageRef>) {
    if images.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        for image in images {
            if let Err(e) = state.open_one(image, None) {
                let _ = app.emit("toast", format!("열 수 없음: {e}"));
            }
        }
    });
}

/// Re-open windows saved by the previous session.
pub fn restore_saved(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let saved = state.layout.lock().unwrap().load();
        for w in saved {
            let restore = Restore { bounds: w.bounds, view: w.view };
            if let Err(e) = state.open_one(w.image, Some(restore)) {
                eprintln!("restore failed: {e}");
            }
        }
    });
}

pub fn set_quitting(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.quitting.store(true, Ordering::SeqCst);
    state.persist();
}
