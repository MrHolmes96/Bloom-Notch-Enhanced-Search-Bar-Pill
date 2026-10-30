use std::path::Path;
use std::sync::mpsc::{channel, sync_channel, Sender, SyncSender};
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicIsize, AtomicU8, Ordering},
    Mutex, OnceLock,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use windows::core::BOOL;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetWindowLongW, GetWindowThreadProcessId, IsWindowVisible, SetWindowsHookExW,
    GWL_EXSTYLE, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_MOUSEMOVE, WS_EX_TOOLWINDOW,
};

static KEYBOARD_HOOK_APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
/// Physical Win key state, tracked so Win+1-9 can be claimed while the Win key
/// itself keeps flowing to the shell (a lone Win tap must still open Start).
static WIN_KEY_DOWN: AtomicBool = AtomicBool::new(false);
/// Digit (1-9) of the currently held Win+Number combo, 0 when none. Key
/// auto-repeat re-fires the keydown; only the first press may toggle an app.
static WIN_NUMBER_HELD: AtomicU8 = AtomicU8::new(0);
/// Virtual key Microsoft documents as "unassigned", used as the mask key.
/// See `send_start_menu_mask`.
const MASK_VK: u16 = 0xE8;

/// Raw handle of the installed keyboard hook, 0 while none is installed.
static KEYBOARD_HOOK: AtomicIsize = AtomicIsize::new(0);

/// Win+1-9 is the only feature that needs a global keyboard hook: Explorer
/// already registers those hotkeys, so `RegisterHotKey` can't claim them.
/// Volume and brightness keys are left to Windows, and Bloom's overlays follow
/// the resulting changes instead. The hook only exists while Win+1-9 is in use
/// (the dock replaces the taskbar and the setting is on); the rest of the time
/// Bloom doesn't see the keyboard at all.
///
/// The hook has a thread of its own, which does nothing but answer it: every
/// key press anywhere waits for a low-level hook, so it must never wait for
/// Bloom's windows (on the main thread, busy with the notch and the dock),
/// or typing in every app would stall whenever Bloom is busy.
pub fn setup_keyboard_hook(app_handle: AppHandle) {
    let _ = KEYBOARD_HOOK_APP_HANDLE.set(app_handle);
    let _ = std::thread::Builder::new()
        .name("bloom-keyboard-hook".into())
        .spawn(|| unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, KillTimer, PeekMessageW, SetTimer, TranslateMessage,
                MSG, PM_NOREMOVE, WM_TIMER,
            };
            let mut message = MSG::default();
            // The thread's message queue, before the timer and the hook.
            let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
            // Wakes the thread to see whether the hook is still wanted.
            let timer = SetTimer(None, 0, 500, None);
            loop {
                // Exiting removes the hook with the process; never touch it
                // mid-shutdown.
                if SHUTTING_DOWN.load(Ordering::Relaxed) {
                    break;
                }
                set_keyboard_hook(
                    NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed) && dock_win_number_enabled(),
                );
                if GetMessageW(&mut message, None, 0, 0).0 <= 0 {
                    break;
                }
                if message.message != WM_TIMER {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            if timer != 0 {
                let _ = KillTimer(None, timer);
            }
        });
}

fn set_keyboard_hook(enabled: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{UnhookWindowsHookEx, HHOOK, WH_KEYBOARD_LL};
    let current = KEYBOARD_HOOK.load(Ordering::Relaxed);
    unsafe {
        if enabled && current == 0 {
            if let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) {
                KEYBOARD_HOOK.store(hook.0 as isize, Ordering::Relaxed);
            }
        } else if !enabled && current != 0 {
            let _ = UnhookWindowsHookEx(HHOOK(current as *mut _));
            KEYBOARD_HOOK.store(0, Ordering::Relaxed);
            WIN_KEY_DOWN.store(false, Ordering::Relaxed);
            WIN_NUMBER_HELD.store(0, Ordering::Relaxed);
        }
    }
}

/// Maps the top-row digit keys `1`-`9` to the zero-based dock slot.
fn win_number_index(vk: u16) -> Option<u8> {
    match vk {
        0x31..=0x39 => Some((vk - 0x31) as u8),
        _ => None,
    }
}

/// The dock's Win+Number replacement can be turned off in Settings > Dock.
fn dock_win_number_enabled() -> bool {
    let Some(app) = KEYBOARD_HOOK_APP_HANDLE.get() else {
        return true;
    };
    crate::utils::get_setting_str(app, "bloom-dock-win-number-enabled")
        .map(|v| v != "false")
        .unwrap_or(true)
}

/// Whether one of Bloom's HUD overlays is enabled (`bloom-volume-overlay-enabled`,
/// `bloom-brightness-overlay-enabled`). With an overlay off, the matching keys
/// and the native flyout are left to Windows instead of being taken over.
fn overlay_enabled(app: &AppHandle, key: &str) -> bool {
    crate::utils::get_setting_str(app, key)
        .map(|v| v != "false")
        .unwrap_or(true)
}

/// Physical Win state straight from the OS. The tracked flag can go stale when
/// a keyup is never delivered (secure desktop, keyboard unplugged, hook
/// timeout); without this check a stale flag would swallow digits forever.
fn win_key_physically_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LWIN, VK_RWIN};
    unsafe {
        (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0
    }
}

/// Explorer opens the Start menu when it only sees a Win keydown and keyup.
/// Swallowing a Win+Number combo would look exactly like that on Win release,
/// so a tap of an unassigned key is injected first — the same trick as
/// AutoHotkey's `#MenuMaskKey` — making the shell treat Win as a real modifier.
fn send_start_menu_mask() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    let mask = VIRTUAL_KEY(MASK_VK);
    let inputs = [
        INPUT {
            r#type: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: mask,
                    wScan: 0,
                    dwFlags: Default::default(),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: mask,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Hands the slot to the dock window, whose click handler already knows how to
/// focus a running window or launch a pinned app.
fn emit_dock_win_number(index: u8) {
    let Some(app) = KEYBOARD_HOOK_APP_HANDLE.get().cloned() else {
        return;
    };
    // Never block the input pipeline on WebView IPC.
    tauri::async_runtime::spawn(async move {
        let _ = app.emit_to("dock", "dock-win-number", index);
    });
}

unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_LWIN, VK_RWIN};
    use windows::Win32::UI::WindowsAndMessaging::{
        KBDLLHOOKSTRUCT, LLKHF_INJECTED, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };
    if code >= 0 {
        let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk_code = VIRTUAL_KEY(kb.vkCode as u16);
        let is_down = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
        let is_up = wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize;

        // Only physical presses drive the Win+Number replacement; injected
        // events (our own Start taps and mask key) must not re-enter it.
        if (kb.flags.0 & LLKHF_INJECTED.0) == 0 {
            if vk_code == VK_LWIN || vk_code == VK_RWIN {
                if is_down {
                    WIN_KEY_DOWN.store(true, Ordering::Relaxed);
                    // A fresh Win press always starts a fresh combo, even if the
                    // previous digit's keyup was missed.
                    WIN_NUMBER_HELD.store(0, Ordering::Relaxed);
                } else if is_up {
                    WIN_KEY_DOWN.store(false, Ordering::Relaxed);
                    WIN_NUMBER_HELD.store(0, Ordering::Relaxed);
                }
            } else if WIN_KEY_DOWN.load(Ordering::Relaxed) {
                if let Some(index) = win_number_index(vk_code.0) {
                    let slot = index + 1;
                    if is_up {
                        let _ = WIN_NUMBER_HELD.compare_exchange(
                            slot,
                            0,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        );
                    } else if is_down {
                        if !win_key_physically_down() {
                            WIN_KEY_DOWN.store(false, Ordering::Relaxed);
                            WIN_NUMBER_HELD.store(0, Ordering::Relaxed);
                        } else if NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed)
                            && dock_win_number_enabled()
                        {
                            if WIN_NUMBER_HELD.swap(slot, Ordering::Relaxed) != slot {
                                send_start_menu_mask();
                                emit_dock_win_number(index);
                            }
                            return windows::Win32::Foundation::LRESULT(1);
                        }
                    }
                }
            }
        }
    }
    windows::Win32::UI::WindowsAndMessaging::CallNextHookEx(None, code, wparam, lparam)
}

use crate::state::*;
use crate::types::*;
use crate::utils::*;

/// The hooks that put Explorer's taskbar straight back when it tries to show
/// or slide in (auto-hide). Scoped to Explorer's process: this event also
/// fires for the pointer on every mouse move and for every window in every
/// app, none of which this needs.
static TASKBAR_HOOKS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// Sets the taskbar hooks up again for the Explorer that is running now. Must
/// run on the UI thread, which receives the hooks' events.
pub fn setup_taskbar_hook() {
    unsafe {
        use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
        use windows::Win32::UI::WindowsAndMessaging::{
            EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, WINEVENT_OUTOFCONTEXT,
        };
        let Ok(mut hooks) = TASKBAR_HOOKS.lock() else {
            return;
        };
        for hook in hooks.drain(..) {
            let _ = UnhookWinEvent(HWINEVENTHOOK(hook as *mut _));
        }
        let Some(taskbar) = crate::tray_host::explorer_taskbar() else {
            // Set up once Explorer's taskbar is there (explorer_taskbar_changed).
            return;
        };
        let mut explorer = 0u32;
        GetWindowThreadProcessId(taskbar, Some(&mut explorer));
        if explorer == 0 {
            return;
        }
        for event in [EVENT_OBJECT_SHOW, EVENT_OBJECT_LOCATIONCHANGE] {
            let hook = SetWinEventHook(
                event,
                event,
                None,
                Some(taskbar_event_proc),
                explorer,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if !hook.is_invalid() {
                hooks.push(hook.0 as isize);
            }
        }
    }
}

static WINDOW_CHANGE_APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static LAST_WINDOW_CHANGE_MS: AtomicI64 = AtomicI64::new(0);
static WINDOW_CHANGE_PENDING: AtomicBool = AtomicBool::new(false);

pub fn setup_window_change_hook(app_handle: AppHandle) {
    unsafe {
        let _ = WINDOW_CHANGE_APP_HANDLE.set(app_handle);

        use windows::Win32::UI::Accessibility::SetWinEventHook;
        use windows::Win32::UI::WindowsAndMessaging::{
            EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW,
            WINEVENT_OUTOFCONTEXT,
        };

        // Hook create and destroy events for top-level windows
        let _create_hook = SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_CREATE,
            None,
            Some(window_change_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let _destroy_hook = SetWinEventHook(
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_DESTROY,
            None,
            Some(window_change_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        // Hook show/hide events (fires when windows become visible/invisible)
        let _show_hook = SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            None,
            Some(window_change_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let _hide_hook = SetWinEventHook(
            EVENT_OBJECT_HIDE,
            EVENT_OBJECT_HIDE,
            None,
            Some(window_change_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        // Apps that go to the tray by minimising or cloaking their window
        // (rather than hiding it) leave the dock just as quickly.
        let _minimize_hook = SetWinEventHook(
            windows::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_MINIMIZESTART,
            windows::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_MINIMIZEEND,
            None,
            Some(window_change_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let _cloak_hook = SetWinEventHook(
            windows::Win32::UI::WindowsAndMessaging::EVENT_OBJECT_CLOAKED,
            windows::Win32::UI::WindowsAndMessaging::EVENT_OBJECT_UNCLOAKED,
            None,
            Some(window_change_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );

        // A window dragged or resized: it may now be under the notch.
        let _move_hook = SetWinEventHook(
            windows::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_MOVESIZEEND,
            windows::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_MOVESIZEEND,
            None,
            Some(window_change_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );

        // Store hooks to prevent them from being dropped
        Box::leak(Box::new(_create_hook));
    }
}

unsafe extern "system" fn window_change_event_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _ms_event_time: u32,
) {
    if hwnd.0.is_null() {
        return;
    }
    if event == windows::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_MOVESIZEEND {
        // Moving a window changes nothing in the dock's list.
        NOTCH_RECHECK.store(true, Ordering::Relaxed);
        return;
    }

    // Explorer's taskbar shown (made at sign-in after Bloom started, or
    // again after Explorer restarted): hidden at once while Bloom's dock
    // replaces it, before the 2 s check that registers the dock again.
    if id_object == 0
        && id_child == 0
        && (event == windows::Win32::UI::WindowsAndMessaging::EVENT_OBJECT_SHOW
            || event == windows::Win32::UI::WindowsAndMessaging::EVENT_OBJECT_CREATE)
        && (NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed)
            || TASKBAR_TAKEOVER.load(Ordering::Relaxed))
    {
        let mut class_name = [0u8; 32];
        let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(hwnd, &mut class_name);
        let class = &class_name[..len as usize];
        if class == b"Shell_TrayWnd" || class == b"Shell_SecondaryTrayWnd" {
            let mut pid = 0u32;
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != std::process::id() && set_taskbar_visibility(false, false) {
                NATIVE_TASKBAR_HIDDEN.store(true, Ordering::Relaxed);
            }
        }
    }

    // A top-level window appeared/disappeared (or focus moved): capture UI
    // state may have changed. Cheap flag; the worker thread does the scan.
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW,
        };
        if id_object == 0
            && id_child == 0
            && (event == EVENT_OBJECT_SHOW
                || event == EVENT_OBJECT_HIDE
                || event == EVENT_OBJECT_DESTROY)
        {
            CAPTURE_RECHECK.store(true, Ordering::Relaxed);
        }
        if id_object == 0 && id_child == 0 {
            NOTCH_RECHECK.store(true, Ordering::Relaxed);
        }
    }

    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetWindowLongW, GetWindowThreadProcessId, IsWindow, GA_ROOT, GWL_EXSTYLE,
        WS_EX_TOOLWINDOW,
    };

    // Only whole windows matter to the dock: carets, cursors, menus' items and
    // the child controls inside apps (a browser makes hundreds) fire these
    // events too, and each one used to make the dock re-list every window.
    if id_object != 0 || id_child != 0 {
        return;
    }
    if !IsWindow(Some(hwnd)).as_bool() || GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }

    // Skip tool windows (docks, trays, etc.)
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
        return;
    }

    // Skip our own process
    let my_pid = std::process::id();
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == my_pid {
        return;
    }

    // Debounce bursts: closing a window (especially a UWP app) emits several
    // show/hide/destroy events within milliseconds. Every event stores its
    // timestamp and schedules an emit; only the newest one actually fires, so
    // the final state of a burst is always reported. Dropping the trailing
    // events would leave closed apps visible in the dock.
    // One waiting task covers a whole burst, instead of a task per event.
    let now = crate::utils::get_now_ms();
    LAST_WINDOW_CHANGE_MS.store(now, Ordering::Relaxed);
    if WINDOW_CHANGE_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Some(app_handle) = WINDOW_CHANGE_APP_HANDLE.get().cloned() {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                let quiet_for =
                    crate::utils::get_now_ms() - LAST_WINDOW_CHANGE_MS.load(Ordering::Relaxed);
                if quiet_for >= 190 {
                    break;
                }
            }
            // Cleared before emitting: an event arriving now starts a new wait.
            WINDOW_CHANGE_PENDING.store(false, Ordering::Release);
            let _ = app_handle.emit_to("dock", "windows-changed", ());
        });
    } else {
        WINDOW_CHANGE_PENDING.store(false, Ordering::Release);
    }
}

pub fn setup_thumbnail_capture(_app_handle: AppHandle) {
    unsafe {
        use windows::Win32::UI::Accessibility::SetWinEventHook;
        use windows::Win32::UI::WindowsAndMessaging::{
            EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
            WINEVENT_SKIPOWNPROCESS,
        };

        let shown_hook = SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            None,
            Some(window_shown_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        Box::leak(Box::new(shown_hook));

        let focus_hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(focus_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        Box::leak(Box::new(focus_hook));
    }
}

unsafe extern "system" fn focus_event_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _ms_event_time: u32,
) {
    if hwnd.0.is_null() {
        return;
    }
    // Foreground moved — e.g. Snipping Tool opened/closed or got minimised.
    CAPTURE_RECHECK.store(true, Ordering::Relaxed);
    NOTCH_RECHECK.store(true, Ordering::Relaxed);
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, IsWindow, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    // The app window that was in front just left it, while it is still on
    // screen: its picture is how it was left (skipped when only seconds old).
    // Taken once the switch has settled, and only if it stayed in the
    // background: a picture asks the app to draw itself, which must never
    // compete with it (or the app now in front) handling the switch.
    let previous = LAST_FRONT.load(Ordering::Relaxed);
    if previous != 0 && previous != hwnd.0 as isize {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));
            let window = HWND(previous as *mut _);
            if windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() != window {
                refresh_snapshot(window, 2000);
            }
        });
    }

    if !IsWindow(Some(hwnd)).as_bool() {
        return;
    }
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
        return;
    }

    let my_pid = std::process::id();
    let mut pid = 0u32;
    windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == my_pid {
        return;
    }

    let hwnd_raw = hwnd.0 as isize;
    LAST_FRONT.store(hwnd_raw, Ordering::Relaxed);
    watch_titles_of(pid);
    if let Some(map) = crate::state::FOCUS_TIMESTAMPS.get() {
        if let Ok(mut guard) = map.lock() {
            guard.insert(hwnd_raw, crate::utils::get_now_ms());
        }
    }
}

/// A window was shown: an app window that opens gets its one picture for the
/// static previews a moment later (once it has drawn itself), and again a
/// little later should that first one show nothing yet. Never afterwards.
unsafe extern "system" fn window_shown_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _ms_event_time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GA_ROOT};
    // The window itself (not a caret, cursor or child control), top-level.
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() || GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }
    if crate::window_capture::cached(hwnd).is_some() {
        return;
    }
    let raw = hwnd.0 as isize;
    let pending = OPENING
        .lock()
        .map(|mut list| {
            if list.contains(&raw) {
                false
            } else {
                list.push(raw);
                true
            }
        })
        .unwrap_or(false);
    if !pending {
        return;
    }
    std::thread::spawn(move || {
        for delay in [1500u64, 3000] {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            let hwnd = HWND(raw as *mut _);
            if take_once(hwnd) {
                break;
            }
        }
        if let Ok(mut list) = OPENING.lock() {
            list.retain(|&waiting| waiting != raw);
        }
    });
}

/// Windows waiting for their one picture.
static OPENING: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// The app window last in front (Bloom's own windows aside).
static LAST_FRONT: AtomicIsize = AtomicIsize::new(0);

/// The app window last in front (Bloom's own windows aside), 0 for none.
pub fn last_front_window() -> isize {
    LAST_FRONT.load(Ordering::Relaxed)
}

/// Takes a new picture of a window for the static previews, unless its
/// picture is younger than `fresh_ms` (see `take_once` for which windows).
pub fn refresh_snapshot(hwnd: HWND, fresh_ms: i64) {
    if crate::window_capture::age_ms(hwnd) < fresh_ms {
        return;
    }
    if SNAPSHOT_IN_FLIGHT.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = take_picture(hwnd);
    SNAPSHOT_IN_FLIGHT.store(false, Ordering::Release);
}

/// One picture is taken at a time.
static SNAPSHOT_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Takes a window's one picture for the static previews: an app window
/// (not minimized, full-screen like a game, untitled or one of Bloom's),
/// unless it already has one. True when it has a picture now (or never
/// will, so there is nothing to wait for).
fn take_once(hwnd: HWND) -> bool {
    crate::window_capture::cached(hwnd).is_some() || take_picture(hwnd)
}

/// Takes the picture of an app window (not minimized, untitled, full-screen
/// or one of Bloom's). True when it has one now, or never will.
fn take_picture(hwnd: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, GetWindowTextW, IsIconic, IsWindow, IsWindowVisible, GWL_EXSTYLE,
        WS_EX_TOOLWINDOW,
    };
    unsafe {
        if hwnd.is_invalid() || !IsWindow(Some(hwnd)).as_bool() {
            return true;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return true;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return true;
        }
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let mut text = [0u16; 2];
        if GetWindowTextW(hwnd, &mut text) == 0 {
            return false;
        }
    }
    if crate::utils::is_window_fullscreen(hwnd) {
        return true;
    }
    let size = crate::window_capture::PREVIEW_BOX;
    crate::window_capture::take(hwnd, size, size, false).is_some_and(|shot| shot.usable)
}

/// How long after a switch of the window in front its pictures wait.
const SETTLE_MS: u64 = 1500;

/// How long since the last key press or mouse input, in milliseconds.
pub fn idle_ms() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        GetTickCount().wrapping_sub(info.dwTime) as u64
    }
}

