//! Small changes to how other apps start, so they feel part of Bloom.
//!
//! Portals (desktop organizer): its loading pop-up (logo, title and loading
//! bar, a window titled "SplashScreen") is kept off the screen for as long as
//! it exists, and isn't shown in the dock. It isn't hidden or closed: Portals
//! goes on drawing it and starting exactly as always, it just can't be seen.
//! It is caught as early as Windows reports it: when Portals makes or names
//! the window, before it is shown, so it is off the screen when it appears.
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
        GetAncestor, GetWindowThreadProcessId, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
        EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, GA_ROOT,
        WINEVENT_OUTOFCONTEXT,
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
    // Caught as early as Windows tells: when Portals makes the window or
    // names it (before it is shown), else as it is shown.
    let stage = match event {
        e if e == EVENT_OBJECT_CREATE => "made (not shown yet)",
        e if e == EVENT_OBJECT_NAMECHANGE => "named (not shown yet)",
        e if e == EVENT_OBJECT_SHOW => "shown",
        _ => return,
    };
    if GetAncestor(hwnd, GA_ROOT) != hwnd {
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
            "Portals' loading pop-up caught when {stage}: {}",
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
                DispatchMessageW, GetMessageW, TranslateMessage, EVENT_OBJECT_CREATE,
                EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, MSG,
                WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
            };
            for event in [
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_NAMECHANGE,
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_DESTROY,
            ] {
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

// ---------------------------------------------------------------- Portals at sign-in

/// Bloom starts Portals after sign-in instead of Portals starting itself
/// (Settings > Integrations > Portals). Portals started by Windows at sign-in
/// is too busy loading to answer anything, so its loading pop-up can't be
/// moved off the screen then; started by Bloom it is quick, as when started
/// by hand: its files are read into memory first, and it is started as soon
/// as the computer has settled (a few seconds), not after a fixed wait.
pub const START_PORTALS_KEY: &str = "bloom-start-portals";

/// Where Portals is installed.
fn portals_exe() -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();
    for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(dir) = std::env::var_os(variable) {
            candidates.push(
                std::path::PathBuf::from(dir)
                    .join("Portals")
                    .join("Portals.exe"),
            );
        }
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            std::path::PathBuf::from(dir)
                .join("Programs")
                .join("Portals")
                .join("Portals.exe"),
        );
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Whether Portals starts itself with Windows: its own entry under Windows'
/// "Run" startup entries, or a shortcut in the Startup folder (switched on).
fn portals_starts_itself() -> bool {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_READ,
    };
    let run = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    let wide: Vec<u16> = run.encode_utf16().chain(std::iter::once(0)).collect();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(
                root,
                windows::core::PCWSTR(wide.as_ptr()),
                None,
                KEY_READ,
                &mut key,
            )
            .is_err()
            {
                continue;
            }
            let mut found = false;
            for index in 0..256u32 {
                let mut name = [0u16; 256];
                let mut name_len = name.len() as u32;
                let mut data = [0u8; 2048];
                let mut data_len = data.len() as u32;
                if RegEnumValueW(
                    key,
                    index,
                    Some(windows::core::PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                )
                .is_err()
                {
                    break;
                }
                let units: Vec<u16> = data[..data_len as usize]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let command = String::from_utf16_lossy(&units).to_lowercase();
                if command.contains("portals.exe") {
                    found = true;
                    break;
                }
            }
            let _ = RegCloseKey(key);
            if found {
                return true;
            }
        }
    }
    let startup = std::env::var_os("APPDATA").map(|dir| {
        std::path::PathBuf::from(dir).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup")
    });
    startup
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .is_some_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("portals")
            })
        })
}

fn portals_running() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("Portals.exe")
            {
                found = true;
                break;
            }
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

/// Reads Portals' program files once, so Windows has them in memory and
/// Portals starts as quickly as when started by hand (at most 600 MB).
fn warm_up(dir: &std::path::Path) {
    use std::io::Read;
    let mut budget: u64 = 600 * 1024 * 1024;
    let mut folders = vec![dir.to_path_buf()];
    let mut buffer = vec![0u8; 1 << 20];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                folders.push(path);
                continue;
            }
            let Ok(mut file) = std::fs::File::open(&path) else {
                continue;
            };
            while let Ok(read) = file.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                budget = budget.saturating_sub(read as u64);
                if budget == 0 {
                    return;
                }
            }
        }
    }
}

/// How busy the processor is now (0–100), measured over `over`.
fn processor_busy(over: std::time::Duration) -> Option<f64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetSystemTimes;
    let read = || unsafe {
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()?;
        let n = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        Some((n(idle), n(kernel) + n(user)))
    };
    let (idle_a, total_a) = read()?;
    std::thread::sleep(over);
    let (idle_b, total_b) = read()?;
    let total = total_b.saturating_sub(total_a) as f64;
    (total > 0.0).then(|| 100.0 * (1.0 - idle_b.saturating_sub(idle_a) as f64 / total))
}

/// After sign-in (Bloom started by its startup task): starts Portals when
/// the user chose that, it isn't running yet and doesn't start itself.
pub fn start_portals_after_sign_in(app: &tauri::AppHandle) {
    if crate::utils::get_setting_str(app, START_PORTALS_KEY).as_deref() != Some("true") {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("bloom-start-portals".into())
        .spawn(|| {
            let started = std::time::Instant::now();
            let Some(exe) = portals_exe() else {
                log("Starting Portals: it isn't installed where Bloom looks");
                return;
            };
            if portals_starts_itself() {
                log("Starting Portals: left to Portals, its own Launch on Startup is on");
                return;
            }
            if portals_running() {
                return;
            }
            if let Some(dir) = exe.parent() {
                warm_up(dir);
            }
            let warmed = started.elapsed();
            // Settled: the processor mostly free for a second, at most 12 s
            // after Bloom started.
            let mut calm = 0;
            while started.elapsed() < std::time::Duration::from_secs(12) {
                match processor_busy(std::time::Duration::from_millis(250)) {
                    Some(busy) if busy < 40.0 => calm += 1,
                    Some(_) => calm = 0,
                    None => break,
                }
                if calm >= 4 {
                    break;
                }
            }
            if portals_running() {
                return;
            }
            let result = std::process::Command::new(&exe)
                .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
                .spawn();
            log(&format!(
                "Started Portals {:.1} s after Bloom (its files read in {:.1} s){}",
                started.elapsed().as_secs_f64(),
                warmed.as_secs_f64(),
                match result {
                    Ok(_) => String::new(),
                    Err(e) => format!(": couldn't ({e})"),
                }
            ));
        });
}

#[derive(serde::Serialize)]
pub struct PortalsStatus {
    installed: bool,
    starts_itself: bool,
}

#[tauri::command]
pub async fn portals_status() -> PortalsStatus {
    tauri::async_runtime::spawn_blocking(|| PortalsStatus {
        installed: portals_exe().is_some(),
        starts_itself: portals_starts_itself(),
    })
    .await
    .unwrap_or(PortalsStatus {
        installed: false,
        starts_itself: false,
    })
}
