//! The desktop's right-click menu beside the dock, opened at the pointer.
//!
//! The strip beside the dock is kept free for Bloom, so it's outside the
//! area Windows gives apps. A right click on the desktop there still opens
//! the desktop's menu, but Explorer places its menu inside the apps' area,
//! a long way above the pointer, as if the old taskbar were still there. It
//! can't be told otherwise: it moves itself back when moved, and it places
//! itself from its own record of the dock's space, not from the screen's
//! settings other apps can change.
//!
//! So that right click is Bloom's: held back from Windows, and Bloom's dock
//! shows the same menu Windows 11 does (View, Sort by, Refresh, New,
//! Display settings, Personalise, Open in Terminal, Show more options) at
//! the pointer. Each item does what Windows' does, on the desktop itself:
//! the view, sorting and icon settings go to the desktop's own view, New
//! makes the item the way Explorer does (from Windows' list of new items)
//! where the click was and starts renaming it, and Show more options opens
//! Windows' own full classic menu at the pointer. In the strip beside a
//! notch that keeps its place at the top, Windows' menu is left as it is.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetShellWindow, GetWindowThreadProcessId, WindowFromPoint, GA_ROOT,
};

/// Where the strip is: the bottom (beside the dock) or the top (beside a
/// notch that keeps its place).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Strip {
    Bottom,
    Top,
}

/// A right press on the desktop in a strip was held back from Windows.
static HELD_BACK: AtomicBool = AtomicBool::new(false);
/// The release that goes with it, waiting for the cursor worker.
static RELEASED_AT: AtomicI64 = AtomicI64::new(i64::MIN);
/// One click taken over at a time.
static OPENING: AtomicBool = AtomicBool::new(false);

fn log(line: &str) {
    use std::io::Write;
    let Some(path) = crate::utils::data_file("desktop-menu.log") else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 24 * 1024) {
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

fn monitor_of(point: POINT) -> Option<MONITORINFO> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(monitor, &mut info)
            .as_bool()
            .then_some(info)
    }
}

/// Which strip the point is in, when it's outside the apps' area at the top
/// or bottom of its screen.
fn strip_at(point: POINT) -> Option<Strip> {
    let info = monitor_of(point)?;
    let (work, screen) = (info.rcWork, info.rcMonitor);
    if point.y >= work.bottom && point.y < screen.bottom {
        Some(Strip::Bottom)
    } else if point.y < work.top && point.y >= screen.top {
        Some(Strip::Top)
    } else {
        None
    }
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn shell_pid() -> u32 {
    pid_of(unsafe { GetShellWindow() })
}

/// Whether the desktop itself is under the point (not the dock, a window
/// or a notch).
fn desktop_under(point: POINT) -> bool {
    unsafe {
        let under = WindowFromPoint(point);
        if under.0.is_null() {
            return false;
        }
        let root = GetAncestor(under, GA_ROOT);
        let root = if root.0.is_null() { under } else { root };
        let mut class = [0u16; 32];
        let length = GetClassNameW(root, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..length.min(class.len())]);
        (class == "Progman" || class == "WorkerW") && pid_of(root) == shell_pid()
    }
}

/// From the mouse hook (it must answer at once): whether to hold this right
/// press back from Windows. Only a real (not Bloom-made) press on the desktop
/// in a strip is.
pub fn hold_back_press(point: POINT, injected: bool) -> bool {
    if injected || OPENING.load(Ordering::Relaxed) {
        return false;
    }
    let take = strip_at(point).is_some() && desktop_under(point);
    HELD_BACK.store(take, Ordering::Relaxed);
    take
}

/// From the mouse hook: whether to hold this right release back too (it
/// belongs to a press that was). Notes where, for `open_menu_pending`.
pub fn hold_back_release(point: POINT, injected: bool) -> bool {
    if injected || !HELD_BACK.swap(false, Ordering::Relaxed) {
        return false;
    }
    RELEASED_AT.store(
        ((point.x as u32 as u64) << 32 | point.y as u32 as u64) as i64,
        Ordering::Relaxed,
    );
    true
}