/// The window in front changed its title (a browser's tab, a document): its
/// picture is taken once the title has settled and nothing has been typed or
/// clicked for a moment (never while it's being used), at most every 15 s.
unsafe extern "system" fn title_changed_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _ms_event_time: u32,
) {
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    if windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() != hwnd {
        return;
    }
    let raw = hwnd.0 as isize;
    let serial = TITLE_SERIAL.fetch_add(1, Ordering::Relaxed) + 1;
    if TITLE_WAITING.swap(true, Ordering::AcqRel) {
        return;
    }
    std::thread::spawn(move || {
        // Until no further change for a second.
        let mut seen = serial;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let now = TITLE_SERIAL.load(Ordering::Relaxed);
            if now == seen {
                break;
            }
            seen = now;
        }
        let hwnd = HWND(raw as *mut _);
        // Up to 20 s for a pause in typing and clicking; once it is no longer
        // in front, the switch takes its picture instead.
        for _ in 0..20 {
            if windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() != hwnd {
                break;
            }
            if idle_ms() >= SETTLE_MS {
                refresh_snapshot(hwnd, 15_000);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        TITLE_WAITING.store(false, Ordering::Release);
    });
}

/// Title changes are only listened to in the app in front (Windows reports
/// every label change in every app otherwise). Runs on the hooks' thread.
unsafe fn watch_titles_of(pid: u32) {
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{EVENT_OBJECT_NAMECHANGE, WINEVENT_OUTOFCONTEXT};
    if TITLE_HOOK_PROCESS.swap(pid, Ordering::Relaxed) == pid {
        return;
    }
    let old = TITLE_HOOK.swap(0, Ordering::Relaxed);
    if old != 0 {
        let _ = UnhookWinEvent(HWINEVENTHOOK(old as *mut _));
    }
    let hook = SetWinEventHook(
        EVENT_OBJECT_NAMECHANGE,
        EVENT_OBJECT_NAMECHANGE,
        None,
        Some(title_changed_proc),
        pid,
        0,
        WINEVENT_OUTOFCONTEXT,
    );
    TITLE_HOOK.store(hook.0 as isize, Ordering::Relaxed);
}

static TITLE_HOOK: AtomicIsize = AtomicIsize::new(0);
static TITLE_HOOK_PROCESS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static TITLE_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static TITLE_WAITING: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn taskbar_event_proc(
    _h_win_event_hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _dw_event_thread: u32,
    _dwms_event_time: u32,
) {
    // Only windows themselves (not the cursor or a caret, which fire this
    // event on every mouse move).
    if _id_object != 0 || _id_child != 0 || hwnd.0.is_null() {
        return;
    }
    if NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed) {
        let mut class_name = [0u8; 256];
        let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(hwnd, &mut class_name);
        let class_str = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("");

        if class_str == "Shell_TrayWnd" || class_str == "Shell_SecondaryTrayWnd" {
            // Taskbar is trying to show or move: slap it back down.
            set_taskbar_visibility(false, false);
        }
    }
}

pub fn setup_audio_visualization(app_handle: AppHandle) {
    std::thread::spawn(move || {
        use windows::Win32::Media::Audio::{
            eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
            AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
        };
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        };
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            const NUM_BANDS: usize = 5;
            let mut max_band_energies = [0.01f32; NUM_BANDS];
            let mut prev_values = [0.1f32; NUM_BANDS];

            loop {
                let _: Result<(), String> = (|| {
                    let enumerator: IMMDeviceEnumerator = CoCreateInstance(
                        &windows::Win32::Media::Audio::MMDeviceEnumerator,
                        None,
                        CLSCTX_ALL,
                    )
                    .map_err(|e| format!("CoCreateInstance failed: {:?}", e))?;

                    let device: IMMDevice =
                        enumerator
                            .GetDefaultAudioEndpoint(eRender, eConsole)
                            .map_err(|e| format!("GetDefaultAudioEndpoint failed: {:?}", e))?;

                    let current_id_str = if let Ok(id) = device.GetId() {
                        let id_str = windows::core::PCWSTR::from_raw(id.0)
                            .to_string()
                            .unwrap_or_default();
                        CoTaskMemFree(Some(id.0 as *const _));
                        id_str
                    } else {
                        String::new()
                    };

                    let audio_client: IAudioClient = device
                        .Activate(CLSCTX_ALL, None)
                        .map_err(|e| format!("Activate failed: {:?}", e))?;

                    let format_ptr = audio_client
                        .GetMixFormat()
                        .map_err(|e| format!("GetMixFormat failed: {:?}", e))?;

                    let channels = (*format_ptr).nChannels as usize;
                    let bits_per_sample = (*format_ptr).wBitsPerSample;
                    let bytes_per_sample = (bits_per_sample / 8) as usize;

                    if bytes_per_sample == 0 || channels == 0 || bytes_per_sample > 4 {
                        CoTaskMemFree(Some(format_ptr as *const _));
                        return Err(format!(
                            "Invalid audio format: channels={}, bits={}",
                            channels, bits_per_sample
                        ));
                    }

                    let buffer_duration = 10_000_000i64;
                    audio_client
                        .Initialize(
                            AUDCLNT_SHAREMODE_SHARED,
                            AUDCLNT_STREAMFLAGS_LOOPBACK,
                            buffer_duration,
                            0,
                            format_ptr,
                            Some(std::ptr::null()),
                        )
                        .map_err(|e| format!("Initialize failed: {:?}", e))?;

                    CoTaskMemFree(Some(format_ptr as *const _));

                    let capture_client: IAudioCaptureClient = audio_client
                        .GetService()
                        .map_err(|e| format!("GetService failed: {:?}", e))?;

                    audio_client
                        .Start()
                        .map_err(|e| format!("Start failed: {:?}", e))?;

                    const FFT_SIZE: usize = 512;
                    let mut fft_buffer = vec![0.0f32; FFT_SIZE];
                    let mut fft_input: Vec<rustfft::num_complex::Complex<f32>> =
                        vec![rustfft::num_complex::Complex::new(0.0, 0.0); FFT_SIZE];
                    let mut buffer_pos = 0;

                    let mut planner = rustfft::FftPlanner::<f32>::new();
                    let fft = planner.plan_fft_forward(FFT_SIZE);

                    let mut last_device_check = std::time::Instant::now();
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(32));

                        if last_device_check.elapsed().as_secs() >= 2 {
                            last_device_check = std::time::Instant::now();
                            if let Ok(new_device) =
                                enumerator.GetDefaultAudioEndpoint(eRender, eConsole)
                            {
                                if let Ok(new_id) = new_device.GetId() {
                                    let new_str = windows::core::PCWSTR::from_raw(new_id.0)
                                        .to_string()
                                        .unwrap_or_default();
                                    CoTaskMemFree(Some(new_id.0 as *const _));
                                    if current_id_str != new_str {
                                        return Err("Default device changed".into());
                                    }
                                }
                            }
                        }

                        // Skip all heavy processing if nothing is playing
                        if !ANY_MEDIA_PLAYING.load(Ordering::Relaxed) {
                            // Still gotta clear the buffer to avoid lag when it starts
                            loop {
                                let len = match capture_client.GetNextPacketSize() {
                                    Ok(l) => l,
                                    Err(_) => {
                                        return Err("Device invalidated".into());
                                    }
                                };
                                if len == 0 {
                                    break;
                                }
                                let mut data_ptr: *mut u8 = std::ptr::null_mut();
                                let mut num_frames = 0u32;
                                let mut flags = 0u32;
                                if capture_client
                                    .GetBuffer(
                                        &mut data_ptr,
                                        &mut num_frames,
                                        &mut flags,
                                        None,
                                        None,
                                    )
                                    .is_err()
                                {
                                    return Err("Device invalidated".into());
                                }
                                let _ = capture_client.ReleaseBuffer(num_frames);
                            }
                            continue;
                        }

                        let mut device_invalidated = false;
                        // A window of audio is ready ~94 times a second; the
                        // notch (the only window showing it) gets the newest
                        // one once per pass, ~30 a second, and eases between.
                        let mut latest_bands: Option<[f32; NUM_BANDS]> = None;
                        loop {
                            let packet_length = match capture_client.GetNextPacketSize() {
                                Ok(len) => len,
                                Err(_) => {
                                    device_invalidated = true;
                                    break;
                                }
                            };
                            if packet_length == 0 {
                                break;
                            }
                            let mut data_ptr: *mut u8 = std::ptr::null_mut();
                            let mut num_frames = 0u32;
                            let mut flags = 0u32;

                            if capture_client
                                .GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
                                .is_err()
                            {
                                device_invalidated = true;
                                break;
                            }

                            if !data_ptr.is_null()
                                && num_frames > 0
                                && bytes_per_sample > 0
                                && channels > 0
                            {
                                let stride = channels * bytes_per_sample;
                                for frame in 0..num_frames as usize {
                                    let frame_offset = frame * stride;
                                    let mut sample_val: i64 = 0;
                                    for ch in 0..channels {
                                        let sample_offset = frame_offset + ch * bytes_per_sample;
                                        let sample: i64 = match bytes_per_sample {
                                            2 => {
                                                *(data_ptr.add(sample_offset) as *const i16) as i64
                                            }
                                            4 => {
                                                *(data_ptr.add(sample_offset) as *const i32) as i64
                                            }
                                            _ => 0,
                                        };
                                        sample_val = sample_val.wrapping_add(sample);
                                    }
                                    sample_val /= channels as i64;
                                    let normalized = match bytes_per_sample {
                                        2 => (sample_val as f32) / 32768.0,
                                        4 => (sample_val as f32) / 2147483648.0,
                                        _ => 0.0,
                                    };
                                    if buffer_pos < FFT_SIZE {
                                        let window = 0.5
                                            * (1.0
                                                - (2.0 * std::f32::consts::PI * buffer_pos as f32
                                                    / FFT_SIZE as f32)
                                                    .cos());
                                        fft_buffer[buffer_pos] = normalized * window;
                                        buffer_pos += 1;
                                    }
                                    if buffer_pos >= FFT_SIZE {
                                        // Run FFT on the windowed buffer
                                        for (i, &sample) in fft_buffer.iter().enumerate() {
                                            fft_input[i] =
                                                rustfft::num_complex::Complex::new(sample, 0.0);
                                        }
                                        fft.process(&mut fft_input);

                                        let band_ranges =
                                            [(1, 2), (2, 6), (6, 18), (18, 60), (60, 200)];
                                        let mut output = [0.0f32; NUM_BANDS];
                                        for (band_idx, &(bin_start, bin_end)) in
                                            band_ranges.iter().enumerate()
                                        {
                                            let mut total_mag = 0.0f32;
                                            let mut count = 0u32;
                                            for bin in bin_start..bin_end {
                                                if bin >= FFT_SIZE / 2 {
                                                    break;
                                                }
                                                let mag = fft_input[bin].norm();
                                                total_mag += mag;
                                                count += 1;
                                            }
                                            let avg_mag = total_mag / count.max(1) as f32;
                                            let mut scaled_mag = avg_mag;
                                            let weighting = [1.2, 1.2, 1.5, 2.8, 5.0];
                                            scaled_mag *= weighting[band_idx];
                                            if scaled_mag > max_band_energies[band_idx] {
                                                max_band_energies[band_idx] = scaled_mag;
                                            } else {
                                                max_band_energies[band_idx] *= 0.99;
                                            }
                                            let target = (scaled_mag
                                                / max_band_energies[band_idx].max(0.12))
                                            .min(1.0)
                                            .powf(0.75);
                                            let is_rising = target > prev_values[band_idx];
                                            let smooth_factor = if is_rising { 0.10 } else { 0.20 };
                                            output[band_idx] = prev_values[band_idx]
                                                * smooth_factor
                                                + target * (1.0 - smooth_factor);
                                            output[band_idx] = output[band_idx].clamp(0.18, 1.0);
                                            prev_values[band_idx] = output[band_idx];
                                        }
                                        latest_bands = Some(output);
                                        buffer_pos = 0;
                                    }
                                }
                                let _ = capture_client.ReleaseBuffer(num_frames);
                            }
                        }

                        if let Some(bands) = latest_bands {
                            let _ = app_handle.emit_to(
                                "main",
                                "audio-visualization",
                                AudioVisualizationData {
                                    frequencies: bands.to_vec(),
                                },
                            );
                        }
                        if device_invalidated {
                            return Err("Device invalidated".into());
                        }
                    }
                })();

                // Wait before retrying (increased to 2500ms to allow Windows to fully update default endpoint)
                std::thread::sleep(std::time::Duration::from_millis(2500));
            }
            // if com_initialized { CoUninitialize(); }
        }
    });
}

