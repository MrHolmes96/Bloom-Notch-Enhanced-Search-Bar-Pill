//! The Search Bar Pill: a Flow Launcher-style search bar that opens on a
//! global hotkey. The search itself runs in the launcher webview; this module
//! owns the hotkeys and the window, and the submodules give the webview what a
//! browser page can't reach: the Windows Search index, the shell, processes,
//! browser bookmarks and search suggestions.
//!
//! The hotkeys are plain `RegisterHotKey` registrations on a small message
//! thread, so nothing polls while the bar is closed.

pub mod actions;
pub mod bookmarks;
pub mod icons;
pub mod index;
pub mod suggest;
pub mod system;

use serde::Deserialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY,
};

/// The main hotkey; custom query hotkeys use the ids after it.
const HOTKEY_ID: i32 = 0x0B10;
const WM_RELOAD_HOTKEYS: u32 = WM_APP + 0x0B1;

/// Thread id of the hotkey message loop, 0 until it has started.
static HOTKEY_THREAD: AtomicU32 = AtomicU32::new(0);
/// "Game mode" from the search bar's system commands: hotkeys are ignored.
static GAME_MODE: AtomicBool = AtomicBool::new(false);
/// Queries opened by the custom query hotkeys, by hotkey id offset.
static QUERY_HOTKEYS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Whether the main hotkey could be registered, for the settings page.
static HOTKEY_STATUS: Mutex<Option<bool>> = Mutex::new(None);

/// Bloom's own shortcuts (Settings > General > Shortcuts), registered on the
/// same thread as the search bar's but kept whether or not the search bar is
/// on, and not muted by its game mode: the setting key, and what it does.
const APP_HOTKEYS: [(&str, AppHotkey); 2] = [
    ("bloom-hotkey-screen-off", AppHotkey::ScreenOff),
    ("bloom-hotkey-notification-center", AppHotkey::NotificationCenter),
];
/// Their ids follow the search bar's (main and up to 32 query hotkeys).
const APP_HOTKEY_BASE: i32 = HOTKEY_ID + 64;

#[derive(Clone, Copy)]
enum AppHotkey {
    ScreenOff,
    NotificationCenter,
}

/// A shortcut is being recorded in Settings: Bloom's own shortcuts wait.
static RECORDING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn set_hotkey_recording(recording: bool) {
    RECORDING.store(recording, Ordering::Relaxed);
}

/// Whether each of Bloom's own shortcuts could be registered (null: none set).
static APP_HOTKEY_STATUS: Mutex<[Option<bool>; 2]> = Mutex::new([None, None]);

fn register_app_hotkeys(app: &AppHandle, registered: &mut Vec<i32>) {
    let mut status = [None; 2];
    for (index, (key, _)) in APP_HOTKEYS.iter().enumerate() {
        let Some(combo) = setting(app, key).filter(|c| !c.trim().is_empty()) else {
            continue;
        };
        let id = APP_HOTKEY_BASE + index as i32;
        let ok = parse_hotkey(&combo)
            .is_some_and(|(m, k)| unsafe { RegisterHotKey(None, id, m, k).is_ok() });
        if ok {
            registered.push(id);
        }
        status[index] = Some(ok);
    }
    if let Ok(mut saved) = APP_HOTKEY_STATUS.lock() {
        *saved = status;
    }
}

fn run_app_hotkey(app: &AppHandle, what: AppHotkey) {
    match what {
        AppHotkey::ScreenOff => crate::black_screen::screen_off(),
        AppHotkey::NotificationCenter => {
            let app = app.clone();
            std::thread::spawn(move || crate::commands::toggle_notification_center(&app));
        }
    }
}

/// Whether one of Bloom's own shortcuts is registered: true, false when
/// another app has that shortcut, or null when none is set.
#[tauri::command]
pub fn app_hotkey_status(key: String) -> Option<bool> {
    let index = APP_HOTKEYS.iter().position(|(k, _)| *k == key)?;
    APP_HOTKEY_STATUS.lock().ok().and_then(|s| s[index])
}

fn setting(app: &AppHandle, key: &str) -> Option<String> {
    crate::utils::get_setting_str(app, key)
}

fn setting_on(app: &AppHandle, key: &str, default: bool) -> bool {
    match setting(app, key).as_deref() {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

/// Parses "Ctrl+Shift+K", "Alt + Space", "Win+F1" into RegisterHotKey values.
pub fn parse_hotkey(text: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    let mut key: Option<u32> = None;
    for part in text.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "windows" | "meta" | "super" => modifiers |= MOD_WIN,
            name => key = Some(virtual_key(name)?),
        }
    }
    let key = key?;
    // A bare key would swallow normal typing everywhere.
    if modifiers.0 == 0 && !(0x70..=0x87).contains(&key) {
        return None;
    }
    Some((modifiers | MOD_NOREPEAT, key))
}

