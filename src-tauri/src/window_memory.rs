//! Remembers app windows' sizes and places and puts them back when the
//! windows open again (Settings > Windows > Remember Window Sizes).
//!
//! Only what the user does is saved: dragging or resizing a window, or
//! snapping, maximising or moving it with the keyboard. An app sizing its own
//! window (as it opens, comes back from the tray, or a screen changes) is
//! not. For its first seconds a window put back is held there, so the app
//! can't size it over again.
//!
//! Each of an app's windows is kept apart by its title when that is a fixed
//! name ("Steam", "Friends List"); see `window_rules.rs`. Per app, Settings
//! can leave it alone or limit Bloom to the windows ticked.
//!
//! Only ordinary resizable app windows count: no dialogs, tool windows,
//! borderless games or Bloom's own windows. Kept in window_positions.json;
//! what happens is written to window-memory.log (in the diagnostics zip).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowPlacement,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SetWindowPlacement, GWL_EXSTYLE,
    GWL_STYLE, GW_OWNER, SW_SHOWMAXIMIZED, SW_SHOWNORMAL, WINDOWPLACEMENT, WS_CAPTION, WS_CHILD,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_THICKFRAME,
};

use crate::window_rules::{self, AppMem, Choice, Mode, Place, Store};

static APP: OnceLock<AppHandle> = OnceLock::new();
static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
static DIRTY: AtomicBool = AtomicBool::new(false);
/// The app windows open now, by handle.
static TRACKED: OnceLock<Mutex<HashMap<isize, Tracked>>> = OnceLock::new();
/// Saves waiting for a window to settle: when, and what the user did.
static PENDING: OnceLock<Mutex<HashMap<isize, (Instant, &'static str)>>> = OnceLock::new();

/// How long a window put back is held there.
const HOLD: Duration = Duration::from_millis(2500);
/// Input this recent means a window's move was the user's.
const USER_INPUT_MS: u32 = 500;

#[derive(Clone)]
struct Tracked {
    /// Its program's path (lower case).
    app: String,
    class: String,
    /// Its name when it opened ("" = its app's shared entry).
    name: String,
    /// When it showed, in GetTickCount's time.
    shown_tick: u32,
    /// Until when Bloom holds it where it was put (or leaves its size to the
    /// app); `None` once the user moves it.
    hold_until: Option<Instant>,
}

fn enabled() -> bool {
    APP.get().is_some_and(|app| {
        crate::utils::get_setting_str(app, "bloom-window-memory").as_deref() != Some("false")
    })
}

fn store_path() -> Option<PathBuf> {
    Some(
        APP.get()?
            .path()
            .app_config_dir()
            .ok()?
            .join("window_positions.json"),
    )
}

fn store() -> &'static Mutex<Store> {
    STORE.get_or_init(|| {
        let text = store_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Mutex::new(window_rules::read_store(&text))
    })
}

fn tracked() -> &'static Mutex<HashMap<isize, Tracked>> {
    TRACKED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending() -> &'static Mutex<HashMap<isize, (Instant, &'static str)>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn log(line: &str) {
    use std::io::Write;
    let Some(path) = crate::utils::data_file("window-memory.log") else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 48 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {line}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        );
    }
}

/// "lm studio.exe “LM Studio”" for the log.
fn who(app: &str, name: &str) -> String {
    let file = app.rsplit('\\').next().unwrap_or(app);
    if name.is_empty() {
        format!("{file} (its windows with changing titles)")
    } else {
        format!("{file} “{name}”")
    }
}

fn describe(place: &Place) -> String {
    format!(
        "{}×{} at ({}, {}){}",
        place.right - place.left,
        place.bottom - place.top,
        place.left,
        place.top,
        if place.maximized { ", maximised" } else { "" }
    )
}

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
}

fn title_of(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
}

/// Program path (lower case) and window class of an ordinary resizable app
/// window.
fn identify(hwnd: HWND) -> Option<(String, String)> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_CHILD.0 != 0
            || style & WS_THICKFRAME.0 == 0
            || style & WS_CAPTION.0 != WS_CAPTION.0
            || ex_style & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0
        {
            return None;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let path = crate::commands::process_image_path(pid)?.to_lowercase();
        let file = path.rsplit('\\').next().unwrap_or_default();
        // Windows itself handles these (or re-parents them).
        if matches!(
            file,
            "applicationframehost.exe" | "shellexperiencehost.exe" | "searchhost.exe"
        ) {
            return None;
        }
        Some((path, class_name(hwnd)))
    }
}

