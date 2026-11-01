//! The desktop's right-click menu, opened where the pointer is.
//!
//! The strip beside the dock (and beside the notch, when it keeps its place
//! at the top) is kept free for Bloom, so it's outside the area Windows
//! gives apps. A right click on the desktop there still opens the desktop's
//! menu, but Windows places that menu inside the apps' area, so it opened a
//! long way from the pointer, as if the old taskbar were still there. Moving
//! the menu after it opens doesn't work: Windows' menu (WinUI) puts itself
//! back.
//!
//! So Bloom takes that click over: a right click on the desktop in the
//! strip is held back from Windows, the apps' area is made the whole screen
//! for a moment, the same click is given to the desktop again, and once its
//! menu has been placed (at the pointer, since the strip is part of the
//! screen now) the apps' area is put back. Nothing is told to other apps in
//! between, so no window moves; whatever happens, the area is put back
//! within two seconds.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetShellWindow, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, IsWindowVisible, WindowFromPoint, GA_ROOT, GWL_STYLE,
};

/// Where the strip is: the bottom (beside the dock) or the top (beside a
/// notch that keeps its place).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Strip {
    Bottom,
    Top,
}

/// A right press on the desktop in a strip was held back from Windows.
static HELD_BACK: AtomicBool = AtomicBool::new(false);
/// The release that goes with it, waiting for the cursor worker.
static RELEASED_AT: AtomicI64 = AtomicI64::new(i64::MIN);
/// One click taken over at a time.
static OPENING: AtomicBool = AtomicBool::new(false);

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

/// From the mouse hook (it must answer at once): whether to hold this right
/// press back from Windows. Only a real (not Bloom-made) press on the desktop
/// in a strip is.
pub fn hold_back_press(point: POINT, injected: bool) -> bool {
    if injected || OPENING.load(Ordering::Relaxed) {
        return false;
    }
    let take = strip_at(point).is_some() && desktop_under(point);
    HELD_BACK.store(take, Ordering::Relaxed);
    take
}

/// From the mouse hook: whether to hold this right release back too (it
/// belongs to a press that was). Notes where, for `open_menu_pending`.
pub fn hold_back_release(point: POINT, injected: bool) -> bool {
    if injected || !HELD_BACK.swap(false, Ordering::Relaxed) {
        return false;
    }
    RELEASED_AT.store(
        ((point.x as u32 as u64) << 32 | point.y as u32 as u64) as i64,
        Ordering::Relaxed,
    );
    true
}

/// From the cursor worker: opens the desktop's menu for a click held back.
pub fn open_menu_pending() {
    let packed = RELEASED_AT.swap(i64::MIN, Ordering::Relaxed);
    if packed == i64::MIN {
        return;
    }
    let point = POINT {
        x: (packed as u64 >> 32) as u32 as i32,
        y: packed as u32 as i32,
    };
    if OPENING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(move || {
        open_at(point);
        OPENING.store(false, Ordering::Relaxed);
    });
}

fn set_work_area(area: &RECT) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_SETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut area = *area;
    unsafe {
        SystemParametersInfoW(
            SPI_SETWORKAREA,
            0,
            Some(&mut area as *mut RECT as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
    }
}

/// The same right click again, where the pointer is (marked as Bloom's, so
/// the mouse hook lets it through).
fn right_click() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };
    let event = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    unsafe {
        SendInput(
            &[event(MOUSEEVENTF_RIGHTDOWN), event(MOUSEEVENTF_RIGHTUP)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// The desktop's menu, once it's on screen with its size (WinUI's, or the
/// classic one).
fn menu_shown(shell: u32) -> Option<HWND> {
    explorer_windows(shell).into_iter().find(|&hwnd| {
        let class = class_of(hwnd);
        if class != "Microsoft.UI.Content.PopupWindowSiteBridge" && class != "#32768" {
            return false;
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok()
            && rect.right - rect.left > 40
            && rect.bottom - rect.top > 40
    })
}

fn open_at(point: POINT) {
    let Some(info) = monitor_of(point) else {
        right_click();
        return;
    };
    let (work, screen) = (info.rcWork, info.rcMonitor);
    let shell = shell_pid();
    let widened = set_work_area(&screen);
    log(&format!(
        "right click at {},{} taken over: apps' area made the whole screen: {widened}",
        point.x, point.y
    ));
    right_click();
    if !widened {
        return;
    }
    let started = Instant::now();
    let mut menu = None;
    while started.elapsed() < Duration::from_millis(1800) {
        std::thread::sleep(Duration::from_millis(10));
        if let Some(found) = menu_shown(shell) {
            menu = Some(found);
            break;
        }
    }
    // Placed as it gets its size; a moment more for it to settle.
    if menu.is_some() {
        std::thread::sleep(Duration::from_millis(120));
    }
    let restored = set_work_area(&work);
    log(&format!(
        "menu {} after {} ms; apps' area put back: {restored}",
        menu.map(describe).unwrap_or_else(|| "didn't open".into()),
        started.elapsed().as_millis()
    ));
}
