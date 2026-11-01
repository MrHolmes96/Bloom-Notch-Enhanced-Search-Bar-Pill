//! The desktop's right-click menu, opened where the pointer is.
//!
//! The strip beside the dock (and beside the notch, when it keeps its place
//! at the top) is kept free for Bloom, so it's outside the area Windows
//! gives apps. A right click on the desktop there still opens the desktop's
//! menu, but Windows keeps that menu inside the apps' area, so it opened a
//! long way from the pointer, as if the old taskbar were still there.
//!
//! Bloom notes a right click on the desktop in that strip, and moves the menu
//! the desktop opens for it (and the classic one "Show more options" opens
//! from it) to the pointer: above it in the strip at the bottom, below it in
//! the strip at the top, kept on the screen. Sub-menus open from the menu
//! where it now is, so they aren't moved.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetShellWindow, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, SetWindowPos, WindowFromPoint, GA_ROOT,
    GWL_STYLE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WS_CAPTION, WS_POPUP,
};

/// Where the strip is, for a press in it: the bottom (menu above the
/// pointer) or the top (menu below it).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Strip {
    Bottom,
    Top,
}

/// A right press on the desktop in a strip, waiting for its release.
static PRESSED_AT: AtomicI64 = AtomicI64::new(i64::MIN);
/// One menu followed at a time.
static FOLLOWING: AtomicBool = AtomicBool::new(false);

fn log(line: &str) {
    use std::io::Write;
    let Some(path) = crate::utils::data_file("desktop-menu.log") else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 24 * 1024) {
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

fn monitor_of(point: POINT) -> Option<MONITORINFO> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(monitor, &mut info)
            .as_bool()
            .then_some(info)
    }
}

/// Which strip the point is in, when it's outside the apps' area at the top
/// or bottom of its screen.
fn strip_at(point: POINT) -> Option<Strip> {
    let info = monitor_of(point)?;
    let (work, screen) = (info.rcWork, info.rcMonitor);
    if point.y >= work.bottom && point.y < screen.bottom {
        Some(Strip::Bottom)
    } else if point.y < work.top && point.y >= screen.top {
        Some(Strip::Top)
    } else {
        None
    }
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn shell_pid() -> u32 {
    pid_of(unsafe { GetShellWindow() })
}

/// Whether the desktop itself is under the point (not the dock, a window
/// or a notch).
fn desktop_under(point: POINT) -> bool {
    unsafe {
        let under = WindowFromPoint(point);
        if under.0.is_null() {
            return false;
        }
        let root = GetAncestor(under, GA_ROOT);
        let root = if root.0.is_null() { under } else { root };
        let mut class = [0u16; 32];
        let length = GetClassNameW(root, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..length.min(class.len())]);
        (class == "Progman" || class == "WorkerW") && pid_of(root) == shell_pid()
    }
}

fn class_of(hwnd: HWND) -> String {
    let mut class = [0u16; 128];
    let length = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
    String::from_utf16_lossy(&class[..length.min(class.len())])
}