fn placement(hwnd: HWND) -> Option<Place> {
    let mut wp = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe { GetWindowPlacement(hwnd, &mut wp) }.ok()?;
    // Minimised windows keep their last normal place; nothing new to learn.
    if wp.showCmd == 2 || wp.showCmd == 6 || wp.showCmd == 7 {
        return None;
    }
    let r = wp.rcNormalPosition;
    if r.right - r.left < 100 || r.bottom - r.top < 80 {
        return None;
    }
    Some(Place {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
        maximized: wp.showCmd == SW_SHOWMAXIMIZED.0 as u32,
    })
}

/// Where window placements' "workspace" coordinates start, on screen: the
/// primary screen's work area (Windows measures `rcNormalPosition` from it).
fn workspace_offset() -> (i32, i32) {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let primary = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        if !GetMonitorInfoW(primary, &mut info).as_bool() {
            return (0, 0);
        }
    }
    (
        info.rcWork.left - info.rcMonitor.left,
        info.rcWork.top - info.rcMonitor.top,
    )
}

/// The place to put a window: the saved one, or, when its normal size hangs
/// off the screen (a window first seen maximised keeps the maximised
/// overhang, as LM Studio's did at (-12, -12)), fitted into that screen's
/// work area. `None` when it isn't on any screen now.
fn fitted(place: &Place) -> Option<Place> {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONULL,
    };
    let (dx, dy) = workspace_offset();
    let on_screen = RECT {
        left: place.left + dx,
        top: place.top + dy,
        right: place.right + dx,
        bottom: place.bottom + dy,
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let monitor = MonitorFromRect(&on_screen, MONITOR_DEFAULTTONULL);
        if monitor.is_invalid() || !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
    }
    let work = info.rcWork;
    let (w, h) = (
        on_screen.right - on_screen.left,
        on_screen.bottom - on_screen.top,
    );
    let (work_w, work_h) = (work.right - work.left, work.bottom - work.top);
    // The title bar has to be reachable, and the window has to fit.
    if on_screen.top >= work.top && w <= work_w && h <= work_h {
        return Some(*place);
    }
    let w = w.min(work_w);
    let h = h.min(work_h);
    let left = on_screen.left.clamp(work.left, work.right - w);
    let top = on_screen.top.clamp(work.top, work.bottom - h);
    Some(Place {
        left: left - dx,
        top: top - dy,
        right: left + w - dx,
        bottom: top + h - dy,
        maximized: place.maximized,
    })
}

fn apply(hwnd: HWND, place: &Place) {
    let mut wp = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe {
        if GetWindowPlacement(hwnd, &mut wp).is_err() {
            return;
        }
        wp.rcNormalPosition = RECT {
            left: place.left,
            top: place.top,
            right: place.right,
            bottom: place.bottom,
        };
        wp.showCmd = if place.maximized {
            SW_SHOWMAXIMIZED.0 as u32
        } else {
            SW_SHOWNORMAL.0 as u32
        };
        let _ = SetWindowPlacement(hwnd, &wp);
    }
}

fn mouse_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}

fn tick_now() -> u32 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount() }
}

/// When the last key press or mouse input was, in GetTickCount's time.
fn last_input_tick() -> Option<u32> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe { GetLastInputInfo(&mut info) }
        .as_bool()
        .then_some(info.dwTime)
}

fn held(t: &Tracked) -> bool {
    t.hold_until.is_some_and(|until| Instant::now() < until)
}

/// Another window of the same app and name is open (an app's second window
/// keeps the place the app gives it).
fn another_open(hwnd: HWND, app: &str, name: &str, class: &str) -> bool {
    let others: Vec<isize> = tracked()
        .lock()
        .map(|map| {
            map.iter()
                .filter(|(h, t)| {
                    **h != hwnd.0 as isize && t.app == app && t.name == name && t.class == class
                })
                .map(|(h, _)| *h)
                .collect()
        })
        .unwrap_or_default();
    others
        .into_iter()
        .any(|h| unsafe { IsWindowVisible(HWND(h as *mut _)).as_bool() })
}

