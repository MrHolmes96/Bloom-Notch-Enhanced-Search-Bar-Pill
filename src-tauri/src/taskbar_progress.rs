//! The progress apps show on their taskbar button (a download, a copy, an
//! install), for the dock to draw on their icon.
//!
//! Apps report it with `ITaskbarList3::SetProgressValue` and
//! `SetProgressState` (Steam through Millennium's "Taskbar Download progress"
//! plugin, browsers, Explorer's copies). Windows sends those calls to the
//! window named by the `TaskbandHWND` property of a window of class
//! `Shell_TrayWnd`: Explorer names its task list there on its own taskbar.
//! Bloom names its own window instead, on its tray host (the first window of
//! that class, see `tray_host`) and on Explorer's taskbar, so the calls reach
//! Bloom whichever taskbar an app finds. Bloom records each window's progress
//! and passes every message on unchanged to Explorer's task list, so Explorer
//! keeps everything apps tell it (thumbnail buttons, overlay icons, tabs).
//!
//! This is how ManagedShell (RetroBar, Cairo) and Emerge Desktop receive
//! these calls alongside Explorer. Bloom checks both properties every two
//! seconds (Explorer sets its own again when it restarts) and gives Explorer
//! its own back when Bloom quits.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, CreateWindowExW, DefWindowProcW, EnumChildWindows, GetAncestor,
    GetClassNameW, GetPropW, GetWindowThreadProcessId, InSendMessage, IsWindow, KillTimer,
    PostMessageW, RegisterClassExW, RemovePropW, SendMessageTimeoutW, SetPropW, SetTimer,
    GA_ROOTOWNER, MSGFLT_ALLOW, SMTO_ABORTIFHUNG, SMTO_NORMAL, WM_COPYDATA, WM_TIMER, WM_USER,
    WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// The property of `Shell_TrayWnd` that names the taskbar's task list.
const PROPERTY: PCWSTR = w!("TaskbandHWND");
/// `ITaskbarList3::SetProgressValue`: the window in `wParam`, the progress in
/// `lParam`, from 0 to `FULL`.
const SET_PROGRESS_VALUE: u32 = WM_USER + 64;
/// `ITaskbarList3::SetProgressState`: the window in `wParam`, a `TBPFLAG` in
/// `lParam`.
const SET_PROGRESS_STATE: u32 = WM_USER + 65;
/// The task list's messages (`ITaskbarList` uses `WM_USER + 50` to `+ 90`).
const LIST_MESSAGES: std::ops::Range<u32> = WM_USER..WM_USER + 0x100;
/// The progress of a finished task, as Windows scales it.
const FULL: f64 = 65534.0;

const TBPF_NOPROGRESS: u32 = 0x0;
const TBPF_INDETERMINATE: u32 = 0x1;
const TBPF_NORMAL: u32 = 0x2;
const TBPF_ERROR: u32 = 0x4;
const TBPF_PAUSED: u32 = 0x8;

const TIMER_ANNOUNCE: usize = 1;
const TIMER_PRUNE: usize = 2;
const TIMER_CLAIM: usize = 3;
/// The dock hears of changes at most this often (apps update many times a second).
const ANNOUNCE_GAP_MS: u32 = 250;
/// How often windows that closed without clearing their progress are dropped.
const PRUNE_MS: u32 = 3000;
/// How often both taskbars are checked to still name Bloom's window.
const CLAIM_MS: u32 = 2000;
/// The smallest change in progress worth redrawing (the bar is about 24 px).
const STEP: f64 = 0.005;

struct Entry {
    hwnd: isize,
    owner: isize,
    /// The window's program, then the programs that started it from the same
    /// install (Steam's window belongs to `steamwebhelper.exe`, which
    /// `steam.exe` started), so the dock finds the app's icon.
    programs: Vec<String>,
    /// The processes of the app's install: the first program of the chain
    /// above and those it started from the same install, whose windows are
    /// the app's (Steam's main window, its helper's, its tray's).
    family: Vec<u32>,
    state: u32,
    value: Option<f64>,
    /// The value the dock last heard of.
    announced: Option<f64>,
}