fn virtual_key(name: &str) -> Option<u32> {
    let vk = match name {
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "backspace" => 0x08,
        "escape" | "esc" => 0x1B,
        "insert" => 0x2D,
        "delete" | "del" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "`" | "backquote" => 0xC0,
        "-" | "minus" => 0xBD,
        "=" | "equal" => 0xBB,
        "[" | "bracketleft" => 0xDB,
        "]" | "bracketright" => 0xDD,
        "\\" | "backslash" => 0xDC,
        ";" | "semicolon" => 0xBA,
        "'" | "quote" => 0xDE,
        "," | "comma" => 0xBC,
        "." | "period" => 0xBE,
        "/" | "slash" => 0xBF,
        _ => {
            let bytes = name.as_bytes();
            if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
                bytes[0].to_ascii_uppercase() as u32
            } else if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
                if !(1..=24).contains(&n) {
                    return None;
                }
                0x6F + n
            } else {
                let n = name.strip_prefix("numpad")?.parse::<u32>().ok()?;
                if n > 9 {
                    return None;
                }
                0x60 + n
            }
        }
    };
    Some(vk)
}

#[derive(Deserialize)]
struct QueryHotkey {
    hotkey: String,
    query: String,
}

/// Registers Bloom's own shortcuts, the search bar's main hotkey and its
/// custom query hotkeys from settings. Returns the ids that were registered.
fn register_all(app: &AppHandle) -> Vec<i32> {
    let mut registered = Vec::new();
    register_app_hotkeys(app, &mut registered);
    if !setting_on(app, "bloom-launcher-enabled", true) {
        if let Ok(mut status) = HOTKEY_STATUS.lock() {
            *status = None;
        }
        return registered;
    }
    let combo = setting(app, "bloom-launcher-hotkey").unwrap_or_else(|| "Alt+Space".into());
    let ok = parse_hotkey(&combo)
        .is_some_and(|(m, k)| unsafe { RegisterHotKey(None, HOTKEY_ID, m, k).is_ok() });
    if ok {
        registered.push(HOTKEY_ID);
    }
    if let Ok(mut status) = HOTKEY_STATUS.lock() {
        *status = Some(ok);
    }

    let extra: Vec<QueryHotkey> = setting(app, "bloom-launcher-query-hotkeys")
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let mut queries = Vec::new();
    for (i, entry) in extra.iter().enumerate().take(32) {
        queries.push(entry.query.clone());
        let id = HOTKEY_ID + 1 + i as i32;
        if let Some((m, k)) = parse_hotkey(&entry.hotkey) {
            if unsafe { RegisterHotKey(None, id, m, k).is_ok() } {
                registered.push(id);
            }
        }
    }
    if let Ok(mut q) = QUERY_HOTKEYS.lock() {
        *q = queries;
    }
    registered
}

/// Starts the hotkey listener. Call once from setup.
pub fn start_hotkey_listener(app: AppHandle) {
    std::thread::spawn(move || unsafe {
        let mut msg = MSG::default();
        // Creates this thread's message queue before anyone posts to it.
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        HOTKEY_THREAD.store(
            windows::Win32::System::Threading::GetCurrentThreadId(),
            Ordering::Relaxed,
        );
        let mut registered = register_all(&app);
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_HOTKEY {
                let id = msg.wParam.0 as i32;
                if let Some((_, what)) = id
                    .checked_sub(APP_HOTKEY_BASE)
                    .and_then(|index| APP_HOTKEYS.get(index as usize))
                {
                    if !RECORDING.load(Ordering::Relaxed) {
                        run_app_hotkey(&app, *what);
                    }
                    continue;
                }
                if GAME_MODE.load(Ordering::Relaxed) {
                    continue;
                }
                if setting_on(&app, "bloom-launcher-ignore-fullscreen", false)
                    && crate::state::CURRENT_FOREGROUND_FULLSCREEN.load(Ordering::Relaxed)
                {
                    continue;
                }
                if id == HOTKEY_ID {
                    toggle_launcher(&app, None);
                } else if id > HOTKEY_ID {
                    let query = QUERY_HOTKEYS
                        .lock()
                        .ok()
                        .and_then(|q| q.get((id - HOTKEY_ID - 1) as usize).cloned());
                    if let Some(query) = query {
                        toggle_launcher(&app, Some(query));
                    }
                }
            } else if msg.message == WM_RELOAD_HOTKEYS {
                for id in registered.drain(..) {
                    let _ = UnregisterHotKey(None, id);
                }
                registered = register_all(&app);
                let _ = app.emit("launcher-hotkey-status", hotkey_status());
                let _ = app.emit("app-hotkeys-changed", ());
            }
        }
    });
}

/// Re-reads the search bar's hotkey settings (called when they change).
pub fn reload_hotkeys() {
    let thread = HOTKEY_THREAD.load(Ordering::Relaxed);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_RELOAD_HOTKEYS, WPARAM(0), LPARAM(0));
        }
    }
}