/// A window appeared.
fn on_show(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    if tracked().lock().is_ok_and(|map| map.contains_key(&raw)) {
        return;
    }
    let Some((app, class)) = identify(hwnd) else {
        return;
    };
    // Held from the start, so whatever the app does while opening isn't saved.
    if let Ok(mut map) = tracked().lock() {
        map.insert(
            raw,
            Tracked {
                app: app.clone(),
                class: class.clone(),
                name: String::new(),
                shown_tick: tick_now(),
                hold_until: Some(Instant::now() + HOLD),
            },
        );
    }
    let title = title_of(hwnd);
    if !title.trim().is_empty() {
        place_new(hwnd, &app, &class, &title);
        return;
    }
    // Some apps title their window just after showing it.
    std::thread::spawn(move || {
        let hwnd = HWND(raw as *mut _);
        let mut title = String::new();
        for _ in 0..15 {
            std::thread::sleep(Duration::from_millis(20));
            title = title_of(hwnd);
            if !title.trim().is_empty() {
                break;
            }
        }
        place_new(hwnd, &app, &class, &title);
    });
}

/// Puts a window that just opened where it was last left.
fn place_new(hwnd: HWND, app: &str, class: &str, title: &str) {
    let raw = hwnd.0 as isize;
    let name = window_rules::window_name(title);
    let still_open = tracked()
        .lock()
        .map(|mut map| match map.get_mut(&raw) {
            Some(t) => {
                t.name = name.clone();
                true
            }
            None => false,
        })
        .unwrap_or(false);
    if !still_open || !enabled() {
        return;
    }
    let choice = store()
        .lock()
        .map(|s| window_rules::choose(s.apps.get(app), class, &name))
        .unwrap_or(Choice::Nothing);
    let place = match choice {
        Choice::Off => {
            log(&format!("{}: Off in Settings, left alone", who(app, &name)));
            return;
        }
        Choice::NotMain => {
            log(&format!(
                "{}: not ticked as a main window, left alone",
                who(app, &name)
            ));
            return;
        }
        Choice::Nothing => {
            log(&format!(
                "{} opened: nothing saved for it, the app places it",
                who(app, &name)
            ));
            return;
        }
        Choice::Use(_, place) => place,
    };
    let Some(place) = fitted(&place) else {
        log(&format!(
            "{} opened: its saved place is off every screen now, left alone",
            who(app, &name)
        ));
        return;
    };
    if another_open(hwnd, app, &name, class) {
        log(&format!(
            "{} opened: another of the same window is open, the app places it",
            who(app, &name)
        ));
        return;
    }
    apply(hwnd, &place);
    log(&format!(
        "{} opened: put back to {}",
        who(app, &name),
        describe(&place)
    ));
    hold_in_place(raw, app.to_string(), name, place);
}

/// Some apps size their window again just after it shows (LM Studio coming
/// back from the tray, for one): put it back each time, until the hold ends
/// or the user moves it.
fn hold_in_place(raw: isize, app: String, name: String, place: Place) {
    std::thread::spawn(move || {
        let hwnd = HWND(raw as *mut _);
        let started = Instant::now();
        let mut told = false;
        for at in [100u64, 250, 500, 900, 1500, 2400] {
            let wait = Duration::from_millis(at).saturating_sub(started.elapsed());
            std::thread::sleep(wait);
            let still_held = tracked()
                .lock()
                .ok()
                .and_then(|map| map.get(&raw).map(held))
                .unwrap_or(false);
            if !still_held || !unsafe { IsWindowVisible(hwnd).as_bool() } {
                return;
            }
            let Some(now) = placement(hwnd) else {
                continue;
            };
            if now != place && !mouse_button_down() {
                apply(hwnd, &place);
                if !told {
                    told = true;
                    log(&format!(
                        "{}: the app sized it itself to {}; put back",
                        who(&app, &name),
                        describe(&now)
                    ));
                }
            }
        }
    });
}

/// Saves a window's place once it has settled.
fn save_later(raw: isize, after: Duration, why: &'static str) {
    if let Ok(mut map) = pending().lock() {
        map.insert(raw, (Instant::now() + after, why));
    }
}

fn commit(raw: isize, why: &str) {
    let hwnd = HWND(raw as *mut _);
    if !enabled() || !unsafe { IsWindowVisible(hwnd).as_bool() } {
        return;
    }
    let Some(t) = tracked().lock().ok().and_then(|map| map.get(&raw).cloned()) else {
        return;
    };
    let Some(place) = placement(hwnd) else {
        return;
    };
    let title = title_of(hwnd);
    let changed = store()
        .lock()
        .map(|mut s| {
            let app: &mut AppMem = s.apps.entry(t.app.clone()).or_default();
            window_rules::learn(app, &t.class, &t.name, &title, place, now_secs())
        })
        .unwrap_or(false);
    if changed {
        DIRTY.store(true, Ordering::Relaxed);
        log(&format!(
            "{}: saved {} ({why})",
            who(&t.app, &t.name),
            describe(&place)
        ));
    }
}

