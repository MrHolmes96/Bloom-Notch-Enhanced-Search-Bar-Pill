//! Small changes to how other apps start, so they feel part of Bloom.
//!
//! Portals (desktop organizer): its loading pop-up (logo, title and loading
//! bar, a window titled "SplashScreen") is moved off the screen the moment
//! it is shown. It isn't hidden or closed: Portals goes on drawing it and
//! starting exactly as always, it just can't be seen. Portals and its portals
//! are untouched: only that one window is, found by its program, its kind of
//! window (Portals' "HwndWrapper[Portals.exe;…]") and its title.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;

/// Whether this is Portals' loading pop-up.
unsafe fn is_portals_splash(hwnd: HWND) -> bool {
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

unsafe extern "system" fn on_show(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, SetWindowPos, GA_ROOT, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE,
        SWP_NOZORDER,
    };
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() || GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }
    if is_portals_splash(hwnd) {
        // Asynchronous: never waits on Portals while it is busy starting.
        let _ = SetWindowPos(
            hwnd,
            None,
            -32000,
            -32000,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        );
    }
}

/// Starts watching for windows being shown (a thread of its own).
pub fn start() {
    let _ = std::thread::Builder::new()
        .name("bloom-app-tweaks".into())
        .spawn(|| unsafe {
            use windows::Win32::UI::Accessibility::SetWinEventHook;
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, TranslateMessage, EVENT_OBJECT_SHOW, MSG,
                WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
            };
            let _ = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                None,
                Some(on_show),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
}
