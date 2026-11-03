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
use std::sync::Mutex;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{UnhookWinEvent, HWINEVENTHOOK};

/// The loading pop-up being kept off the screen (0: none).
static SPLASH: AtomicIsize = AtomicIsize::new(0);
/// The hook following where that pop-up is placed (0: none).
static MOVE_HOOK: AtomicIsize = AtomicIsize::new(0);

/// Off the screen, where Windows itself parks minimized windows.
const AWAY: i32 = -32000;
/// Portals closed its loading pop-up (the launcher below may end).
static SPLASH_CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
            crate::startup_timing::mark("Portals closed its loading pop-up (Portals is ready)");
            SPLASH_CLOSED.store(true, Ordering::Relaxed);
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
/// Whether a startup entry is on: Windows marks the ones switched off (in
/// Task Manager or Settings > Apps > Startup) under `StartupApproved`, with
/// an odd first byte.
fn startup_entry_on(root: windows::Win32::System::Registry::HKEY, kind: &str, name: &str) -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_BINARY};
    let key: Vec<u16> =
        format!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\{kind}")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
    let value: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut data = [0u8; 16];
    let mut size = data.len() as u32;
    let read = unsafe {
        RegGetValueW(
            root,
            windows::core::PCWSTR(key.as_ptr()),
            windows::core::PCWSTR(value.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    // No mark: on.
    read.is_err() || size == 0 || data[0] & 1 == 0
}

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
                // Switched off in Task Manager or Settings > Apps > Startup
                // (the entry stays, marked off): it doesn't start then.
                let value_name = String::from_utf16_lossy(&name[..name_len as usize]);
                if command.contains("portals.exe") && startup_entry_on(root, "Run", &value_name) {
                    log(&format!(
                        "Portals' own startup entry \"{value_name}\" is on ({command})"
                    ));
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
                let name = entry.file_name().to_string_lossy().to_string();
                name.to_lowercase().contains("portals")
                    && startup_entry_on(
                        windows::Win32::System::Registry::HKEY_CURRENT_USER,
                        "StartupFolder",
                        &name,
                    )
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

/// Whether "Start Portals with Bloom" is on, read straight from
/// settings.json (this runs before Bloom's settings are loaded).
fn start_portals_on() -> bool {
    crate::utils::data_file("settings.json")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            v.get(START_PORTALS_KEY)
                .and_then(|v| v.as_str())
                .map(|v| v == "true")
        })
        .unwrap_or(false)
}

/// At sign-in, the first thing Bloom does (before its own windows): makes
/// sure Portals starts as administrator, at once. Bloom's task starts it by
/// itself as Windows signs in, alongside Bloom; while that task is still the
/// older kind (run by Bloom, set up again with one permission prompt), Bloom
/// runs it now. Nothing waits for the processor or reads Portals' files
/// first. Each step goes into startup-timing.log.
pub fn start_portals_after_sign_in() {
    if !start_portals_on() {
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
                // Left to Portals, but made sure of: if it hasn't started in
                // 20 s, Bloom starts it.
                log("Starting Portals: its own Launch on Startup is on; Bloom checks it starts");
                let since = std::time::Instant::now();
                while since.elapsed() < std::time::Duration::from_secs(20) {
                    if portals_running() {
                        log("Portals started by itself");
                        crate::startup_timing::mark("Portals is running (started by itself)");
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                log("Portals didn't start by itself in 20 s: Bloom starts it");
            }
            if portals_running() {
                log("Starting Portals: it was running already");
                crate::startup_timing::mark("Portals was already running when Bloom started");
                return;
            }
            let run = |why: &str| {
                let how = match run_portals_task() {
                    Ok(()) => "as administrator (Bloom's task)".to_string(),
                    Err(e) => {
                        let result = std::process::Command::new(&exe)
                            .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
                            .spawn();
                        match result {
                            Ok(_) => format!("NOT as administrator ({e})"),
                            Err(e2) => format!("couldn't start it at all ({e}; {e2})"),
                        }
                    }
                };
                log(&format!(
                    "Starting Portals {:.1} s after Bloom started ({why}): {how}",
                    started.elapsed().as_secs_f64()
                ));
                crate::startup_timing::mark(&format!("Bloom started Portals: {how}"));
            };
            let at_sign_in = task_starts_at_sign_in();
            if at_sign_in {
                log("Starting Portals: Bloom's task starts it by itself at sign-in");
            } else {
                run("Bloom's task is the older kind, run by Bloom");
            }
            // Only said to have started once it runs.
            let mut ran_again = false;
            while started.elapsed() < std::time::Duration::from_secs(40) {
                if portals_running() {
                    log(&format!(
                        "Portals is running ({:.1} s after Bloom started)",
                        started.elapsed().as_secs_f64()
                    ));
                    crate::startup_timing::mark("Portals is running");
                    return;
                }
                // The task's own start didn't happen (or failed): run it.
                if at_sign_in
                    && !ran_again
                    && started.elapsed() > std::time::Duration::from_secs(10)
                {
                    ran_again = true;
                    run("its task hadn't started it 10 s after Bloom");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            log("Portals did NOT start within 40 s");
            crate::startup_timing::mark("Portals did NOT start within 40 s");
        });
}

// ---------------------------------------------------------------- Portals' scheduled tasks

/// The scheduled tasks Bloom switched off so Bloom starts Portals instead
/// (by their path in Task Scheduler), to switch back on later.
const SWITCHED_OFF_FILE: &str = "portals-tasks-switched-off.txt";

/// The last problem switching a task, for Settings.
static TASK_PROBLEM: Mutex<Option<String>> = Mutex::new(None);

/// Every scheduled task whose program is Portals, with whether it's on.
unsafe fn portals_tasks(
    service: &windows::Win32::System::TaskScheduler::ITaskService,
) -> Vec<(String, bool)> {
    use windows::core::{Interface, BSTR};
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ITaskFolder, TASK_ACTION_EXEC, TASK_ENUM_HIDDEN,
    };
    use windows::Win32::System::Variant::VARIANT;
    fn walk(folder: &ITaskFolder, found: &mut Vec<(String, bool)>, depth: u32) {
        unsafe {
            if let Ok(tasks) = folder.GetTasks(TASK_ENUM_HIDDEN.0) {
                let count = tasks.Count().unwrap_or(0);
                for i in 1..=count {
                    let Ok(task) = tasks.get_Item(&VARIANT::from(i)) else {
                        continue;
                    };
                    let Ok(actions) = task.Definition().and_then(|d| d.Actions()) else {
                        continue;
                    };
                    let mut runs_portals = false;
                    let mut count = 0i32;
                    let _ = actions.Count(&mut count);
                    for a in 1..=count {
                        let Ok(action) = actions.get_Item(a) else {
                            continue;
                        };
                        let mut kind = TASK_ACTION_EXEC;
                        if action.Type(&mut kind).is_err() || kind != TASK_ACTION_EXEC {
                            continue;
                        }
                        let Ok(exec) = action.cast::<IExecAction>() else {
                            continue;
                        };
                        let mut path = BSTR::new();
                        if exec.Path(&mut path).is_ok()
                            && path.to_string().to_lowercase().contains("portals.exe")
                        {
                            runs_portals = true;
                        }
                    }
                    if runs_portals {
                        let path = task.Path().map(|p| p.to_string()).unwrap_or_default();
                        let enabled = task.Enabled().map(|e| e.as_bool()).unwrap_or(false);
                        if !path.is_empty() {
                            found.push((path, enabled));
                        }
                    }
                }
            }
            if depth >= 6 {
                return;
            }
            if let Ok(folders) = folder.GetFolders(0) {
                for i in 1..=folders.Count().unwrap_or(0) {
                    if let Ok(inner) = folders.get_Item(&VARIANT::from(i)) {
                        walk(&inner, found, depth + 1);
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    if let Ok(root) = service.GetFolder(&BSTR::from("\\")) {
        walk(&root, &mut found, 0);
    }
    found
}

fn switched_off() -> Vec<String> {
    crate::utils::data_file(SWITCHED_OFF_FILE)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| {
            t.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn save_switched_off(paths: &[String]) {
    if let Some(path) = crate::utils::data_file(SWITCHED_OFF_FILE) {
        if paths.is_empty() {
            let _ = std::fs::remove_file(path);
        } else {
            let _ = std::fs::write(path, paths.join("\n"));
        }
    }
}

/// Bloom's task that starts Portals as administrator, without asking: made
/// once with Windows' permission (one prompt), and run by Bloom after
/// sign-in. Bloom itself doesn't run as administrator.
const PORTALS_TASK: &str = "Bloom Start Portals";
/// `bloom.exe --set-up-portals-start on|off`, run as administrator.
pub const PORTALS_SETUP_ARG: &str = "--set-up-portals-start";
/// `bloom.exe --launch-portals`: what Bloom's task runs, as administrator.
pub const PORTALS_LAUNCH_ARG: &str = "--launch-portals";

/// Run as administrator by Bloom's task: starts Portals (so it runs as
/// administrator, as it did from your own task) and keeps its loading
/// pop-up off the screen. Bloom itself can't move that pop-up then: Windows
/// doesn't let a program move the windows of one with more rights. Ends
/// once the pop-up is gone (at most half a minute). Exit code 0 when
/// Portals started.
pub fn launch_portals_as_admin() -> bool {
    let Some(exe) = portals_exe() else {
        log("Launcher: Portals isn't installed where Bloom looks");
        return false;
    };
    crate::startup_timing::begin("Portals launcher (Bloom's task, as administrator)");
    // Normal priority, for this launcher and so for Portals: a task Bloom set
    // up before ran below normal (Task Scheduler's default).
    set_normal_priority(None);
    start();
    // The hooks are in place before Portals makes its pop-up.
    std::thread::sleep(std::time::Duration::from_millis(150));
    let started = std::process::Command::new(&exe)
        .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
        .spawn();
    let child = match started {
        Ok(child) => child,
        Err(e) => {
            log(&format!(
                "Launcher: couldn't start Portals as administrator: {e}"
            ));
            crate::startup_timing::mark("couldn't start Portals");
            return false;
        }
    };
    set_normal_priority(Some(child.id()));
    log("Launcher: started Portals as administrator");
    crate::startup_timing::mark("Portals started (as administrator, normal priority)");
    let since = std::time::Instant::now();
    while since.elapsed() < std::time::Duration::from_secs(30)
        && !SPLASH_CLOSED.load(Ordering::Relaxed)
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    true
}

/// Normal priority for this process (`None`) or another one.
fn set_normal_priority(pid: Option<u32>) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, SetPriorityClass, NORMAL_PRIORITY_CLASS,
        PROCESS_SET_INFORMATION,
    };
    unsafe {
        match pid {
            None => {
                let _ = SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
            }
            Some(pid) => {
                if let Ok(process) = OpenProcess(PROCESS_SET_INFORMATION, false, pid) {
                    let _ = SetPriorityClass(process, NORMAL_PRIORITY_CLASS);
                    let _ = CloseHandle(process);
                }
            }
        }
    }
}

unsafe fn task_service() -> Result<windows::Win32::System::TaskScheduler::ITaskService, String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::TaskScheduler::{ITaskService, TaskScheduler};
    use windows::Win32::System::Variant::VARIANT;
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
        .map_err(|e| format!("Task Scheduler: {e}"))?;
    let none = VARIANT::default();
    service
        .Connect(&none, &none, &none, &none)
        .map_err(|e| format!("Task Scheduler: {e}"))?;
    Ok(service)
}

/// Whether Bloom's task is there (set up).
fn portals_task_ready() -> bool {
    unsafe {
        task_service()
            .and_then(|service| {
                service
                    .GetFolder(&windows::core::BSTR::from("\\"))
                    .and_then(|root| root.GetTask(&windows::core::BSTR::from(PORTALS_TASK)))
                    .map_err(|e| e.to_string())
            })
            .is_ok()
    }
}

/// Whether Bloom's task is the current kind: started by Windows itself at
/// sign-in (alongside Bloom, not after it) and at normal priority (Task
/// Scheduler's default runs it, and so Portals, below normal).
fn task_starts_at_sign_in() -> bool {
    use windows::Win32::System::TaskScheduler::{TASK_TRIGGER_LOGON, TASK_TRIGGER_TYPE2};
    unsafe {
        let Ok(service) = task_service() else {
            return false;
        };
        let Ok(definition) = service
            .GetFolder(&windows::core::BSTR::from("\\"))
            .and_then(|root| root.GetTask(&windows::core::BSTR::from(PORTALS_TASK)))
            .and_then(|task| task.Definition())
        else {
            return false;
        };
        let mut priority = 7;
        let _ = definition
            .Settings()
            .and_then(|s| s.Priority(&mut priority));
        let mut at_logon = false;
        if let Ok(triggers) = definition.Triggers() {
            let mut count = 0;
            let _ = triggers.Count(&mut count);
            for i in 1..=count {
                let mut kind = TASK_TRIGGER_TYPE2(0);
                if triggers.get_Item(i).and_then(|t| t.Type(&mut kind)).is_ok()
                    && kind == TASK_TRIGGER_LOGON
                {
                    at_logon = true;
                }
            }
        }
        at_logon && priority <= 4
    }
}

/// Starts Portals through Bloom's task (as administrator, no prompt).
fn run_portals_task() -> Result<(), String> {
    use windows::Win32::System::Variant::VARIANT;
    unsafe {
        let service = task_service()?;
        let task = service
            .GetFolder(&windows::core::BSTR::from("\\"))
            .and_then(|root| root.GetTask(&windows::core::BSTR::from(PORTALS_TASK)))
            .map_err(|_| "Bloom's Portals task isn't set up".to_string())?;
        task.Run(&VARIANT::default())
            .map(|_| ())
            .map_err(|e| format!("Bloom's Portals task didn't run: {e}"))
    }
}

/// As administrator: makes Bloom's task (on), or removes it (off). With it
/// on, the other scheduled tasks that start Portals at sign-in are switched
/// off (they'd start it before Bloom can keep its loading pop-up out of
/// sight), and switched back on with it off. Exit code 0 when done.
pub fn set_up_portals_start(rest: &[std::ffi::OsString]) -> bool {
    let on = rest
        .first()
        .map(|a| a.to_string_lossy().to_string())
        .as_deref()
        != Some("off");
    match unsafe { set_up(on) } {
        Ok(()) => {
            log(&format!(
                "Set up as administrator: Bloom's Portals task {}",
                if on { "made" } else { "removed" }
            ));
            true
        }
        Err(e) => {
            log(&format!("Setting up Bloom's Portals task failed: {e}"));
            false
        }
    }
}

unsafe fn set_up(on: bool) -> Result<(), String> {
    use windows::core::{Interface, BSTR};
    use windows::Win32::Foundation::{VARIANT_FALSE, VARIANT_TRUE};
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ILogonTrigger, TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE,
        TASK_INSTANCES_IGNORE_NEW, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_HIGHEST,
        TASK_TRIGGER_LOGON,
    };
    use windows::Win32::System::Variant::VARIANT;
    let service = task_service()?;
    let root = service
        .GetFolder(&BSTR::from("\\"))
        .map_err(|e| format!("root folder: {e}"))?;
    let none = VARIANT::default();
    let mut off = switched_off();
    if on {
        let exe = portals_exe().ok_or("Portals isn't installed where Bloom looks")?;
        let sid = crate::ghelper::user_sid().ok_or("the Windows account couldn't be read")?;
        let task = service.NewTask(0).map_err(|e| format!("new task: {e}"))?;
        task.RegistrationInfo()
            .and_then(|info| {
                info.SetDescription(&BSTR::from(
                    "Bloom: starts Portals as administrator as you sign in, keeping its loading pop-up out of sight (Settings > Integrations > Portals)",
                ))
            })
            .map_err(|e| format!("description: {e}"))?;
        let principal = task.Principal().map_err(|e| format!("principal: {e}"))?;
        principal
            .SetUserId(&BSTR::from(sid.as_str()))
            .and_then(|_| principal.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN))
            .and_then(|_| principal.SetRunLevel(TASK_RUNLEVEL_HIGHEST))
            .map_err(|e| format!("account: {e}"))?;
        let settings = task.Settings().map_err(|e| format!("settings: {e}"))?;
        settings
            .SetDisallowStartIfOnBatteries(VARIANT_FALSE)
            .and_then(|_| settings.SetStopIfGoingOnBatteries(VARIANT_FALSE))
            .and_then(|_| settings.SetAllowDemandStart(VARIANT_TRUE))
            .and_then(|_| settings.SetExecutionTimeLimit(&BSTR::from("PT0S")))
            .and_then(|_| settings.SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW))
            // Normal priority: Task Scheduler's default (7) is below normal,
            // and Portals would inherit it.
            .and_then(|_| settings.SetPriority(4))
            .map_err(|e| format!("settings: {e}"))?;
        // Started by Windows itself as this account signs in, at the same
        // time as Bloom (not after it), as your own task for Portals did.
        let trigger: ILogonTrigger = task
            .Triggers()
            .and_then(|triggers| triggers.Create(TASK_TRIGGER_LOGON))
            .and_then(|trigger| trigger.cast())
            .map_err(|e| format!("sign-in trigger: {e}"))?;
        trigger
            .SetUserId(&BSTR::from(sid.as_str()))
            .map_err(|e| format!("sign-in trigger: {e}"))?;
        let action: IExecAction = task
            .Actions()
            .and_then(|actions| actions.Create(TASK_ACTION_EXEC))
            .and_then(|action| action.cast())
            .map_err(|e| format!("action: {e}"))?;
        // Bloom, as administrator, starts Portals and keeps its pop-up away.
        let bloom = std::env::current_exe().map_err(|e| format!("Bloom's own path: {e}"))?;
        action
            .SetPath(&BSTR::from(bloom.display().to_string()))
            .and_then(|_| action.SetArguments(&BSTR::from(PORTALS_LAUNCH_ARG)))
            .and_then(|_| {
                action.SetWorkingDirectory(&BSTR::from(
                    exe.parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ))
            })
            .map_err(|e| format!("command: {e}"))?;
        // Administrators and Windows keep full control; this account may
        // read and run it, nothing more.
        let access = VARIANT::from(BSTR::from(format!(
            "D:(A;;FA;;;BA)(A;;FA;;;SY)(A;;GRGX;;;{sid})"
        )));
        root.RegisterTaskDefinition(
            &BSTR::from(PORTALS_TASK),
            &task,
            TASK_CREATE_OR_UPDATE.0,
            &none,
            &none,
            TASK_LOGON_INTERACTIVE_TOKEN,
            &access,
        )
        .map_err(|e| format!("register: {e}"))?;
        for (path, enabled) in portals_tasks(&service) {
            if !enabled
                || path
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(PORTALS_TASK)
            {
                continue;
            }
            match root
                .GetTask(&BSTR::from(path.as_str()))
                .and_then(|t| t.SetEnabled(VARIANT_FALSE))
            {
                Ok(()) => {
                    log(&format!(
                        "Scheduled task \"{path}\" (starts Portals at sign-in) switched off"
                    ));
                    if !off.contains(&path) {
                        off.push(path);
                    }
                }
                Err(e) => log(&format!(
                    "Couldn't switch off scheduled task \"{path}\": {e}"
                )),
            }
        }
    } else {
        let _ = root.DeleteTask(&BSTR::from(PORTALS_TASK), 0);
        off.retain(|path| {
            match root
                .GetTask(&BSTR::from(path.as_str()))
                .and_then(|t| t.SetEnabled(VARIANT_TRUE))
            {
                Ok(()) => {
                    log(&format!("Scheduled task \"{path}\" switched back on"));
                    false
                }
                Err(e) if e.code().0 as u32 == 0x8007_0002 => false,
                Err(e) => {
                    log(&format!(
                        "Couldn't switch scheduled task \"{path}\" back on: {e}"
                    ));
                    true
                }
            }
        });
    }
    save_switched_off(&off);
    Ok(())
}

/// Runs the set-up as administrator (Windows asks once). Waits for it.
fn set_up_as_admin(on: bool) -> Result<(), String> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let verb = wide("runas");
    let file = wide(&exe.display().to_string());
    let params = wide(&format!(
        "{PORTALS_SETUP_ARG} {}",
        if on { "on" } else { "off" }
    ));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: windows::core::PCWSTR(verb.as_ptr()),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        lpParameters: windows::core::PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ShellExecuteExW(&mut info).map_err(|_| {
            log("Setting up Bloom's Portals task: Windows' permission prompt was declined");
            "Windows' permission prompt was declined".to_string()
        })?;
        if info.hProcess.is_invalid() {
            return Err("The set-up didn't start".into());
        }
        WaitForSingleObject(info.hProcess, 60_000);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = windows::Win32::Foundation::CloseHandle(info.hProcess);
        if code != 0 {
            return Err("The set-up didn't finish (see app-tweaks.log)".into());
        }
    }
    Ok(())
}

/// With "Start Portals with Bloom" on but Bloom's task not made yet (it's
/// new, or Bloom was updated to it), Windows' permission is asked for once,
/// a few seconds after Bloom starts; declined, Settings offers it again.
pub fn set_up_portals_if_needed(app: &tauri::AppHandle) {
    if crate::utils::get_setting_str(app, START_PORTALS_KEY).as_deref() != Some("true") {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("bloom-portals-setup".into())
        .spawn(|| {
            if portals_exe().is_none() {
                return;
            }
            // Asked once to set it up, and once more to bring an older task
            // up to date (started by Windows at sign-in, normal priority).
            let ready = portals_task_ready();
            let name = if ready {
                "portals-setup-sign-in-asked.txt"
            } else {
                "portals-setup-asked.txt"
            };
            let Some(marker) = crate::utils::data_file(name) else {
                return;
            };
            if marker.exists() || (ready && task_starts_at_sign_in()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
            let _ = std::fs::write(&marker, "asked");
            let result = set_up_as_admin(true);
            if let Ok(mut slot) = TASK_PROBLEM.lock() {
                *slot = result.err();
            }
        });
}

/// Sets Portals' start up (on) or takes it down (off), asking Windows'
/// permission once (Settings > Integrations > Portals).
#[tauri::command]
pub async fn portals_set_up(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = set_up_as_admin(on);
        if let Ok(mut slot) = TASK_PROBLEM.lock() {
            *slot = result.as_ref().err().cloned();
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct PortalsStatus {
    installed: bool,
    starts_itself: bool,
    /// What went wrong setting it up, when something did.
    task_problem: Option<String>,
    /// Bloom's task to start it as administrator is set up.
    task_ready: bool,
    /// And of the current kind: Windows starts it as you sign in, at normal
    /// priority (an older one is run by Bloom once Bloom is up).
    starts_at_sign_in: bool,
    /// Other scheduled tasks that start Portals at sign-in, still on.
    other_tasks: Vec<String>,
}

#[tauri::command]
pub async fn portals_status() -> PortalsStatus {
    tauri::async_runtime::spawn_blocking(|| PortalsStatus {
        installed: portals_exe().is_some(),
        starts_itself: portals_starts_itself(),
        task_problem: TASK_PROBLEM.lock().ok().and_then(|p| p.clone()),
        task_ready: portals_task_ready(),
        starts_at_sign_in: task_starts_at_sign_in(),
        other_tasks: unsafe {
            task_service()
                .map(|service| {
                    portals_tasks(&service)
                        .into_iter()
                        .filter(|(path, enabled)| {
                            *enabled
                                && !path
                                    .trim_start_matches('\\')
                                    .eq_ignore_ascii_case(PORTALS_TASK)
                        })
                        .map(|(path, _)| path.trim_start_matches('\\').to_string())
                        .collect()
                })
                .unwrap_or_default()
        },
    })
    .await
    .unwrap_or(PortalsStatus {
        installed: false,
        starts_itself: false,
        task_problem: None,
        task_ready: false,
        starts_at_sign_in: false,
        other_tasks: Vec::new(),
    })
}