/// One window's progress, as the dock draws it.
#[derive(Clone, Serialize)]
pub struct Progress {
    pub hwnd: isize,
    /// The window that owns it (a dialog's main window), or itself.
    pub owner: isize,
    /// Its program first, then the ones that started it from the same install.
    pub programs: Vec<String>,
    /// The app's other top-level windows (all processes of its install).
    pub windows: Vec<isize>,
    /// "normal", "indeterminate", "paused" or "error".
    pub state: &'static str,
    /// From 0 to 1; none while indeterminate before any value was given.
    pub value: Option<f64>,
}

static ENTRIES: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Bloom's `Shell_TrayWnd`.
static HOST: AtomicIsize = AtomicIsize::new(0);
/// Explorer's own task list: what its taskbar named before Bloom.
static EXPLORER_LIST: AtomicIsize = AtomicIsize::new(0);
static ANNOUNCING: AtomicBool = AtomicBool::new(false);
static PRUNING: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<AppHandle> = OnceLock::new();

fn state_name(state: u32) -> &'static str {
    match state {
        TBPF_INDETERMINATE => "indeterminate",
        TBPF_ERROR => "error",
        TBPF_PAUSED => "paused",
        _ => "normal",
    }
}

/// The state a `TBPFLAG` asks for (`TBPF_NOPROGRESS` included), or none for
/// a value that isn't one.
fn state_of(flags: u32) -> Option<u32> {
    [TBPF_ERROR, TBPF_PAUSED, TBPF_NORMAL, TBPF_INDETERMINATE]
        .into_iter()
        .find(|state| flags & state != 0)
        .or((flags == TBPF_NOPROGRESS).then_some(TBPF_NOPROGRESS))
}

fn value_of(lparam: LPARAM) -> f64 {
    (lparam.0 as u32 as f64 / FULL).clamp(0.0, 1.0)
}

/// The folder of a program's path, lower-cased, ending in a separator.
fn folder_of(path: &str) -> Option<String> {
    let path = path.to_lowercase().replace('/', "\\");
    let end = path.rfind('\\')?;
    Some(path[..=end].to_string())
}

/// Whether `parent` belongs to the same install as `child`: its folder holds
/// the child's (`Steam\steam.exe` and `Steam\bin\cef\...\steamwebhelper.exe`).
fn same_install(child: &str, parent: &str) -> bool {
    match (folder_of(child), folder_of(parent)) {
        (Some(child), Some(parent)) => {
            // A program in Windows' own folder starts all sorts of things.
            !parent.starts_with("c:\\windows\\") && child.starts_with(&parent)
        }
        _ => false,
    }
}

/// Every process with its parent.
unsafe fn processes() -> Vec<(u32, u32)> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    if let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    pairs
}

/// A game a launcher started from its own library (Steam keeps games inside
/// its folder, in `steamapps`): the game's own app, not part of the launcher.
fn is_library_game(path: &str) -> bool {
    path.to_lowercase().contains("\\steamapps\\")
}

/// A process's program, then the programs that started it from the same
/// install; and the processes of that install (the topmost of those programs
/// and every process below it from the same install).
unsafe fn lineage(pid: u32) -> (Vec<String>, Vec<u32>) {
    let Some(own) = crate::commands::any_process_path(pid) else {
        return (Vec::new(), vec![pid]);
    };
    let pairs = processes();
    let mut programs = vec![own];
    let mut root = pid;
    for _ in 0..4 {
        let Some(&(_, parent)) = pairs.iter().find(|(p, _)| *p == root) else {
            break;
        };
        if parent == 0 || parent == root {
            break;
        }
        let Some(path) = crate::commands::any_process_path(parent) else {
            break;
        };
        if !same_install(programs.last().unwrap(), &path) {
            break;
        }
        if !programs.iter().any(|p| p.eq_ignore_ascii_case(&path)) {
            programs.push(path);
        }
        root = parent;
    }
    let top = programs.last().cloned().unwrap_or_default();
    let mut family = vec![root];
    let mut index = 0;
    while index < family.len() && family.len() < 64 {
        let parent = family[index];
        for &(child, of) in &pairs {
            if of == parent && child != parent && !family.contains(&child) {
                let path = crate::commands::any_process_path(child).unwrap_or_default();
                if same_install(&path, &top) && !is_library_game(&path) {
                    family.push(child);
                }
            }
        }
        index += 1;
    }
    if !family.contains(&pid) {
        family.push(pid);
    }
    (programs, family)
}