pub fn setup_system_worker(app_handle: AppHandle) -> Sender<SystemCommand> {
    let (tx, rx) = channel::<SystemCommand>();
    let handle_system = app_handle.clone();
    std::thread::spawn(move || {
        use base64::{engine::general_purpose, Engine as _};
        use windows::Media::Control::{
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        };
        use windows::Storage::Streams::DataReader;
        use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
        use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
        };

        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator = CoCreateInstance::<_, IMMDeviceEnumerator>(
                &windows::Win32::Media::Audio::MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            )
            .ok();
            let mut device = enumerator
                .as_ref()
                .and_then(|e| e.GetDefaultAudioEndpoint(eRender, eConsole).ok());
            let mut audio_endpoint_volume: Option<IAudioEndpointVolume> = device
                .as_ref()
                .and_then(|d| d.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok());

            let mut current_device_id = String::new();
            if let Some(ref d) = device {
                if let Ok(id) = d.GetId() {
                    current_device_id = windows::core::PCWSTR::from_raw(id.0)
                        .to_string()
                        .unwrap_or_default();
                    CoTaskMemFree(Some(id.0 as *const _));
                }
            }

            let mut manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                .and_then(|op| op.get())
                .ok();
            let mut last_processed_media = std::time::Instant::now();
            let mut last_device_check = std::time::Instant::now();
            #[allow(clippy::type_complexity)]
            let mut last_emitted_info: Option<(
                String,
                String,
                bool,
                bool,
                Option<String>,
                i64,
                i64,
            )> = None;
            let mut last_volume: f32 = -1.0;
            let mut last_muted: bool = false;
            let mut waiting: Option<SystemCommand> = None;

            // Only hide the native flyout when Bloom's matching overlay replaces it.
            let hide_osd = |overlay_key: &str| {
                if !overlay_enabled(&handle_system, overlay_key) {
                    return;
                }
                use windows::Win32::UI::WindowsAndMessaging::{FindWindowA, ShowWindow, SW_HIDE};
                let class1 = windows::core::PCSTR(c"NativeHWNDHost".as_ptr() as *const u8);
                if let Ok(hwnd1) = FindWindowA(class1, windows::core::PCSTR::null()) {
                    let _ = ShowWindow(hwnd1, SW_HIDE);
                }
            };

            loop {
                // Check for device change
                if last_device_check.elapsed().as_secs() >= 2 {
                    last_device_check = std::time::Instant::now();
                    if let Some(ref enum_ref) = enumerator {
                        if let Ok(new_device) = enum_ref.GetDefaultAudioEndpoint(eRender, eConsole)
                        {
                            if let Ok(id) = new_device.GetId() {
                                let new_id = windows::core::PCWSTR::from_raw(id.0)
                                    .to_string()
                                    .unwrap_or_default();
                                CoTaskMemFree(Some(id.0 as *const _));
                                if new_id != current_device_id || audio_endpoint_volume.is_none() {
                                    current_device_id = new_id;
                                    device = Some(new_device);
                                    audio_endpoint_volume = device.as_ref().and_then(|d| {
                                        d.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
                                    });
                                    // Reset last_volume to force an update event
                                    last_volume = -1.0;
                                }
                            }
                        }
                    }
                }

                if manager.is_none() {
                    manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                        .and_then(|op| op.get())
                        .ok();
                }
                while let Some(cmd) = waiting.take().or_else(|| rx.try_recv().ok()) {
                    // Only the volume commands need the audio endpoint. Media, brightness and
                    // visibility commands must keep working when there is no output device.
                    match (cmd, audio_endpoint_volume.as_ref()) {
                        (SystemCommand::SetVolume(volume), Some(aev)) => {
                            let _ = aev.SetMasterVolumeLevelScalar(
                                volume.clamp(0.0, 1.0),
                                std::ptr::null(),
                            );
                            if volume > 0.0 {
                                let _ = aev.SetMute(false, std::ptr::null());
                            }
                            hide_osd("bloom-volume-overlay-enabled");
                        }
                        (SystemCommand::SetVolume(_), None) => {}
                        (SystemCommand::MediaPlayPause, _) => {
                            if let Some(ref mgr) = manager {
                                if let Ok(session) = mgr.GetCurrentSession() {
                                    let _ = session.TryTogglePlayPauseAsync();
                                }
                            }
                        }
                        (SystemCommand::MediaNext, _) => {
                            if let Some(ref mgr) = manager {
                                if let Ok(session) = mgr.GetCurrentSession() {
                                    let _ = session.TrySkipNextAsync();
                                }
                            }
                        }
                        (SystemCommand::MediaPrevious, _) => {
                            if let Some(ref mgr) = manager {
                                if let Ok(session) = mgr.GetCurrentSession() {
                                    let _ = session.TrySkipPreviousAsync();
                                }
                            }
                        }
                        (SystemCommand::MediaSeek(position_ms), _) => {
                            if let Some(ref mgr) = manager {
                                if let Ok(session) = mgr.GetCurrentSession() {
                                    let ticks = position_ms * 10_000;
                                    let _ = session.TryChangePlaybackPositionAsync(ticks);
                                }
                            }
                        }
                    }
                }
                if let Some(ref aev) = audio_endpoint_volume {
                    if let (Ok(vol), Ok(muted)) = (aev.GetMasterVolumeLevelScalar(), aev.GetMute())
                    {
                        let is_muted: bool = muted.into();
                        if (vol - last_volume).abs() > 0.001 || is_muted != last_muted {
                            last_volume = vol;
                            last_muted = is_muted;
                            crate::state::CURRENT_VOLUME
                                .store((vol * 100.0) as u32, Ordering::Relaxed);
                            let _ = handle_system.emit(
                                "volume-change",
                                VolumeChangeEvent {
                                    volume: vol,
                                    is_muted,
                                },
                            );
                            hide_osd("bloom-volume-overlay-enabled");
                        }
                    }
                }
                if last_processed_media.elapsed().as_millis() >= 2000 {
                    last_processed_media = std::time::Instant::now();
                    let mut best_info: Option<MediaInfo> = None;
                    if let Some(ref mgr) = manager {
                        if let Ok(sessions) = mgr.GetSessions() {
                            for i in 0..sessions.Size().unwrap_or(0) {
                                if let Ok(session) = sessions.GetAt(i) {
                                    let is_playing = session
										.GetPlaybackInfo()
										.ok()
										.and_then(|p| p.PlaybackStatus().ok())
										== Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
                                    if let Ok(props) =
                                        session.TryGetMediaPropertiesAsync().and_then(|op| op.get())
                                    {
                                        let title = props.Title().unwrap_or_default().to_string();
                                        if !title.is_empty() {
                                            let artist =
                                                props.Artist().unwrap_or_default().to_string();
                                            let mut artwork = None;
                                            let mut got_art_from_cache = false;
                                            if let Some((
                                                ref last_title,
                                                ref last_artist,
                                                _,
                                                _,
                                                ref last_art,
                                                _,
                                                _,
                                            )) = last_emitted_info
                                            {
                                                if last_title == &title && last_artist == &artist {
                                                    artwork = last_art
                                                        .as_ref()
                                                        .map(|art| vec![art.clone()]);
                                                    got_art_from_cache = true;
                                                }
                                            }

                                            if !got_art_from_cache {
                                                artwork = (|| -> Option<Vec<String>> {
                                                    let stream = props
                                                        .Thumbnail()
                                                        .ok()?
                                                        .OpenReadAsync()
                                                        .ok()?
                                                        .get()
                                                        .ok()?;
                                                    let content_type = stream
                                                        .ContentType()
                                                        .ok()?
                                                        .to_string()
                                                        .split(',')
                                                        .next()
                                                        .unwrap_or("image/jpeg")
                                                        .trim()
                                                        .to_string();
                                                    let reader =
                                                        DataReader::CreateDataReader(&stream)
                                                            .ok()?;
                                                    let mut all_bytes = Vec::new();
                                                    let chunk_size = 65536u32;
                                                    loop {
                                                        let loaded = reader
                                                            .LoadAsync(chunk_size)
                                                            .ok()?
                                                            .get()
                                                            .ok()?;
                                                        if loaded == 0 {
                                                            break;
                                                        }
                                                        let mut chunk = vec![0u8; loaded as usize];
                                                        reader.ReadBytes(&mut chunk).ok()?;
                                                        all_bytes.extend_from_slice(&chunk);
                                                    }
                                                    if all_bytes.is_empty() {
                                                        return None;
                                                    }
                                                    Some(vec![format!(
                                                        "data:{};base64,{}",
                                                        content_type,
                                                        general_purpose::STANDARD.encode(all_bytes)
                                                    )])
                                                })(
                                                );
                                            }

                                            // Extract timeline properties for progress bar
                                            let (position_ms, duration_ms, seek_enabled) = {
                                                let defaults = (0i64, 0i64, false);
                                                match session.GetTimelineProperties() {
                                                    Ok(timeline) => {
                                                        let start = timeline
                                                            .StartTime()
                                                            .map(|t| t.Duration / 10_000)
                                                            .unwrap_or(0);
                                                        let end = timeline
                                                            .EndTime()
                                                            .map(|t| t.Duration / 10_000)
                                                            .unwrap_or(0);
                                                        let pos = timeline
                                                            .Position()
                                                            .map(|t| t.Duration / 10_000)
                                                            .unwrap_or(0);
                                                        let dur = (end - start).max(0);
                                                        // If duration is 0 but position > 0, some players don't report start/end
                                                        // but still track position — use position as fallback duration indicator
                                                        let effective_dur =
                                                            if dur > 0 { dur } else { 0 };
                                                        // Check if seeking is supported
                                                        let seek = session
                                                            .GetPlaybackInfo()
                                                            .ok()
                                                            .and_then(|pi| pi.Controls().ok())
                                                            .map(|c| {
                                                                c.IsPlaybackPositionEnabled()
                                                                    .unwrap_or(false)
                                                            })
                                                            .unwrap_or(false);
                                                        (pos, effective_dur, seek)
                                                    }
                                                    Err(_) => defaults,
                                                }
                                            };

                                            let now_ms = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .map(|d| d.as_millis() as u64)
                                                .unwrap_or(0);
                                            let info = MediaInfo {
                                                title,
                                                artist,
                                                is_playing,
                                                has_media: true,
                                                artwork,
                                                position_ms,
                                                duration_ms,
                                                seek_enabled,
                                                position_updated_at: now_ms,
                                            };
                                            if is_playing {
                                                best_info = Some(info);
                                                break;
                                            } else if best_info.is_none() {
                                                best_info = Some(info);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let current = best_info.unwrap_or(MediaInfo {
                        title: "".into(),
                        artist: "".into(),
                        is_playing: false,
                        has_media: false,
                        artwork: None,
                        position_ms: 0,
                        duration_ms: 0,
                        seek_enabled: false,
                        position_updated_at: 0,
                    });
                    let art_str = current.artwork.as_ref().and_then(|a| a.first()).cloned();
                    if last_emitted_info.as_ref().is_none_or(|(t, a, p, h, art, pos, _dur)| {
                        t != &current.title || a != &current.artist || p != &current.is_playing || h != &current.has_media || art != &art_str ||
                        // For position: emit if >1s difference (avoid spamming on every poll)
                        (*pos - current.position_ms).abs() > 1000
                    }) {
                        let _ = handle_system.emit("media-update", current.clone());
                        ANY_MEDIA_PLAYING.store(current.is_playing, Ordering::Relaxed);
                        last_emitted_info = Some((current.title, current.artist, current.is_playing, current.has_media, art_str, current.position_ms, current.duration_ms));
                    }
                }
                // Woken at once by a command (play/pause, volume), else after
                // 64 ms for the volume and media checks.
                match rx.recv_timeout(std::time::Duration::from_millis(64)) {
                    Ok(cmd) => waiting = Some(cmd),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        std::thread::sleep(std::time::Duration::from_millis(64))
                    }
                }
            }
        }
    });

    crate::brightness::start(app_handle.clone());

    let handle_visibility = app_handle.clone();
    std::thread::spawn(move || {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowRect, IsIconic, IsZoomed,
        };
        let mut last_visible = true;
        let mut last_visibility_said = Instant::now();
        let mut last_dock_overlap: Option<bool> = None;
        let mut last_notch_overlap: Option<bool> = None;
        let mut last_dock_maximized: Option<bool> = None;
        let mut last_fg_maximized = false;
        let mut last_hwnd = HWND(std::ptr::null_mut());
        let mut last_emit = Instant::now();
        let mut is_known_shell = false;
        let my_process_id = std::process::id();
        let mut last_monitor_update = Instant::now() - Duration::from_secs(5);
        let mut cached_scale = 1.0f64;
        // Re-check the native flyout every ~1.5s: picks up setting changes and
        // a host window recreated by an Explorer restart.
        let mut osd_sync_ticks = 0u32;
        // Whether an app window lies under the notch, looked at again when
        // windows change (and at least every second).
        let mut strip_covered = false;
        let mut strip_checked = Instant::now() - Duration::from_secs(5);
        // Explorer's taskbar window, looked at every ~2 s: a new one means
        // Explorer (re)started and the dock and notch must register again.
        let mut explorer_taskbar = crate::tray_host::explorer_taskbar()
            .map(|h| h.0 as isize)
            .unwrap_or(0);
        let mut explorer_checked = Instant::now();

        loop {
            if SHUTTING_DOWN.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(150));
                continue;
            }
            unsafe {
                let now = Instant::now();
                if now.duration_since(last_monitor_update) > Duration::from_millis(1000) {
                    if let Some(m) = handle_visibility.primary_monitor().ok().flatten() {
                        cached_scale = m.scale_factor();
                        last_monitor_update = now;
                    }
                }
                let mut hwnd = GetForegroundWindow();

                // Find the first meaningful window for overlap detection.
                // We skip Bloom windows, invisible windows, minimized windows, and 'cloaked' system ghosts.
                let mut check_count = 0;
                while !hwnd.is_invalid() && check_count < 15 {
                    let mut process_id = 0u32;
                    GetWindowThreadProcessId(hwnd, Some(&mut process_id));

                    let mut class_name = [0u8; 256];
                    let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(
                        hwnd,
                        &mut class_name,
                    );
                    let class_str = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("");

                    // A menu (a tray icon's, opened from the notch, or any
                    // other) is not a window the notch should step aside for.
                    let is_bloom = process_id == my_process_id
                        || class_str.contains("Bloom")
                        || class_str == "#32768"
                        || crate::tray_host::is_open_menu(hwnd.0 as isize);
                    let is_visible = IsWindowVisible(hwnd).as_bool();
                    let is_iconic = IsIconic(hwnd).as_bool();

                    let mut cloaked = 0u32;
                    let is_cloaked = DwmGetWindowAttribute(
                        hwnd,
                        windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
                        &mut cloaked as *mut _ as *mut _,
                        4,
                    )
                    .is_ok()
                        && cloaked != 0;

                    let mut rect = RECT::default();
                    let has_valid_rect = GetWindowRect(hwnd, &mut rect).is_ok()
                        && (rect.right - rect.left) > 0
                        && (rect.bottom - rect.top) > 0;

                    if is_bloom || !is_visible || is_iconic || is_cloaked || !has_valid_rect {
                        hwnd = windows::Win32::UI::WindowsAndMessaging::GetWindow(
                            hwnd,
                            windows::Win32::UI::WindowsAndMessaging::GW_HWNDNEXT,
                        )
                        .unwrap_or_default();
                        check_count += 1;
                    } else {
                        break;
                    }
                }

                let mut should_overlap = false;
                let mut should_notch_overlap = false;
                let mut should_maximized = false;
                let mut current_is_fs = false;

                // Cheap check every tick: the foreground hwnd doesn't change when
                // the user clicks maximize, so without this the adaptive dock
                // would wait for the 3s fallback recompute to react.
                let fg_is_maximized = !hwnd.is_invalid() && IsZoomed(hwnd).as_bool();

                if !hwnd.is_invalid()
                    && (hwnd != last_hwnd
                        || fg_is_maximized != last_fg_maximized
                        || last_emit.elapsed() >= Duration::from_secs(3))
                {
                    last_hwnd = hwnd;
                    last_fg_maximized = fg_is_maximized;
                    let mut class_name = [0u8; 256];
                    let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(
                        hwnd,
                        &mut class_name,
                    );
                    let class_str = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("");
                    let mut text = [0u16; 512];
                    let text_len =
                        windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut text);
                    let title = String::from_utf16_lossy(&text[..text_len as usize]);

                    let is_desktop = class_str == "Progman" || class_str == "WorkerW";
                    let is_start = (class_str == "Windows.UI.Core.CoreWindow"
                        || class_str == "SimpleWindow")
                        && (title == "Start" || title == "Search");
                    let is_shell = class_str == "Shell_TrayWnd"
                        || class_str == "Shell_SecondaryTrayWnd"
                        || is_start;

                    is_known_shell = is_desktop || is_shell;
                }

                if !hwnd.is_invalid() {
                    if !is_known_shell {
                        let mut state = window_overlap_state(hwnd, cached_scale);
                        // A pop-up (a dialog, a browser bubble, a tool window)
                        // sits on top of the window it belongs to. It must not
                        // bring the notch or dock back over that window, for
                        // example over a full-screen video in the browser.
                        if let Some(base) = popup_base_window(hwnd, my_process_id) {
                            state = state.merge(window_overlap_state(base, cached_scale));
                        }
                        should_overlap = state.dock;
                        should_notch_overlap = state.notch;
                        should_maximized = state.maximized;
                        current_is_fs = state.fullscreen;
                    }
                } else {
                    last_hwnd = hwnd;
                    should_overlap = false;
                    current_is_fs = false;
                    is_known_shell = false;
                }

                // Smart mode also steps aside for any app window under the
                // notch, not only the one in front: a small pop-up or dialog
                // in front of a maximized window keeps the notch hidden, and
                // so does a window partly behind another.
                if NOTCH_RECHECK.swap(false, Ordering::Relaxed)
                    || strip_checked.elapsed() >= Duration::from_secs(1)
                {
                    strip_covered = notch_strip_covered(my_process_id, cached_scale);
                    strip_checked = Instant::now();
                }
                should_notch_overlap |= strip_covered;
                // While a tray icon's menu from the notch is open, the notch
                // stays put until the menu closes, like the taskbar's tray.
                if crate::tray_host::menu_open() {
                    should_notch_overlap = false;
                }

                // Only report dock overlap when the dock window is actually visible.
                // On autostart, the overlap thread starts before init_dock shows the window,
                // and without this guard it emits dock-overlap:true which hides the dock
                // right after init_dock emits dock-overlap:false.
                let dock_visible =
                    cached_hwnd(&handle_visibility, "dock").is_some_and(window_shown);
                let effective_dock_overlap = should_overlap && dock_visible;

                // Update overlap state
                CURRENT_DOCK_OVERLAP.store(
                    if effective_dock_overlap { 1 } else { 0 },
                    Ordering::Relaxed,
                );
                CURRENT_NOTCH_OVERLAP
                    .store(if should_notch_overlap { 1 } else { 0 }, Ordering::Relaxed);
                CURRENT_FOREGROUND_FULLSCREEN.store(current_is_fs, Ordering::Relaxed);

                if Some(effective_dock_overlap) != last_dock_overlap
                    || last_emit.elapsed() >= Duration::from_secs(3)
                {
                    let _ = handle_visibility.emit("dock-overlap", effective_dock_overlap);
                    last_dock_overlap = Some(effective_dock_overlap);
                    last_emit = Instant::now();
                }

                if Some(should_notch_overlap) != last_notch_overlap
                    || last_emit.elapsed() >= Duration::from_secs(3)
                {
                    let _ = handle_visibility.emit("notch-overlap", should_notch_overlap);
                    last_notch_overlap = Some(should_notch_overlap);
                }

                // Adaptive dock signal: a standard maximized foreground window.
                // Unlike dock-overlap this is not guarded by dock visibility, so
                // init_dock can emit the current value when the dock is enabled.
                CURRENT_FOREGROUND_MAXIMIZED.store(should_maximized, Ordering::Relaxed);
                if Some(should_maximized) != last_dock_maximized
                    || last_emit.elapsed() >= Duration::from_secs(3)
                {
                    let _ = handle_visibility.emit("dock-maximized", should_maximized);
                    last_dock_maximized = Some(should_maximized);
                }

                // Full-screen app in front: the notch and the dock step
                // aside, and come back when it leaves. Applied here, at once:
                // it used to go through the media worker, which can be held up
                // for seconds by a media app that is slow to answer, and a
                // dock told late (or never) stayed away after the game. Said
                // again every 3 s, so a page that missed it catches up.
                if current_is_fs == last_visible
                    || last_visibility_said.elapsed() >= Duration::from_secs(3)
                {
                    let leaving_full_screen = !current_is_fs && !last_visible;
                    last_visible = !current_is_fs;
                    last_visibility_said = Instant::now();
                    let _ = handle_visibility.emit("visibility-change", last_visible);
                    if leaving_full_screen {
                        back_from_full_screen(&handle_visibility);
                    }
                }

                osd_sync_ticks += 1;
                if osd_sync_ticks >= 10 {
                    osd_sync_ticks = 0;
                    sync_native_osd(&handle_visibility);
                }

                if explorer_checked.elapsed() >= Duration::from_secs(2) {
                    explorer_checked = Instant::now();
                    let current = crate::tray_host::explorer_taskbar()
                        .map(|h| h.0 as isize)
                        .unwrap_or(0);
                    if current != explorer_taskbar {
                        explorer_taskbar = current;
                        if current != 0 {
                            explorer_taskbar_changed(&handle_visibility);
                        }
                    }
                }

                // Enforce native taskbar hiding (periodic check)
                if NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed) {
                    use windows::Win32::UI::WindowsAndMessaging::{FindWindowA, IsWindowVisible};
                    let secondary_tray_class =
                        windows::core::PCSTR(c"Shell_SecondaryTrayWnd".as_ptr() as *const u8);

                    let mut should_rehide = false;
                    if let Some(tray_hwnd) = crate::tray_host::explorer_taskbar() {
                        if IsWindowVisible(tray_hwnd).as_bool() {
                            should_rehide = true;
                        }
                    }
                    if !should_rehide {
                        if let Ok(secondary_tray_hwnd) =
                            FindWindowA(secondary_tray_class, windows::core::PCSTR::null())
                        {
                            if IsWindowVisible(secondary_tray_hwnd).as_bool() {
                                should_rehide = true;
                            }
                        }
                    }

                    if should_rehide {
                        set_taskbar_visibility(false, false);
                    }
                }
            }

            // Capture UI state is event-driven: WinEvent callbacks set the
            // recheck flag on top-level window show/hide/destroy and foreground
            // changes, so the scan below only runs when something changed
            // (throttled to coalesce bursts of window events).
            let scan_now = now_ms();
            if CAPTURE_RECHECK.load(Ordering::Relaxed)
                && scan_now - CAPTURE_LAST_SCAN_MS.load(Ordering::Relaxed) >= 400
            {
                CAPTURE_RECHECK.store(false, Ordering::Relaxed);
                CAPTURE_LAST_SCAN_MS.store(scan_now, Ordering::Relaxed);
                let active = is_capture_ui_present();
                if active != CAPTURE_UI_ACTIVE.load(Ordering::Relaxed) {
                    CAPTURE_UI_ACTIVE.store(active, Ordering::Relaxed);
                    apply_capture_ui_state(&handle_visibility, active);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    });

    tx
}

/// A full-screen app left the front (a game was closed or switched away
/// from). Games that change the screen's resolution, or keep the screen for a
/// long time, can leave the strips the notch and the dock reserve dropped by
/// Windows and Bloom's windows below the game in the always-on-top band. Both
/// are put back, a moment later, once the desktop has settled.
fn back_from_full_screen(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        if SHUTTING_DOWN.load(Ordering::Relaxed)
            || CURRENT_FOREGROUND_FULLSCREEN.load(Ordering::Relaxed)
        {
            return;
        }
        crate::commands::sync_appbar(app.clone()).await;
        for label in ["main", "dock"] {
            if let Some(hwnd) = cached_hwnd(&app, label) {
                re_assert_topmost(hwnd);
            }
        }
        // The pages decide again from where the pointer is now.
        MH_LAST_MAIN_IGNORE.store(-1, Ordering::Relaxed);
        MH_LAST_DOCK_IGNORE.store(-1, Ordering::Relaxed);
        MH_LAST_OV_IGNORE.store(-1, Ordering::Relaxed);
        let _ = app.emit_to("dock", "dock-report-rect", ());
    });
}

/// What the window in front means for the dock and notch.
#[derive(Clone, Copy, Default)]
struct OverlapState {
    /// The dock should get out of the way.
    dock: bool,
    /// The notch should get out of the way.
    notch: bool,
    /// A standard maximized window (the adaptive dock stretches).
    maximized: bool,
    /// A true full-screen window.
    fullscreen: bool,
}

impl OverlapState {
    /// Combines a pop-up's own state with the window it sits on.
    fn merge(self, other: OverlapState) -> OverlapState {
        let fullscreen = self.fullscreen || other.fullscreen;
        OverlapState {
            dock: self.dock || other.dock,
            notch: self.notch || other.notch,
            maximized: (self.maximized || other.maximized) && !fullscreen,
            fullscreen,
        }
    }
}

/// Whether a window covers its monitor, is maximized, or reaches into the
/// dock or notch area.
unsafe fn window_overlap_state(hwnd: HWND, cached_scale: f64) -> OverlapState {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoA, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, IsZoomed, GWL_STYLE, WS_CAPTION, WS_MAXIMIZE,
    };

    let mut state = OverlapState::default();
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    // Tool windows float above other windows and never decide anything on
    // their own; popup_base_window finds what they sit on.
    if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
        return state;
    }

    let mut rect = RECT::default();
    let dwm_res = DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut rect as *mut _ as *mut _,
        std::mem::size_of::<RECT>() as u32,
    );
    if dwm_res.is_err() && GetWindowRect(hwnd, &mut rect).is_err() {
        return state;
    }

    let h_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoA(h_monitor, &mut mi).as_bool() {
        return state;
    }
    let screen_rect = mi.rcMonitor;
    let is_maximized = IsZoomed(hwnd).as_bool() || (style & WS_MAXIMIZE.0) != 0;
    let is_maximized_standard = is_maximized && (style & WS_CAPTION.0) != 0;

    let mut is_client_fullscreen = false;
    let mut client_rect = RECT::default();
    if GetClientRect(hwnd, &mut client_rect).is_ok() {
        let mut top_left = POINT {
            x: client_rect.left,
            y: client_rect.top,
        };
        let mut bottom_right = POINT {
            x: client_rect.right,
            y: client_rect.bottom,
        };
        let _ = ClientToScreen(hwnd, &mut top_left);
        let _ = ClientToScreen(hwnd, &mut bottom_right);

        is_client_fullscreen = top_left.x <= screen_rect.left
            && top_left.y <= screen_rect.top
            && bottom_right.x >= screen_rect.right
            && bottom_right.y >= screen_rect.bottom;
    }

    let is_matches_screen = rect.left <= screen_rect.left
        && rect.top <= screen_rect.top
        && rect.right >= screen_rect.right
        && rect.bottom >= screen_rect.bottom;

    // Truly fullscreen means client covers screen, OR window matches screen but is not just a standard maximized window
    state.fullscreen = (is_client_fullscreen || is_matches_screen) && !is_maximized_standard;

    if state.fullscreen || is_maximized {
        state.dock = true;
        state.notch = true;
        // Standard maximized windows leave the dock's reserved strip empty on
        // both sides, so the dock can stretch to a full taskbar. True
        // fullscreen windows cover the screen and should not.
        state.maximized = is_maximized && !state.fullscreen;
        return state;
    }

    if let Ok(dock_rect_lock) = DOCK_RECT.lock() {
        if let Some(dr) = *dock_rect_lock {
            let d_left = (dr.x as f64 * cached_scale) as i32;
            let d_right = d_left + (dr.width as f64 * cached_scale) as i32;
            let res_h = (56.0 * cached_scale) as i32;
            let trigger_y = screen_rect.bottom - res_h;

            if rect.left < d_right - 4 && rect.right > d_left + 4 && rect.bottom > trigger_y + 4 {
                state.dock = true;
            }
        }
    }

    if let Ok(notch_rect_lock) = NOTCH_RECT.lock() {
        if let Some(nr) = *notch_rect_lock {
            let n_left = (nr.x as f64 * cached_scale) as i32;
            let n_right = n_left + (nr.width as f64 * cached_scale) as i32;
            let res_h = (36.0 * cached_scale) as i32;
            let trigger_y = screen_rect.top + res_h;

            if rect.left < n_right - 4 && rect.right > n_left + 4 && rect.top < trigger_y - 4 {
                state.notch = true;
            }
        }
    }
    state
}

