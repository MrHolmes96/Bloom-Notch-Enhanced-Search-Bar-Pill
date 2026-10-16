//! Remembers each app's window size and position and puts it back the next
//! time the app opens (Settings > General > Remember Window Sizes).
//!
//! One entry per program and window class, kept in window_positions.json in
//! Bloom's config folder. Only ordinary resizable app windows count: no
//! dialogs, tool windows, borderless games or Bloom's own windows. A saved
//! place is used only for an app's first window, and only when it still lies
//! on a connected screen.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindow, GetWindowLongW, GetWindowPlacement, GetWindowThreadProcessId,
    IsWindowVisible, SetWindowPlacement, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, SW_SHOWMAXIMIZED,
    SW_SHOWNORMAL, WINDOWPLACEMENT, WS_CAPTION, WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_THICKFRAME,
};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
struct Place {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    maximized: bool,
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static PLACES: OnceLock<Mutex<HashMap<String, Place>>> = OnceLock::new();
static DIRTY: AtomicBool = AtomicBool::new(false);
/// Windows already put in place (or seen) this run, so each is placed once.
static HANDLED: OnceLock<Mutex<HashSet<isize>>> = OnceLock::new();

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

fn places() -> &'static Mutex<HashMap<String, Place>> {
    PLACES.get_or_init(|| {
        let map = store_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Mutex::new(map)
    })
}

fn handled() -> &'static Mutex<HashSet<isize>> {
    HANDLED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
}

/// "program path|window class" for an ordinary resizable app window.
fn window_key(hwnd: HWND) -> Option<String> {
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
        Some(format!("{}|{}", path, class_name(hwnd)))
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

fn on_a_screen(place: &Place) -> bool {
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONULL};
    // The title bar has to be reachable: its middle must be on some screen.
    let point = POINT {
        x: (place.left + place.right) / 2,
        y: place.top + 10,
    };
    unsafe { !MonitorFromPoint(point, MONITOR_DEFAULTTONULL).is_invalid() }
}

fn remember(hwnd: HWND) {
    let Some(key) = window_key(hwnd) else {
        return;
    };
    let Some(place) = placement(hwnd) else {
        return;
    };
    if let Ok(mut map) = places().lock() {
        if map.get(&key) != Some(&place) {
            map.insert(key, place);
            DIRTY.store(true, Ordering::Relaxed);
        }
    }
}

/// Whether another window with this key is already open (the app's second
/// window keeps the place the app gives it).
fn another_open(hwnd: HWND, key: &str) -> bool {
    use windows::core::BOOL;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    struct Search<'a> {
        skip: HWND,
        key: &'a str,
        found: bool,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut Search);
        if hwnd != search.skip && window_key(hwnd).as_deref() == Some(search.key) {
            search.found = true;
            return false.into();
        }
        true.into()
    }
    let mut search = Search {
        skip: hwnd,
        key,
        found: false,
    };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
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

/// A window appeared: put it where its app was last left.
fn restore(hwnd: HWND) {
    let first_time = handled()
        .lock()
        .map(|mut set| set.insert(hwnd.0 as isize))
        .unwrap_or(false);
    if !first_time {
        return;
    }
    let Some(key) = window_key(hwnd) else {
        return;
    };
    let saved = places().lock().ok().and_then(|map| map.get(&key).copied());
    let Some(place) = saved else {
        return;
    };
    if !on_a_screen(&place) || another_open(hwnd, &key) {
        return;
    }
    apply(hwnd, &place);
    // Some apps set their own size just after showing; settle once more.
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let hwnd = HWND(raw as *mut _);
        if let Some(now) = placement(hwnd) {
            if now != place && !mouse_button_down() {
                apply(hwnd, &place);
            }
        }
    });
}

fn mouse_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
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
        EVENT_SYSTEM_MOVESIZEEND,
    };
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() || !enabled() {
        return;
    }
    match event {
        e if e == EVENT_OBJECT_SHOW => restore(hwnd),
        e if e == EVENT_SYSTEM_MOVESIZEEND || e == EVENT_OBJECT_LOCATIONCHANGE => {
            // Only windows already placed or seen count, so an app's own
            // first positioning isn't mistaken for the user's.
            let known = handled()
                .lock()
                .map(|set| set.contains(&(hwnd.0 as isize)))
                .unwrap_or(false);
            if known && !mouse_button_down() {
                remember(hwnd);
            }
        }
        e if e == EVENT_OBJECT_HIDE || e == EVENT_OBJECT_DESTROY => {
            if let Ok(mut set) = handled().lock() {
                set.remove(&(hwnd.0 as isize));
            }
        }
        _ => {}
    }
}

/// Windows already open when Bloom starts count as seen, so their moves are
/// remembered too.
fn mark_open_windows() {
    use windows::core::BOOL;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    unsafe extern "system" fn visit(hwnd: HWND, _: LPARAM) -> BOOL {
        if window_key(hwnd).is_some() {
            if let Ok(mut set) = handled().lock() {
                set.insert(hwnd.0 as isize);
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
    let text = places()
        .lock()
        .ok()
        .and_then(|map| serde_json::to_string(&*map).ok());
    if let Some(text) = text {
        let _ = std::fs::write(path, text);
    }
}

pub fn start(app: AppHandle) {
    if APP.set(app).is_err() {
        return;
    }
    let _ = places();
    mark_open_windows();
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        save_now();
    });
    std::thread::spawn(|| unsafe {
        use windows::Win32::UI::Accessibility::SetWinEventHook;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW,
            EVENT_SYSTEM_MOVESIZEEND, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        };
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        for (min, max) in [
            (EVENT_OBJECT_SHOW, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_DESTROY),
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
            (EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZEEND),
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

/// Settings > General > Forget Window Sizes.
#[tauri::command]
pub fn forget_window_sizes() {
    if let Ok(mut map) = places().lock() {
        map.clear();
    }
    DIRTY.store(true, Ordering::Relaxed);
    save_now();
}