/// The top-level windows of each family, in the same order.
unsafe fn windows_of(families: &[&[u32]]) -> Vec<Vec<isize>> {
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    unsafe extern "system" fn visit(hwnd: HWND, list: LPARAM) -> BOOL {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (*(list.0 as *mut Vec<(isize, u32)>)).push((hwnd.0 as isize, pid));
        true.into()
    }
    let mut all: Vec<(isize, u32)> = Vec::new();
    let _ = EnumWindows(Some(visit), LPARAM(&mut all as *mut _ as isize));
    families
        .iter()
        .map(|family| {
            all.iter()
                .filter(|(_, pid)| family.contains(pid))
                .map(|(hwnd, _)| *hwnd)
                .take(256)
                .collect()
        })
        .collect()
}

unsafe fn new_entry(hwnd: isize, state: u32, value: Option<f64>) -> Entry {
    let window = HWND(hwnd as *mut _);
    let mut pid = 0u32;
    GetWindowThreadProcessId(window, Some(&mut pid));
    let owner = GetAncestor(window, GA_ROOTOWNER);
    let (programs, family) = lineage(pid);
    note(&format!(
        "progress from window={hwnd:#x} program={} processes={}",
        programs.join(" < "),
        family.len()
    ));
    Entry {
        hwnd,
        owner: if owner.0.is_null() {
            hwnd
        } else {
            owner.0 as isize
        },
        programs,
        family,
        state,
        value,
        announced: None,
    }
}

/// Records `SetProgressValue`; true when the dock should redraw.
unsafe fn record_value(hwnd: isize, value: f64) -> bool {
    let Ok(mut entries) = ENTRIES.lock() else {
        return false;
    };
    if let Some(entry) = entries.iter_mut().find(|e| e.hwnd == hwnd) {
        // A value ends the indeterminate state; paused and error stay until
        // the app changes them, the value updating underneath.
        let state = match entry.state {
            TBPF_ERROR | TBPF_PAUSED => entry.state,
            _ => TBPF_NORMAL,
        };
        let redraw = state != entry.state
            || entry.announced.is_none_or(|shown| {
                (shown - value).abs() >= STEP || (value >= 1.0) != (shown >= 1.0)
            });
        entry.state = state;
        entry.value = Some(value);
        return redraw;
    }
    entries.push(new_entry(hwnd, TBPF_NORMAL, Some(value)));
    true
}

/// Records `SetProgressState`; true when the dock should redraw.
unsafe fn record_state(hwnd: isize, state: u32) -> bool {
    let Ok(mut entries) = ENTRIES.lock() else {
        return false;
    };
    let index = entries.iter().position(|e| e.hwnd == hwnd);
    match (index, state) {
        (Some(index), TBPF_NOPROGRESS) => {
            entries.remove(index);
            true
        }
        (None, TBPF_NOPROGRESS) => false,
        (Some(index), state) => {
            let entry = &mut entries[index];
            let redraw = entry.state != state;
            entry.state = state;
            redraw
        }
        (None, state) => {
            entries.push(new_entry(hwnd, state, None));
            true
        }
    }
}

