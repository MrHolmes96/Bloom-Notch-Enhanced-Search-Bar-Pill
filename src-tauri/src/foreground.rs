//! Letting the app the user picks in Bloom come to the front, as it does from
//! the taskbar.
//!
//! Windows only lets a process put a window in front when that process took
//! part in the last input. Clicks on the dock, the notch and the cards land in
//! their web view, which runs in a separate process, so Bloom itself does not
//! count: an app it opened or switched to stayed behind whatever was in front
//! (Portals, Explorer, the previous app) and only flashed.
//!
//! Supplying one empty input event makes Bloom the latest input provider, which
//! is what Windows checks. Bloom then hands that right on to the app with
//! `AllowSetForegroundWindow`, so a launched app, a tray icon's menu or a
//! notification's app can bring its own window forward. The event moves nothing
//! and presses nothing.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, ASFW_ANY, SW_RESTORE, SW_SHOW,
};

/// Lets the next window any app activates come to the front. Call right
/// before launching or activating something on the user's behalf.
pub fn grant() {
    unsafe {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT::default(),
            },
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// Grants foreground rights to one process (an icon's owner, a launched app).
pub fn grant_to(pid: u32) {
    grant();
    if pid != 0 {
        unsafe {
            let _ = AllowSetForegroundWindow(pid);
        }
    }
}

/// Shows, restores and activates a window, like clicking its taskbar button.
pub fn bring_to_front(hwnd: HWND) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    unsafe {
        if hwnd.is_invalid() {
            return false;
        }
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        grant();
        if SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd {
            return true;
        }
        // Windows still refused (a full-screen app holding the foreground):
        // share the foreground thread's input for a moment, which it accepts.
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let own_thread = GetCurrentThreadId();
        let attached = foreground_thread != 0
            && foreground_thread != own_thread
            && AttachThreadInput(own_thread, foreground_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let done = SetForegroundWindow(hwnd).as_bool();
        if attached {
            let _ = AttachThreadInput(own_thread, foreground_thread, false);
        }
        done
    }
}

/// True while the Start menu or Search has the foreground (it is hosted by
/// StartMenuExperienceHost, or SearchHost on newer builds).
fn start_menu_open() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(foreground, Some(&mut pid));
        crate::commands::process_image_path(pid).is_some_and(|path| {
            let file = path.rsplit('\\').next().unwrap_or("").to_lowercase();
            file == "startmenuexperiencehost.exe" || file == "searchhost.exe"
        })
    }
}

/// Closes the Start menu when it is open, and waits for it to be gone. As it
/// closes it hands the foreground back to the window before it (a full-screen
/// game, say), which would undo a switch made while it was still open.
pub fn leave_start_menu() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_ESCAPE,
    };
    if !start_menu_open() {
        return;
    }
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_ESCAPE,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    unsafe {
        SendInput(
            &[key(Default::default()), key(KEYEVENTF_KEYUP)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        if !start_menu_open() {
            // Its window hands the foreground on as it fades out.
            std::thread::sleep(std::time::Duration::from_millis(30));
            return;
        }
    }
}

fn process_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// A full-screen window that keeps itself on top of everything (some games
/// do) and still covers the window just brought forward: minimized, as when
/// switching away from a full-screen game, so the chosen window is seen.
unsafe fn step_aside(covering: HWND, target: HWND) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, GetWindowRect, GWL_EXSTYLE, SW_MINIMIZE, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    };
    if covering.is_invalid() || covering == target {
        return;
    }
    let owner = process_of(covering);
    if owner == 0 || owner == std::process::id() || owner == process_of(target) {
        return;
    }
    let style = GetWindowLongW(covering, GWL_EXSTYLE) as u32;
    // Overlays (click-through, see-through) are never the game itself.
    let overlay = style & WS_EX_TRANSPARENT.0 != 0 && style & WS_EX_LAYERED.0 != 0;
    if style & WS_EX_TOPMOST.0 == 0 || style & WS_EX_TOOLWINDOW.0 != 0 || overlay {
        return;
    }
    if !IsWindowVisible(covering).as_bool() || IsIconic(covering).as_bool() {
        return;
    }
    let monitor = MonitorFromWindow(covering, MONITOR_DEFAULTTONEAREST);
    if monitor != MonitorFromWindow(target, MONITOR_DEFAULTTONEAREST) {
        return;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let mut rect = RECT::default();
    if !GetMonitorInfoW(monitor, &mut info).as_bool() || GetWindowRect(covering, &mut rect).is_err()
    {
        return;
    }
    let screen = info.rcMonitor;
    let fills = rect.left <= screen.left
        && rect.top <= screen.top
        && rect.right >= screen.right
        && rect.bottom >= screen.bottom;
    if fills {
        let _ = ShowWindow(covering, SW_MINIMIZE);
    }
}

/// Switches to a window as its taskbar button does, also from over a
/// full-screen game: the Start menu (open from the Windows key) is closed
/// first, the switch is checked and made again for a moment in case the game
/// takes the foreground back, and a game that keeps itself on top of
/// everything steps aside.
pub fn switch_to(hwnd: HWND) -> bool {
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    /// Bumped by every switch, so an earlier one never undoes a later one.
    static SWITCHES: AtomicU64 = AtomicU64::new(0);
    if hwnd.is_invalid() {
        return false;
    }
    let this = SWITCHES.fetch_add(1, Ordering::SeqCst) + 1;
    leave_start_menu();
    let before = unsafe { GetForegroundWindow() };
    let target = process_of(hwnd);
    let mut done = bring_to_front(hwnd);
    let mut confirmed = 0;
    for delay in [50u64, 120, 250] {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        if SWITCHES.load(Ordering::SeqCst) != this {
            return done;
        }
        let now = unsafe { GetForegroundWindow() };
        if !now.is_invalid() && (now == hwnd || process_of(now) == target) {
            done = true;
            confirmed += 1;
            if confirmed == 2 {
                break;
            }
            continue;
        }
        // Taken back by the window before (the game) or the Start menu, not
        // by something the user picked meanwhile.
        if now.is_invalid() || now == before || start_menu_open() {
            confirmed = 0;
            done = bring_to_front(hwnd);
        } else {
            return done;
        }
    }
    unsafe { step_aside(before, hwnd) };
    done
}