/// True when an app window lies under the notch's strip at the top of the
/// screen. Windows are looked at from the front back to the desktop. Counted
/// are app windows (the kind the taskbar shows) and the dialogs and pop-ups
/// they own; not counted are Bloom, tool windows (desktop organizers, floating
/// palettes), click-through overlays, invisible, minimized and cloaked windows,
/// and Windows' own surfaces (Start, search, the taskbar).
unsafe fn notch_strip_covered(my_process_id: u32, scale: f64) -> bool {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetAncestor, GetClassNameW, GetLayeredWindowAttributes, GetWindow,
        GetWindowRect, IsIconic, GA_ROOTOWNER, GW_OWNER, LAYERED_WINDOW_ATTRIBUTES_FLAGS,
        LWA_ALPHA, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
    };
    let Some((win_pos, _)) = MAIN_WINDOW_RECT.lock().ok().and_then(|g| *g) else {
        return false;
    };
    let Some(notch) = NOTCH_RECT.lock().ok().and_then(|g| *g) else {
        return false;
    };
    let strip = RECT {
        left: win_pos.x + (notch.x as f64 * scale) as i32 + 4,
        right: win_pos.x + ((notch.x + notch.width) as f64 * scale) as i32 - 4,
        top: win_pos.y,
        bottom: win_pos.y + (36.0 * scale) as i32 - 4,
    };
    if strip.right <= strip.left {
        return false;
    }

    struct Scan {
        strip: RECT,
        own: u32,
        covered: bool,
    }

    unsafe fn class_of(hwnd: HWND) -> String {
        let mut class = [0u16; 64];
        let length = GetClassNameW(hwnd, &mut class) as usize;
        String::from_utf16_lossy(&class[..length.min(class.len())])
    }

    unsafe fn is_app_window(hwnd: HWND) -> bool {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        if ex & WS_EX_APPWINDOW.0 != 0 {
            return true;
        }
        let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null());
        !owned && ex & WS_EX_NOACTIVATE.0 == 0
    }

    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let scan = &mut *(param.0 as *mut Scan);
        let class = class_of(hwnd);
        // Reached the desktop: nothing further back can be over the notch.
        if class == "Progman" || class == "WorkerW" {
            return false.into();
        }
        if !IsWindowVisible(hwnd).as_bool()
            || IsIconic(hwnd).as_bool()
            || matches!(
                class.as_str(),
                "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow"
            )
        {
            return true.into();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == scan.own {
            return true.into();
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & (WS_EX_TRANSPARENT.0 | WS_EX_TOOLWINDOW.0) != 0 {
            return true.into();
        }
        if ex & WS_EX_LAYERED.0 != 0 {
            let mut alpha = 255u8;
            let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
            if GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), Some(&mut flags)).is_ok()
                && flags.0 & LWA_ALPHA.0 != 0
                && alpha == 0
            {
                return true.into();
            }
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            4,
        )
        .is_ok()
            && cloaked != 0
        {
            return true.into();
        }
        // An app window, or a dialog or pop-up of one.
        if !is_app_window(hwnd) {
            let root = GetAncestor(hwnd, GA_ROOTOWNER);
            if root.is_invalid()
                || root == hwnd
                || !IsWindowVisible(root).as_bool()
                || !is_app_window(root)
            {
                return true.into();
            }
        }
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
            && GetWindowRect(hwnd, &mut rect).is_err()
        {
            return true.into();
        }
        let strip = scan.strip;
        if rect.left < strip.right
            && rect.right > strip.left
            && rect.top < strip.bottom
            && rect.bottom > strip.top
        {
            scan.covered = true;
            return false.into();
        }
        true.into()
    }

    let mut scan = Scan {
        strip,
        own: my_process_id,
        covered: false,
    };
    let _ = EnumWindows(Some(visit), LPARAM(&mut scan as *mut Scan as isize));
    scan.covered
}

/// A visible app window that can be the one a pop-up belongs to: not Bloom,
/// not hidden, minimized or cloaked, and not the desktop or the taskbar.
unsafe fn is_base_candidate(hwnd: HWND, my_process_id: u32) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameA, GetWindowRect, IsIconic};
    if hwnd.is_invalid() || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return false;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == my_process_id {
        return false;
    }
    let mut cloaked = 0u32;
    if DwmGetWindowAttribute(
        hwnd,
        windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
        &mut cloaked as *mut _ as *mut _,
        4,
    )
    .is_ok()
        && cloaked != 0
    {
        return false;
    }
    let mut class_name = [0u8; 256];
    let len = GetClassNameA(hwnd, &mut class_name);
    let class = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("");
    if matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    ) {
        return false;
    }
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect).is_ok() && rect.right > rect.left && rect.bottom > rect.top
}

/// For a pop-up in front, the app window it belongs to: the window that owns
/// it (dialogs, browser bubbles, find bars), or for a free-floating tool
/// window or dialog the app window right underneath it. None for an ordinary
/// window.
unsafe fn popup_base_window(hwnd: HWND, my_process_id: u32) -> Option<HWND> {
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetClassNameA, GetWindow, GA_ROOTOWNER, GW_HWNDNEXT,
    };

    let root_owner = GetAncestor(hwnd, GA_ROOTOWNER);
    let owned = !root_owner.is_invalid() && root_owner != hwnd;
    if owned && is_base_candidate(root_owner, my_process_id) {
        return Some(root_owner);
    }

    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let mut class_name = [0u8; 256];
    let len = GetClassNameA(hwnd, &mut class_name);
    let is_dialog = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("") == "#32770";
    let floats = (ex_style & WS_EX_TOOLWINDOW.0) != 0 || is_dialog;
    // An owned pop-up whose owner is hidden (a common pattern for dialogs)
    // is treated like a free-floating one.
    if !(owned || floats) {
        return None;
    }

    // The window directly underneath, on the same monitor.
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut next = GetWindow(hwnd, GW_HWNDNEXT).unwrap_or_default();
    for _ in 0..40 {
        if next.is_invalid() {
            break;
        }
        let next_ex = GetWindowLongW(next, GWL_EXSTYLE) as u32;
        if (next_ex & WS_EX_TOOLWINDOW.0) == 0 && is_base_candidate(next, my_process_id) {
            if MonitorFromWindow(next, MONITOR_DEFAULTTONEAREST) == monitor {
                return Some(next);
            }
        } else {
            let mut class_name = [0u8; 256];
            let len = GetClassNameA(next, &mut class_name);
            let class = std::str::from_utf8(&class_name[..len as usize]).unwrap_or("");
            // Reached the desktop: the pop-up sits on nothing.
            if class == "Progman" || class == "WorkerW" {
                break;
            }
        }
        next = GetWindow(next, GW_HWNDNEXT).unwrap_or_default();
    }
    None
}

/// Explorer-owned windows that host the Windows 11 volume/brightness flyout.
/// The flyout lives in a small, non-activating `XamlExplorerHostIslandWindow`;
/// the same class also hosts full-screen surfaces (Alt+Tab, Task View), which
/// the size check excludes.
fn native_osd_windows() -> Vec<HWND> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, GetSystemMetrics, GetWindowRect, SM_CXSCREEN, SM_CYSCREEN, WS_EX_NOACTIVATE,
    };
    let mut found = Vec::new();
    unsafe {
        let Some(tray) = crate::tray_host::explorer_taskbar() else {
            return found;
        };
        let mut explorer_pid = 0u32;
        GetWindowThreadProcessId(tray, Some(&mut explorer_pid));
        let (screen_w, screen_h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));

        let mut after: Option<HWND> = None;
        while let Ok(hwnd) = FindWindowExW(
            None,
            after,
            windows::core::w!("XamlExplorerHostIslandWindow"),
            windows::core::PCWSTR::null(),
        ) {
            after = Some(hwnd);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let mut rect = RECT::default();
            let small = GetWindowRect(hwnd, &mut rect).is_ok()
                && (rect.right - rect.left) < screen_w / 2
                && (rect.bottom - rect.top) < screen_h / 2;
            if pid == explorer_pid && (ex_style & WS_EX_NOACTIVATE.0) != 0 && small {
                found.push(hwnd);
            }
        }
    }
    found
}

/// Laptop brightness keys bypass input hooks (the firmware changes brightness
/// and notifies the display driver directly), so unlike the volume keys they
/// cannot be swallowed to keep Windows from drawing its own flyout. While
/// Bloom's overlay replaces it, the flyout host is kept fully transparent and
/// click-through instead; Explorer may still show it, but nothing is visible.
pub fn set_native_osd_suppressed(suppress: bool) {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetLayeredWindowAttributes, SetLayeredWindowAttributes, SetWindowLongW, LWA_ALPHA,
        WS_EX_LAYERED, WS_EX_TRANSPARENT,
    };
    for hwnd in native_osd_windows() {
        unsafe {
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let mut alpha = 255u8;
            // Explorer never makes this window layered itself, so a layered
            // window at alpha 0 is Bloom's doing, including from a crashed run.
            let suppressed = (ex_style & WS_EX_LAYERED.0) != 0
                && GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), None).is_ok()
                && alpha == 0;
            if suppress && !suppressed {
                SetWindowLongW(
                    hwnd,
                    GWL_EXSTYLE,
                    (ex_style | WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as i32,
                );
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
            } else if !suppress && suppressed {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
                SetWindowLongW(
                    hwnd,
                    GWL_EXSTYLE,
                    (ex_style & !(WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0)) as i32,
                );
            }
        }
    }
}

/// The volume and brightness flyouts share one host window, so it is only
/// suppressed while Bloom replaces both. With either overlay turned off, the
/// user asked for the native flyout and it must stay visible.
fn sync_native_osd(app_handle: &AppHandle) {
    let enabled = |key: &str| {
        get_setting_str(app_handle, key)
            .map(|v| v != "false")
            .unwrap_or(true)
    };
    set_native_osd_suppressed(
        enabled("bloom-brightness-overlay-enabled") && enabled("bloom-volume-overlay-enabled"),
    );
}

static MOUSE_HOOK_UPDATES: OnceLock<SyncSender<()>> = OnceLock::new();
static MOUSE_HOOK_CURSOR: AtomicI64 = AtomicI64::new(0);
static MOUSE_HOOK_LAST_EVENT_MS: AtomicI64 = AtomicI64::new(0);
/// Where the last button press landed while a dock menu was open (packed
/// like `MOUSE_HOOK_CURSOR`), for the cursor worker to check.
static MOUSE_HOOK_PRESS: AtomicI64 = AtomicI64::new(0);
static MOUSE_HOOK_PRESS_PENDING: AtomicBool = AtomicBool::new(false);
/// Held while deciding whether the notch, the dock or the overlay takes
/// clicks. The cursor worker, the edge watchdogs and the input guard all
/// decide; one at a time, so two can never leave a window taking clicks
/// while Bloom records it as click-through.
static HIT_LOCK: Mutex<()> = Mutex::new(());

fn hit_lock() -> std::sync::MutexGuard<'static, ()> {
    HIT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
static MH_LAST_MAIN_IGNORE: AtomicI32 = AtomicI32::new(-1);
static MH_LAST_DOCK_IGNORE: AtomicI32 = AtomicI32::new(-1);
static MH_LAST_OV_IGNORE: AtomicI32 = AtomicI32::new(-1);

/// The overlay window was just made (a moment after start) click-through.
pub fn overlay_made() {
    MH_LAST_OV_IGNORE.store(1, Ordering::Relaxed);
}
static MH_LAST_EDGE_HOVER: AtomicI32 = AtomicI32::new(-1);
static MH_LAST_TOP_EDGE_HOVER: AtomicI32 = AtomicI32::new(-1);
static MH_LAST_LEFT_EDGE_HOVER: AtomicI32 = AtomicI32::new(-1);
static MH_LAST_RIGHT_EDGE_HOVER: AtomicI32 = AtomicI32::new(-1);
static MH_DOCK_EXPIRY_MS: AtomicI64 = AtomicI64::new(0);
static MH_TOPBAR_EXPIRY_MS: AtomicI64 = AtomicI64::new(0);
/// Since when the pointer has been off the open notch (0 = it is on it).
static MH_NOTCH_OFF_SINCE_MS: AtomicI64 = AtomicI64::new(0);
/// When Bloom last told the notch the pointer is off it.
static MH_NOTCH_LEFT_SENT_MS: AtomicI64 = AtomicI64::new(0);
/// How long the pointer is off the open notch before Bloom closes it.
const NOTCH_OFF_CLOSE_MS: i64 = 500;
static MH_LEFT_EXPIRY_MS: AtomicI64 = AtomicI64::new(0);
static MH_RIGHT_EXPIRY_MS: AtomicI64 = AtomicI64::new(0);
static MH_LAST_MONITOR_UPDATE_MS: AtomicI64 = AtomicI64::new(0);
static MH_CACHED_MON_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);
static MH_CACHED_MON_SIZE: Mutex<Option<(u32, u32)>> = Mutex::new(None);
/// The primary monitor's scale (f64 bits), refreshed with its position and size.
static MH_CACHED_MON_SCALE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0x3FF0_0000_0000_0000);
static CAPTURE_UI_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPTURE_RECHECK: AtomicBool = AtomicBool::new(true);
/// Set when a top-level window appeared, moved, changed size, went away or
/// came to the front, so smart mode looks again at what is under the notch.
static NOTCH_RECHECK: AtomicBool = AtomicBool::new(true);
static CAPTURE_LAST_SCAN_MS: AtomicI64 = AtomicI64::new(0);

/// Hover-intent ("pressure") gate for the top edge. Contact with the top band
/// only starts a dwell clock; the edge is armed — and only then peeks the notch
/// or claims cursor input — after the cursor has rested in the band for the
/// configured delay. A pass-through to a maximized window's tab/title-bar
/// controls never arms it. The delay is user-tunable via
/// `bloom-notch-edge-delay` (ms); `0` restores the old instant behavior.
const TOP_EDGE_BAND_PX: f64 = 8.0;
const TOP_EDGE_DWELL_MS: i64 = 200;
/// The mouse hook only fires on WM_MOUSEMOVE, so a pending dwell is completed
/// by a watchdog that re-checks at this cadence.
const TOP_EDGE_POLL_MS: u64 = 40;
static MH_TOP_EDGE_ENTER_MS: AtomicI64 = AtomicI64::new(0);
static MH_TOP_EDGE_ARMED: AtomicBool = AtomicBool::new(false);

/// Hover-intent ("pressure") gate for the bottom edge, the dock-side mirror of
/// the top-edge gate. Contact with the bottom band only starts a dwell clock;
/// the edge is armed — and only then reveals the dock or claims cursor input —
/// after the cursor has rested in the band for the configured delay. A
/// pass-through along the bottom strip never arms it. The delay is user-tunable
/// via `bloom-dock-edge-delay` (ms); `0` restores the old instant behavior.
const BOTTOM_EDGE_BAND_PX: f64 = 8.0;
const BOTTOM_EDGE_DWELL_MS: i64 = 200;
/// The mouse hook only fires on WM_MOUSEMOVE, so a pending dwell is completed
/// by a watchdog that re-checks at this cadence.
const BOTTOM_EDGE_POLL_MS: u64 = 40;
static MH_DOCK_EDGE_ENTER_MS: AtomicI64 = AtomicI64::new(0);
static MH_DOCK_EDGE_ARMED: AtomicBool = AtomicBool::new(false);

/// Physical bounds `(x, y, width, height)` of the expanded per-app volume
/// mixer (notch + panel), if it is open. The frontend reports the card in
/// overlay CSS pixels; scale and monitor offset convert it to virtual-desktop
/// coordinates.
fn volume_mixer_physical_rect_for(app: &AppHandle) -> Option<(i32, i32, i32, i32)> {
    let (x, y, width, height) = (*crate::state::VOLUME_MIXER_RECT.lock().ok()?)?;
    let ov_win = app.get_webview_window("overlay")?;
    let monitor = ov_win.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let pos = monitor.position();
    Some((
        pos.x + (x * scale) as i32,
        pos.y + (y * scale) as i32,
        (width * scale) as i32,
        (height * scale) as i32,
    ))
}

/// The mouse hook only runs on mouse movement, so a cursor that stops just
/// outside the mixer would leave it open indefinitely. This watchdog re-checks
/// the position and closes the card as soon as the cursor is off it.
fn setup_volume_mixer_watchdog(app_handle: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(200));
        if SHUTTING_DOWN.load(Ordering::Relaxed) {
            continue;
        }
        let Some((rx, ry, rw, rh)) = volume_mixer_physical_rect_for(&app_handle) else {
            continue;
        };
        let mut pt = windows::Win32::Foundation::POINT::default();
        if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_err() {
            continue;
        }
        let inside = pt.x >= rx && pt.x <= rx + rw && pt.y >= ry && pt.y <= ry + rh;
        if inside {
            continue;
        }
        let at_left_edge = app_handle
            .primary_monitor()
            .ok()
            .flatten()
            .is_some_and(|m| {
                let pos = m.position();
                let size = m.size();
                pt.x <= pos.x + 8 && pt.y >= pos.y && pt.y <= pos.y + size.height as i32
            });
        if !at_left_edge && MH_LAST_LEFT_EDGE_HOVER.swap(0, Ordering::Relaxed) != 0 {
            MH_LEFT_EXPIRY_MS.store(0, Ordering::Relaxed);
            let _ = app_handle.emit("volume-edge-hover", false);
        }
    });
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Re-runs notch hit-testing immediately. Called when the frontend's hover
/// flag changes so edge-hover state cannot go stale while the cursor is
/// stationary — a native drag-out ends with the pointer outside the notch but
/// the webview never saw it leave.
pub fn refresh_main_interaction(app_handle: &AppHandle) {
    let mut pt = windows::Win32::Foundation::POINT::default();
    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_err() {
        return;
    }
    let fg_fs = CURRENT_FOREGROUND_FULLSCREEN.load(Ordering::Relaxed);
    update_main_interaction(app_handle, pt, now_ms(), fg_fs);
}

/// True while a screen-capture UI (Windows Snipping Tool) has a visible window.
/// Bloom's notch sits exactly where that toolbar lives, so it must get out of
/// the way even if the capture window isn't recognised as fullscreen.
unsafe extern "system" fn capture_ui_enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    use windows::Win32::UI::WindowsAndMessaging::IsIconic;
    let found = &mut *(lparam.0 as *mut bool);
    if *found {
        return BOOL(0);
    }
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return BOOL(1);
    }

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != 0 && pid != std::process::id() {
        if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            if QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .is_ok()
            {
                let path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
                let name = path.rsplit('\\').next().unwrap_or("");
                if name == "snippingtool.exe"
                    || name == "screenclippinghost.exe"
                    || name == "screensketch.exe"
                {
                    *found = true;
                }
            }
            let _ = CloseHandle(handle);
        }
    }

    if !*found {
        use windows::Win32::UI::WindowsAndMessaging::GetClassNameA;
        let mut class_buf = [0u8; 256];
        let len = GetClassNameA(hwnd, &mut class_buf);
        let class = std::str::from_utf8(&class_buf[..len as usize])
            .unwrap_or("")
            .to_lowercase();
        if class.contains("snipping") {
            *found = true;
        }
    }

    BOOL(1)
}

fn is_capture_ui_present() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    let mut found = false;
    unsafe {
        let _ = EnumWindows(
            Some(capture_ui_enum_proc),
            LPARAM(&mut found as *mut bool as isize),
        );
    }
    found
}

fn apply_capture_ui_state(app: &AppHandle, active: bool) {
    if SHUTTING_DOWN.load(Ordering::Relaxed) {
        return;
    }
    if let Some(main_win) = app.get_webview_window("main") {
        if active {
            let _ = main_win.set_ignore_cursor_events(true);
            let _ = main_win.hide();
        } else {
            let _ = main_win.show();
            reconcile_main_appbar(app);
        }
    }
    if active {
        if let Some(dock_win) = app.get_webview_window("dock") {
            let _ = dock_win.set_ignore_cursor_events(true);
        }
        if let Some(ov_win) = app.get_webview_window("overlay") {
            let _ = ov_win.set_ignore_cursor_events(true);
        }
    }
    // Force the mouse hook to re-evaluate hit-testing on the next move.
    MH_LAST_MAIN_IGNORE.store(-1, Ordering::Relaxed);
    MH_LAST_DOCK_IGNORE.store(-1, Ordering::Relaxed);
    MH_LAST_OV_IGNORE.store(-1, Ordering::Relaxed);
    // Cancel any pending top/bottom-edge dwell so they cannot arm while Bloom
    // is click-through for the capture tool.
    MH_TOP_EDGE_ENTER_MS.store(0, Ordering::Relaxed);
    MH_TOP_EDGE_ARMED.store(false, Ordering::Relaxed);
    MH_DOCK_EDGE_ENTER_MS.store(0, Ordering::Relaxed);
    MH_DOCK_EDGE_ARMED.store(false, Ordering::Relaxed);
}