/// From the cursor worker: opens the desktop's menu for a click held back.
pub fn open_menu_pending(app: &tauri::AppHandle) {
    let packed = RELEASED_AT.swap(i64::MIN, Ordering::Relaxed);
    if packed == i64::MIN {
        return;
    }
    let point = POINT {
        x: (packed as u64 >> 32) as u32 as i32,
        y: packed as u32 as i32,
    };
    if OPENING.swap(true, Ordering::Relaxed) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if strip_at(point) == Some(Strip::Bottom) {
            if let Err(why) = show(&app, point) {
                log(&format!(
                    "Bloom's desktop menu couldn't open ({why}): Windows' instead"
                ));
                right_click();
            }
        } else {
            // Beside the notch: Windows' own menu.
            right_click();
        }
        OPENING.store(false, Ordering::Relaxed);
    });
}

/// The same right click again, where the pointer is (marked as Bloom's, so
/// the mouse hook lets it through).
fn right_click() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };
    let event = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    unsafe {
        SendInput(
            &[event(MOUSEEVENTF_RIGHTDOWN), event(MOUSEEVENTF_RIGHTUP)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

// ---------------------------------------------------------------- the desktop's view

/// Runs `f` on a thread of its own with COM set up for the shell (an
/// apartment thread), and gives back what it gave.
fn on_shell_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    std::thread::spawn(move || unsafe {
        let started = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = f();
        if started {
            CoUninitialize();
        }
        result
    })
    .join()
    .ok()
}

struct DesktopView {
    view: windows::Win32::UI::Shell::IShellView,
    folder: windows::Win32::UI::Shell::IFolderView2,
    hwnd: HWND,
}

/// The desktop's own view (in Explorer), the way Windows documents reaching
/// it from another program.
unsafe fn desktop_view() -> Result<DesktopView, String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, IServiceProvider, CLSCTX_ALL};
    use windows::Win32::System::Ole::IOleWindow;
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::{
        IShellBrowser, IShellWindows, SID_STopLevelBrowser, ShellWindows, SWC_DESKTOP,
        SWFO_NEEDDISPATCH,
    };
    let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)
        .map_err(|e| format!("the shell's windows: {e}"))?;
    let location = VARIANT::from(0i32); // the desktop
    let root = VARIANT::default();
    let mut hwnd = 0i32;
    let dispatch = windows
        .FindWindowSW(&location, &root, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
        .map_err(|e| format!("the desktop: {e}"))?;
    let provider: IServiceProvider = dispatch.cast().map_err(|e| e.to_string())?;
    let browser: IShellBrowser = provider
        .QueryService(&SID_STopLevelBrowser)
        .map_err(|e| format!("the desktop's browser: {e}"))?;
    let view = browser
        .QueryActiveShellView()
        .map_err(|e| format!("the desktop's view: {e}"))?;
    let folder = view.cast().map_err(|e| e.to_string())?;
    let hwnd = view
        .cast::<IOleWindow>()
        .and_then(|w| w.GetWindow())
        .unwrap_or_default();
    Ok(DesktopView { view, folder, hwnd })
}

const FMTID_STORAGE: windows::core::GUID =
    windows::core::GUID::from_u128(0xb725f130_47ef_101a_a5f1_02608c9eebac);

fn sort_key(name: &str) -> Option<windows::Win32::Foundation::PROPERTYKEY> {
    let pid = match name {
        "name" => 10,
        "size" => 12,
        "type" => 4,
        "date" => 14,
        _ => return None,
    };
    Some(windows::Win32::Foundation::PROPERTYKEY {
        fmtid: FMTID_STORAGE,
        pid,
    })
}

#[derive(Clone, Debug, serde::Serialize)]
struct ViewState {
    /// "large", "medium" or "small".
    size: String,
    auto_arrange: bool,
    grid: bool,
    icons: bool,
    /// "name", "size", "type", "date" or "".
    sort: String,
}