/// Moves a menu to the pointer: above it from the bottom strip, below it
/// from the top one, kept on the pointer's screen.
fn move_to_pointer(menu: HWND, point: POINT, strip: Strip) -> bool {
    let Some(info) = monitor_of(point) else {
        return false;
    };
    let screen = info.rcMonitor;
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(menu, &mut rect) }.is_err() {
        return false;
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = point
        .x
        .clamp(screen.left, (screen.right - width).max(screen.left));
    let y = match strip {
        Strip::Bottom => point.y - height,
        Strip::Top => point.y,
    }
    .clamp(screen.top, (screen.bottom - height).max(screen.top));
    if (x, y) == (rect.left, rect.top) {
        return true;
    }
    unsafe {
        SetWindowPos(
            menu,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .is_ok()
    }
}

/// Every shown top-level window of Explorer's right now.
fn explorer_windows(shell: u32) -> Vec<HWND> {
    struct Search {
        shell: u32,
        found: Vec<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let search = &mut *(param.0 as *mut Search);
        if IsWindowVisible(hwnd).as_bool() && pid_of(hwnd) == search.shell {
            search.found.push(hwnd);
        }
        true.into()
    }
    let mut search = Search {
        shell,
        found: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

fn describe(hwnd: HWND) -> String {
    let mut rect = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    format!(
        "{:#x} {} style {style:#x} at {},{} {}x{}",
        hwnd.0 as isize,
        class_of(hwnd),
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top
    )
}

/// A right press: noted when it's in a strip, with what Explorer has open
/// already (so only the menu it opens next is moved). What's under the
/// pointer doesn't matter: in the strip there's only the desktop and Bloom's
/// dock, and a press on the dock opens no Explorer menu.
pub fn right_pressed(point: POINT) {
    PRESSED_AT.store(i64::MIN, Ordering::Relaxed);
    let Some(strip) = strip_at(point) else {
        return;
    };
    let before: Vec<isize> = explorer_windows(shell_pid())
        .into_iter()
        .map(|h| h.0 as isize)
        .collect();
    log(&format!(
        "right press at {},{} in the {strip:?} strip (desktop under it: {})",
        point.x,
        point.y,
        desktop_under(point)
    ));
    PRESSED_AT.store(crate::utils::get_now_ms(), Ordering::Relaxed);
    if let Ok(mut slot) = BEFORE.lock() {
        *slot = before;
    }
}

static BEFORE: std::sync::Mutex<Vec<isize>> = std::sync::Mutex::new(Vec::new());

/// The right button let go: the desktop opens its menu now. Follows it.
pub fn right_released(point: POINT) {
    let pressed = PRESSED_AT.swap(i64::MIN, Ordering::Relaxed);
    if pressed == i64::MIN || crate::utils::get_now_ms() - pressed > 3000 {
        return;
    }
    let Some(strip) = strip_at(point) else {
        log("released outside the strip: nothing to do");
        return;
    };
    if FOLLOWING.swap(true, Ordering::Relaxed) {
        log("already following a menu");
        return;
    }
    let before = BEFORE.lock().map(|b| b.clone()).unwrap_or_default();
    std::thread::spawn(move || {
        follow(point, strip, before);
        FOLLOWING.store(false, Ordering::Relaxed);
    });
}

/// Whether a window is a menu: a pop-up without a caption, of menu size.
fn menu_sized_popup(hwnd: HWND) -> bool {
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    if style & WS_POPUP.0 == 0 || style & WS_CAPTION.0 == WS_CAPTION.0 {
        return false;
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return false;
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    (40..=1200).contains(&width) && (40..=1600).contains(&height)
}

fn shown(hwnd: HWND) -> bool {
    unsafe { IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool() }
}

/// Keeps `menu` at the pointer while it's open (Windows may place it again
/// as it lays it out). Gives back when it closed.
fn hold_at_pointer(menu: HWND, point: POINT, strip: Strip, limit: Duration) {
    let started = Instant::now();
    while started.elapsed() < limit && shown(menu) {
        move_to_pointer(menu, point, strip);
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn follow(point: POINT, strip: Strip, mut before: Vec<isize>) {
    let shell = shell_pid();
    let new_windows = |before: &[isize]| -> Vec<HWND> {
        explorer_windows(shell)
            .into_iter()
            .filter(|h| !before.contains(&(h.0 as isize)))
            .collect()
    };
    // The desktop's menu, once it's open.
    let started = Instant::now();
    let menu = loop {
        let fresh = new_windows(&before);
        if let Some(menu) = fresh.iter().copied().find(|&h| menu_sized_popup(h)) {
            break menu;
        }
        for other in &fresh {
            log(&format!(
                "new Explorer window, not a menu: {}",
                describe(*other)
            ));
            before.push(other.0 as isize);
        }
        if started.elapsed() > Duration::from_millis(2000) {
            log("no menu came from Explorer");
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let placed = describe(menu);
    let moved = move_to_pointer(menu, point, strip);
    log(&format!(
        "desktop menu {placed} moved to the pointer at {},{} ({strip:?} strip): {moved}, now {}",
        point.x,
        point.y,
        describe(menu)
    ));
    // Windows lays the menu out as it opens: it's held at the pointer for a
    // moment, and checked while it stays open.
    hold_at_pointer(menu, point, strip, Duration::from_millis(600));
    // Everything open now is the menu's own (its sub-menus open from where
    // it is now). "Show more options" closes it and opens the classic menu
    // in the old place: that one is moved too.
    before.extend(explorer_windows(shell).into_iter().map(|h| h.0 as isize));
    let open_until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < open_until {
        std::thread::sleep(Duration::from_millis(40));
        if shown(menu) {
            move_to_pointer(menu, point, strip);
            before.extend(explorer_windows(shell).into_iter().map(|h| h.0 as isize));
            continue;
        }
        let closed_at = Instant::now();
        while closed_at.elapsed() < Duration::from_millis(1000) {
            if let Some(classic) = new_windows(&before)
                .into_iter()
                .find(|&h| class_of(h) == "#32768")
            {
                move_to_pointer(classic, point, strip);
                log(&format!(
                    "classic desktop menu moved to the pointer: {}",
                    describe(classic)
                ));
                hold_at_pointer(classic, point, strip, Duration::from_millis(300));
                return;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        return;
    }
}
