//! Small changes to how other apps start, so they feel part of Bloom.
//!
//! Portals (desktop organizer): its loading pop-up (logo, title and loading
//! bar, a window titled "SplashScreen") is kept off the screen for as long as
//! it exists, and isn't shown in the dock. It isn't hidden or closed: Portals
//! goes on drawing it and starting exactly as always, it just can't be seen.
//! Portals places the pop-up again while it starts (at sign-in especially),
//! so each time it comes back onto the screen it is moved off again. Portals
//! and its portals are untouched: only that one window is, found by its
//! program, its kind of window (Portals' "HwndWrapper[Portals.exe;…]") and its
//! title. What it did goes into app-tweaks.log (Export Diagnostics).

use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{UnhookWinEvent, HWINEVENTHOOK};

/// The loading pop-up being kept off the screen (0: none).
static SPLASH: AtomicIsize = AtomicIsize::new(0);
/// The hook following where that pop-up is placed (0: none).
static MOVE_HOOK: AtomicIsize = AtomicIsize::new(0);

/// Off the screen, where Windows itself parks minimized windows.
const AWAY: i32 = -32000;

fn log(line: &str) {
    use std::io::Write;
    let Some(path) = crate::utils::data_file("app-tweaks.log") else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 32 * 1024) {
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

/// Whether this is Portals' loading pop-up.
pub(crate) unsafe fn is_portals_splash(hwnd: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowTextW};
    let mut class = [0u16; 96];
    let class_len = GetClassNameW(hwnd, &mut class) as usize;
    let class = String::from_utf16_lossy(&class[..class_len.min(class.len())]);
    if !class
        .to_ascii_lowercase()
        .starts_with("hwndwrapper[portals.exe;")
    {
        return false;
    }
    let mut title = [0u16; 32];
    let title_len = GetWindowTextW(hwnd, &mut title) as usize;
    String::from_utf16_lossy(&title[..title_len.min(title.len())]) == "SplashScreen"
}

/// Moves the pop-up off the screen when it is on it. True when it moved it.
unsafe fn send_away(hwnd: HWND) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_ok() && rect.left <= AWAY + 100 {
        return false;
    }
    // Asynchronous: never waits on Portals while it is busy starting.
    SetWindowPos(
        hwnd,
        None,
        AWAY,
        AWAY,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
    )
    .is_ok()
}

/// Stops following the pop-up (it was closed).
unsafe fn stop_following() {
    SPLASH.store(0, Ordering::Relaxed);
    let hook = MOVE_HOOK.swap(0, Ordering::Relaxed);
    if hook != 0 {
        let _ = UnhookWinEvent(HWINEVENTHOOK(hook as *mut _));
    }
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    use windows::Win32::UI::Accessibility::SetWinEventHook;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetWindowThreadProcessId, EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE,
        EVENT_OBJECT_SHOW, GA_ROOT, WINEVENT_OUTOFCONTEXT,
    };
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    let following = SPLASH.load(Ordering::Relaxed);
    if event == EVENT_OBJECT_LOCATIONCHANGE {
        if hwnd.0 as isize == following && send_away(hwnd) {
            log("Portals placed its loading pop-up on the screen again: moved off it again");
        }
        return;
    }
    if event == EVENT_OBJECT_DESTROY {
        if hwnd.0 as isize == following {
            stop_following();
            log("Portals closed its loading pop-up");
        }
        return;
    }
    if event != EVENT_OBJECT_SHOW || GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }
    if !is_portals_splash(hwnd) {
        return;
    }
    let moved = send_away(hwnd);
    if hwnd.0 as isize != following {
        stop_following();
        SPLASH.store(hwnd.0 as isize, Ordering::Relaxed);
        // Where Portals places it from now on, followed for its process only.
        let mut pid = 0u32;
        let thread = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 && thread != 0 {
            let hook = SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(on_event),
                pid,
                thread,
                WINEVENT_OUTOFCONTEXT,
            );
            MOVE_HOOK.store(hook.0 as isize, Ordering::Relaxed);
        }
        log(&format!(
            "Portals showed its loading pop-up: {}",
            if moved {
                "moved off the screen"
            } else {
                "already off the screen"
            }
        ));
    }
}

/// Starts watching for windows being shown (a thread of its own).
pub fn start() {
    let _ = std::thread::Builder::new()
        .name("bloom-app-tweaks".into())
        .spawn(|| unsafe {
            use windows::Win32::UI::Accessibility::SetWinEventHook;
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, TranslateMessage, EVENT_OBJECT_DESTROY,
                EVENT_OBJECT_SHOW, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
            };
            for event in [EVENT_OBJECT_SHOW, EVENT_OBJECT_DESTROY] {
                let _ = SetWinEventHook(
                    event,
                    event,
                    None,
                    Some(on_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                );
            }
            // A pop-up already on screen when Bloom starts (Portals was first).
            catch_up();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
}

/// Treats a loading pop-up that is already shown as if it had just appeared.
unsafe fn catch_up() {
    use windows::core::BOOL;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, IsWindowVisible, EVENT_OBJECT_SHOW,
    };
    unsafe extern "system" fn each(hwnd: HWND, _: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd).as_bool() && is_portals_splash(hwnd) {
            on_event(
                HWINEVENTHOOK::default(),
                EVENT_OBJECT_SHOW,
                hwnd,
                0,
                0,
                0,
                0,
            );
        }
        BOOL(1)
    }
    let _ = EnumWindows(Some(each), LPARAM(0));
}