unsafe fn view_state(view: &DesktopView) -> ViewState {
    use windows::Win32::UI::Shell::{
        FOLDERVIEWMODE, FWF_AUTOARRANGE, FWF_NOICONS, FWF_SNAPTOGRID, SORTCOLUMN,
    };
    let mut mode = FOLDERVIEWMODE(0);
    let mut size = 48;
    let _ = view.folder.GetViewModeAndIconSize(&mut mode, &mut size);
    let flags = view.folder.GetCurrentFolderFlags().unwrap_or(0);
    let mut sort = String::new();
    if view.folder.GetSortColumnCount().unwrap_or(0) > 0 {
        let mut columns = [SORTCOLUMN::default()];
        if view.folder.GetSortColumns(&mut columns).is_ok() {
            for name in ["name", "size", "type", "date"] {
                if sort_key(name).is_some_and(|k| {
                    k.fmtid == columns[0].propkey.fmtid && k.pid == columns[0].propkey.pid
                }) {
                    sort = name.into();
                }
            }
        }
    }
    ViewState {
        size: if size >= 72 {
            "large"
        } else if size >= 40 {
            "medium"
        } else {
            "small"
        }
        .into(),
        auto_arrange: flags & FWF_AUTOARRANGE.0 as u32 != 0,
        grid: flags & FWF_SNAPTOGRID.0 as u32 != 0,
        icons: flags & FWF_NOICONS.0 as u32 == 0,
        sort,
    }
}

// ---------------------------------------------------------------- New

