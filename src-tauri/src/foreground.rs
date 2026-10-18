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
