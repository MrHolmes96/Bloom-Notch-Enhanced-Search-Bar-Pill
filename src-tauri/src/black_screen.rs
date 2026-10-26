//! Screen Off: turns the screen off without sleeping, from the notch's
//! control center or its own shortcut (Settings > General > Shortcuts).
//!
//! The screen is turned off the way Windows does it when its own timer runs
//! out (`SC_MONITORPOWER`): only the display goes dark. The PC keeps running,
//! so music and videos (a YouTube tab) keep playing. That doesn't depend on
//! Modern Standby: with S3 sleep it is the same. While the screen is off,
//! Bloom asks Windows not to sleep for being idle, so playback isn't cut off
//! by the sleep timer; that request doesn't cover the lid or the power
//! button, so closing the lid still puts the PC to sleep as set in Windows.
//! Any key or pointer move turns the screen on again, and the request ends
//! with it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::{w, GUID};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Power::{
    RegisterPowerSettingNotification, SetThreadExecutionState, UnregisterPowerSettingNotification,
    ES_CONTINUOUS, ES_SYSTEM_REQUIRED, POWERBROADCAST_SETTING,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, KillTimer, PeekMessageW,
    PostMessageW, RegisterClassExW, SetTimer, TranslateMessage, DEVICE_NOTIFY_WINDOW_HANDLE,
    HWND_BROADCAST, MSG, PM_REMOVE, SC_MONITORPOWER, WM_POWERBROADCAST,
    WM_SYSCOMMAND, WNDCLASSEXW,
};

/// GUID_CONSOLE_DISPLAY_STATE: the display went off (0), on (1) or dimmed (2).
const DISPLAY_STATE: GUID = GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);
const PBT_POWERSETTINGCHANGE: usize = 0x8013;
/// The longest the screen is kept off before Bloom lets go of the request
/// (Windows' own timers apply again then).
const LONGEST: Duration = Duration::from_secs(12 * 60 * 60);

/// A Screen Off is in progress.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The display has gone off since this Screen Off began.
static WENT_OFF: AtomicBool = AtomicBool::new(false);
/// The display came on again.
static CAME_ON: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_POWERBROADCAST && wparam.0 == PBT_POWERSETTINGCHANGE && lparam.0 != 0 {
        let setting = &*(lparam.0 as *const POWERBROADCAST_SETTING);
        if setting.PowerSetting == DISPLAY_STATE && setting.DataLength >= 1 {
            match setting.Data[0] {
                0 => WENT_OFF.store(true, Ordering::Relaxed),
                1 if WENT_OFF.load(Ordering::Relaxed) => CAME_ON.store(true, Ordering::Relaxed),
                _ => {}
            }
        }
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Whether one of the keys of a shortcut, or a mouse button, is still held.
fn keys_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // Mouse buttons, Shift, Ctrl, Alt, the Windows keys, and every letter,
    // digit and F-key a shortcut may use.
    let keys = [0x01, 0x02, 0x04, 0x10, 0x11, 0x12, 0x5B, 0x5C]
        .into_iter()
        .chain(0x30..=0x5A)
        .chain(0x70..=0x87)
        .chain(0x20..=0x2F)
        .chain(0xBA..=0xDF);
    keys.into_iter()
        .any(|vk| unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0)
}

/// Turns the screen off. From a shortcut, waits until its keys are let go
/// (letting go of a key would turn the screen straight back on); from a
/// click, until the button is up. Does nothing when a Screen Off is already
/// in progress.
pub fn screen_off() {
    if ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    let started = std::thread::Builder::new()
        .name("bloom-screen-off".into())
        .spawn(|| {
            let waiting = Instant::now();
            while keys_held() && waiting.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(30));
            }
            // A moment for the release to settle, as the screen wakes on input.
            std::thread::sleep(Duration::from_millis(350));
            unsafe { run() };
            ACTIVE.store(false, Ordering::SeqCst);
        });
    if started.is_err() {
        ACTIVE.store(false, Ordering::SeqCst);
    }
}

unsafe fn run() {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    WENT_OFF.store(false, Ordering::Relaxed);
    CAME_ON.store(false, Ordering::Relaxed);
    let Ok(module) = GetModuleHandleW(None) else {
        return;
    };
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: module.into(),
        lpszClassName: w!("BloomScreenOff"),
        ..Default::default()
    };
    // A hidden window (never shown) that hears when the display goes off and
    // on. Registered once per run of Bloom; a second time fails harmlessly.
    RegisterClassExW(&class);
    let Ok(window) = CreateWindowExW(
        Default::default(),
        w!("BloomScreenOff"),
        w!(""),
        Default::default(),
        0,
        0,
        0,
        0,
        None,
        None,
        Some(module.into()),
        None,
    ) else {
        return;
    };
    let notify = RegisterPowerSettingNotification(
        HANDLE(window.0),
        &DISPLAY_STATE,
        DEVICE_NOTIFY_WINDOW_HANDLE,
    )
    .ok();
    // Awake while dark, not asleep for being idle; lid and power button
    // still do what Windows is set to.
    SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
    let _ = PostMessageW(
        Some(HWND_BROADCAST),
        WM_SYSCOMMAND,
        WPARAM(SC_MONITORPOWER as usize),
        LPARAM(2),
    );
    let timer = SetTimer(Some(window), 1, 250, None);
    let started = Instant::now();
    let mut message = MSG::default();
    loop {
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if CAME_ON.load(Ordering::Relaxed) || started.elapsed() > LONGEST {
            break;
        }
        // Windows didn't turn it off (no display that can, or it refused).
        if !WENT_OFF.load(Ordering::Relaxed) && started.elapsed() > Duration::from_secs(5) {
            break;
        }
        if crate::state::SHUTTING_DOWN.load(Ordering::Relaxed) {
            break;
        }
        windows::Win32::UI::WindowsAndMessaging::WaitMessage().ok();
    }
    SetThreadExecutionState(ES_CONTINUOUS);
    let _ = KillTimer(Some(window), timer);
    if let Some(notify) = notify {
        let _ = UnregisterPowerSettingNotification(notify);
    }
    let _ = DestroyWindow(window);
}

/// The control center's Screen Off button.
#[tauri::command]
pub fn screen_off_now() {
    screen_off();
}