pub fn setup_mouse_hook(app_handle: AppHandle) {
    let (updates, receiver) = sync_channel::<()>(1);
    if MOUSE_HOOK_UPDATES.set(updates).is_err() {
        return;
    }
    setup_volume_mixer_watchdog(app_handle.clone());
    setup_dock_edge_watchdog(app_handle.clone());
    setup_top_edge_watchdog(app_handle.clone());
    setup_input_guard(app_handle.clone());
    std::thread::Builder::new()
        .name("bloom-cursor-worker".into())
        .spawn(move || {
            let frame = Duration::from_millis(32);
            let mut last_processed = Instant::now() - frame;
            while !SHUTTING_DOWN.load(Ordering::Relaxed) {
                match receiver.recv_timeout(Duration::from_millis(500)) {
                    Ok(()) => {
                        if let Some(remaining) = frame.checked_sub(last_processed.elapsed()) {
                            std::thread::sleep(remaining);
                        }
                        let cursor = unpack_cursor(MOUSE_HOOK_CURSOR.load(Ordering::Relaxed));
                        process_mouse_position(&app_handle, cursor);
                        if MOUSE_HOOK_PRESS_PENDING.swap(false, Ordering::Relaxed) {
                            let press = unpack_cursor(MOUSE_HOOK_PRESS.load(Ordering::Relaxed));
                            close_dock_menu_if_outside(&app_handle, press);
                        }
                        last_processed = Instant::now();
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("Failed to start cursor worker");
    std::thread::Builder::new()
        .name("bloom-mouse-hook".into())
        .spawn(|| {
            while !SHUTTING_DOWN.load(Ordering::Relaxed) {
                run_mouse_hook_thread();
                if !SHUTTING_DOWN.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        })
        .expect("Failed to start mouse hook thread");
}

fn pack_cursor(point: windows::Win32::Foundation::POINT) -> i64 {
    ((point.x as u32 as u64) << 32 | point.y as u32 as u64) as i64
}

fn unpack_cursor(packed: i64) -> windows::Win32::Foundation::POINT {
    windows::Win32::Foundation::POINT {
        x: (packed as u64 >> 32) as u32 as i32,
        y: packed as u32 as i32,
    }
}

fn queue_cursor(point: windows::Win32::Foundation::POINT) {
    MOUSE_HOOK_CURSOR.store(pack_cursor(point), Ordering::Relaxed);
    if let Some(updates) = MOUSE_HOOK_UPDATES.get() {
        let _ = updates.try_send(());
    }
}

fn hook_tick_ms() -> i64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() as i64 }
}

fn run_mouse_hook_thread() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetCursorPos, GetMessageW, KillTimer, PeekMessageW, SetTimer,
        TranslateMessage, UnhookWindowsHookEx, MSG, PM_NOREMOVE, WM_TIMER,
    };
    unsafe {
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        // A timer wakes GetMessageW even when the mouse is idle, so the thread
        // can shut down and detect a hook Windows silently removed on timeout.
        let timer = SetTimer(None, 1, 500, None);
        if timer == 0 {
            eprintln!("Bloom mouse hook: timer creation failed; retrying");
            return;
        }
        let mut hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0).ok();
        let mut point = windows::Win32::Foundation::POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            queue_cursor(point);
            MOUSE_HOOK_LAST_EVENT_MS.store(hook_tick_ms(), Ordering::Relaxed);
        }
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            if message.message == WM_TIMER && message.wParam.0 == timer {
                if SHUTTING_DOWN.load(Ordering::Relaxed) {
                    break;
                }
                let mut current = windows::Win32::Foundation::POINT::default();
                let cursor_moved_without_hook = GetCursorPos(&mut current).is_ok()
                    && pack_cursor(current) != MOUSE_HOOK_CURSOR.load(Ordering::Relaxed)
                    && hook_tick_ms() - MOUSE_HOOK_LAST_EVENT_MS.load(Ordering::Relaxed) > 750;
                if hook.is_none() || cursor_moved_without_hook {
                    if let Some(old) = hook.take() {
                        let _ = UnhookWindowsHookEx(old);
                    }
                    hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0).ok();
                    if hook.is_some() {
                        queue_cursor(current);
                        MOUSE_HOOK_LAST_EVENT_MS.store(hook_tick_ms(), Ordering::Relaxed);
                    }
                }
            } else {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if timer != 0 {
            let _ = KillTimer(None, timer);
        }
        if let Some(hook) = hook {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}

/// Top-edge hover-intent delay in milliseconds. `0` disables the gate so the
/// notch reveals the moment the band is touched (the pre-fix behavior).
fn top_edge_dwell_ms(app_handle: &AppHandle) -> i64 {
    crate::utils::get_setting_str(app_handle, "bloom-notch-edge-delay")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(TOP_EDGE_DWELL_MS)
        .clamp(0, 2000)
}

/// True when the cursor sits inside the top-edge hot band on the primary
/// monitor. Shared by the dwell gate and top-edge hit-testing continuity.
fn in_top_edge_band(cursor: windows::Win32::Foundation::POINT, scale: f64) -> bool {
    let (mon_x, mon_y) = MH_CACHED_MON_POS
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or((0, 0));
    let mon_w = MH_CACHED_MON_SIZE
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|s| s.0 as i32)
        .unwrap_or(1920);

    cursor.y <= (mon_y + (TOP_EDGE_BAND_PX * scale) as i32)
        && cursor.x >= mon_x
        && cursor.x <= (mon_x + mon_w)
}

/// True once the cursor has stayed inside the top band long enough to count as
/// a deliberate reveal. Mere contact does not arm: a pass through the top strip
/// on the way to a maximized window's tab/title-bar controls must not peek the
/// notch. Leaving the band disarms and restarts the clock.
///
/// The mouse hook only fires on movement, so `setup_top_edge_watchdog` also
/// calls this while a dwell is pending so it can complete on a stationary
/// cursor.
fn top_edge_armed_for(
    app_handle: &AppHandle,
    cursor: windows::Win32::Foundation::POINT,
    now: i64,
    scale: f64,
) -> bool {
    // Only at the notch: the pointer resting on a maximized browser's tabs
    // elsewhere along the top edge must not bring a hidden notch back.
    let main_rect = MAIN_WINDOW_RECT.lock().ok().and_then(|g| *g);
    let notch_rect = NOTCH_RECT.try_lock().ok().and_then(|g| *g);
    let at_notch = match (main_rect, notch_rect) {
        (Some((win_pos, _)), Some(r)) => {
            let margin = (80.0 * scale) as i32;
            let left = win_pos.x + (r.x as f64 * scale) as i32 - margin;
            let right = win_pos.x + ((r.x + r.width) as f64 * scale) as i32 + margin;
            cursor.x >= left && cursor.x <= right
        }
        _ => true,
    };
    if !at_notch || !in_top_edge_band(cursor, scale) {
        MH_TOP_EDGE_ENTER_MS.store(0, Ordering::Relaxed);
        MH_TOP_EDGE_ARMED.store(false, Ordering::Relaxed);
        return false;
    }
    if MH_TOP_EDGE_ARMED.load(Ordering::Relaxed) {
        return true;
    }
    let dwell_ms = top_edge_dwell_ms(app_handle);
    if dwell_ms <= 0 {
        MH_TOP_EDGE_ARMED.store(true, Ordering::Relaxed);
        return true;
    }
    let enter = MH_TOP_EDGE_ENTER_MS.load(Ordering::Relaxed);
    if enter == 0 {
        MH_TOP_EDGE_ENTER_MS.store(now, Ordering::Relaxed);
        return false;
    }
    if now - enter >= dwell_ms {
        MH_TOP_EDGE_ARMED.store(true, Ordering::Relaxed);
        return true;
    }
    false
}

/// Hover + click-through state for the notch window. Shared by the mouse hook
/// (on movement) and the top-edge watchdog (to finish a stationary dwell).
/// The handles of Bloom's lasting windows (the notch, the dock, the volume
/// and brightness overlay), looked up once. Asking Tauri for a window's
/// handle, visibility or scale is a round trip to the UI thread, and the
/// pointer watchdogs ask dozens of times a second; Windows answers these
/// directly.
static HWND_MAIN: AtomicIsize = AtomicIsize::new(0);
static HWND_DOCK: AtomicIsize = AtomicIsize::new(0);
static HWND_OVERLAY: AtomicIsize = AtomicIsize::new(0);

pub(crate) fn cached_hwnd(app: &AppHandle, label: &str) -> Option<HWND> {
    let slot = match label {
        "main" => &HWND_MAIN,
        "dock" => &HWND_DOCK,
        "overlay" => &HWND_OVERLAY,
        _ => return app.get_webview_window(label)?.hwnd().ok(),
    };
    let cached = HWND(slot.load(Ordering::Relaxed) as *mut _);
    if !cached.0.is_null()
        && unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(cached)) }.as_bool()
    {
        return Some(cached);
    }
    let hwnd = app.get_webview_window(label)?.hwnd().ok()?;
    slot.store(hwnd.0 as isize, Ordering::Relaxed);
    Some(hwnd)
}

/// Whether a window is shown (what Tauri's `is_visible` reports).
fn window_shown(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) }.as_bool()
}

/// A window's display scale (what Tauri's `scale_factor` reports).
fn window_scale(hwnd: HWND) -> f64 {
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f64 / 96.0
    }
}

fn update_main_interaction(
    app_handle: &AppHandle,
    cursor: windows::Win32::Foundation::POINT,
    now: i64,
    fg_fs: bool,
) {
    let _hit = hit_lock();
    if fg_fs {
        // Fullscreen foreground app or capture overlay: Bloom steps aside and
        // any pending top-edge dwell is dropped.
        MH_TOP_EDGE_ENTER_MS.store(0, Ordering::Relaxed);
        MH_TOP_EDGE_ARMED.store(false, Ordering::Relaxed);
        if let Some(main_win) = app_handle.get_webview_window("main") {
            if MH_LAST_MAIN_IGNORE.load(Ordering::Relaxed) != 1 {
                if let Some(hwnd) = cached_hwnd(app_handle, "main") {
                    re_assert_topmost(hwnd);
                }
                let _ = main_win.set_ignore_cursor_events(true);
                MH_LAST_MAIN_IGNORE.store(1, Ordering::Relaxed);
            }
        }
        if MH_LAST_TOP_EDGE_HOVER.swap(0, Ordering::Relaxed) != 0 {
            let _ = app_handle.emit("notch-edge-hover", false);
        }
        // Bloom is fully click-through here, so the webview cannot clear its
        // own hover state; do it for it.
        if NOTCH_IS_HOVERED.swap(false, Ordering::Relaxed) {
            let _ = app_handle.emit("notch-pointer-left", ());
        }
        return;
    }

    let Some(main_win) = app_handle.get_webview_window("main") else {
        return;
    };
    let Some(main_hwnd) = cached_hwnd(app_handle, "main") else {
        return;
    };
    if !window_shown(main_hwnd) {
        return;
    }

    let in_notch_hover = NOTCH_IS_HOVERED.load(Ordering::Relaxed);
    let scale = window_scale(main_hwnd);
    // The top-edge reveal exists only to bring a hidden notch on screen: it is
    // dwell-gated, and fixed mode (always visible) skips it entirely. This is
    // about showing the notch — the hit-testing continuity below is separate
    // and must stay immediate.
    let edge_armed = if notch_mode_reserves_work_area(
        get_setting_str(app_handle, "bloom-notch-mode").as_deref(),
    ) {
        MH_TOP_EDGE_ENTER_MS.store(0, Ordering::Relaxed);
        MH_TOP_EDGE_ARMED.store(false, Ordering::Relaxed);
        false
    } else {
        top_edge_armed_for(app_handle, cursor, now, scale)
    };

    let mut is_notch_hovered = false;
    if edge_armed || in_notch_hover {
        is_notch_hovered = true;
        MH_TOPBAR_EXPIRY_MS.store(now + 500, Ordering::Relaxed);
    }

    // An OS file drag can only reach the webview's drop target when the
    // window is not click-through: WS_EX_TRANSPARENT hides the entire window
    // (and its webview children) from OLE hit-testing, which shows the
    // no-drop cursor and never emits drag events. Detect a held left button
    // so a drag over the notch can force interactivity.
    let drag_held = unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
        (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0
    };

    let mut is_click_interactive = false;
    // Whether the pointer is on the notch (None: its place isn't known now).
    let mut pointer_on_notch: Option<bool> = None;
    let main_rect_val = MAIN_WINDOW_RECT.lock().ok().and_then(|g| *g);

    if let Some((win_pos, _)) = main_rect_val {
        if let Ok(region) = NOTCH_RECT.try_lock() {
            if let Some(r) = *region {
                let pad_x = (20.0 * scale) as i32;
                let pad_y_bottom = (5.0 * scale) as i32;
                // Hysteresis keeps the notch interactive a little past its
                // bounds once grabbed, so removing the edge-forced
                // interactivity doesn't reintroduce boundary flicker.
                let hyst = if MH_LAST_MAIN_IGNORE.load(Ordering::Relaxed) == 0 {
                    (10.0 * scale) as i32
                } else {
                    0
                };
                let rx = win_pos.x + (r.x as f64 * scale) as i32 - pad_x - hyst;
                let rw = (r.width as f64 * scale) as i32 + (pad_x * 2) + (hyst * 2);
                let ry_top = win_pos.y;
                let ry_bottom = win_pos.y + (r.height as f64 * scale) as i32 + pad_y_bottom + hyst;

                let in_notch_rect = cursor.x >= rx
                    && cursor.x <= (rx + rw)
                    && cursor.y >= ry_top
                    && cursor.y <= ry_bottom;
                pointer_on_notch = Some(in_notch_rect);

                // Approaching along the top edge keeps the window interactive
                // while near the notch's horizontal span. This must stay
                // immediate (never dwell-gated): NOTCH_RECT lags the expand/
                // contract animation, and letting click-through flip mid-hover
                // makes the notch expand/contract repeatedly. It is gated on
                // the notch being on screen (or a live hover) so a hidden
                // notch still can't swallow pass-by clicks.
                let edge_pad = (60.0 * scale) as i32;
                let in_top_span = in_top_edge_band(cursor, scale)
                    && cursor.x >= rx - edge_pad
                    && cursor.x <= rx + rw + edge_pad;

                let notch_visible = NOTCH_IS_VISIBLE.load(Ordering::Relaxed);
                let hover_active =
                    is_notch_hovered || now < MH_TOPBAR_EXPIRY_MS.load(Ordering::Relaxed);
                if (notch_visible || hover_active) && (in_notch_rect || in_top_span) {
                    is_click_interactive = true;
                }

                // A held left button means a possible OS drag: OLE cannot
                // hit-test a click-through window, so force interactivity over
                // the notch or the drop target is unreachable.
                if drag_held && (in_notch_rect || in_top_span) {
                    is_click_interactive = true;
                }
            }
        }
    }

    // The open notch closes once the pointer has been off it for a moment,
    // whatever the webview saw: it misses the pointer leaving when the window
    // turns click-through first, or when another window takes the pointer.
    // Said again each second while the notch stays open, as a view may hold
    // it open (a tray menu, typing) and let go later; the notch decides.
    if let Some(on_notch) = pointer_on_notch {
        if NOTCH_IS_OPEN.load(Ordering::Relaxed) && !on_notch && !drag_held {
            let since = MH_NOTCH_OFF_SINCE_MS.load(Ordering::Relaxed);
            if since == 0 {
                MH_NOTCH_OFF_SINCE_MS.store(now, Ordering::Relaxed);
            } else if now - since >= NOTCH_OFF_CLOSE_MS
                && now - MH_NOTCH_LEFT_SENT_MS.load(Ordering::Relaxed) >= 1000
            {
                MH_NOTCH_LEFT_SENT_MS.store(now, Ordering::Relaxed);
                let _ = app_handle.emit_to("main", "notch-pointer-left", ());
            }
        } else {
            MH_NOTCH_OFF_SINCE_MS.store(0, Ordering::Relaxed);
        }
    }

    let final_notch_hover = is_notch_hovered || now < MH_TOPBAR_EXPIRY_MS.load(Ordering::Relaxed);
    let prev = MH_LAST_TOP_EDGE_HOVER.load(Ordering::Relaxed);
    let new_val = if final_notch_hover { 1 } else { 0 };
    if prev != new_val {
        let _ = app_handle.emit("notch-edge-hover", final_notch_hover);
        MH_LAST_TOP_EDGE_HOVER.store(new_val, Ordering::Relaxed);
    }

    // MENU_IS_OPEN is the dock's context menu, which lives in the dock window.
    // Letting it hold the notch window interactive made the whole strip
    // across the top of the screen swallow clicks while a dock menu was open
    // (or if one closed without saying so), so apps there stopped responding.
    let final_ignore = !is_click_interactive;
    let prev_ignore = MH_LAST_MAIN_IGNORE.load(Ordering::Relaxed);
    let new_ignore = if final_ignore { 1 } else { 0 };
    if prev_ignore != new_ignore {
        // Z-order re-assertion is a synchronous window operation; skip it
        // while a drag may be active, where it can visibly hitch the drag.
        if !drag_held {
            re_assert_topmost(main_hwnd);
        }
        let _ = main_win.set_ignore_cursor_events(final_ignore);
        MH_LAST_MAIN_IGNORE.store(new_ignore, Ordering::Relaxed);
        // Once the window is click-through the webview can never receive the
        // mouseleave that would clear its hover state (it may already have
        // missed it if the pointer left in the same frame). Tell it explicitly
        // or the notch stays expanded in peek/smart mode.
        if new_ignore == 1 && NOTCH_IS_HOVERED.swap(false, Ordering::Relaxed) {
            let _ = app_handle.emit("notch-pointer-left", ());
        }
    }
}

/// A press outside the dock's open menu (or its add-app panel) closes it, as
/// a click outside a menu does. The dock window lets clicks through outside
/// the dock and its menu, so its page never sees that press itself.
fn close_dock_menu_if_outside(app_handle: &AppHandle, press: windows::Win32::Foundation::POINT) {
    if !MENU_IS_OPEN.load(Ordering::Relaxed) {
        return;
    }
    let Some(dock_hwnd) = cached_hwnd(app_handle, "dock") else {
        return;
    };
    let Some((win_pos, _)) = DOCK_WINDOW_RECT.lock().ok().and_then(|g| *g) else {
        return;
    };
    let scale = window_scale(dock_hwnd);
    let pad = (10.0 * scale) as i32;
    let inside = |r: Option<IntRect>| {
        r.is_some_and(|r| {
            let x = win_pos.x + (r.x as f64 * scale) as i32 - pad;
            let y = win_pos.y + (r.y as f64 * scale) as i32 - pad;
            let w = (r.width as f64 * scale) as i32 + pad * 2;
            let h = (r.height as f64 * scale) as i32 + pad * 2;
            press.x >= x && press.x <= x + w && press.y >= y && press.y <= y + h
        })
    };
    let menu = MENU_RECT.lock().ok().and_then(|g| *g);
    let dock = DOCK_RECT.lock().ok().and_then(|g| *g);
    if inside(menu) || inside(dock) {
        return;
    }
    MENU_IS_OPEN.store(false, Ordering::Relaxed);
    let _ = app_handle.emit_to("dock", "dock-dismiss-menus", ());
}

/// Every quarter second, checks that Bloom's notch, dock and overlay take
/// clicks only where they should, from where the pointer really is.
///
/// Those three are transparent windows over the top, the bottom and the
/// whole of the screen. Over anything but their own content they must let
/// clicks through; one that doesn't makes the app under it look frozen (it
/// still shows, but nothing in it can be clicked) until Bloom restarts. They
/// were only switched on pointer moves seen by the mouse hook, so a hook
/// Windows dropped (it does when a busy moment, such as a game, makes the
/// hook late), a pointer resting after the last switch, or a window whose
/// style changed underneath (remade after a crash of its web view) could leave
/// one taking clicks. The guard compares each window's real style with what
/// Bloom decided, forces a fresh decision where they differ, and decides again
/// from the pointer's real position either way.
fn setup_input_guard(app_handle: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("bloom-input-guard".into())
        .spawn(move || {
            use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, WS_EX_TRANSPARENT};
            loop {
                std::thread::sleep(Duration::from_millis(250));
                if SHUTTING_DOWN.load(Ordering::Relaxed) {
                    continue;
                }
                for (label, decided) in [
                    ("main", &MH_LAST_MAIN_IGNORE),
                    ("dock", &MH_LAST_DOCK_IGNORE),
                    ("overlay", &MH_LAST_OV_IGNORE),
                ] {
                    let Some(hwnd) = cached_hwnd(&app_handle, label) else {
                        continue;
                    };
                    if !window_shown(hwnd) {
                        continue;
                    }
                    let _hit = hit_lock();
                    let wanted = decided.load(Ordering::Relaxed);
                    if wanted == -1 {
                        continue;
                    }
                    let style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
                    let passes_clicks = style & WS_EX_TRANSPARENT.0 != 0;
                    if passes_clicks != (wanted == 1) {
                        // Out of step: the next decision applies itself.
                        decided.store(-1, Ordering::Relaxed);
                    }
                }
                let mut point = windows::Win32::Foundation::POINT::default();
                if unsafe { GetCursorPos(&mut point) }.is_ok() {
                    process_mouse_position(&app_handle, point);
                }
            }
        });
}

/// Keeps notch hit-testing fresh while the cursor is stationary: completes a
/// pending top-edge dwell, forces interactivity during a held-button drag, and
/// catches boundary crossings the movement-only hook misses (so the click-
/// through flip — and its hover-clear — always happens).
fn setup_top_edge_watchdog(app_handle: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(TOP_EDGE_POLL_MS));
        if CAPTURE_UI_ACTIVE.load(Ordering::Relaxed) || SHUTTING_DOWN.load(Ordering::Relaxed) {
            continue;
        }
        let mut pt = windows::Win32::Foundation::POINT::default();
        if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_err() {
            continue;
        }
        let fg_fs = CURRENT_FOREGROUND_FULLSCREEN.load(Ordering::Relaxed);
        update_main_interaction(&app_handle, pt, now_ms(), fg_fs);
    });
}

/// Bottom-edge hover-intent delay in milliseconds. `0` disables the gate so the
/// dock reveals the moment the band is touched (the pre-fix behavior).
fn dock_edge_dwell_ms(app_handle: &AppHandle) -> i64 {
    crate::utils::get_setting_str(app_handle, "bloom-dock-edge-delay")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(BOTTOM_EDGE_DWELL_MS)
        .clamp(0, 2000)
}