unsafe extern "system" fn on_event(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW,
        EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART,
    };
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    let raw = hwnd.0 as isize;
    match event {
        e if e == EVENT_OBJECT_SHOW => on_show(hwnd),
        e if e == EVENT_SYSTEM_MOVESIZESTART || e == EVENT_SYSTEM_MOVESIZEEND => {
            // Dragging or resizing it: the user's, without a doubt.
            let known = tracked()
                .lock()
                .map(|mut map| match map.get_mut(&raw) {
                    Some(t) => {
                        t.hold_until = None;
                        true
                    }
                    None => false,
                })
                .unwrap_or(false);
            if known && e == EVENT_SYSTEM_MOVESIZEEND {
                save_later(raw, Duration::from_millis(300), "dragged or resized");
            }
        }
        e if e == EVENT_OBJECT_LOCATIONCHANGE => {
            let Some(t) = tracked().lock().ok().and_then(|map| map.get(&raw).cloned()) else {
                return;
            };
            // Snapping, maximising, Win+arrow keys: right after the user's
            // input, to the window in front. Anything else moving it is the
            // app or Windows (a screen change), and isn't saved.
            let Some(input) = last_input_tick() else {
                return;
            };
            let recent = tick_now().wrapping_sub(input) <= USER_INPUT_MS;
            if !recent || GetForegroundWindow() != hwnd {
                return;
            }
            if held(&t) {
                // While held, only input after it showed counts (the click
                // that brought it back from the tray doesn't).
                if input.wrapping_sub(t.shown_tick) as i32 <= 150 {
                    return;
                }
                if let Ok(mut map) = tracked().lock() {
                    if let Some(t) = map.get_mut(&raw) {
                        t.hold_until = None;
                    }
                }
            }
            save_later(
                raw,
                Duration::from_millis(400),
                "snapped, maximised or moved with keys",
            );
        }
        e if e == EVENT_OBJECT_HIDE || e == EVENT_OBJECT_DESTROY => {
            // Closing to the tray can shuffle a window first; that's not saved.
            if let Ok(mut map) = pending().lock() {
                map.remove(&raw);
            }
            if let Ok(mut map) = tracked().lock() {
                map.remove(&raw);
            }
        }
        _ => {}
    }
}

/// Windows already open when Bloom starts count as seen, so their moves are
/// saved too.
fn track_open_windows() {
    use windows::core::BOOL;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    unsafe extern "system" fn visit(hwnd: HWND, _: LPARAM) -> BOOL {
        if let Some((app, class)) = identify(hwnd) {
            let name = window_rules::window_name(&title_of(hwnd));
            if let Ok(mut map) = tracked().lock() {
                map.insert(
                    hwnd.0 as isize,
                    Tracked {
                        app,
                        class,
                        name,
                        shown_tick: tick_now(),
                        hold_until: None,
                    },
                );
            }
        }
        true.into()
    }
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(0));
    }
}

fn save_now() {
    if !DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let Some(path) = store_path() else {
        return;
    };
    let text = store()
        .lock()
        .ok()
        .and_then(|s| serde_json::to_string(&*s).ok());
    if let Some(text) = text {
        let _ = std::fs::write(path, text);
    }
}