/// Every window showing progress right now.
pub fn snapshot() -> Vec<Progress> {
    let Ok(mut entries) = ENTRIES.lock() else {
        return Vec::new();
    };
    if entries.is_empty() {
        return Vec::new();
    }
    // Looked up each time: the app opens and closes windows as it runs.
    let families: Vec<&[u32]> = entries.iter().map(|e| e.family.as_slice()).collect();
    let windows = unsafe { windows_of(&families) };
    entries
        .iter_mut()
        .zip(windows)
        .map(|(entry, windows)| {
            entry.announced = entry.value;
            Progress {
                hwnd: entry.hwnd,
                owner: entry.owner,
                programs: entry.programs.clone(),
                windows,
                state: state_name(entry.state),
                value: entry.value,
            }
        })
        .collect()
}

#[tauri::command]
pub fn get_taskbar_progress() -> Vec<Progress> {
    snapshot()
}

/// Drops windows that closed without clearing their progress.
fn prune() -> (bool, bool) {
    let Ok(mut entries) = ENTRIES.lock() else {
        return (false, false);
    };
    let before = entries.len();
    entries.retain(|e| unsafe { IsWindow(Some(HWND(e.hwnd as *mut _))) }.as_bool());
    (entries.len() != before, entries.is_empty())
}

unsafe fn changed(window: HWND) {
    if !ANNOUNCING.swap(true, Ordering::Relaxed) {
        SetTimer(Some(window), TIMER_ANNOUNCE, ANNOUNCE_GAP_MS, None);
    }
    if !PRUNING.swap(true, Ordering::Relaxed) {
        SetTimer(Some(window), TIMER_PRUNE, PRUNE_MS, None);
    }
}

fn announce() {
    if let Some(app) = APP.get() {
        let _ = app.emit_to("dock", "taskbar-progress", snapshot());
    }
}

fn valid(hwnd: isize) -> bool {
    hwnd != 0 && unsafe { IsWindow(Some(HWND(hwnd as *mut _))) }.as_bool()
}

/// Explorer's task list window (`MSTaskSwWClass`) inside its taskbar.
unsafe fn find_task_list(taskbar: HWND) -> Option<HWND> {
    unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> BOOL {
        let mut class = [0u16; 32];
        let length = GetClassNameW(hwnd, &mut class) as usize;
        if String::from_utf16_lossy(&class[..length]) == "MSTaskSwWClass" {
            *(found.0 as *mut isize) = hwnd.0 as isize;
            return false.into();
        }
        true.into()
    }
    let mut found: isize = 0;
    let _ = EnumChildWindows(
        Some(taskbar),
        Some(visit),
        LPARAM(&mut found as *mut isize as isize),
    );
    (found != 0).then_some(HWND(found as *mut _))
}

/// Explorer's own task list, where every message is passed on.
unsafe fn explorer_list() -> Option<HWND> {
    let known = EXPLORER_LIST.load(Ordering::Relaxed);
    if valid(known) {
        return Some(HWND(known as *mut _));
    }
    let taskbar = crate::tray_host::explorer_taskbar()?;
    let own = WINDOW.load(Ordering::Relaxed);
    let named = GetPropW(taskbar, PROPERTY).0 as isize;
    let list = if named != own && valid(named) {
        HWND(named as *mut _)
    } else {
        find_task_list(taskbar)?
    };
    EXPLORER_LIST.store(list.0 as isize, Ordering::Relaxed);
    Some(list)
}

/// Makes both taskbars name Bloom's window, remembering Explorer's own list.
unsafe fn claim() {
    let own = WINDOW.load(Ordering::Relaxed);
    if own == 0 {
        return;
    }
    let mine = Some(HANDLE(own as *mut _));
    let host = HOST.load(Ordering::Relaxed);
    if valid(host) && GetPropW(HWND(host as *mut _), PROPERTY).0 as isize != own {
        let _ = SetPropW(HWND(host as *mut _), PROPERTY, mine);
    }
    let Some(taskbar) = crate::tray_host::explorer_taskbar() else {
        return;
    };
    let named = GetPropW(taskbar, PROPERTY).0 as isize;
    if named == own {
        return;
    }
    // Explorer (re)named its list: that is where messages go on to.
    if valid(named) {
        EXPLORER_LIST.store(named, Ordering::Relaxed);
    }
    if SetPropW(taskbar, PROPERTY, mine).is_ok() {
        note("taskbars now name Bloom's window");
    }
}

