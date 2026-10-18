//! Bloom's windows that aren't on screen at start, made just after it.
//!
//! Every Bloom window is a web view with a browser process of its own. Made all
//! at once at sign-in, the five that nobody sees yet (the volume and
//! brightness overlay, notifications, the notification center, the search bar
//! and Settings) took the processor and the disk from the two that are on
//! screen, so the notch and the dock came up long after Windows' own taskbar
//! would have. Now only the notch and the dock are made at start
//! (`"create": false` for the rest in tauri.conf.json); the others follow one
//! at a time as soon as the notch is on screen, a second or two later. Asked
//! for before then (Settings opened from the tray menu at once), a window is
//! made right away.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewWindow};

/// In the order they are made: what can be needed soonest first.
const LATER: [&str; 5] = [
    "overlay",
    "notifications",
    "notification-center",
    "launcher",
    "settings",
];

/// One window is made at a time.
static MAKING: Mutex<()> = Mutex::new(());

/// The window, making it first if it isn't there yet. Must not be called on
/// the UI thread (making a web view waits for it); see `with_window`.
pub fn ensure(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    if let Some(window) = app.get_webview_window(label) {
        return Some(window);
    }
    let _making = MAKING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(window) = app.get_webview_window(label) {
        return Some(window);
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == label)?
        .clone();
    let window = tauri::WebviewWindowBuilder::from_config(app, &config)
        .ok()?
        .build()
        .ok()?;
    after_made(app, &window);
    Some(window)
}

/// Runs `f` with the window: at once when it is there, else on a thread of its
/// own once it has been made. For callers on the UI thread (the tray menu).
pub fn with_window(
    app: &AppHandle,
    label: &'static str,
    f: impl FnOnce(WebviewWindow) + Send + 'static,
) {
    if let Some(window) = app.get_webview_window(label) {
        f(window);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(window) = ensure(&app, label) {
            f(window);
        }
    });
}

/// What a window needs once it exists (what start-up used to do for it).
fn after_made(app: &AppHandle, window: &WebviewWindow) {
    match window.label() {
        "overlay" => {
            // On Windows, set_position doesn't take effect on a window that
            // has never been shown: show it once to register it with the
            // compositor, then hide it at once.
            let _ = window.show();
            let _ = window.hide();
            let handle = app.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    crate::commands::restore_taskbar_and_exit(&handle);
                }
            });
            crate::services::sync_overlays(app);
        }
        "settings" => {
            let _ = window_vibrancy::apply_mica(window, None);
            let settings = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = settings.hide();
                }
            });
        }
        _ => {}
    }
    crate::services::watch_webview_process(app, window);
}

/// Makes the windows that aren't on screen at start, once the notch is.
pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("bloom-later-windows".into())
        .spawn(move || {
            // Until the notch is on screen (at most 3 s), and a moment for it
            // and the dock to draw.
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(3) {
                let shown = app
                    .get_webview_window("main")
                    .and_then(|main| main.is_visible().ok())
                    .unwrap_or(false);
                if shown {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            std::thread::sleep(Duration::from_millis(400));
            for label in LATER {
                if crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                let _ = ensure(&app, label);
                std::thread::sleep(Duration::from_millis(250));
            }
        })
        .ok();
}

/// Makes one of these windows now (the notch asks for the overlay at once
/// when it shows the welcome splash, which plays there).
#[tauri::command]
pub async fn ensure_window(app: AppHandle, label: String) {
    if !LATER.contains(&label.as_str()) {
        return;
    }
    let _ = tauri::async_runtime::spawn_blocking(move || ensure(&app, &label)).await;
}