pub fn start(app: AppHandle) {
    if APP.set(app).is_err() {
        return;
    }
    let _ = store();
    track_open_windows();
    // Saves that are due, and the file every few seconds.
    std::thread::spawn(|| {
        let mut ticks = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let due: Vec<(isize, &'static str)> = pending()
                .lock()
                .map(|mut map| {
                    let now = Instant::now();
                    let due: Vec<_> = map
                        .iter()
                        .filter(|(_, (at, _))| *at <= now)
                        .map(|(h, (_, why))| (*h, *why))
                        .collect();
                    for (h, _) in &due {
                        map.remove(h);
                    }
                    due
                })
                .unwrap_or_default();
            for (raw, why) in due {
                if mouse_button_down() {
                    // Still being dragged: wait for the release.
                    save_later(raw, Duration::from_millis(200), why);
                } else {
                    commit(raw, why);
                }
            }
            ticks += 1;
            if ticks >= 30 {
                ticks = 0;
                save_now();
            }
        }
    });
    std::thread::spawn(|| unsafe {
        use windows::Win32::UI::Accessibility::SetWinEventHook;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW,
            EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, MSG, WINEVENT_OUTOFCONTEXT,
            WINEVENT_SKIPOWNPROCESS,
        };
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        for (min, max) in [
            (EVENT_OBJECT_SHOW, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_DESTROY),
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
            (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
        ] {
            let _ = SetWinEventHook(min, max, None, Some(on_event), 0, 0, flags);
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

/// Settings > Windows > Forget Saved Sizes: every app's sizes (which windows
/// are ticked, and apps set to Off or Main only, stay).
#[tauri::command]
pub fn forget_window_sizes() {
    if let Ok(mut s) = store().lock() {
        s.apps.retain(|_, app| !window_rules::forget_sizes(app));
    }
    log("forgot every app's saved sizes (Settings)");
    DIRTY.store(true, Ordering::Relaxed);
    save_now();
}

#[derive(Serialize)]
pub struct WindowEntry {
    id: String,
    /// Its name, or for windows with changing titles the last one seen.
    title: String,
    /// The entry shared by the app's windows with changing titles.
    shared: bool,
    main: bool,
    /// "1200 × 800" (or "Maximised"); `None` when nothing is saved.
    size: Option<String>,
}

#[derive(Serialize)]
pub struct AppEntry {
    path: String,
    name: String,
    mode: Mode,
    windows: Vec<WindowEntry>,
}

fn entry_of(path: &str, app: &AppMem) -> AppEntry {
    let mut windows: Vec<(&String, &window_rules::WinMem)> = app.windows.iter().collect();
    // Named windows first, most recently used first; the shared one last.
    windows.sort_by(|a, b| {
        a.1.name
            .is_empty()
            .cmp(&b.1.name.is_empty())
            .then(b.1.used.cmp(&a.1.used))
    });
    AppEntry {
        path: path.to_string(),
        name: crate::commands::app_display_name(path, None),
        mode: app.mode,
        windows: windows
            .into_iter()
            .map(|(id, w)| WindowEntry {
                id: id.clone(),
                title: if w.name.is_empty() {
                    w.title.clone()
                } else {
                    w.name.clone()
                },
                shared: w.name.is_empty(),
                main: w.main,
                size: w.place.map(|p| {
                    if p.maximized {
                        "Maximised".to_string()
                    } else {
                        format!("{} × {}", p.right - p.left, p.bottom - p.top)
                    }
                }),
            })
            .collect(),
    }
}

/// Settings: every app Bloom keeps sizes or a setting for.
#[tauri::command]
pub async fn window_memory_apps() -> Vec<AppEntry> {
    let apps: Vec<(String, AppMem)> = store()
        .lock()
        .map(|s| s.apps.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    let mut list = tauri::async_runtime::spawn_blocking(move || {
        apps.iter()
            .map(|(path, app)| entry_of(path, app))
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    list.sort_by_key(|a| a.name.to_lowercase());
    list
}

fn change_app(path: &str, change: impl FnOnce(&mut AppMem)) {
    if let Ok(mut s) = store().lock() {
        let app = s.apps.entry(path.to_string()).or_default();
        change(app);
    }
    DIRTY.store(true, Ordering::Relaxed);
    save_now();
}

/// Settings: All windows / Main window only / Off for one app.
#[tauri::command]
pub fn window_memory_set_mode(path: String, mode: String) -> Result<(), String> {
    let mode = Mode::parse(&mode).ok_or("unknown mode")?;
    let name = crate::commands::app_display_name(&path, None);
    change_app(&path, |app| {
        app.mode = mode;
        if mode == Mode::Main {
            window_rules::tick_default_main(app, &name);
        }
    });
    log(&format!("{}: set to {mode:?} (Settings)", who(&path, "")));
    Ok(())
}

/// Settings: whether one of an app's windows counts as a main window.
#[tauri::command]
pub fn window_memory_set_main(path: String, id: String, main: bool) {
    change_app(&path, |app| {
        if let Some(w) = app.windows.get_mut(&id) {
            w.main = main;
        }
    });
}

/// Settings: forget one app's sizes (its setting stays).
#[tauri::command]
pub fn window_memory_forget_app(path: String) {
    if let Ok(mut s) = store().lock() {
        if s.apps
            .get_mut(&path)
            .is_some_and(window_rules::forget_sizes)
        {
            s.apps.remove(&path);
        }
    }
    log(&format!("{}: sizes forgotten (Settings)", who(&path, "")));
    DIRTY.store(true, Ordering::Relaxed);
    save_now();
}