/// True once the cursor has stayed inside the bottom band long enough to count
/// as a deliberate reveal. Mere contact does not arm: a pass along the bottom
/// strip on the way to a maximized window's bottom controls must not reveal the
/// dock. Leaving the band disarms and restarts the clock.
///
/// The mouse hook only fires on movement, so `setup_dock_edge_watchdog` also
/// calls this while a dwell is pending so it can complete on a stationary
/// cursor.
fn dock_edge_armed_for(
    app_handle: &AppHandle,
    cursor: windows::Win32::Foundation::POINT,
    now: i64,
    scale: f64,
) -> bool {
    let (mon_x, mon_y) = MH_CACHED_MON_POS
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or((0, 0));
    let (mon_w, mon_h) = MH_CACHED_MON_SIZE
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|s| (s.0 as i32, s.1 as i32))
        .unwrap_or((1920, 1080));

    let in_band = cursor.y >= (mon_y + mon_h - (BOTTOM_EDGE_BAND_PX * scale) as i32)
        && cursor.x >= mon_x
        && cursor.x <= (mon_x + mon_w);

    if !in_band {
        MH_DOCK_EDGE_ENTER_MS.store(0, Ordering::Relaxed);
        MH_DOCK_EDGE_ARMED.store(false, Ordering::Relaxed);
        return false;
    }
    if MH_DOCK_EDGE_ARMED.load(Ordering::Relaxed) {
        return true;
    }
    let dwell_ms = dock_edge_dwell_ms(app_handle);
    if dwell_ms <= 0 {
        MH_DOCK_EDGE_ARMED.store(true, Ordering::Relaxed);
        return true;
    }
    let enter = MH_DOCK_EDGE_ENTER_MS.load(Ordering::Relaxed);
    if enter == 0 {
        MH_DOCK_EDGE_ENTER_MS.store(now, Ordering::Relaxed);
        return false;
    }
    if now - enter >= dwell_ms {
        MH_DOCK_EDGE_ARMED.store(true, Ordering::Relaxed);
        return true;
    }
    false
}

/// Hover + click-through state for the dock window. Shared by the mouse hook
/// (on movement) and the bottom-edge watchdog (to finish a stationary dwell).
fn update_dock_interaction(
    app_handle: &AppHandle,
    cursor: windows::Win32::Foundation::POINT,
    now: i64,
    fg_fs: bool,
) {
    let _hit = hit_lock();
    if fg_fs {
        // Fullscreen foreground app or capture overlay: the dock must not
        // intercept input (e.g. a Snipping Tool selection ending at the
        // bottom edge), so keep it click-through.
        MH_DOCK_EDGE_ENTER_MS.store(0, Ordering::Relaxed);
        MH_DOCK_EDGE_ARMED.store(false, Ordering::Relaxed);
        if MH_LAST_DOCK_IGNORE.load(Ordering::Relaxed) != 1 {
            if let Some(dock_win) = app_handle.get_webview_window("dock") {
                let _ = dock_win.set_ignore_cursor_events(true);
            }
            MH_LAST_DOCK_IGNORE.store(1, Ordering::Relaxed);
        }
        if MH_LAST_EDGE_HOVER.swap(0, Ordering::Relaxed) != 0 {
            let _ = app_handle.emit("dock-edge-hover", false);
        }
        return;
    }

    let Some(dock_win) = app_handle.get_webview_window("dock") else {
        return;
    };
    let Some(dock_hwnd) = cached_hwnd(app_handle, "dock") else {
        return;
    };
    if !window_shown(dock_hwnd) {
        return;
    }

    let (mon_x, mon_y) = MH_CACHED_MON_POS
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or((0, 0));
    let (mon_w, mon_h) = MH_CACHED_MON_SIZE
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|s| (s.0 as i32, s.1 as i32))
        .unwrap_or((1920, 1080));

    let mut is_click_interactive = false;
    let mut is_hovered = false;
    let mut dock_span: Option<(i32, i32)> = None;

    let dock_rect_val = DOCK_WINDOW_RECT.lock().ok().and_then(|g| *g);

    if let Some((win_pos, win_size)) = dock_rect_val {
        let in_window = cursor.x >= win_pos.x
            && cursor.x <= (win_pos.x + win_size.width as i32)
            && cursor.y >= win_pos.y
            && cursor.y <= (win_pos.y + win_size.height as i32);

        if in_window {
            if let Ok(region) = DOCK_RECT.try_lock() {
                if let Some(r) = *region {
                    let scale = window_scale(dock_hwnd);
                    let pad_x = (5.0 * scale) as i32;
                    let pad_y_top = (8.0 * scale) as i32;
                    let pad_y_bottom = (5.0 * scale) as i32;
                    // Hysteresis keeps the dock interactive a little past
                    // its bounds once grabbed, so removing the edge-forced
                    // interactivity doesn't reintroduce boundary flicker.
                    let hyst = if MH_LAST_DOCK_IGNORE.load(Ordering::Relaxed) == 0 {
                        (10.0 * scale) as i32
                    } else {
                        0
                    };
                    let rx = win_pos.x + (r.x as f64 * scale) as i32 - pad_x - hyst;
                    let ry = win_pos.y + (r.y as f64 * scale) as i32 - pad_y_top - hyst;
                    let rw = (r.width as f64 * scale) as i32 + (pad_x * 2) + (hyst * 2);
                    let rh =
                        (r.height as f64 * scale) as i32 + pad_y_top + pad_y_bottom + (hyst * 2);
                    if cursor.x >= rx
                        && cursor.x <= (rx + rw)
                        && cursor.y >= ry
                        && cursor.y <= (ry + rh)
                    {
                        is_click_interactive = true;
                    }
                    dock_span = Some((rx, rx + rw));
                }
            }

            if !is_click_interactive && MENU_IS_OPEN.load(Ordering::Relaxed) {
                if let Ok(rect) = MENU_RECT.try_lock() {
                    if let Some(r) = *rect {
                        let scale = window_scale(dock_hwnd);
                        let rx = win_pos.x + (r.x as f64 * scale) as i32 - (5.0 * scale) as i32;
                        let ry = win_pos.y + (r.y as f64 * scale) as i32 - (5.0 * scale) as i32;
                        let rw = (r.width as f64 * scale) as i32 + (10.0 * scale) as i32;
                        let rh = (r.height as f64 * scale) as i32 + (10.0 * scale) as i32;
                        if cursor.x >= rx
                            && cursor.x <= (rx + rw)
                            && cursor.y >= ry
                            && cursor.y <= (ry + rh)
                        {
                            is_click_interactive = true;
                        }
                    }
                }
            }
        }
    }

    // Hot-edge detection (Bottom edge). Only a dwelled (armed) edge, a dock
    // hover, or a recent hover reveals the dock — a pass through the band does
    // not.
    let in_dock_hover = DOCK_IS_HOVERED.load(Ordering::Relaxed);
    let scale = window_scale(dock_hwnd);
    let edge_armed = dock_edge_armed_for(app_handle, cursor, now, scale);
    let at_bottom_edge = cursor.y >= (mon_y + mon_h - (BOTTOM_EDGE_BAND_PX * scale) as i32)
        && cursor.x >= mon_x
        && cursor.x <= (mon_x + mon_w);

    if edge_armed || in_dock_hover {
        is_hovered = true;
        MH_DOCK_EXPIRY_MS.store(now + 500, Ordering::Relaxed);
    }

    // Approaching along the bottom edge keeps the dock interactive while near
    // its horizontal span, so the reveal can't be clicked through mid-animation.
    // Only an armed edge may claim the strip; the corners and a mere pass stay
    // click-through.
    if at_bottom_edge && edge_armed {
        if let Some((span_left, span_right)) = dock_span {
            let edge_pad = (60.0 * scale) as i32;
            if cursor.x >= span_left - edge_pad && cursor.x <= span_right + edge_pad {
                is_click_interactive = true;
            }
        }
    }

    let final_dock_hover = is_hovered || now < MH_DOCK_EXPIRY_MS.load(Ordering::Relaxed);
    let prev = MH_LAST_EDGE_HOVER.load(Ordering::Relaxed);
    let new_val = if final_dock_hover { 1 } else { 0 };
    if prev != new_val {
        let _ = app_handle.emit("dock-edge-hover", final_dock_hover);
        MH_LAST_EDGE_HOVER.store(new_val, Ordering::Relaxed);
    }

    // Only the dock, its previews and its open menu take clicks. An open menu
    // used to make the whole dock window (the bottom 600 px of the screen)
    // take them, so the windows there stopped responding whenever a menu was
    // left open or its page missed closing it. A press outside the menu now
    // closes it through the mouse hook (`close_dock_menu_if_outside`).
    let should_ignore = !is_click_interactive;
    let prev_ignore = MH_LAST_DOCK_IGNORE.load(Ordering::Relaxed);
    let new_ignore = if should_ignore { 1 } else { 0 };
    if prev_ignore != new_ignore {
        re_assert_topmost(dock_hwnd);
        let _ = dock_win.set_ignore_cursor_events(should_ignore);
        MH_LAST_DOCK_IGNORE.store(new_ignore, Ordering::Relaxed);
    }
}

/// Keeps dock hit-testing fresh while the cursor is stationary: completes a
/// pending bottom-edge dwell and catches boundary crossings the movement-only
/// hook misses (so the click-through flip always happens).
fn setup_dock_edge_watchdog(app_handle: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(BOTTOM_EDGE_POLL_MS));
        if CAPTURE_UI_ACTIVE.load(Ordering::Relaxed) || SHUTTING_DOWN.load(Ordering::Relaxed) {
            continue;
        }
        let mut pt = windows::Win32::Foundation::POINT::default();
        if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_err() {
            continue;
        }
        let fg_fs = CURRENT_FOREGROUND_FULLSCREEN.load(Ordering::Relaxed);
        update_dock_interaction(&app_handle, pt, now_ms(), fg_fs);
    });
}

unsafe extern "system" fn mouse_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    const WM_LBUTTONDOWN: usize = 0x0201;
    const WM_RBUTTONDOWN: usize = 0x0204;
    const WM_MBUTTONDOWN: usize = 0x0207;
    if code >= 0 && !SHUTTING_DOWN.load(Ordering::Relaxed) {
        if wparam.0 == WM_MOUSEMOVE as usize {
            let cursor = (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt;
            queue_cursor(cursor);
            MOUSE_HOOK_LAST_EVENT_MS.store(hook_tick_ms(), Ordering::Relaxed);
        } else if matches!(wparam.0, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)
            && MENU_IS_OPEN.load(Ordering::Relaxed)
        {
            // Noted only: the worker decides whether it landed outside the
            // dock's menu (this callback must return at once).
            let cursor = (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt;
            MOUSE_HOOK_PRESS.store(pack_cursor(cursor), Ordering::Relaxed);
            MOUSE_HOOK_PRESS_PENDING.store(true, Ordering::Relaxed);
            queue_cursor(cursor);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn process_mouse_position(app_handle: &AppHandle, cursor: windows::Win32::Foundation::POINT) {
    if SHUTTING_DOWN.load(Ordering::Relaxed) {
        return;
    }
    let now = now_ms();
    {
        // While a capture UI (Snipping Tool) is up, Bloom is fully
        // click-through and skipped entirely so the tool owns the screen.
        if CAPTURE_UI_ACTIVE.load(Ordering::Relaxed) {
            if MH_LAST_MAIN_IGNORE.load(Ordering::Relaxed) != 1 {
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.set_ignore_cursor_events(true);
                }
                MH_LAST_MAIN_IGNORE.store(1, Ordering::Relaxed);
            }
            if MH_LAST_DOCK_IGNORE.load(Ordering::Relaxed) != 1 {
                if let Some(w) = app_handle.get_webview_window("dock") {
                    let _ = w.set_ignore_cursor_events(true);
                }
                MH_LAST_DOCK_IGNORE.store(1, Ordering::Relaxed);
            }
            if MH_LAST_OV_IGNORE.load(Ordering::Relaxed) != 1 {
                if let Some(w) = app_handle.get_webview_window("overlay") {
                    let _ = w.set_ignore_cursor_events(true);
                }
                MH_LAST_OV_IGNORE.store(1, Ordering::Relaxed);
            }
            if MH_LAST_EDGE_HOVER.swap(0, Ordering::Relaxed) != 0 {
                let _ = app_handle.emit("dock-edge-hover", false);
            }
            if MH_LAST_TOP_EDGE_HOVER.swap(0, Ordering::Relaxed) != 0 {
                let _ = app_handle.emit("notch-edge-hover", false);
            }
            return;
        }

        // Refresh cached monitor info every 1s
        if now - MH_LAST_MONITOR_UPDATE_MS.load(Ordering::Relaxed) > 1000 {
            if let Ok(Some(monitor)) = app_handle.primary_monitor() {
                let pos = *monitor.position();
                let size = *monitor.size();
                *MH_CACHED_MON_POS.lock().unwrap() = Some((pos.x, pos.y));
                *MH_CACHED_MON_SIZE.lock().unwrap() = Some((size.width, size.height));
                MH_CACHED_MON_SCALE.store(monitor.scale_factor().to_bits(), Ordering::Relaxed);
                MH_LAST_MONITOR_UPDATE_MS.store(now, Ordering::Relaxed);
            }
        }

        let cached_pos = MH_CACHED_MON_POS.lock().unwrap().unwrap_or((0, 0));
        let cached_size = MH_CACHED_MON_SIZE.lock().unwrap().unwrap_or((1920, 1080));
        let mon_x = cached_pos.0;
        let mon_y = cached_pos.1;
        let mon_w = cached_size.0 as i32;
        let mon_h = cached_size.1 as i32;
        // The volume (left) and brightness (right) cards, 42 x 196 px at the
        // notch's size, centred on the screen's side edges; from the cached
        // monitor, as this runs for every pointer move.
        let card_scale = f64::from_bits(MH_CACHED_MON_SCALE.load(Ordering::Relaxed))
            * crate::utils::get_notch_scale(app_handle);
        let card_w = (42.0 * card_scale) as i32;
        let card_h = (196.0 * card_scale) as i32;
        let card_y = mon_y + (mon_h / 2) - (card_h / 2);
        let over_card_at = |left: i32| {
            cursor.x >= left
                && cursor.x <= left + card_w
                && cursor.y >= card_y
                && cursor.y <= card_y + card_h
        };

        let fg_fs = CURRENT_FOREGROUND_FULLSCREEN.load(Ordering::Relaxed);

        // --- Dock Interaction ---
        update_dock_interaction(app_handle, cursor, now, fg_fs);

        // --- Main (TopBar) Interaction ---
        update_main_interaction(app_handle, cursor, now, fg_fs);

        // Expanded per-app volume mixer (if open) — the reported rect covers
        // the notch plus panel, so it keeps the card alive and clickable.
        let mixer_rect = volume_mixer_physical_rect_for(app_handle);
        let in_mixer = mixer_rect.is_some_and(|(rx, ry, rw, rh)| {
            cursor.x >= rx && cursor.x <= rx + rw && cursor.y >= ry && cursor.y <= ry + rh
        });

        // --- Left Edge (Volume) ---
        if !fg_fs {
            let at_left_edge =
                cursor.x <= (mon_x + 8) && cursor.y >= mon_y && cursor.y <= (mon_y + mon_h);

            // The collapsed card extends past the 8px edge band, so once the
            // edge hover is established, keep it alive while the cursor is
            // over the card too (otherwise pausing on the mixer button lets
            // the hover expire and the HUD hides mid-click).
            let over_card = !at_left_edge
                && !in_mixer
                && MH_LAST_LEFT_EDGE_HOVER.load(Ordering::Relaxed) != 0
                && over_card_at(mon_x);

            if at_left_edge || in_mixer || over_card {
                MH_LEFT_EXPIRY_MS.store(now + 500, Ordering::Relaxed);
            }

            let final_left_hover = now < MH_LEFT_EXPIRY_MS.load(Ordering::Relaxed);
            let prev = MH_LAST_LEFT_EDGE_HOVER.load(Ordering::Relaxed);
            let new_val = if final_left_hover { 1 } else { 0 };
            if prev != new_val {
                let _ = app_handle.emit("volume-edge-hover", final_left_hover);
                MH_LAST_LEFT_EDGE_HOVER.store(new_val, Ordering::Relaxed);
            }
        } else {
            let prev = MH_LAST_LEFT_EDGE_HOVER.load(Ordering::Relaxed);
            if prev != 0 {
                let _ = app_handle.emit("volume-edge-hover", false);
                MH_LAST_LEFT_EDGE_HOVER.store(0, Ordering::Relaxed);
            }
        }

        // --- Right Edge (Brightness) ---
        if !fg_fs {
            let at_right_edge =
                cursor.x >= (mon_x + mon_w - 8) && cursor.y >= mon_y && cursor.y <= (mon_y + mon_h);

            if at_right_edge {
                MH_RIGHT_EXPIRY_MS.store(now + 500, Ordering::Relaxed);
            }

            let final_right_hover = now < MH_RIGHT_EXPIRY_MS.load(Ordering::Relaxed);
            let prev = MH_LAST_RIGHT_EDGE_HOVER.load(Ordering::Relaxed);
            let new_val = if final_right_hover { 1 } else { 0 };
            if prev != new_val {
                let _ = app_handle.emit("brightness-edge-hover", final_right_hover);
                MH_LAST_RIGHT_EDGE_HOVER.store(new_val, Ordering::Relaxed);
            }
        } else {
            let prev = MH_LAST_RIGHT_EDGE_HOVER.load(Ordering::Relaxed);
            if prev != 0 {
                let _ = app_handle.emit("brightness-edge-hover", false);
                MH_LAST_RIGHT_EDGE_HOVER.store(0, Ordering::Relaxed);
            }
        }

        // --- Overlay cursor passthrough ---
        let _hit = hit_lock();
        if let Some(ov_win) = app_handle.get_webview_window("overlay") {
            if fg_fs {
                let prev = MH_LAST_OV_IGNORE.load(Ordering::Relaxed);
                if prev != 1 {
                    if let Some(hwnd) = cached_hwnd(app_handle, "overlay") {
                        re_assert_topmost(hwnd);
                    }
                    let _ = ov_win.set_ignore_cursor_events(true);
                    MH_LAST_OV_IGNORE.store(1, Ordering::Relaxed);
                }
            } else {
                // The overlay applies the notch's size as CSS zoom, so the
                // cards are larger than 42x196 CSS px for non-default sizes.
                let over_left = in_mixer || over_card_at(mon_x);
                let over_right = over_card_at(mon_x + mon_w - card_w);

                let should_ignore = !(over_left || over_right);
                let prev = MH_LAST_OV_IGNORE.load(Ordering::Relaxed);
                let new_val = if should_ignore { 1 } else { 0 };
                if prev != new_val {
                    if let Some(hwnd) = cached_hwnd(app_handle, "overlay") {
                        re_assert_topmost(hwnd);
                    }
                    let _ = ov_win.set_ignore_cursor_events(should_ignore);
                    MH_LAST_OV_IGNORE.store(new_val, Ordering::Relaxed);
                }
            }
        }
    }
}

pub fn trigger_app_scan() {
    if IS_SCANNING.load(Ordering::Relaxed) {
        return;
    }
    IS_SCANNING.store(true, Ordering::Relaxed);

    std::thread::spawn(|| {
        use windows::Win32::System::Com::{
            CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
        };
        use windows::Win32::UI::Shell::{
            FOLDERID_AppsFolder, IEnumIDList, ILCombine, ILFree, IShellFolder, SHGetDesktopFolder,
            SHGetKnownFolderIDList, SHGetNameFromIDList, SIGDN_DESKTOPABSOLUTEPARSING,
            SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY, SIGDN_URL,
        };
        let mut apps = Vec::new();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            // Use a scope to ensure COM objects are dropped before CoUninitialize
            {
                if let Ok(pidl_apps) = SHGetKnownFolderIDList(&FOLDERID_AppsFolder, 0, None) {
                    if let Ok(desktop) = SHGetDesktopFolder() {
                        if let Ok(apps_folder) =
                            desktop.BindToObject::<_, IShellFolder>(pidl_apps, None)
                        {
                            let mut enum_id: Option<IEnumIDList> = None;
                            let res = apps_folder.EnumObjects(
                                HWND(std::ptr::null_mut()),
                                (windows::Win32::UI::Shell::SHCONTF_FOLDERS.0
                                    | windows::Win32::UI::Shell::SHCONTF_NONFOLDERS.0)
                                    as u32,
                                &mut enum_id,
                            );

                            if res.is_ok() {
                                if let Some(enum_id) = enum_id {
                                    // Must be a real array the enumerator can write into;
                                    // `&mut [pidl_item]` would write into a temporary copy.
                                    let mut pidl_buf: [*mut windows::Win32::UI::Shell::Common::ITEMIDLIST; 1] =
										[std::ptr::null_mut()];
                                    let mut fetched = 0;
                                    while enum_id.Next(&mut pidl_buf, Some(&mut fetched)).is_ok()
                                        && fetched > 0
                                    {
                                        let pidl_item = pidl_buf[0];
                                        pidl_buf[0] = std::ptr::null_mut();
                                        if pidl_item.is_null() {
                                            continue;
                                        }

                                        // Child PIDLs from EnumObjects are relative; SHGetNameFromIDList
                                        // needs an absolute PIDL, otherwise every call fails with E_INVALIDARG.
                                        let absolute_pidl = ILCombine(
                                            Some(pidl_apps as *const _),
                                            Some(pidl_item as *const _),
                                        );
                                        if absolute_pidl.is_null() {
                                            CoTaskMemFree(Some(pidl_item as *const _));
                                            continue;
                                        }

                                        let name = if let Ok(n_ptr) =
                                            SHGetNameFromIDList(absolute_pidl, SIGDN_NORMALDISPLAY)
                                        {
                                            let s = String::from_utf16_lossy(
                                                windows::core::PCWSTR(n_ptr.0).as_wide(),
                                            );
                                            CoTaskMemFree(Some(n_ptr.0 as *const _));
                                            s
                                        } else {
                                            "Unknown".to_string()
                                        };

                                        // Parsing name: a full exe path for Win32 apps, an
                                        // AppUserModelID for packaged apps (Store/UWP/PWAs).
                                        let path = if let Ok(p_ptr) = SHGetNameFromIDList(
                                            absolute_pidl,
                                            SIGDN_DESKTOPABSOLUTEPARSING,
                                        ) {
                                            let s = String::from_utf16_lossy(
                                                windows::core::PCWSTR(p_ptr.0).as_wide(),
                                            );
                                            CoTaskMemFree(Some(p_ptr.0 as *const _));
                                            s
                                        } else if let Ok(p_ptr) =
                                            SHGetNameFromIDList(absolute_pidl, SIGDN_FILESYSPATH)
                                        {
                                            let s = String::from_utf16_lossy(
                                                windows::core::PCWSTR(p_ptr.0).as_wide(),
                                            );
                                            CoTaskMemFree(Some(p_ptr.0 as *const _));
                                            s
                                        } else if let Ok(p_ptr) =
                                            SHGetNameFromIDList(absolute_pidl, SIGDN_URL)
                                        {
                                            let s = String::from_utf16_lossy(
                                                windows::core::PCWSTR(p_ptr.0).as_wide(),
                                            );
                                            CoTaskMemFree(Some(p_ptr.0 as *const _));
                                            s
                                        } else {
                                            name.clone()
                                        };

                                        let path = normalize_apps_folder_path(&path);
                                        if is_launchable_entry(&name, &path) {
                                            // AUMIDs have no executable of their own; storing the
                                            // whole id here would make exe-name matching think any
                                            // browser window belongs to this app.
                                            let executable =
                                                if path.contains('\\') || path.contains('/') {
                                                    std::path::Path::new(&path)
                                                        .file_name()
                                                        .and_then(|n| n.to_str())
                                                        .map(|s| s.to_string())
                                                } else {
                                                    None
                                                };
                                            apps.push(AppInfo {
                                                name,
                                                path,
                                                icon: None,
                                                is_running: false,
                                                hwnd: None,
                                                executable,
                                                all_hwnds: None,
                                            });
                                        }
                                        ILFree(Some(absolute_pidl as *const _));
                                        CoTaskMemFree(Some(pidl_item as *const _));
                                    }
                                }
                            }
                        }
                    }
                    CoTaskMemFree(Some(pidl_apps as *const _));
                }
            } // Close COM scope
            CoUninitialize();
        }

        // Scan Start Menu .lnk shortcuts (depth-limited, no .exe scanning)
        // This catches Win32 apps that FOLDERID_AppsFolder may miss
        let mut start_menu_dirs: Vec<String> = Vec::new();
        if let Ok(programdata) = std::env::var("PROGRAMDATA") {
            start_menu_dirs.push(format!(
                r"{}\Microsoft\Windows\Start Menu\Programs",
                programdata
            ));
        }
        if let Ok(appdata) = std::env::var("APPDATA") {
            start_menu_dirs.push(format!(
                r"{}\Microsoft\Windows\Start Menu\Programs",
                appdata
            ));
        }

        // Shortcut resolution uses IShellLinkW, which needs COM on this thread.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        for dir in &start_menu_dirs {
            let root = std::path::Path::new(dir);
            if root.exists() {
                collect_shortcuts(root, &mut apps, 0);
            }
        }
        unsafe {
            CoUninitialize();
        }

        if let Some(c) = INSTALLED_APPS_CACHE.get() {
            if let Ok(mut lock) = c.lock() {
                *lock = apps;
            }
        }
        IS_SCANNING.store(false, Ordering::Relaxed);
        // Pinned dock items saved under their executable's name pick up their
        // Start menu name once this list exists (see refresh_pinned_names).
        if let Some(app) = crate::state::DISPLAY_MONITOR_HANDLE.get() {
            let _ = app.emit("installed-apps-updated", ());
        }
    });
}

/// The Start menu's parsing name for a desktop app is often a path under a
/// known folder written as "{GUID}\dir\app.exe", which no file API can
/// open: launching it failed and its icon came out generic. It becomes the
/// real program path, or, when that can't be found, a shell:AppsFolder id
/// that Windows launches and draws exactly like its Start menu entry.
pub fn normalize_apps_folder_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('{') && trimmed.contains("}\\") {
        return crate::launcher::actions::launcher_resolve_app_path(trimmed.to_string())
            .unwrap_or_else(|| format!("shell:AppsFolder\\{}", trimmed));
    }
    trimmed.to_string()
}