fn hotkey_status() -> Option<bool> {
    HOTKEY_STATUS.lock().ok().and_then(|s| *s)
}

/// Whether the main hotkey is registered: true, false when another app owns
/// that shortcut, or null when the search bar is off.
#[tauri::command]
pub fn launcher_hotkey_status() -> Option<bool> {
    hotkey_status()
}

/// The monitor the bar opens on, by the "bloom-launcher-screen" setting.
unsafe fn target_monitor(app: &AppHandle) -> Option<windows::Win32::Foundation::RECT> {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow};
    let monitor = match setting(app, "bloom-launcher-screen").as_deref() {
        Some("primary") => MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY),
        Some("focus") => MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY),
        _ => {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            MonitorFromPoint(cursor, MONITOR_DEFAULTTOPRIMARY)
        }
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    GetMonitorInfoW(monitor, &mut info)
        .as_bool()
        .then_some(info.rcMonitor)
}

/// Places the bar horizontally centred on the target monitor: just under the
/// notch, in the upper third, or around the middle.
fn position_window(app: &AppHandle, window: &tauri::WebviewWindow) {
    let Some(area) = (unsafe { target_monitor(app) }) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let width = window.outer_size().map(|s| s.width as i32).unwrap_or(0);
    let monitor_width = area.right - area.left;
    let monitor_height = area.bottom - area.top;
    let x = area.left + (monitor_width - width) / 2;
    let y = match setting(app, "bloom-launcher-position").as_deref() {
        Some("upper") => area.top + monitor_height / 4,
        Some("center") => area.top + monitor_height * 2 / 5,
        // Just below the notch (36 px tall at 100 %).
        _ => area.top + (52.0 * scale) as i32,
    };
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Shows or hides the bar. A preset query (from a custom query hotkey) always
/// shows it with that query.
pub fn toggle_launcher(app: &AppHandle, query: Option<String>) {
    let Some(window) = app.get_webview_window("launcher") else {
        // Pressed in the moment after start, before the bar is made.
        let app = app.clone();
        std::thread::spawn(move || {
            if crate::later_windows::ensure(&app, "launcher").is_some() {
                toggle_launcher(&app, query);
            }
        });
        return;
    };
    if window.is_visible().unwrap_or(false) && query.is_none() {
        let _ = window.hide();
        return;
    }
    position_window(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(
        "launcher",
        "launcher-show",
        serde_json::json!({ "query": query }),
    );
}

#[tauri::command]
pub fn launcher_hide(app: AppHandle) {
    if let Some(window) = app.get_webview_window("launcher") {
        let _ = window.hide();
    }
}

/// Sizes the bar (logical pixels). The width keeps it centred where it is.
#[tauri::command]
pub fn launcher_set_size(window: tauri::WebviewWindow, width: f64, height: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let old_width = window.outer_size().map(|s| s.width as i32).unwrap_or(0);
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    let new_width = (width * scale) as i32;
    if new_width != old_width {
        if let Ok(pos) = window.outer_position() {
            let x = pos.x + (old_width - new_width) / 2;
            let _ = window.set_position(tauri::PhysicalPosition::new(x, pos.y));
        }
    }
}

/// Game mode: while on, the search bar's hotkeys do nothing.
#[tauri::command]
pub fn launcher_set_game_mode(enabled: bool) -> bool {
    GAME_MODE.store(enabled, Ordering::Relaxed);
    enabled
}

#[tauri::command]
pub fn launcher_game_mode() -> bool {
    GAME_MODE.load(Ordering::Relaxed)
}

/// Opens Bloom's settings on the Search Bar Pill page.
#[tauri::command]
pub fn launcher_open_settings(app: AppHandle) {
    crate::commands::open_settings_tab(app, "search-bar".into());
}

/// Opens the folder where Bloom keeps settings.json.
#[tauri::command]
pub fn launcher_open_config_dir(app: AppHandle) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    actions::shell_execute(&dir.to_string_lossy(), None, None, None, true)
}

/// Re-scans installed apps (the "Reload Plugin Data" command and F5).
#[tauri::command]
pub fn launcher_reindex() {
    crate::services::trigger_app_scan();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hotkeys() {
        let (m, k) = parse_hotkey("Alt+Space").unwrap();
        assert_eq!(k, 0x20);
        assert_eq!(m.0, (MOD_ALT | MOD_NOREPEAT).0);
        let (m, k) = parse_hotkey("Ctrl + Shift + K").unwrap();
        assert_eq!(k, b'K' as u32);
        assert_eq!(m.0, (MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT).0);
        assert_eq!(parse_hotkey("Win+F1").unwrap().1, 0x70);
        assert!(parse_hotkey("F13").is_some());
        assert!(parse_hotkey("K").is_none());
        assert!(parse_hotkey("Ctrl+").is_none());
        assert!(parse_hotkey("Ctrl+Nope").is_none());
    }
}
