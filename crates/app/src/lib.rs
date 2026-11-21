mod scheduler;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mothball_core::{Engine, ScanOptions, Settings, SweepQuery};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    image::Image, AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;

pub struct AppState {
    engine: Mutex<Engine>,
    sweep_cancel: Arc<AtomicBool>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ToastMessage {
    kind: String,
    title: String,
    body: String,
}

struct ToastSlot(Mutex<ToastMessage>);

pub fn run() {
    let engine = Engine::open_default().unwrap_or_else(|err| {
        eprintln!("Mothball could not open its data folder: {err}");
        std::process::exit(1);
    });
    let start_at_login = engine.settings().start_at_login;
    scheduler::arm_catch_up(&engine);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app, "projects")))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(AppState {
            engine: Mutex::new(engine),
            sweep_cancel: Arc::new(AtomicBool::new(false)),
        })
        .manage(ToastSlot(Mutex::new(ToastMessage {
            kind: "heads-up".into(),
            title: "Mothball".into(),
            body: String::new(),
        })))
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            get_tray,
            list_projects,
            scan_roots,
            preview_sweep,
            run_sweep,
            cancel_sweep,
            rehydrate,
            set_protected,
            set_allow_unsafe,
            get_history,
            export_history,
            get_settings,
            save_settings,
            add_root,
            remove_root,
            pick_folder,
            suggest_roots,
            get_caches,
            prune_cache,
            set_pause,
            open_main,
            get_toast
        ])
        .setup(move |app| {
            let icon = load_icon();
            build_windows(app.handle())?;
            build_tray(app.handle(), icon)?;
            if !cfg!(debug_assertions) {
                apply_autostart(app.handle(), start_at_login);
            }
            if std::env::args().any(|arg| arg == "--minimized") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            scheduler::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == "tray" => {
                let _ = window.hide();
            }
            WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) if window.label() == "main" => {
                let app = window.app_handle().clone();
                let paths = paths.to_vec();
                std::thread::spawn(move || add_dropped_roots(&app, paths));
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("Mothball failed to start: {err}");
            std::process::exit(1);
        });
}

fn build_windows(app: &AppHandle) -> tauri::Result<()> {
    let icon = load_icon();
    let minimized = std::env::args().any(|arg| arg == "--minimized");
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Mothball")
        .inner_size(1180.0, 780.0)
        .min_inner_size(960.0, 640.0)
        .center()
        .visible(!minimized)
        .background_color(tauri::window::Color(16, 20, 27, 255))
        .initialization_script("window.__MOTHBALL_WINDOW__ = 'main';")
        .icon(icon.clone())?
        .build()?;

    WebviewWindowBuilder::new(app, "tray", WebviewUrl::App("index.html".into()))
        .title("Mothball")
        .inner_size(392.0, 640.0)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .shadow(true)
        .background_color(tauri::window::Color(23, 28, 37, 255))
        .initialization_script("window.__MOTHBALL_WINDOW__ = 'tray';")
        .build()?;

    WebviewWindowBuilder::new(app, "toast", WebviewUrl::App("index.html".into()))
        .title("Mothball")
        .inner_size(460.0, 210.0)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .shadow(true)
        .background_color(tauri::window::Color(26, 32, 48, 255))
        .initialization_script("window.__MOTHBALL_WINDOW__ = 'toast';")
        .build()?;
    Ok(())
}

fn build_tray(app: &AppHandle, icon: Image<'static>) -> tauri::Result<()> {
    let sweep = MenuItem::with_id(app, "sweep", "Sweep now", true, None::<&str>)?;
    let preview = MenuItem::with_id(app, "preview", "Preview", true, None::<&str>)?;
    let rescan = MenuItem::with_id(app, "rescan", "Rescan", true, None::<&str>)?;
    let pause_1h = MenuItem::with_id(app, "pause-1h", "For 1 hour", true, None::<&str>)?;
    let pause_tomorrow = MenuItem::with_id(app, "pause-tomorrow", "Until tomorrow", true, None::<&str>)?;
    let pause_forever = MenuItem::with_id(app, "pause-forever", "Until I resume", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume", true, None::<&str>)?;
    let pause = Submenu::with_id_and_items(app, "pause", "Pause", true, &[&pause_1h, &pause_tomorrow, &pause_forever, &resume])?;
    let open = MenuItem::with_id(app, "open", "Open Mothball", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let sep_a = PredefinedMenuItem::separator(app)?;
    let sep_b = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&sweep, &preview, &rescan, &sep_a, &pause, &sep_b, &open, &settings, &quit])?;

    let builder = TrayIconBuilder::with_id("mothball")
        .tooltip("Mothball")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "sweep" => scheduler::lock_and_sweep(app, "tray"),
            "preview" => show_main(app, "preview"),
            "rescan" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let mut engine = engine_mut(&state);
                    let notify = app.clone();
                    let _ = engine.scan(ScanOptions::default(), &AtomicBool::new(false), move |progress| {
                        let _ = notify.emit("scan-progress", &progress);
                    });
                    let _ = app.emit("data-changed", ());
                });
            }
            "pause-1h" => pause_from_tray(app, "1h"),
            "pause-tomorrow" => pause_from_tray(app, "tomorrow"),
            "pause-forever" => pause_from_tray(app, "forever"),
            "resume" => pause_from_tray(app, "resume"),
            "open" => show_main(app, "projects"),
            "settings" => show_main(app, "settings"),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                toggle_tray(tray.app_handle(), position);
            }
        });
    let _ = builder.build(app)?;
    Ok(())
}