/// Filters out shell entries that are not real launchable apps (web links,
/// documents, protocol handlers, helpers) so the add-app list stays clean.
fn is_launchable_entry(name: &str, path: &str) -> bool {
    if name.is_empty() || name == "Unknown" || is_helper_name(name) {
        return false;
    }
    let lower = path.to_lowercase();
    if lower.is_empty() {
        return false;
    }
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file:")
        || lower.starts_with("steam:")
        || lower.starts_with("::{")
        || (lower.starts_with("shell:") && !lower.starts_with("shell:appsfolder"))
    {
        return false;
    }
    true
}

/// Companion/helper entries nobody pins: uninstallers, installers, help and
/// documentation, troubleshooters. Matched on whole words only — substring
/// matching would nuke legit names like "Helper" or "Remover".
fn is_helper_name(name: &str) -> bool {
    const HELPER_WORDS: &[&str] = &[
        "uninstall",
        "uninstaller",
        "installer",
        "setup",
        "help",
        "readme",
        "manual",
        "documentation",
        "troubleshoot",
        "troubleshooting",
        "repair",
    ];
    let lower = name.to_lowercase();
    if lower.contains("release notes") {
        return true;
    }
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| HELPER_WORDS.contains(&token))
}

fn collect_shortcuts(dir: &std::path::Path, apps: &mut Vec<AppInfo>, depth: i32) {
    if depth > 3 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_shortcuts(&path, apps, depth + 1);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
            {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                if is_helper_name(&name) || name.starts_with("Install") {
                    continue;
                }

                // Keep the .lnk itself: its args identify PWAs and arguments must be
                // passed through when launching. The resolved target is stored as the
                // executable name so pinned entries still match running windows.
                let path_str = path.to_string_lossy().to_string();
                let executable =
                    crate::utils::resolve_shortcut(&path_str).and_then(|(target, _args)| {
                        let file = std::path::Path::new(&target)
                            .file_name()
                            .and_then(|n| n.to_str())?
                            .to_string();
                        // UWP shortcuts launch explorer.exe with a shell:AppsFolder argument;
                        // storing that would wrongly match File Explorer windows.
                        if file.eq_ignore_ascii_case("explorer.exe") {
                            None
                        } else {
                            Some(file)
                        }
                    });

                if !apps.iter().any(|a| a.name == name || a.path == path_str) {
                    apps.push(AppInfo {
                        name,
                        path: path_str,
                        icon: None,
                        is_running: false,
                        hwnd: None,
                        executable,
                        all_hwnds: None,
                    });
                }
            }
        }
    }
}

pub fn sync_overlays(app: &AppHandle) {
    // Don't reposition while splash is playing
    if crate::state::OVERLAY_IN_SPLASH.load(Ordering::Relaxed) {
        return;
    }
    // Full-screen overlay — notches render at left/right edges via CSS
    // The window covers the entire primary monitor so it never needs repositioning
    if let Some(ov_win) = app.get_webview_window("overlay") {
        if let Ok(Some(monitor)) = ov_win.primary_monitor() {
            let size = monitor.size();
            let pos = monitor.position();
            let _ = ov_win.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
            let _ = ov_win.set_size(tauri::PhysicalSize::new(size.width, size.height));
        }
        if let Ok(hwnd) = ov_win.hwnd() {
            re_assert_topmost(hwnd);
        }
    }
}

/// The notch window's height in pixels: room for its pages, or the whole
/// screen while a note is open.
pub fn notch_window_height(monitor_height: u32, scale: f64, bloom_scale: f64) -> i32 {
    if NOTCH_TALL.load(Ordering::Relaxed) {
        monitor_height as i32
    } else {
        (((420.0 * bloom_scale) * scale) as i32).min(monitor_height as i32)
    }
}

/// A note opened or closed in the notch: its window grows to the screen's
/// height, or back. Only the height changes; where it is and what it
/// reserves stay as they are.
#[tauri::command]
pub fn set_notch_tall(app: AppHandle, tall: bool) {
    if NOTCH_TALL.swap(tall, Ordering::Relaxed) == tall {
        return;
    }
    let Some(main_win) = app.get_webview_window("main") else {
        return;
    };
    let Ok(Some(monitor)) = main_win.primary_monitor() else {
        return;
    };
    let height = notch_window_height(
        monitor.size().height,
        monitor.scale_factor(),
        crate::utils::get_notch_scale(&app),
    );
    let Ok(hwnd) = main_win.hwnd() else {
        return;
    };
    unsafe {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        };
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_ok() {
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                rect.right - rect.left,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

pub fn register_appbar(window: tauri::WebviewWindow) {
    if let Ok(Some(monitor)) = window.app_handle().primary_monitor() {
        let m_size = monitor.size();
        let m_pos = monitor.position();
        let hwnd = window.hwnd().unwrap();
        let scale = monitor.scale_factor();
        let bloom_scale = crate::utils::get_notch_scale(window.app_handle());
        let ph = notch_window_height(m_size.height, scale, bloom_scale);
        let pr = ((40.0 * bloom_scale) * scale) as i32; // Scale the reserved top screen space

        unsafe {
            use windows::Win32::Foundation::RECT;
            use windows::Win32::UI::Shell::{
                SHAppBarMessage, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_SETPOS, APPBARDATA,
            };
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, WS_EX_NOACTIVATE as WS_EX_NA,
                WS_EX_TOOLWINDOW,
            };

            // Set styles first
            let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as usize;
            ex_style |= (WS_EX_TOOLWINDOW.0 | WS_EX_NA.0) as usize;
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);

            let mut abd = APPBARDATA {
                cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                hWnd: hwnd,
                ..Default::default()
            };

            if !MAIN_APPBAR_REGISTERED.load(Ordering::Relaxed) {
                // Already known to Explorer (the flag was reset): start over.
                if SHAppBarMessage(ABM_NEW, &mut abd) == 0 {
                    SHAppBarMessage(windows::Win32::UI::Shell::ABM_REMOVE, &mut abd);
                    if SHAppBarMessage(ABM_NEW, &mut abd) == 0 {
                        // No taskbar yet (Bloom started first at sign-in):
                        // registered again once it is there.
                        return;
                    }
                }
                MAIN_APPBAR_REGISTERED.store(true, Ordering::Relaxed);
            }

            abd.uEdge = ABE_TOP;
            abd.rc = RECT {
                left: m_pos.x,
                top: m_pos.y,
                right: m_pos.x + m_size.width as i32,
                bottom: m_pos.y + pr,
            };

            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            SHAppBarMessage(ABM_SETPOS, &mut abd);

            // Use the shell-approved rect for the final position, but keep our ph height for the window
            let final_width = abd.rc.right - abd.rc.left;

            let mut current_rect = RECT::default();
            let mut already_positioned = false;
            if GetWindowRect(hwnd, &mut current_rect).is_ok() {
                let current_width = current_rect.right - current_rect.left;
                let current_height = current_rect.bottom - current_rect.top;
                if current_rect.left == abd.rc.left
                    && current_rect.top == abd.rc.top
                    && current_width == final_width
                    && current_height == ph
                {
                    already_positioned = true;
                }
            }

            if !already_positioned {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    abd.rc.left,
                    abd.rc.top,
                    final_width,
                    ph,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }

            // Re-assert topmost after repositioning — use re_assert_topmost instead of
            // set_always_on_top(true) to include SWP_NOACTIVATE and re-stamp WS_EX_NOACTIVATE.
            // This prevents WM_ACTIVATE from reaching WebView2, which caused bloom windows
            // to blank/hide when other windows were minimized or closed.
            re_assert_topmost(hwnd);

            if !window.is_visible().unwrap_or(false) {
                let _ = window.show();
            }
        }
    } else {
        let w = window.clone();
        tauri::async_runtime::spawn(async move {
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if let Ok(Some(_monitor)) = w.app_handle().primary_monitor() {
                    // Reconcile rather than register directly: the notch mode
                    // may have changed while waiting for the monitor.
                    reconcile_main_appbar(w.app_handle());
                    break;
                }
            }
        });
    }
}

/// Serializes main AppBar register/unregister decisions. Mode switches and
/// display/power events can otherwise interleave between reading the notch mode
/// and applying it, leaving the reservation in the wrong state until the next
/// reconcile.
static MAIN_APPBAR_LOCK: Mutex<()> = Mutex::new(());

fn lock_main_appbar() -> std::sync::MutexGuard<'static, ()> {
    MAIN_APPBAR_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Apply a reservation decision to the main window. Callers must hold
/// `MAIN_APPBAR_LOCK`.
fn set_main_appbar_reservation(main_win: tauri::WebviewWindow, reserve: bool) {
    if reserve {
        register_appbar(main_win);
    } else if let Ok(hwnd) = main_win.hwnd() {
        // Ask the shell to remove the reservation unconditionally: the flag
        // may look clean while the shell still holds the strip.
        unregister_appbar_native(hwnd);
        MAIN_APPBAR_REGISTERED.store(false, Ordering::Relaxed);
        re_assert_topmost(hwnd);
    }
}

/// Whether the notch's 40px top strip should currently be reserved as an AppBar.
/// Only fixed mode reserves screen space; smart and peek hide the notch freely.
/// An unset mode defaults to fixed, matching SETTINGS.md and `re_register_appbars`.
fn notch_mode_reserves_work_area(mode: Option<&str>) -> bool {
    mode.map(|m| m == "fixed").unwrap_or(true)
}

/// Bring the main AppBar registration in line with the configured notch mode.
///
/// `MAIN_APPBAR_REGISTERED` can drift from the setting: a DPI/scale change used
/// to register the window unconditionally, and subsequent power/display events
/// kept re-registering it from the stale flag, leaving the work-area gap in
/// smart/peek mode until restart (issue #109). Deciding from the mode makes
/// every re-registration path self-healing.
pub fn reconcile_main_appbar(app: &AppHandle) {
    let _guard = lock_main_appbar();
    let mode = get_setting_str(app, "bloom-notch-mode");
    if let Some(main_win) = app.get_webview_window("main") {
        set_main_appbar_reservation(main_win, notch_mode_reserves_work_area(mode.as_deref()));
    }
}

/// Apply an explicit mode to the main AppBar. `change_notch_mode` knows the
/// target mode even when the settings cache has not caught up yet, so it cannot
/// rely on a reconcile read.
pub fn apply_main_appbar_mode(app: &AppHandle, reserve: bool) {
    let _guard = lock_main_appbar();
    if let Some(main_win) = app.get_webview_window("main") {
        set_main_appbar_reservation(main_win, reserve);
    }
}

pub fn register_dock_appbar(window: tauri::WebviewWindow) {
    register_dock_appbar_inner(window, 0);
}

/// Explorer's taskbar appeared after Bloom (Bloom starts first at sign-in) or
/// was made again (Explorer restarted). Explorer then starts with no bars that
/// reserve a strip, so the notch and the dock register again, and the new
/// taskbar is hidden again behind the dock.
pub fn explorer_taskbar_changed(app: &AppHandle) {
    if SHUTTING_DOWN.load(Ordering::Relaxed) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Let the new taskbar finish starting.
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        if SHUTTING_DOWN.load(Ordering::Relaxed) {
            return;
        }
        // The taskbar hooks follow Explorer's process.
        let _ = app.run_on_main_thread(setup_taskbar_hook);
        if let Some(main_win) = app.get_webview_window("main") {
            if let Ok(hwnd) = main_win.hwnd() {
                unregister_appbar_native(hwnd);
            }
        }
        MAIN_APPBAR_REGISTERED.store(false, Ordering::Relaxed);
        reconcile_main_appbar(&app);
        if crate::utils::dock_enabled() {
            if let Some(dock_win) = app.get_webview_window("dock") {
                let _guard = lock_dock_appbar();
                if let Ok(hwnd) = dock_win.hwnd() {
                    unregister_appbar_native(hwnd);
                }
                DOCK_APPBAR_REGISTERED.store(false, Ordering::Relaxed);
            }
            let mode = match get_setting_str(&app, "bloom-dock-mode") {
                Some(mode) if mode != "auto-hide" => mode,
                _ => "smart".to_string(),
            };
            crate::commands::init_dock(app.clone(), mode).await;
        }
        sync_overlays(&app);
    });
}

static DOCK_APPBAR_LOCK: Mutex<()> = Mutex::new(());

fn lock_dock_appbar() -> std::sync::MutexGuard<'static, ()> {
    DOCK_APPBAR_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn disable_dock_appbar(window: tauri::WebviewWindow) {
    let _guard = lock_dock_appbar();
    let _ = window.hide();
    if let Ok(hwnd) = window.hwnd() {
        unregister_appbar_native(hwnd);
    }
    DOCK_APPBAR_REGISTERED.store(false, Ordering::Relaxed);
}

fn register_dock_appbar_inner(window: tauri::WebviewWindow, attempt: i32) {
    // Registration retries may outlive a settings change that disabled the dock.
    if !crate::utils::dock_enabled() {
        return;
    }
    if let Ok(Some(monitor)) = window.app_handle().primary_monitor() {
        let m_size = monitor.size();
        let m_pos = monitor.position();
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let scale = monitor.scale_factor();
        let bloom_scale = crate::utils::get_dock_scale(window.app_handle());

        // ph = full physical window height. outer_size() can return 0 before the
        // window has rendered. Never guess a value — bail and let the retry wrapper handle it.
        let ph = window.outer_size().map(|s| s.height as i32).unwrap_or(0);
        if ph <= 0 {
            if attempt < 10 {
                let w = window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    register_dock_appbar_inner(w, attempt + 1);
                });
            }
            return;
        }

        // The dock's own height (as it draws it) and the gap above it (in
        // screen pixels, a setting), so snapped and maximized windows stop
        // just above the dock. Until
        // the dock has said how tall it is, its usual height.
        let dock_height = crate::commands::DOCK_HEIGHT
            .lock()
            .ok()
            .and_then(|h| *h)
            .unwrap_or(52.0 * bloom_scale);
        let pr =
            (dock_height * scale).round() as i32 + crate::commands::dock_gap(window.app_handle());

        // Serialize with disable_dock_appbar, then check the setting again.
        // A pending registration must not outlive a dock-disable request.
        let _guard = lock_dock_appbar();
        if !crate::utils::dock_enabled() {
            return;
        }

        unsafe {
            use windows::Win32::Foundation::RECT;
            use windows::Win32::UI::Shell::{
                SHAppBarMessage, ABE_BOTTOM, ABM_NEW, ABM_QUERYPOS, ABM_SETPOS, APPBARDATA,
            };
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, GetWindowRect, RegisterWindowMessageW, SetWindowLongPtrW,
                SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
                WS_EX_NOACTIVATE as WS_EX_NA, WS_EX_TOOLWINDOW,
            };

            // Set extended styles (ToolWindow and NoActivate)
            let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as usize;
            ex_style |= (WS_EX_TOOLWINDOW.0 | WS_EX_NA.0) as usize;
            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);

            let callback_message = RegisterWindowMessageW(windows::core::w!("BloomDockAppBar"));
            if callback_message == 0 {
                return;
            }
            let mut abd = APPBARDATA {
                cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                hWnd: hwnd,
                uCallbackMessage: callback_message,
                ..Default::default()
            };

            if !DOCK_APPBAR_REGISTERED.load(Ordering::Relaxed) {
                // Already known to Explorer (the flag was reset): start over.
                if SHAppBarMessage(ABM_NEW, &mut abd) == 0 {
                    SHAppBarMessage(windows::Win32::UI::Shell::ABM_REMOVE, &mut abd);
                    if SHAppBarMessage(ABM_NEW, &mut abd) == 0 {
                        // No taskbar yet (Bloom started first at sign-in).
                        if attempt < 30 {
                            let w = window.clone();
                            tauri::async_runtime::spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                                register_dock_appbar_inner(w, attempt + 1);
                            });
                        }
                        return;
                    }
                }
                DOCK_APPBAR_REGISTERED.store(true, Ordering::Relaxed);
            }

            abd.uEdge = ABE_BOTTOM;
            abd.rc = RECT {
                left: m_pos.x,
                top: m_pos.y + m_size.height as i32 - pr,
                right: m_pos.x + m_size.width as i32,
                bottom: m_pos.y + m_size.height as i32,
            };

            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            SHAppBarMessage(ABM_SETPOS, &mut abd);

            // Critical: Force the window to the actual bottom of the screen,
            // ignoring what ABM_SETPOS might have tried to "correct" (like stacking on invisible taskbar)
            let final_y = m_pos.y + m_size.height as i32 - ph;
            let final_width = abd.rc.right - abd.rc.left;
            if final_width <= 0 {
                return;
            }

            let mut current_rect = RECT::default();
            let mut already_positioned = false;
            if GetWindowRect(hwnd, &mut current_rect).is_ok() {
                let current_width = current_rect.right - current_rect.left;
                let current_height = current_rect.bottom - current_rect.top;
                if current_rect.left == abd.rc.left
                    && current_rect.top == final_y
                    && current_width == final_width
                    && current_height == ph
                {
                    already_positioned = true;
                }
            }

            if !already_positioned
                && SetWindowPos(
                    hwnd,
                    None,
                    abd.rc.left,
                    final_y,
                    final_width,
                    ph,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )
                .is_err()
            {
                return;
            }
            let mut placed = RECT::default();
            if GetWindowRect(hwnd, &mut placed).is_err()
                || placed.left != abd.rc.left
                || placed.top != final_y
                || placed.right - placed.left != final_width
                || placed.bottom - placed.top != ph
            {
                return;
            }

            // Re-assert topmost — same reason as register_appbar.
            re_assert_topmost(hwnd);

            // Always ensure the window is visible — show() is idempotent
            if !window.is_visible().unwrap_or(false) && window.show().is_err() {
                return;
            }
            if window.is_visible().unwrap_or(false) && set_taskbar_visibility(false, false) {
                NATIVE_TASKBAR_HIDDEN.store(true, Ordering::Relaxed);
            }
        }
    } else {
        let w = window.clone();
        tauri::async_runtime::spawn(async move {
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if let Ok(Some(_monitor)) = w.app_handle().primary_monitor() {
                    register_dock_appbar(w);
                    break;
                }
            }
        });
    }
}