/// One of the things New makes, from Windows' own list of them.
#[derive(Clone, Debug, serde::Serialize)]
struct NewItem {
    /// "Folder", or the file's extension (".txt").
    kind: String,
    name: String,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn registry_value(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    value: Option<&str>,
    kinds: windows::Win32::System::Registry::REG_ROUTINE_FLAGS,
) -> Option<(u32, Vec<u8>)> {
    use windows::Win32::System::Registry::{RegGetValueW, REG_VALUE_TYPE};
    let key = wide(key);
    let value = value.map(wide);
    let name = value.as_ref().map_or(windows::core::PCWSTR::null(), |v| {
        windows::core::PCWSTR(v.as_ptr())
    });
    let mut size = 0u32;
    let mut kind = REG_VALUE_TYPE(0);
    unsafe {
        RegGetValueW(
            root,
            windows::core::PCWSTR(key.as_ptr()),
            name,
            kinds,
            Some(&mut kind),
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut data = vec![0u8; size as usize];
        RegGetValueW(
            root,
            windows::core::PCWSTR(key.as_ptr()),
            name,
            kinds,
            Some(&mut kind),
            Some(data.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        data.truncate(size as usize);
        Some((kind.0, data))
    }
}

fn utf16_strings(data: &[u8]) -> Vec<String> {
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    units
        .split(|&u| u == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

fn friendly_name(kind: &str) -> String {
    use windows::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_NONE, ASSOCSTR_FRIENDLYDOCNAME};
    if kind.eq_ignore_ascii_case("folder") {
        return "Folder".into();
    }
    let assoc = wide(kind);
    let mut buffer = [0u16; 260];
    let mut length = buffer.len() as u32;
    let ok = unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_FRIENDLYDOCNAME,
            windows::core::PCWSTR(assoc.as_ptr()),
            windows::core::PCWSTR::null(),
            Some(windows::core::PWSTR(buffer.as_mut_ptr())),
            &mut length,
        )
    }
    .is_ok();
    let name = String::from_utf16_lossy(&buffer[..(length as usize).saturating_sub(1)]);
    if ok && !name.trim().is_empty() {
        name
    } else {
        format!("{} File", kind.trim_start_matches('.').to_uppercase())
    }
}

/// Windows' list of what New makes (the one Explorer keeps): Folder and
/// Shortcut first, then the rest by name.
fn new_items() -> Vec<NewItem> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_MULTI_SZ};
    let mut kinds = registry_value(
        HKEY_CURRENT_USER,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Discardable\\PostSetup\\ShellNew",
        Some("Classes"),
        RRF_RT_REG_MULTI_SZ,
    )
    .map(|(_, data)| utf16_strings(&data))
    .unwrap_or_default();
    if kinds.is_empty() {
        kinds = vec!["Folder".into(), ".lnk".into(), ".txt".into()];
    }
    let mut items: Vec<NewItem> = kinds
        .into_iter()
        .filter(|k| k.eq_ignore_ascii_case("folder") || k.starts_with('.'))
        .map(|kind| NewItem {
            name: friendly_name(&kind),
            kind,
        })
        .collect();
    let rank = |item: &NewItem| {
        if item.kind.eq_ignore_ascii_case("folder") {
            0
        } else if item.kind.eq_ignore_ascii_case(".lnk") {
            1
        } else {
            2
        }
    };
    items.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    items.dedup_by(|a, b| a.kind.eq_ignore_ascii_case(&b.kind));
    items
}

/// How Windows makes a new file of a kind: its `ShellNew` entry.
enum Template {
    Empty,
    Copy(std::path::PathBuf),
    Bytes(Vec<u8>),
    Command(String),
}

fn template_of(kind: &str) -> Template {
    use windows::Win32::System::Registry::{HKEY_CLASSES_ROOT, RRF_RT_ANY, RRF_RT_REG_SZ};
    let prog_id = registry_value(HKEY_CLASSES_ROOT, kind, None, RRF_RT_REG_SZ)
        .map(|(_, d)| utf16_strings(&d).into_iter().next().unwrap_or_default())
        .unwrap_or_default();
    let mut keys = vec![format!("{kind}\\ShellNew")];
    if !prog_id.is_empty() {
        keys.insert(0, format!("{kind}\\{prog_id}\\ShellNew"));
    }
    let expand = |text: &str| {
        let source = wide(text);
        let mut out = vec![0u16; 2048];
        let n = unsafe {
            windows::Win32::System::Environment::ExpandEnvironmentStringsW(
                windows::core::PCWSTR(source.as_ptr()),
                Some(&mut out),
            )
        } as usize;
        String::from_utf16_lossy(&out[..n.saturating_sub(1).min(out.len())])
    };
    for key in keys {
        if let Some((_, data)) =
            registry_value(HKEY_CLASSES_ROOT, &key, Some("Command"), RRF_RT_ANY)
        {
            if let Some(command) = utf16_strings(&data).into_iter().next() {
                return Template::Command(expand(&command));
            }
        }
        if let Some((kind, data)) =
            registry_value(HKEY_CLASSES_ROOT, &key, Some("Data"), RRF_RT_ANY)
        {
            // REG_BINARY as it is; text without its closing zero.
            return Template::Bytes(if kind == 3 {
                data
            } else {
                utf16_strings(&data).join("").into_bytes()
            });
        }
        if let Some((_, data)) =
            registry_value(HKEY_CLASSES_ROOT, &key, Some("FileName"), RRF_RT_ANY)
        {
            if let Some(name) = utf16_strings(&data).into_iter().next() {
                let name = expand(&name);
                let mut places = vec![std::path::PathBuf::from(&name)];
                for dir in [
                    expand("%SystemRoot%\\ShellNew"),
                    expand("%APPDATA%\\Microsoft\\Windows\\Templates"),
                    expand("%ProgramData%\\Microsoft\\Windows\\Templates"),
                ] {
                    places.push(std::path::Path::new(&dir).join(&name));
                }
                if let Some(found) = places.into_iter().find(|p| p.is_absolute() && p.is_file()) {
                    return Template::Copy(found);
                }
                return Template::Empty;
            }
        }
        if registry_value(HKEY_CLASSES_ROOT, &key, Some("NullFile"), RRF_RT_ANY).is_some() {
            return Template::Empty;
        }
    }
    Template::Empty
}

fn desktop_dir() -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as *const _));
        text.map(std::path::PathBuf::from)
    }
}

/// "New Text Document.txt", or "New Text Document (2).txt" when taken.
fn free_name(dir: &std::path::Path, stem: &str, ext: &str) -> std::path::PathBuf {
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}{ext}")
        } else {
            format!("{stem} ({n}){ext}")
        };
        let path = dir.join(name);
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

/// Makes a new item on the desktop the way Explorer's New does.
fn make_new(item: &NewItem) -> Result<(std::path::PathBuf, bool), String> {
    let dir = desktop_dir().ok_or("the desktop folder wasn't found")?;
    if item.kind.eq_ignore_ascii_case("folder") {
        let path = free_name(&dir, "New folder", "");
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        return Ok((path, true));
    }
    let path = free_name(&dir, &format!("New {}", item.name), &item.kind);
    match template_of(&item.kind) {
        Template::Empty => std::fs::write(&path, []).map_err(|e| e.to_string())?,
        Template::Copy(from) => {
            std::fs::copy(&from, &path).map_err(|e| e.to_string())?;
        }
        Template::Bytes(bytes) => std::fs::write(&path, bytes).map_err(|e| e.to_string())?,
        Template::Command(command) => {
            // Explorer makes the file, then the command fills it (the new
            // shortcut wizard does that).
            std::fs::write(&path, []).map_err(|e| e.to_string())?;
            let filled = command.replace("%1", &path.display().to_string());
            run_command_line(&filled)?;
            return Ok((path, false));
        }
    }
    Ok((path, true))
}