/// Gives Explorer's taskbar its own task list back. Called on every way Bloom
/// quits; Bloom's own taskbar window goes with Bloom.
pub fn restore() {
    unsafe {
        let own = WINDOW.load(Ordering::Relaxed);
        if own == 0 {
            return;
        }
        let Some(taskbar) = crate::tray_host::explorer_taskbar() else {
            return;
        };
        if GetPropW(taskbar, PROPERTY).0 as isize != own {
            return;
        }
        match explorer_list() {
            Some(list) => {
                let _ = SetPropW(taskbar, PROPERTY, Some(HANDLE(list.0)));
            }
            None => {
                let _ = RemovePropW(taskbar, PROPERTY);
            }
        }
    }
}

/// After a Bloom that couldn't clean up (killed, crashed): Explorer's taskbar
/// naming a window that is gone gets its own task list back. Runs from the
/// watchdog and at start; a live Bloom's window is left alone.
pub fn repair_leftover() {
    unsafe {
        let Some(taskbar) = crate::tray_host::explorer_taskbar() else {
            return;
        };
        let named = GetPropW(taskbar, PROPERTY).0 as isize;
        if named == 0 || valid(named) {
            return;
        }
        match find_task_list(taskbar) {
            Some(list) => {
                let _ = SetPropW(taskbar, PROPERTY, Some(HANDLE(list.0)));
            }
            None => {
                let _ = RemovePropW(taskbar, PROPERTY);
            }
        }
    }
}

/// Passes a message on to Explorer's task list and returns its answer.
unsafe fn forward(msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(target) = explorer_list() else {
        return LRESULT(0);
    };
    if msg == WM_COPYDATA || InSendMessage().as_bool() {
        let mut result = 0usize;
        let sent = SendMessageTimeoutW(
            target,
            msg,
            wparam,
            lparam,
            SMTO_ABORTIFHUNG | SMTO_NORMAL,
            2000,
            Some(&mut result),
        );
        LRESULT(if sent.0 != 0 { result as isize } else { 0 })
    } else {
        let _ = PostMessageW(Some(target), msg, wparam, lparam);
        LRESULT(0)
    }
}

/// Notes what the first messages look like, so a Windows version that numbers
/// them differently can be told from an app that reports nothing.
fn note_first(msg: u32, wparam: WPARAM, lparam: LPARAM) {
    static SEEN: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    let Ok(mut seen) = SEEN.lock() else {
        return;
    };
    if seen.contains(&msg) || seen.len() >= 32 {
        return;
    }
    seen.push(msg);
    drop(seen);
    let program = unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(wparam.0 as *mut _), Some(&mut pid));
        crate::commands::any_process_path(pid).unwrap_or_default()
    };
    note(&format!(
        "first WM_USER+{} window={:#x} value={:#x} program={program}",
        msg.wrapping_sub(WM_USER),
        wparam.0,
        lparam.0
    ));
}