fn pause_from_tray(app: &AppHandle, until: &str) {
    let state = app.state::<AppState>();
    let engine = engine_mut(&state);
    let _ = scheduler::set_pause(&engine, until);
    let _ = app.emit("data-changed", ());
}

fn toggle_tray(app: &AppHandle, position: PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window("tray") else { return };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let width = 392.0;
    let height = 640.0;
    let scale = window.scale_factor().unwrap_or(1.0);
    let width_px = width * scale;
    let height_px = height * scale;
    let x = (position.x - width_px / 2.0).max(8.0);
    let y = if position.y > height_px + 20.0 {
        position.y - height_px - 8.0
    } else {
        position.y + 28.0
    };
    let _ = window.set_size(PhysicalSize::new(width_px, height_px));
    let _ = window.set_position(PhysicalPosition::new(x as i32, y as i32));
    let _ = window.show();
    let _ = window.set_focus();
}

fn show_main(app: &AppHandle, route: &str) {
    let route = match route {
        "projects" | "preview" | "history" | "caches" | "settings" | "onboarding" => route,
        _ => "projects",
    };
    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    let _ = window.eval(&format!("location.hash = '#/{route}'"));
    if let Some(tray) = app.get_webview_window("tray") {
        let _ = tray.hide();
    }
}

pub(crate) fn show_toast(app: &AppHandle, kind: &str, title: &str, body: &str) {
    if let Some(slot) = app.try_state::<ToastSlot>() {
        if let Ok(mut toast) = slot.0.lock() {
            toast.kind = kind.to_string();
            toast.title = title.to_string();
            toast.body = body.to_string();
        }
    }
    let Some(window) = app.get_webview_window("toast") else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    let width = 460.0 * scale;
    let height = 210.0 * scale;
    if let Ok(Some(monitor)) = window.current_monitor() {
        let size = monitor.size();
        let x = size.width as f64 - width - 24.0 * scale;
        let y = size.height as f64 - height - 64.0 * scale;
        let _ = window.set_size(PhysicalSize::new(width, height));
        let _ = window.set_position(PhysicalPosition::new(x as i32, y as i32));
    } else {
        let _ = window.set_size(LogicalSize::new(460.0, 210.0));
    }
    let _ = window.eval("location.reload()");
    let _ = window.show();
}

fn add_dropped_roots(app: &AppHandle, paths: Vec<PathBuf>) {
    let notify = app.clone();
    let state = app.state::<AppState>();
    let mut engine = engine_mut(&state);
    let mut added = false;
    for path in paths {
        if path.is_dir() {
            added |= engine.add_root(path).is_ok();
        }
    }
    if added {
        let _ = engine.scan(ScanOptions::default(), &AtomicBool::new(false), move |progress| {
            let _ = notify.emit("scan-progress", &progress);
        });
        let _ = app.emit("data-changed", ());
    }
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    let auto = app.autolaunch();
    let current = auto.is_enabled().unwrap_or(false);
    if enabled && !current {
        let _ = auto.enable();
    } else if !enabled && current {
        let _ = auto.disable();
    }
}

fn load_icon() -> Image<'static> {
    match Image::from_bytes(include_bytes!("../icons/32x32.png")) {
        Ok(icon) => icon,
        Err(_) => Image::new_owned(vec![240, 180, 41, 255].repeat(32 * 32), 32, 32),
    }
}

pub(crate) fn engine_mut(state: &AppState) -> std::sync::MutexGuard<'_, Engine> {
    state.engine.lock().unwrap_or_else(|err| err.into_inner())
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[tauri::command]
fn get_dashboard(state: State<AppState>) -> Result<mothball_core::Dashboard, String> {
    engine_mut(&state).dashboard().map_err(text)
}

#[tauri::command]
fn get_tray(state: State<AppState>) -> Result<mothball_core::TrayState, String> {
    engine_mut(&state).tray().map_err(text)
}

#[tauri::command]
fn list_projects(state: State<AppState>) -> Result<Vec<mothball_core::ProjectView>, String> {
    engine_mut(&state).projects().map_err(text)
}

