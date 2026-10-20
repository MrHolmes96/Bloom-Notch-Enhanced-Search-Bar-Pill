//! Snapped windows without Windows 11's border, as in Windows 10.
//!
//! Windows 11 draws a thin light border around windows, which shows most on
//! windows snapped side by side. Bloom turns it off on every window that is
//! snapped (`IsWindowArranged`: halves, thirds, quarters, Snap Layouts and
//! Snap Groups) with `DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE`, the setting
//! border tools use on other apps' windows, and gives it back the moment the
//! window is no longer snapped, and when Bloom quits or the setting is off.
//!
//! Low on resources by design: a window is looked at when it comes to the
//! front or a move ends (two system events), and the front window every
//! 300 ms (one quick call; keyboard snaps and Snap Layouts send no event of
//! their own). Settings > Windows > Snapped Window Borders
//! (`bloom-windows-snap-borders`).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use tauri::AppHandle;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumWindows, GetAncestor, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, IsWindow, IsWindowArranged, IsWindowVisible, KillTimer,
    PostThreadMessageW, SetTimer, TranslateMessage, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MOVESIZEEND, GA_ROOT, MSG, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_QUIT, WM_TIMER,
};

const KEY: &str = "bloom-windows-snap-borders";
/// How often the front window is looked at.
const POLL_MS: u32 = 300;

static RUNNING: AtomicBool = AtomicBool::new(false);
static THREAD: AtomicU32 = AtomicU32::new(0);
/// The windows whose border Bloom turned off.
static CHANGED: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// Turns the feature on or off as the setting says (on unless set to "false").
pub fn apply(app: &AppHandle) {
    if crate::utils::get_setting_str(app, KEY).as_deref() == Some("false") {
        stop();
    } else {
        start();
    }
}

fn set_border(hwnd: HWND, color: u32) {
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &color as *const u32 as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// Snapped windows lose the border; ones no longer snapped get it back.
fn check(hwnd: HWND) {
    unsafe {
        if hwnd.0.is_null() {
            return;
        }
        let Ok(mut changed) = CHANGED.lock() else {
            return;
        };
        let known = changed.iter().position(|&h| h == hwnd.0 as isize);
        if !IsWindow(Some(hwnd)).as_bool() {
            if let Some(index) = known {
                changed.remove(index);
            }
            return;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return;
        }
        let snapped = IsWindowVisible(hwnd).as_bool() && IsWindowArranged(hwnd).as_bool();
        match (snapped, known) {
            (true, None) => {
                set_border(hwnd, DWMWA_COLOR_NONE);
                changed.push(hwnd.0 as isize);
            }
            (false, Some(index)) => {
                set_border(hwnd, DWMWA_COLOR_DEFAULT);
                changed.remove(index);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 || child != 0 || hwnd.0.is_null() {
        return;
    }
    check(GetAncestor(hwnd, GA_ROOT));
}

unsafe extern "system" fn visit(hwnd: HWND, _: LPARAM) -> windows::core::BOOL {
    check(hwnd);
    true.into()
}

/// Gives every window Bloom changed its border back.
fn restore_all() {
    let windows: Vec<isize> = CHANGED
        .lock()
        .map(|mut changed| std::mem::take(&mut *changed))
        .unwrap_or_default();
    for hwnd in windows {
        let hwnd = HWND(hwnd as *mut _);
        if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            set_border(hwnd, DWMWA_COLOR_DEFAULT);
        }
    }
}

fn start() {
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("bloom-snap-borders".into())
        .spawn(|| unsafe {
            THREAD.store(GetCurrentThreadId(), Ordering::Release);
            let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
            let hooks = [
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(event),
                    0,
                    0,
                    flags,
                ),
                SetWinEventHook(
                    EVENT_SYSTEM_MOVESIZEEND,
                    EVENT_SYSTEM_MOVESIZEEND,
                    None,
                    Some(event),
                    0,
                    0,
                    flags,
                ),
            ];
            // Windows already snapped when Bloom starts.
            let _ = EnumWindows(Some(visit), LPARAM(0));
            let timer = SetTimer(None, 0, POLL_MS, None);
            let mut msg = MSG::default();
            while RUNNING.load(Ordering::Acquire) && GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_TIMER && msg.hwnd.0.is_null() {
                    check(GetForegroundWindow());
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = KillTimer(None, timer);
            for hook in hooks {
                if !hook.is_invalid() {
                    let _ = UnhookWinEvent(hook);
                }
            }
            THREAD.store(0, Ordering::Release);
            restore_all();
        });
    if spawned.is_err() {
        RUNNING.store(false, Ordering::Release);
    }
}

/// Stops watching and gives every window its border back. Called when the
/// setting is turned off and on every way Bloom quits.
pub fn stop() {
    if !RUNNING.swap(false, Ordering::AcqRel) {
        return;
    }
    let thread = THREAD.load(Ordering::Acquire);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    // Also here, at once, for a Bloom that exits right after.
    restore_all();
}