fn run_command_line(line: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let line = line.trim();
    let (program, rest) = if let Some(stripped) = line.strip_prefix('"') {
        let end = stripped.find('"').unwrap_or(stripped.len());
        (&stripped[..end], stripped.get(end + 1..).unwrap_or(""))
    } else {
        let end = line.find(' ').unwrap_or(line.len());
        (&line[..end], &line[end..])
    };
    std::process::Command::new(program)
        .raw_arg(rest.trim())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Selects a new item on the desktop, where the click was, and starts
/// renaming it (Windows shows it a moment after it's made).
unsafe fn select_and_rename(view: &DesktopView, path: &std::path::Path, point: POINT) {
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        ILFree, SHGetDesktopFolder, SVSI_DESELECTOTHERS, SVSI_EDIT, SVSI_ENSUREVISIBLE,
        SVSI_POSITIONITEM, SVSI_SELECT,
    };
    let Ok(desktop) = SHGetDesktopFolder() else {
        return;
    };
    let name = wide(&path.display().to_string());
    let mut client = point;
    let _ = ScreenToClient(view.hwnd, &mut client);
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(2500) {
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        let mut attributes = 0u32;
        if desktop
            .ParseDisplayName(
                HWND::default(),
                None,
                windows::core::PCWSTR(name.as_ptr()),
                None,
                &mut pidl,
                &mut attributes,
            )
            .is_ok()
            && !pidl.is_null()
        {
            let items = [pidl as *const ITEMIDLIST];
            let placed = view
                .folder
                .SelectAndPositionItems(
                    1,
                    items.as_ptr(),
                    Some(&client),
                    (SVSI_SELECT.0
                        | SVSI_DESELECTOTHERS.0
                        | SVSI_POSITIONITEM.0
                        | SVSI_ENSUREVISIBLE.0) as u32,
                )
                .is_ok();
            let edited = view
                .view
                .SelectItem(
                    pidl,
                    (SVSI_EDIT.0 | SVSI_SELECT.0 | SVSI_DESELECTOTHERS.0) as u32,
                )
                .is_ok();
            ILFree(Some(pidl));
            if placed || edited {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    log("the new item didn't show on the desktop to rename");
}

// ---------------------------------------------------------------- the menu

#[derive(Clone, Debug, serde::Serialize)]
struct MenuPayload {
    /// Where the pointer is, in the dock window's page (CSS pixels).
    x: f64,
    y: f64,
    view: ViewState,
    new: Vec<NewItem>,
    terminal: bool,
}

/// The click's place and its New items, for the item picked.
static SESSION: std::sync::Mutex<Option<(POINT, Vec<NewItem>)>> = std::sync::Mutex::new(None);

fn terminal_installed() -> bool {
    std::env::var_os("LOCALAPPDATA").is_some_and(|dir| {
        std::path::Path::new(&dir)
            .join("Microsoft\\WindowsApps\\wt.exe")
            .exists()
    })
}

fn show(app: &tauri::AppHandle, point: POINT) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    let view = on_shell_thread(|| unsafe { desktop_view().map(|v| view_state(&v)) })
        .ok_or("the shell thread stopped")??;
    let new = new_items();
    let dock = app
        .get_webview_window("dock")
        .ok_or("the dock isn't there")?;
    let place = dock.outer_position().map_err(|e| e.to_string())?;
    let scale = dock.scale_factor().map_err(|e| e.to_string())?;
    let payload = MenuPayload {
        x: (point.x - place.x) as f64 / scale,
        y: (point.y - place.y) as f64 / scale,
        view,
        new: new.clone(),
        terminal: terminal_installed(),
    };
    if let Ok(mut session) = SESSION.lock() {
        *session = Some((point, new));
    }
    dock.emit_to("dock", "desktop-menu", payload)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Does what was picked in the dock's desktop menu.
#[tauri::command]
pub async fn desktop_menu_run(action: String) -> Result<(), String> {
    let (point, new) = SESSION
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .ok_or("The menu is gone")?;
    tauri::async_runtime::spawn_blocking(move || {
        on_shell_thread(move || unsafe { run(&action, point, &new) })
            .unwrap_or_else(|| Err("the shell thread stopped".into()))
    })
    .await
    .map_err(|e| e.to_string())?
}

unsafe fn run(action: &str, point: POINT, new: &[NewItem]) -> Result<(), String> {
    use windows::Win32::UI::Shell::{
        FOLDERVIEWMODE, FVM_ICON, FWF_AUTOARRANGE, FWF_NOICONS, FWF_SNAPTOGRID, SORTCOLUMN,
        SORTDIRECTION,
    };
    log(&format!("desktop menu: {action}"));
    match action {
        "display" => return open_uri("ms-settings:display"),
        "personalise" => return open_uri("ms-settings:personalization"),
        "terminal" => {
            let dir = desktop_dir().unwrap_or_default();
            return std::process::Command::new("wt.exe")
                .arg("-d")
                .arg(&dir)
                .spawn()
                .map(|_| ())
                .or_else(|_| {
                    std::process::Command::new("powershell.exe")
                        .arg("-NoExit")
                        .current_dir(&dir)
                        .spawn()
                        .map(|_| ())
                })
                .map_err(|e| e.to_string());
        }
        "more" => return classic_menu(point),
        _ => {}
    }
    let view = desktop_view()?;
    let flag = |mask: i32| -> Result<(), String> {
        let now = view.folder.GetCurrentFolderFlags().unwrap_or(0);
        let mask = mask as u32;
        view.folder
            .SetCurrentFolderFlags(mask, (now ^ mask) & mask)
            .map_err(|e| e.to_string())
    };
    match action {
        "view:large" | "view:medium" | "view:small" => {
            let size = match action {
                "view:large" => 96,
                "view:medium" => 48,
                _ => 32,
            };
            let mut mode = FOLDERVIEWMODE(0);
            let mut _now = 0;
            let _ = view.folder.GetViewModeAndIconSize(&mut mode, &mut _now);
            view.folder
                .SetViewModeAndIconSize(FVM_ICON, size)
                .map_err(|e| e.to_string())
        }
        "view:auto" => flag(FWF_AUTOARRANGE.0),
        "view:grid" => flag(FWF_SNAPTOGRID.0),
        "view:icons" => flag(FWF_NOICONS.0),
        "refresh" => view.view.Refresh().map_err(|e| e.to_string()),
        _ if action.starts_with("sort:") => {
            let name = &action[5..];
            let key = sort_key(name).ok_or("unknown sort")?;
            // The same column again turns the order round, as in Windows.
            let mut current = [SORTCOLUMN::default()];
            let same = view.folder.GetSortColumnCount().unwrap_or(0) > 0
                && view.folder.GetSortColumns(&mut current).is_ok()
                && current[0].propkey.fmtid == key.fmtid
                && current[0].propkey.pid == key.pid;
            let direction = if same {
                SORTDIRECTION(-current[0].direction.0)
            } else if name == "name" || name == "type" {
                SORTDIRECTION(1)
            } else {
                SORTDIRECTION(-1)
            };
            view.folder
                .SetSortColumns(&[SORTCOLUMN {
                    propkey: key,
                    direction,
                }])
                .map_err(|e| e.to_string())
        }
        _ if action.starts_with("new:") => {
            let index: usize = action[4..].parse().map_err(|_| "unknown item")?;
            let item = new.get(index).ok_or("unknown item")?;
            let (path, rename) = make_new(item)?;
            if rename {
                select_and_rename(&view, &path, point);
            }
            Ok(())
        }
        _ => Err(format!("unknown action {action}")),
    }
}

// ---------------------------------------------------------------- Show more options

thread_local! {
    static MENU_HANDLER: std::cell::RefCell<Option<windows::Win32::UI::Shell::IContextMenu3>> =
        const { std::cell::RefCell::new(None) };
}

unsafe extern "system" fn owner_proc(
    hwnd: HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR,
    };
    if matches!(
        message,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        let handled = MENU_HANDLER.with(|h| {
            h.borrow().as_ref().map(|menu| {
                let mut result = windows::Win32::Foundation::LRESULT(0);
                menu.HandleMenuMsg2(message, wparam, lparam, Some(&mut result))
                    .is_ok()
                    .then_some(result)
            })
        });
        if let Some(Some(result)) = handled {
            return result;
        }
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

/// Windows' dark menus for this program's own menus (what Explorer's
/// classic menu looks like with the dark theme).
unsafe fn dark_menus() {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    let Ok(uxtheme) = LoadLibraryW(windows::core::w!("uxtheme.dll")) else {
        return;
    };
    // SetPreferredAppMode (135) and FlushMenuThemes (136), by number only.
    if let Some(set_mode) = GetProcAddress(uxtheme, windows::core::PCSTR(135 as *const u8)) {
        let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
        set_mode(2);
    }
    if let Some(flush) = GetProcAddress(uxtheme, windows::core::PCSTR(136 as *const u8)) {
        let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
        flush();
    }
}

/// Windows' own full desktop menu (what "Show more options" opens), at the
/// pointer, with View, Sort by and Refresh as in Explorer's.
unsafe fn classic_menu(point: POINT) -> Result<(), String> {
    use windows::core::{Interface, PCSTR, PCWSTR};
    use windows::Win32::UI::Shell::{
        IContextMenu, IContextMenu3, SHGetDesktopFolder, CMF_NORMAL, CMIC_MASK_PTINVOKE,
        CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, DestroyWindow, InsertMenuW,
        PostMessageW, RegisterClassW, SetForegroundWindow, TrackPopupMenuEx, HMENU, MF_BYPOSITION,
        MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING, SW_SHOWNORMAL, TPM_BOTTOMALIGN,
        TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_NULL, WNDCLASSW,
        WS_POPUP,
    };
    // The verb is given as text too (SEE_MASK_UNICODE).
    const CMIC_MASK_UNICODE: u32 = 0x0000_4000;
    dark_menus();
    let class = windows::core::w!("BloomDesktopMenuOwner");
    let instance =
        windows::Win32::System::LibraryLoader::GetModuleHandleW(None).map_err(|e| e.to_string())?;
    let wc = WNDCLASSW {
        lpfnWndProc: Some(owner_proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassW(&wc);
    let owner = CreateWindowExW(
        WINDOW_EX_STYLE(0x80), // WS_EX_TOOLWINDOW
        class,
        PCWSTR::null(),
        WS_POPUP,
        point.x,
        point.y,
        0,
        0,
        None,
        None,
        Some(instance.into()),
        None,
    )
    .map_err(|e| e.to_string())?;
    let desktop = SHGetDesktopFolder().map_err(|e| e.to_string())?;
    let menu: IContextMenu = desktop
        .CreateViewObject(owner)
        .map_err(|e| format!("the desktop's menu: {e}"))?;
    let popup = CreatePopupMenu().map_err(|e| e.to_string())?;
    let _ = menu.QueryContextMenu(popup, 0, 1, 0x6FFF, CMF_NORMAL);
    // View, Sort by and Refresh come from the desktop's view in Explorer:
    // the same ones, at the top, from Bloom.
    let state = desktop_view().map(|v| view_state(&v)).ok();
    const VIEW: u32 = 0x7001;
    let view_menu = CreatePopupMenu().map_err(|e| e.to_string())?;
    let check = |on: bool| if on { MF_CHECKED } else { MF_STRING };
    let size = state.as_ref().map(|s| s.size.clone()).unwrap_or_default();
    let _ = AppendMenuW(
        view_menu,
        check(size == "large") | MF_STRING,
        VIEW as usize,
        windows::core::w!("Large icons"),
    );
    let _ = AppendMenuW(
        view_menu,
        check(size == "medium") | MF_STRING,
        (VIEW + 1) as usize,
        windows::core::w!("Medium icons"),
    );
    let _ = AppendMenuW(
        view_menu,
        check(size == "small") | MF_STRING,
        (VIEW + 2) as usize,
        windows::core::w!("Small icons"),
    );
    let _ = AppendMenuW(view_menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(
        view_menu,
        check(state.as_ref().is_some_and(|s| s.auto_arrange)) | MF_STRING,
        (VIEW + 3) as usize,
        windows::core::w!("Auto arrange icons"),
    );
    let _ = AppendMenuW(
        view_menu,
        check(state.as_ref().is_some_and(|s| s.grid)) | MF_STRING,
        (VIEW + 4) as usize,
        windows::core::w!("Align icons to grid"),
    );
    let _ = AppendMenuW(view_menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(
        view_menu,
        check(state.as_ref().is_none_or(|s| s.icons)) | MF_STRING,
        (VIEW + 5) as usize,
        windows::core::w!("Show desktop icons"),
    );
    let sort_menu = CreatePopupMenu().map_err(|e| e.to_string())?;
    let sort = state.as_ref().map(|s| s.sort.clone()).unwrap_or_default();
    for (i, (key, label)) in [
        ("name", windows::core::w!("Name")),
        ("size", windows::core::w!("Size")),
        ("type", windows::core::w!("Item type")),
        ("date", windows::core::w!("Date modified")),
    ]
    .into_iter()
    .enumerate()
    {
        let _ = AppendMenuW(
            sort_menu,
            check(sort == key) | MF_STRING,
            (VIEW + 10 + i as u32) as usize,
            label,
        );
    }
    let _ = InsertMenuW(
        popup,
        0,
        MF_BYPOSITION | MF_POPUP,
        view_menu.0 as usize,
        windows::core::w!("View"),
    );
    let _ = InsertMenuW(
        popup,
        1,
        MF_BYPOSITION | MF_POPUP,
        sort_menu.0 as usize,
        windows::core::w!("Sort by"),
    );
    let _ = InsertMenuW(
        popup,
        2,
        MF_BYPOSITION | MF_STRING,
        (VIEW + 20) as usize,
        windows::core::w!("Refresh"),
    );
    let _ = InsertMenuW(popup, 3, MF_BYPOSITION | MF_SEPARATOR, 0, PCWSTR::null());

    MENU_HANDLER.with(|h| *h.borrow_mut() = menu.cast::<IContextMenu3>().ok());
    let _ = SetForegroundWindow(owner);
    let picked = TrackPopupMenuEx(
        popup,
        (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN).0,
        point.x,
        point.y,
        owner,
        None,
    )
    .0 as u32;
    let _ = PostMessageW(Some(owner), WM_NULL, Default::default(), Default::default());
    MENU_HANDLER.with(|h| *h.borrow_mut() = None);
    let result = match picked {
        0 => Ok(()),
        id if (VIEW..VIEW + 3).contains(&id) => run(
            ["view:large", "view:medium", "view:small"][(id - VIEW) as usize],
            point,
            &[],
        ),
        id if id == VIEW + 3 => run("view:auto", point, &[]),
        id if id == VIEW + 4 => run("view:grid", point, &[]),
        id if id == VIEW + 5 => run("view:icons", point, &[]),
        id if (VIEW + 10..VIEW + 14).contains(&id) => run(
            ["sort:name", "sort:size", "sort:type", "sort:date"][(id - VIEW - 10) as usize],
            point,
            &[],
        ),
        id if id == VIEW + 20 => run("refresh", point, &[]),
        id => {
            let verb = (id - 1) as usize;
            let info = CMINVOKECOMMANDINFOEX {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
                hwnd: owner,
                lpVerb: PCSTR(verb as *const u8),
                lpVerbW: PCWSTR(verb as *const u16),
                nShow: SW_SHOWNORMAL.0,
                ptInvoke: point,
                ..Default::default()
            };
            menu.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO)
                .map_err(|e| e.to_string())
        }
    };
    let _ = DestroyMenu(popup);
    let _ = DestroyWindow(owner);
    let _: Option<HMENU> = None;
    result
}

fn open_uri(uri: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let uri = wide(uri);
    let verb = wide("open");
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(uri.as_ptr()),
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows couldn't open {}",
            String::from_utf16_lossy(&uri[..uri.len() - 1])
        ))
    }
}