fn note(line: &str) {
    use std::io::Write;
    let Some(path) = APP
        .get()
        .and_then(|app| app.path().app_config_dir().ok())
        .map(|dir| dir.join("taskbar-progress.log"))
    else {
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

unsafe extern "system" fn window_proc(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TIMER {
        match wparam.0 {
            TIMER_ANNOUNCE => {
                let _ = KillTimer(Some(window), TIMER_ANNOUNCE);
                ANNOUNCING.store(false, Ordering::Relaxed);
                announce();
            }
            TIMER_PRUNE => {
                let (dropped, empty) = prune();
                if empty {
                    let _ = KillTimer(Some(window), TIMER_PRUNE);
                    PRUNING.store(false, Ordering::Relaxed);
                }
                if dropped {
                    announce();
                }
            }
            TIMER_CLAIM => claim(),
            _ => {}
        }
        return LRESULT(0);
    }
    // Only the task list's own messages: the window is a hidden top-level
    // one, so broadcasts reach it too, and those are not passed on.
    if msg != WM_COPYDATA && !LIST_MESSAGES.contains(&msg) {
        return DefWindowProcW(window, msg, wparam, lparam);
    }

    note_first(msg, wparam, lparam);
    let hwnd = wparam.0 as isize;
    if hwnd != 0 && (msg == SET_PROGRESS_VALUE || msg == SET_PROGRESS_STATE) {
        let redraw = if msg == SET_PROGRESS_VALUE {
            record_value(hwnd, value_of(lparam))
        } else {
            state_of(lparam.0 as u32).is_some_and(|state| {
                let changed = record_state(hwnd, state);
                if changed {
                    note(&format!(
                        "state {} window={hwnd:#x}",
                        if state == TBPF_NOPROGRESS {
                            "none"
                        } else {
                            state_name(state)
                        }
                    ));
                }
                changed
            })
        };
        if redraw {
            changed(window);
        }
    }
    forward(msg, wparam, lparam)
}

/// Makes the window and names it on Bloom's `Shell_TrayWnd` (`host`) and on
/// Explorer's taskbar. Runs on the tray host's thread, which keeps the
/// window's messages flowing.
pub(crate) unsafe fn start(app: &AppHandle, module: HINSTANCE, host: HWND) {
    let _ = APP.set(app.clone());
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: module,
        lpszClassName: w!("BloomTaskList"),
        ..Default::default()
    };
    if RegisterClassExW(&class) == 0 {
        return;
    }
    // A hidden top-level window, as ManagedShell's, never shown.
    let Ok(window) = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w!("BloomTaskList"),
        w!(""),
        WS_POPUP,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(module),
        None,
    ) else {
        return;
    };
    // Sandboxed programs reach Explorer's task list, so they must reach this.
    let _ = ChangeWindowMessageFilterEx(window, WM_COPYDATA, MSGFLT_ALLOW, None);
    for msg in LIST_MESSAGES {
        let _ = ChangeWindowMessageFilterEx(window, msg, MSGFLT_ALLOW, None);
    }
    WINDOW.store(window.0 as isize, Ordering::Relaxed);
    HOST.store(host.0 as isize, Ordering::Relaxed);
    // Explorer's own list, found before its taskbar is made to name Bloom's.
    let _ = explorer_list();
    claim();
    SetTimer(Some(window), TIMER_CLAIM, CLAIM_MS, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_follow_windows_priority() {
        assert_eq!(state_of(0), Some(TBPF_NOPROGRESS));
        assert_eq!(state_of(TBPF_NORMAL), Some(TBPF_NORMAL));
        assert_eq!(state_of(TBPF_PAUSED | TBPF_NORMAL), Some(TBPF_PAUSED));
        assert_eq!(state_of(0x40), None);
    }

    #[test]
    fn values_are_scaled_and_clamped() {
        assert_eq!(value_of(LPARAM(0)), 0.0);
        assert_eq!(value_of(LPARAM(65534)), 1.0);
        assert_eq!(value_of(LPARAM(70000)), 1.0);
        assert!((value_of(LPARAM(32767)) - 0.5).abs() < 0.001);
    }

    #[test]
    fn a_helper_belongs_to_the_program_that_installed_it() {
        let steam = r"C:\Program Files (x86)\Steam\steam.exe";
        let helper = r"C:\Program Files (x86)\Steam\bin\cef\cef.win64\steamwebhelper.exe";
        assert!(same_install(helper, steam));
        assert!(!same_install(steam, helper));
        assert!(!same_install(steam, r"C:\Windows\explorer.exe"));
        assert!(!same_install(r"C:\Games\game.exe", steam));
        assert!(is_library_game(
            r"C:\Program Files (x86)\Steam\steamapps\common\Game\game.exe"
        ));
        assert!(!is_library_game(helper));
    }
}