/// True when a top-level dialog is a Windows property sheet, detected by its
/// tab-control child. This is locale-independent, unlike matching the
/// "Properties" window title.
unsafe fn dialog_has_tab_control(hwnd: HWND) -> bool {
    unsafe extern "system" fn child_proc(child: HWND, lparam: LPARAM) -> BOOL {
        let found = &mut *(lparam.0 as *mut bool);
        let mut class_name = [0u8; 64];
        let len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(child, &mut class_name);
        if std::str::from_utf8(&class_name[..len as usize]).unwrap_or("") == "SysTabControl32" {
            *found = true;
            return BOOL(0);
        }
        BOOL(1)
    }

    let mut found = false;
    let _ = windows::Win32::UI::WindowsAndMessaging::EnumChildWindows(
        Some(hwnd),
        Some(child_proc),
        LPARAM(&mut found as *mut bool as isize),
    );
    found
}

pub unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let apps = &mut *(lparam.0 as *mut Vec<AppInfo>);

    if IsWindowVisible(hwnd).as_bool() {
        // DWM-cloaked windows are not actually on screen: closed/suspended UWP
        // apps keep a cloaked frame alive, and windows on other virtual desktops
        // are shell-cloaked. Neither belongs in the dock. IsWindowVisible stays
        // true for them, so this needs the DWM check.
        {
            let mut cloaked = 0u32;
            let size = std::mem::size_of::<u32>() as u32;
            if windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                hwnd,
                windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
                &mut cloaked as *mut _ as *mut _,
                size,
            )
            .is_ok()
                && cloaked != 0
            {
                return true.into();
            }
        }

        let mut text = [0u16; 512];
        let len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut text);
        if len > 0 {
            let title = String::from_utf16_lossy(&text[..len as usize]);

            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

            // Basic filter for top-level app windows
            // We include windows without captions if they don't have the ToolWindow style,
            // as many games and modern apps (like Spotify/Valorant) lack WS_CAPTION.
            if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
                return true.into();
            }

            // The native taskbar's own rule: a window owned by another window
            // (a dialog, a pop-up, a find bar) and a non-activating window only
            // get a taskbar button when they opt in with WS_EX_APPWINDOW. Without
            // this, an app's dialogs show up in the dock next to the app itself.
            {
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetWindow, GW_OWNER, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
                };
                let app_window = (ex_style & WS_EX_APPWINDOW.0) != 0;
                let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null());
                let no_activate = (ex_style & WS_EX_NOACTIVATE.0) != 0;
                if !app_window && (owned || no_activate) {
                    return true.into();
                }
            }

            // Filter out system containers and background stuff
            if title == "Program Manager" || title == "Bloom" || title == "Bloom Dock" {
                return true.into();
            }

            // Portals' loading pop-up, kept off the screen (app_tweaks.rs):
            // not an app window to show in the dock either.
            if crate::app_tweaks::is_portals_splash(hwnd) {
                return true.into();
            }

            let mut process_id = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));

            if let Ok(process_handle) =
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
            {
                let mut path_buf = [0u16; 1024];
                let mut path_len = path_buf.len() as u32;
                if QueryFullProcessImageNameW(
                    process_handle,
                    PROCESS_NAME_WIN32,
                    windows::core::PWSTR(path_buf.as_mut_ptr()),
                    &mut path_len,
                )
                .is_ok()
                {
                    let path = String::from_utf16_lossy(&path_buf[..path_len as usize]);
                    let lowercase_path = path.to_lowercase();

                    let mut class_name = [0u8; 256];
                    let class_len = windows::Win32::UI::WindowsAndMessaging::GetClassNameA(
                        hwnd,
                        &mut class_name,
                    );
                    let window_class =
                        std::str::from_utf8(&class_name[..class_len as usize]).unwrap_or("");

                    // Explorer hosts folder windows (CabinetWClass/ExploreWClass) and
                    // shell property sheets (`#32770` dialogs that contain a tab
                    // control). Both are tracked so the Properties window shows up
                    // under the File Explorer dock item.
                    let is_explorer_window = window_class == "CabinetWClass"
                        || window_class == "ExploreWClass"
                        || (window_class == "#32770" && dialog_has_tab_control(hwnd));

                    // Filter out Bloom itself (except the Settings window) and some common background processes
                    if (lowercase_path.contains("bloom.exe") && title != "Settings")
                        || lowercase_path.contains("conhost.exe")
                        || (lowercase_path.contains("explorer.exe") && !is_explorer_window)
                        || lowercase_path.contains("shellexperiencehost.exe")
                        || lowercase_path.contains("searchhost.exe")
                        || lowercase_path.contains("textinputhost.exe")
                        || (lowercase_path.contains("applicationframehost.exe")
                            && window_class != "ApplicationFrameWindow")
                    {
                        let _ = CloseHandle(process_handle);
                        return true.into();
                    }

                    let name = Path::new(&path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&title)
                        .replace(".exe", "");

                    let exe_name = Path::new(&path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string());

                    // The visible top-level window of a UWP app is the
                    // ApplicationFrameWindow owned by ApplicationFrameHost.exe; the
                    // app's own Windows.UI.Core.CoreWindow is a separate top-level
                    // window and not the one users interact with. The frame carries
                    // the title and package identity, so track it and skip the inner
                    // core window to avoid duplicate dock items.
                    let is_uwp_core_window = window_class == "Windows.UI.Core.CoreWindow";
                    let is_uwp_frame = window_class == "ApplicationFrameWindow";

                    let is_browser_host = lowercase_path.contains("msedge.exe")
                        || lowercase_path.contains("chrome.exe")
                        || lowercase_path.contains("brave.exe");

                    // A browser window and an installed PWA running under the same
                    // browser can share a title, so the title alone cannot tell them
                    // apart. The window's AppUserModelID can: PWA windows carry a web
                    // app id (or a package id for Store-installed PWAs), while regular
                    // browser windows only carry the browser itself ("MSEdge", "Chrome",
                    // "Brave", ...). A missing id falls back to the title so unusual
                    // hosts keep the previous behaviour.
                    let window_aumid = if is_uwp_frame
                        || lowercase_path.contains("\\windowsapps\\")
                        || is_browser_host
                    {
                        crate::utils::get_window_app_user_model_id(hwnd)
                    } else {
                        None
                    };
                    let is_browser_pwa = is_browser_host
                        && window_aumid
                            .as_deref()
                            .map_or(false, crate::commands::is_browser_pwa_aumid);

                    let final_name = if ((is_browser_host
                        && (is_browser_pwa || window_aumid.is_none()))
                        || name == "ApplicationFrameHost"
                        || name == "SystemSettings")
                        && !title.is_empty()
                    {
                        // Extract a cleaner name from the window title for host processes (PWAs, UWP apps)
                        title
                            .split(" - ")
                            .next()
                            .map(|s| s.trim())
                            .unwrap_or(&title)
                            .to_string()
                    } else if name == "explorer" && title.is_empty() {
                        "File Explorer".to_string()
                    } else {
                        // The Start menu name ("WhatsApp", "LocalSend") rather than
                        // the executable's ("WhatsApp.Root", "localsend_app").
                        crate::commands::app_display_name(&path, window_aumid.as_deref())
                    };

                    // Only avoid adding the exact same window handle (HWND) multiple times
                    let already_exists = apps.iter().any(|a| a.hwnd == Some(hwnd.0 as isize));

                    if !already_exists && !is_uwp_core_window {
                        // Packaged apps (Store/UWP/PWAs) expose their package identity on the
                        // window. Using it as the path makes pinned shell-app entries match
                        // their running windows and gives the icon code an exact AUMID.
                        let path = if is_uwp_frame
                            || lowercase_path.contains("\\windowsapps\\")
                            || is_browser_host
                        {
                            match window_aumid {
                                Some(aumid) if aumid.contains('!') => aumid,
                                _ => path,
                            }
                        } else {
                            path
                        };
                        apps.push(AppInfo {
                            name: final_name,
                            path,
                            icon: None,
                            is_running: true,
                            hwnd: Some(hwnd.0 as isize),
                            executable: exe_name,
                            all_hwnds: None,
                        });
                    }
                }
                let _ = CloseHandle(process_handle);
            }
        }
    }
    true.into()
}

pub fn unregister_appbar_native(hwnd: HWND) {
    unsafe {
        use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_REMOVE, APPBARDATA};
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            ..Default::default()
        };
        SHAppBarMessage(ABM_REMOVE, &mut abd);
    }
}

fn reposition_all_windows(app_handle: &AppHandle) {
    if SHUTTING_DOWN.load(Ordering::Relaxed) {
        return;
    }
    reconcile_main_appbar(app_handle);
    // Only reposition the dock if it's enabled in settings.
    // Without this guard, power events (plug/unplug, wake) would re-show
    // a dock that the user had previously disabled.
    if crate::utils::dock_enabled() {
        if let Some(dock_win) = app_handle.get_webview_window("dock") {
            if DOCK_APPBAR_REGISTERED.load(Ordering::Relaxed) {
                register_dock_appbar(dock_win);
            } else {
                // Auto-hide mode: reposition dock at bottom of screen
                reposition_autohide_dock(app_handle, dock_win);
            }
        }
    }
    if NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed) {
        set_taskbar_visibility(false, false);
    }
    sync_overlays(app_handle);
    // Re-assert topmost on all windows after repositioning to recover from any z-order loss
    if let Some(main_win) = app_handle.get_webview_window("main") {
        if let Ok(hwnd) = main_win.hwnd() {
            re_assert_topmost(hwnd);
        }
    }
    if let Some(dock_win) = app_handle.get_webview_window("dock") {
        if let Ok(hwnd) = dock_win.hwnd() {
            re_assert_topmost(hwnd);
        }
    }
}

fn reposition_autohide_dock(app_handle: &AppHandle, dock_win: tauri::WebviewWindow) {
    let dock_clone = dock_win.clone();
    let ah = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        for _attempt in 0..5 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if !crate::utils::dock_enabled() {
                return;
            }
            let hwnd_val = match dock_clone.hwnd() {
                Ok(h) => h.0 as isize,
                Err(_) => continue,
            };
            let ph = match dock_clone.outer_size() {
                Ok(s) => s.height as i32,
                Err(_) => continue,
            };
            if ph <= 10 {
                continue;
            }
            let monitor_info = dock_clone.primary_monitor().ok().flatten().map(|m| {
                let s = m.size();
                let p = m.position();
                (s.width as i32, s.height as i32, p.x, p.y)
            });
            if let Some((m_w, m_h, m_x, m_y)) = monitor_info {
                let final_y = m_y + m_h - ph;
                unsafe {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
                    };
                    let _ = SetWindowPos(
                        HWND(hwnd_val as *mut _),
                        None,
                        m_x,
                        final_y,
                        m_w,
                        ph,
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                }
                if let Ok(hwnd) = dock_clone.hwnd() {
                    re_assert_topmost(hwnd);
                }
                if crate::utils::dock_enabled() {
                    let _ = dock_clone.show();
                }
                break;
            }
        }
        if NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed) {
            set_taskbar_visibility(false, false);
        }
        sync_overlays(&ah);
    });
}

pub fn setup_display_change_monitor(app_handle: AppHandle) {
    let _ = crate::state::DISPLAY_MONITOR_HANDLE.set(app_handle);

    std::thread::spawn(|| unsafe {
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::*;

        let class_name = windows::core::PCSTR(c"BloomDisplayMonitor".as_ptr() as *const u8);
        let h_inst = GetModuleHandleW(None).unwrap_or_default().into();

        let wnd_class = WNDCLASSEXA {
            cbSize: std::mem::size_of::<WNDCLASSEXA>() as u32,
            lpfnWndProc: Some(display_monitor_proc),
            hInstance: h_inst,
            lpszClassName: class_name,
            ..Default::default()
        };

        if RegisterClassExA(&wnd_class) == 0 {
            return;
        }

        let hwnd = CreateWindowExA(
            WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0),
            class_name,
            windows::core::PCSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(h_inst),
            None,
        );

        let hwnd = match hwnd {
            Ok(h) => h,
            Err(_) => return,
        };

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, Some(hwnd), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn display_monitor_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::*;

    const WM_DISPLAYCHANGE: u32 = 0x007E;
    const WM_POWERBROADCAST: u32 = 0x0218;
    const PBT_APMPOWERSTATUSCHANGE: u32 = 0x000A;
    const PBT_APMRESUMESUSPEND: u32 = 0x0007;
    const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;
    const WM_SETTINGCHANGE: u32 = 0x001A;

    const WM_ENDSESSION: u32 = 0x0016;
    match msg {
        // Signing out or shutting down: Explorer's taskbar stays in auto-hide
        // and hidden, so the next sign-in starts without it on screen.
        WM_ENDSESSION if wparam.0 != 0 => {
            crate::utils::hand_over_taskbar("session-end");
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            let now = get_now_ms();
            let last = LAST_DISPLAY_CHANGE_MS.load(Ordering::Relaxed);
            if now - last > 500 {
                LAST_DISPLAY_CHANGE_MS.store(now, Ordering::Relaxed);
                if let Some(app_handle) = DISPLAY_MONITOR_HANDLE.get() {
                    let ah = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        reposition_all_windows(&ah);
                    });
                }
            }
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            let pw = wparam.0 as u32;
            if pw == PBT_APMPOWERSTATUSCHANGE || pw == PBT_APMRESUMESUSPEND {
                let now = get_now_ms();
                let last = LAST_DISPLAY_CHANGE_MS.load(Ordering::Relaxed);
                if now - last > 1000 {
                    LAST_DISPLAY_CHANGE_MS.store(now, Ordering::Relaxed);
                    if let Some(app_handle) = DISPLAY_MONITOR_HANDLE.get() {
                        let ah = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            // Longer delay for wake from sleep to allow display to fully initialize
                            let delay = if pw == PBT_APMRESUMESUSPEND { 500 } else { 300 };
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                            reposition_all_windows(&ah);
                        });
                    }
                }
            }
            LRESULT(0)
        }
        WM_DWMCOLORIZATIONCOLORCHANGED | WM_SETTINGCHANGE => {
            if let Some(app_handle) = DISPLAY_MONITOR_HANDLE.get() {
                let color_hex = if let Some(c) = crate::commands::get_windows_accent_color() {
                    c
                } else {
                    let mut color = 0u32;
                    let mut opaque = windows::core::BOOL(0);
                    if unsafe {
                        windows::Win32::Graphics::Dwm::DwmGetColorizationColor(
                            &mut color,
                            &mut opaque,
                        )
                        .is_ok()
                    } {
                        let r = ((color >> 16) & 0xff) as u8;
                        let g = ((color >> 8) & 0xff) as u8;
                        let b = (color & 0xff) as u8;
                        format!("#{:02x}{:02x}{:02x}", r, g, b)
                    } else {
                        "#0078d4".to_string()
                    }
                };
                let _ = app_handle.emit("system-accent-changed", color_hex);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

static WEBVIEWS_WATCHED_AT: OnceLock<Instant> = OnceLock::new();
static RECOVERING_FROM_WEBVIEW_FAILURE: AtomicBool = AtomicBool::new(false);

/// If a webview's browser or renderer process exits (crash, or killed), the
/// notch and dock windows stay on screen with dead content. They also stop
/// switching click-through, so they keep swallowing clicks meant for the
/// windows under them. Bloom restarts itself when that happens.
pub fn watch_webview_processes(app: &AppHandle) {
    let _ = WEBVIEWS_WATCHED_AT.set(Instant::now());
    for (_, window) in app.webview_windows() {
        watch_webview_process(app, &window);
    }
}

/// Watches one window's web view (also the ones made after start).
pub fn watch_webview_process(app: &AppHandle, window: &tauri::WebviewWindow) {
    {
        let handle = app.clone();
        let _ = window.with_webview(move |webview| unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                COREWEBVIEW2_PROCESS_FAILED_KIND,
                COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
                COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
            };
            use webview2_com::ProcessFailedEventHandler;

            let Ok(core) = webview.controller().CoreWebView2() else {
                return;
            };
            let mut token = 0i64;
            let _ = core.add_ProcessFailed(
                &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
                        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                    {
                        recover_from_webview_failure(&handle);
                    }
                    Ok(())
                })),
                &mut token,
            );
        });
    }
}

fn recover_from_webview_failure(handle: &AppHandle) {
    // Quitting destroys the webviews, which ends their processes too. And
    // every window reports the same browser exit, so act once.
    if SHUTTING_DOWN.load(Ordering::Relaxed)
        || RECOVERING_FROM_WEBVIEW_FAILURE.swap(true, Ordering::SeqCst)
    {
        return;
    }
    let started = WEBVIEWS_WATCHED_AT.get().map(Instant::elapsed);
    crate::utils::note_crash("A web view's process exited; Bloom restarts");
    let handle = handle.clone();
    // Leave the WebView2 callback before touching windows.
    tauri::async_runtime::spawn(async move {
        // A webview that dies right after launch would die again after a
        // restart, so give the taskbar back and exit instead of looping.
        if started.is_some_and(|elapsed| elapsed < Duration::from_secs(30)) {
            crate::commands::restore_taskbar_and_exit(&handle);
        } else {
            crate::commands::restart_bloom(handle).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        is_helper_name, is_launchable_entry, notch_mode_reserves_work_area, pack_cursor,
        unpack_cursor, win_number_index,
    };

    #[test]
    fn cursor_snapshot_preserves_negative_virtual_desktop_coordinates() {
        for (x, y) in [(0, 0), (-1920, 540), (3840, -1080), (i32::MIN, i32::MAX)] {
            let point = windows::Win32::Foundation::POINT { x, y };
            assert_eq!(unpack_cursor(pack_cursor(point)), point);
        }
    }

    #[test]
    fn win_number_maps_top_row_digits_only() {
        assert_eq!(win_number_index(0x31), Some(0));
        assert_eq!(win_number_index(0x35), Some(4));
        assert_eq!(win_number_index(0x39), Some(8));
        // 0, letters and numpad digits are not dock slots
        assert_eq!(win_number_index(0x30), None);
        assert_eq!(win_number_index(0x41), None);
        assert_eq!(win_number_index(0x61), None);
    }

    #[test]
    fn only_fixed_notch_mode_reserves_the_work_area() {
        assert!(notch_mode_reserves_work_area(Some("fixed")));
        assert!(!notch_mode_reserves_work_area(Some("smart")));
        assert!(!notch_mode_reserves_work_area(Some("peek")));
        // Legacy alias for smart
        assert!(!notch_mode_reserves_work_area(Some("auto-hide")));
        // Unset defaults to fixed, matching SETTINGS.md
        assert!(notch_mode_reserves_work_area(None));
    }

    #[test]
    fn helper_shortcuts_are_not_launchable() {
        assert!(is_helper_name("Uninstall Brave"));
        assert!(is_helper_name("7-Zip Help"));
        assert!(is_helper_name("Readme"));
        assert!(is_helper_name("Get Help"));
        // Whole-word matching: legit names containing helper words survive.
        assert!(!is_helper_name("Helper"));
        assert!(!is_helper_name("Photo Remover"));
        assert!(!is_helper_name("7-Zip File Manager"));
        assert!(!is_helper_name("Brave"));
        assert!(!is_launchable_entry("Docs", "https://example.com"));
        assert!(is_launchable_entry(
            "Brave",
            "C:\\Program Files\\Brave\\brave.exe"
        ));
    }
}