#[tauri::command]
async fn scan_roots(app: AppHandle) -> Result<mothball_core::ScanSummary, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut engine = engine_mut(&state);
        let notify = app.clone();
        let summary = engine
            .scan(ScanOptions::default(), &AtomicBool::new(false), move |progress| {
                let _ = notify.emit("scan-progress", &progress);
            })
            .map_err(text)?;
        let _ = app.emit("data-changed", ());
        Ok(summary)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
fn preview_sweep(state: State<AppState>, query: SweepQuery) -> Result<mothball_core::SweepPreview, String> {
    engine_mut(&state).preview(&query).map_err(text)
}

#[tauri::command]
async fn run_sweep(app: AppHandle, query: SweepQuery) -> Result<mothball_core::SweepOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.sweep_cancel.store(false, Ordering::Relaxed);
        let cancel = state.sweep_cancel.clone();
        let mut engine = engine_mut(&state);
        let notify = app.clone();
        let outcome = engine
            .sweep(&query, cancel.as_ref(), move |progress| {
                let _ = notify.emit("sweep-progress", &progress);
            })
            .map_err(text)?;
        if !query.dry_run {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            if outcome.errors == 0 || outcome.bytes_freed > 0 {
                let _ = engine.kv_set("last_sweep_day", &today);
                let _ = engine.kv_set("catch_up_at", "");
            }
        }
        let _ = app.emit("data-changed", ());
        Ok(outcome)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
fn cancel_sweep(state: State<AppState>) {
    state.sweep_cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
async fn rehydrate(app: AppHandle, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let engine = engine_mut(&state);
        engine
            .rehydrate(Path::new(&path), |line| {
                let _ = app.emit("rehydrate-line", serde_json::json!({ "line": line }));
            })
            .map_err(text)?;
        let _ = app.emit("data-changed", ());
        Ok(())
    })
    .await
    .map_err(text)?
}

#[tauri::command]
fn set_protected(app: AppHandle, state: State<AppState>, path: String, protected: bool) -> Result<(), String> {
    engine_mut(&state).set_protected(Path::new(&path), protected).map_err(text)?;
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
fn set_allow_unsafe(app: AppHandle, state: State<AppState>, path: String, allow: bool) -> Result<(), String> {
    engine_mut(&state).set_allow_unsafe(Path::new(&path), allow).map_err(text)?;
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
fn get_history(state: State<AppState>, days: u32) -> Result<mothball_core::HistoryView, String> {
    engine_mut(&state).history(days).map_err(text)
}

#[tauri::command]
async fn export_history(app: AppHandle) -> Result<(), String> {
    let csv = {
        let state = app.state::<AppState>();
        let csv = engine_mut(&state).history_csv().map_err(text)?;
        csv
    };
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Export history")
            .set_file_name("mothball-history.csv")
            .add_filter("CSV", &["csv"])
            .blocking_save_file()
    })
    .await
    .map_err(text)?;
    if let Some(path) = picked.and_then(|file| file.into_path().ok()) {
        std::fs::write(path, csv).map_err(text)?;
    }
    Ok(())
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Result<mothball_core::SettingsResponse, String> {
    engine_mut(&state).settings_view().map_err(text)
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    let start = settings.start_at_login;
    engine_mut(&state).save_settings(settings).map_err(text)?;
    apply_autostart(&app, start);
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
fn add_root(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    engine_mut(&state).add_root(PathBuf::from(path)).map_err(text)?;
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
fn remove_root(app: AppHandle, state: State<AppState>, path: String) -> Result<(), String> {
    engine_mut(&state).remove_root(Path::new(&path)).map_err(text)?;
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Choose a folder to watch").blocking_pick_folder()
    })
    .await
    .map_err(text)?;
    Ok(picked.and_then(|file| file.into_path().ok()).map(|path| path.display().to_string()))
}

#[tauri::command]
fn suggest_roots(state: State<AppState>) -> Vec<mothball_core::SuggestedRoot> {
    engine_mut(&state).suggest_roots()
}

#[tauri::command]
fn get_caches(state: State<AppState>) -> Vec<mothball_core::CacheView> {
    engine_mut(&state).caches()
}

#[tauri::command]
fn prune_cache(state: State<AppState>, id: String, confirm: bool) -> Result<String, String> {
    engine_mut(&state).prune_cache(&id, confirm).map_err(text)
}

#[tauri::command]
fn set_pause(app: AppHandle, state: State<AppState>, until: String) -> Result<(), String> {
    scheduler::set_pause(&engine_mut(&state), &until)?;
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
fn open_main(app: AppHandle, route: String) {
    show_main(&app, &route);
}

#[tauri::command]
fn get_toast(slot: State<ToastSlot>) -> ToastMessage {
    slot.0.lock().map(|toast| toast.clone()).unwrap_or_else(|err| err.into_inner().clone())
}
