//! The tray icons of running apps, for the notch's tray and the dock.
//!
//! Bloom's tray host (`tray_host`) sees every icon as apps add and change it,
//! so its list is used whenever it runs: instant, with each icon's own live
//! picture, and clicks go straight to the app. Without it (turned off, or
//! still starting), live icons are found read-only from the registry, where
//! entries are only candidates: Shell_NotifyIconGetRect must confirm each icon
//! before it is used, and clicks go through Explorer's hidden tray.
use serde::Serialize;
use std::collections::HashMap;
use windows::core::{w, Interface, BOOL, GUID, PCWSTR};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    System::{Com::*, Registry::*},
    UI::{Accessibility::*, Shell::*, WindowsAndMessaging::*},
};

#[derive(Clone, Serialize)]
pub struct TrayApp {
    pub name: String,
    pub path: String,
    pub tray_ids: Vec<String>,
    pub window_handles: Vec<isize>,
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

unsafe fn open_key(parent: HKEY, name: &str) -> Option<Key> {
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut key = HKEY::default();
    RegOpenKeyExW(parent, PCWSTR(name.as_ptr()), Some(0), KEY_READ, &mut key)
        .ok()
        .ok()?;
    Some(Key(key))
}

unsafe fn value(key: HKEY, name: &str, expected: REG_VALUE_TYPE) -> Option<Vec<u8>> {
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut kind = REG_VALUE_TYPE::default();
    let mut size = 0;
    RegQueryValueExW(
        key,
        PCWSTR(name.as_ptr()),
        None,
        Some(&mut kind),
        None,
        Some(&mut size),
    )
    .ok()
    .ok()?;
    if kind != expected || size > 65536 {
        return None;
    }
    let mut bytes = vec![0; size as usize];
    RegQueryValueExW(
        key,
        PCWSTR(name.as_ptr()),
        None,
        Some(&mut kind),
        Some(bytes.as_mut_ptr()),
        Some(&mut size),
    )
    .ok()
    .ok()?;
    if kind != expected {
        return None;
    }
    bytes.truncate(size as usize);
    Some(bytes)
}

unsafe fn string(key: HKEY, name: &str) -> Option<String> {
    let bytes = value(key, name, REG_SZ)?;
    let chars: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|c| *c != 0)
        .collect();
    String::from_utf16(&chars).ok()
}

fn parse_guid(text: &str) -> Option<GUID> {
    GUID::try_from(text.trim_start_matches('{').trim_end_matches('}')).ok()
}

unsafe fn resolve_path(path: &str) -> Option<String> {
    if !path.starts_with('{') {
        return Some(path.to_owned());
    }
    let end = path.find('}')?;
    let id = parse_guid(&path[..=end])?;
    let folder = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None).ok()?;
    let base = folder.to_string().ok();
    CoTaskMemFree(Some(folder.0 as _));
    Some(format!("{}{}", base?, &path[end + 1..]))
}

struct Icon {
    key: String,
    path: String,
    identifier: NOTIFYICONIDENTIFIER,
    promoted: bool,
    /// The icon's first tooltip line, as Windows recorded it.
    tooltip: String,
}

// Explorer owns system notification icons such as Bluetooth. They share
// explorer.exe with File Explorer windows but are not File Explorer app actions.
fn is_explorer_hosted_icon(path: &str) -> bool {
    std::path::Path::new(path)
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("explorer.exe"))
}

unsafe extern "system" fn collect_window(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
    let windows = &mut *(param.0 as *mut Vec<(isize, String)>);
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if let Some(path) = crate::commands::process_image_path(pid) {
        windows.push((hwnd.0 as isize, path));
    }
    true.into()
}

unsafe fn process_windows() -> Vec<(isize, String)> {
    let mut windows = Vec::new();
    let param = LPARAM(&mut windows as *mut Vec<(isize, String)> as isize);
    let _ = EnumWindows(Some(collect_window), param);
    // Some apps own their notification icon through a message-only window.
    let mut previous = None;
    while let Ok(hwnd) = FindWindowExW(Some(HWND_MESSAGE), previous, PCWSTR::null(), PCWSTR::null())
    {
        let _ = collect_window(hwnd, param);
        previous = Some(hwnd);
    }
    windows
}

unsafe fn live_icons(windows: &[(isize, String)], include_own: bool) -> Vec<Icon> {
    let Some(root) = open_key(HKEY_CURRENT_USER, "Control Panel\\NotifyIconSettings") else {
        return vec![];
    };
    let own_path = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_lowercase());
    let mut result = Vec::new();
    for index in 0..4096 {
        let mut name = [0u16; 256];
        let mut length = name.len() as u32;
        if RegEnumKeyExW(
            root.0,
            index,
            Some(windows::core::PWSTR(name.as_mut_ptr())),
            &mut length,
            None,
            None,
            None,
            None,
        )
        .is_err()
        {
            break;
        }
        let key_name = String::from_utf16_lossy(&name[..length as usize]);
        let Some(key) = open_key(root.0, &key_name) else {
            continue;
        };
        let Some(path) = string(key.0, "ExecutablePath").and_then(|p| resolve_path(&p)) else {
            continue;
        };
        if !include_own && own_path.as_deref() == Some(&path.to_lowercase()) {
            continue;
        }
        let mut id = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            ..Default::default()
        };
        if let Some(guid) = string(key.0, "IconGuid").and_then(|s| parse_guid(&s)) {
            id.guidItem = guid;
            if !windows
                .iter()
                .any(|(_, exe)| exe.eq_ignore_ascii_case(&path))
                || Shell_NotifyIconGetRect(&id).is_err()
            {
                continue;
            }
        } else if let Some(uid) = value(key.0, "UID", REG_DWORD)
            .and_then(|v| <[u8; 4]>::try_from(v).ok())
            .map(u32::from_le_bytes)
        {
            id.uID = uid;
            let owner = windows.iter().find(|(hwnd, exe)| {
                if !exe.eq_ignore_ascii_case(&path) {
                    return false;
                }
                id.hWnd = HWND(*hwnd as _);
                Shell_NotifyIconGetRect(&id).is_ok()
            });
            if owner.is_none() {
                continue;
            }
        } else {
            continue;
        }
        let tooltip = string(key.0, "InitialTooltip")
            .map(|t| t.lines().next().unwrap_or_default().trim().to_string())
            .filter(|t| !t.starts_with('@'))
            .unwrap_or_default();
        result.push(Icon {
            key: key_name,
            path,
            identifier: id,
            promoted: value(key.0, "IsPromoted", REG_DWORD)
                .and_then(|v| <[u8; 4]>::try_from(v).ok())
                .map(u32::from_le_bytes)
                == Some(1),
            tooltip,
        });
    }
    result
}

pub fn enumerate() -> Vec<TrayApp> {
    if crate::tray_host::ready() {
        return host_enumerate();
    }
    unsafe {
        let windows = process_windows();
        let mut apps: HashMap<String, TrayApp> = HashMap::new();
        // Explorer-hosted system icons still occupy overflow positions, so
        // live_icons keeps them for interact(); they just aren't app trays.
        for icon in live_icons(&windows, false)
            .into_iter()
            .filter(|icon| !is_explorer_hosted_icon(&icon.path))
        {
            let app = apps
                .entry(icon.path.to_lowercase())
                .or_insert_with(|| TrayApp {
                    name: crate::commands::app_display_name(&icon.path, None),
                    window_handles: windows
                        .iter()
                        .filter(|(_, p)| p.eq_ignore_ascii_case(&icon.path))
                        .map(|(h, _)| *h)
                        .collect(),
                    path: icon.path,
                    tray_ids: vec![],
                });
            app.tray_ids.push(icon.key);
        }
        let mut apps: Vec<_> = apps.into_values().collect();
        apps.sort_by(|a, b| a.path.cmp(&b.path));
        apps
    }
}

// ---------------------------------------------------------------- the notch's tray

/// One live tray icon, for the tray in the notch.
#[derive(Clone, Serialize)]
pub struct TrayIconInfo {
    /// Windows' id for the icon (its NotifyIconSettings key), to click it.
    pub id: String,
    /// Bloom's lasting name for it, for where it is shown: the program's file
    /// name, plus Windows' id when one program has several icons.
    pub key: String,
    pub name: String,
    pub tooltip: String,
    pub path: String,
    /// On the taskbar itself rather than in Windows' hidden icons.
    pub promoted: bool,
    /// Hidden by Tray Icon Manager.
    pub manager_hidden: bool,
    /// The icon's live picture (PNG data URI) from Bloom's tray host. Without
    /// it the notch shows the program's own icon.
    pub image: Option<String>,
}

/// An icon Tray Icon Manager hides, whether or not it is running now.
#[derive(Clone, Serialize)]
pub struct ManagerEntry {
    pub name: String,
    /// Bloom's key for it, when it names its program.
    pub key: Option<String>,
    /// It matches a tray icon that is there right now.
    pub running: bool,
}

#[derive(Clone, Serialize)]
pub struct TrayState {
    pub icons: Vec<TrayIconInfo>,
    /// Tray Icon Manager's settings were found.
    pub manager_found: bool,
    /// The settings file that was read.
    pub manager_path: Option<String>,
    pub manager_entries: Vec<ManagerEntry>,
}

impl TrayState {
    pub fn empty() -> Self {
        Self {
            icons: Vec::new(),
            manager_found: false,
            manager_path: None,
            manager_entries: Vec::new(),
        }
    }
}

fn exe_name(path: &str) -> String {
    std::path::Path::new(path.trim().trim_matches('"'))
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Every live tray icon, and which ones Tray Icon Manager hides.
pub fn tray_state() -> TrayState {
    use crate::tray_manager::{hides, normalize_guid, IconFacts};
    if crate::tray_host::ready() {
        return host_state();
    }
    unsafe {
        let windows = process_windows();
        let icons: Vec<Icon> = live_icons(&windows, false)
            .into_iter()
            .filter(|icon| !is_explorer_hosted_icon(&icon.path))
            .collect();
        let manager = crate::tray_manager::read();
        let hidden = manager
            .as_ref()
            .map(|m| m.entries.as_slice())
            .unwrap_or_default();
        let mut per_exe: HashMap<String, usize> = HashMap::new();
        for icon in &icons {
            *per_exe.entry(exe_name(&icon.path)).or_default() += 1;
        }
        let mut matched = vec![false; hidden.len()];
        let mut list: Vec<TrayIconInfo> = icons
            .iter()
            .map(|icon| {
                let exe = exe_name(&icon.path);
                let name = crate::commands::app_display_name(&icon.path, None);
                let guid = normalize_guid(&format!("{:?}", icon.identifier.guidItem));
                let same_program = per_exe.get(&exe).copied().unwrap_or(1);
                let facts = IconFacts {
                    exe: &exe,
                    guid: &guid,
                    tooltip: &icon.tooltip,
                    name: &name,
                    same_program,
                };
                let mut manager_hidden = false;
                for (index, entry) in hidden.iter().enumerate() {
                    if hides(entry, &facts) {
                        manager_hidden = true;
                        matched[index] = true;
                    }
                }
                let key = if same_program > 1 {
                    format!("{}|{}", exe, icon.key)
                } else {
                    exe.clone()
                };
                TrayIconInfo {
                    id: icon.key.clone(),
                    key,
                    name,
                    tooltip: icon.tooltip.clone(),
                    path: icon.path.clone(),
                    promoted: icon.promoted,
                    manager_hidden,
                    image: None,
                }
            })
            .collect();
        list.sort_by_key(|a| a.name.to_lowercase());
        TrayState {
            icons: list,
            manager_found: manager.is_some(),
            manager_path: manager.as_ref().map(|m| m.path.clone()),
            manager_entries: hidden
                .iter()
                .zip(&matched)
                .map(|(entry, running)| ManagerEntry {
                    name: if entry.name.is_empty() {
                        entry.exe.clone()
                    } else {
                        entry.name.clone()
                    },
                    key: (!entry.exe.is_empty()).then(|| entry.exe.clone()),
                    running: *running,
                })
                .collect(),
        }
    }
}

/// What Windows' tray settings say about one icon (`NotifyIconSettings`).
struct RegistryEntry {
    key: String,
    path: String,
    guid: Option<String>,
    uid: Option<u32>,
    promoted: bool,
}

/// Every icon Windows' tray settings know, without checking which run.
fn registry_entries() -> Vec<RegistryEntry> {
    unsafe {
        let Some(root) = open_key(HKEY_CURRENT_USER, "Control Panel\\NotifyIconSettings") else {
            return vec![];
        };
        let mut entries = Vec::new();
        for index in 0..4096 {
            let mut name = [0u16; 256];
            let mut length = name.len() as u32;
            if RegEnumKeyExW(
                root.0,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut length,
                None,
                None,
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            let key_name = String::from_utf16_lossy(&name[..length as usize]);
            let Some(key) = open_key(root.0, &key_name) else {
                continue;
            };
            let Some(path) = string(key.0, "ExecutablePath").and_then(|p| resolve_path(&p)) else {
                continue;
            };
            let dword = |name: &str| {
                value(key.0, name, REG_DWORD)
                    .and_then(|v| <[u8; 4]>::try_from(v).ok())
                    .map(u32::from_le_bytes)
            };
            entries.push(RegistryEntry {
                key: key_name,
                path,
                guid: string(key.0, "IconGuid").map(|g| crate::tray_manager::normalize_guid(&g)),
                uid: dword("UID"),
                promoted: dword("IsPromoted") == Some(1),
            });
        }
        entries
    }
}

/// The notch's tray from Bloom's tray host: every visible icon with its live
/// picture, Windows' "shown on the taskbar" flag and Tray Icon Manager's rules.
fn host_state() -> TrayState {
    use crate::tray_manager::{hides, normalize_guid, IconFacts};
    let hosted = crate::tray_host::snapshot();
    let registry = registry_entries();
    let manager = crate::tray_manager::read();
    let hidden = manager
        .as_ref()
        .map(|m| m.entries.as_slice())
        .unwrap_or_default();
    let mut per_exe: HashMap<String, usize> = HashMap::new();
    for icon in &hosted {
        *per_exe.entry(exe_name(&icon.path)).or_default() += 1;
    }
    let mut matched = vec![false; hidden.len()];
    let mut list: Vec<TrayIconInfo> = hosted
        .iter()
        .filter(|icon| !is_explorer_hosted_icon(&icon.path) && !icon.path.is_empty())
        .map(|icon| {
            let exe = exe_name(&icon.path);
            let name = crate::commands::app_display_name(&icon.path, None);
            let guid = icon.guid.as_deref().map(normalize_guid).unwrap_or_default();
            // Windows' own entry for this icon: by GUID, else by program and id.
            let entry = registry.iter().find(|entry| {
                if !guid.is_empty() {
                    return entry.guid.as_deref() == Some(guid.as_str());
                }
                entry.guid.is_none()
                    && entry.uid == Some(icon.uid)
                    && entry.path.eq_ignore_ascii_case(&icon.path)
            });
            let same_program = per_exe.get(&exe).copied().unwrap_or(1);
            let facts = IconFacts {
                exe: &exe,
                guid: &guid,
                tooltip: &icon.tooltip,
                name: &name,
                same_program,
            };
            let mut manager_hidden = false;
            for (index, rule) in hidden.iter().enumerate() {
                if hides(rule, &facts) {
                    manager_hidden = true;
                    matched[index] = true;
                }
            }
            // The same lasting key as before, so choices made in Bloom stay.
            let key = if same_program > 1 {
                format!(
                    "{}|{}",
                    exe,
                    entry
                        .map(|e| e.key.clone())
                        .unwrap_or_else(|| icon.id.clone())
                )
            } else {
                exe.clone()
            };
            TrayIconInfo {
                id: icon.id.clone(),
                key,
                name,
                tooltip: icon.tooltip.clone(),
                path: icon.path.clone(),
                promoted: entry.is_some_and(|e| e.promoted),
                manager_hidden,
                image: icon.image.clone(),
            }
        })
        .collect();
    list.sort_by_key(|a| a.name.to_lowercase());
    TrayState {
        icons: list,
        manager_found: manager.is_some(),
        manager_path: manager.as_ref().map(|m| m.path.clone()),
        manager_entries: hidden
            .iter()
            .zip(&matched)
            .map(|(entry, running)| ManagerEntry {
                name: if entry.name.is_empty() {
                    entry.exe.clone()
                } else {
                    entry.name.clone()
                },
                key: (!entry.exe.is_empty()).then(|| entry.exe.clone()),
                running: *running,
            })
            .collect(),
    }
}

/// The dock's tray apps from Bloom's tray host, with each app's windows.
fn host_enumerate() -> Vec<TrayApp> {
    let hosted = crate::tray_host::snapshot();
    let pids: Vec<u32> = hosted.iter().map(|icon| icon.pid).collect();
    let mut windows: Vec<(isize, u32)> = Vec::new();
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let found = &mut *(param.0 as *mut Vec<(isize, u32)>);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        found.push((hwnd.0 as isize, pid));
        true.into()
    }
    unsafe {
        let param = LPARAM(&mut windows as *mut Vec<(isize, u32)> as isize);
        let _ = EnumWindows(Some(collect), param);
    }
    windows.retain(|(_, pid)| pids.contains(pid));
    let mut apps: HashMap<String, TrayApp> = HashMap::new();
    for icon in hosted
        .into_iter()
        .filter(|icon| !is_explorer_hosted_icon(&icon.path) && !icon.path.is_empty())
    {
        let app = apps
            .entry(icon.path.to_lowercase())
            .or_insert_with(|| TrayApp {
                name: crate::commands::app_display_name(&icon.path, None),
                window_handles: Vec::new(),
                path: icon.path.clone(),
                tray_ids: vec![],
            });
        for (hwnd, pid) in &windows {
            if *pid == icon.pid && !app.window_handles.contains(hwnd) {
                app.window_handles.push(*hwnd);
            }
        }
        app.tray_ids.push(icon.id);
    }
    let mut apps: Vec<_> = apps.into_values().collect();
    apps.sort_by(|a, b| a.path.cmp(&b.path));
    apps
}

struct ComApartment;
impl ComApartment {
    unsafe fn new() -> Result<Self, String> {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

/// Restore every temporary Explorer change, including on an early error. No
/// AppBar state or user icon-promotion preferences are changed for this operation.
struct TrayExposure {
    taskbar: HWND,
    rect: RECT,
    style: i32,
    was_hidden: bool,
    overflow: Option<HWND>,
}

impl TrayExposure {
    unsafe fn new() -> Result<Self, String> {
        use std::sync::atomic::Ordering;
        if crate::state::SHUTTING_DOWN.load(Ordering::Relaxed) {
            return Err("Bloom is shutting down.".into());
        }
        let hwnd = crate::tray_host::explorer_taskbar().ok_or("Windows taskbar is unavailable.")?;
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).map_err(|e| e.to_string())?;
        if crate::state::TRAY_INTERACTION_ACTIVE.swap(true, Ordering::SeqCst) {
            return Err("Another tray menu is already opening.".into());
        }
        let guard = Self {
            taskbar: hwnd,
            rect,
            style: GetWindowLongW(hwnd, GWL_EXSTYLE),
            was_hidden: crate::state::NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed),
            overflow: None,
        };
        if guard.was_hidden {
            SetWindowLongW(
                hwnd,
                GWL_EXSTYLE,
                guard.style | WS_EX_LAYERED.0 as i32 | WS_EX_TRANSPARENT.0 as i32,
            );
            // A fully transparent XAML island drops its UI Automation tree.
            // One alpha level is visually imperceptible but keeps it accessible.
            SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 1, LWA_ALPHA)
                .map_err(|e| e.to_string())?;
            if let Some(rect) = crate::utils::ORIGINAL_TRAY_RECT
                .lock()
                .ok()
                .and_then(|r| *r)
            {
                SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                )
                .map_err(|e| e.to_string())?;
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(guard)
    }

    unsafe fn expose_overflow(&mut self, automation: &IUIAutomation) -> Result<(), String> {
        let find_overflow = || {
            FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), PCWSTR::null())
                .or_else(|_| FindWindowW(w!("NotifyIconOverflowWindow"), PCWSTR::null()))
        };
        let has_icons = |hwnd| -> bool {
            let bridge = FindWindowExW(
                Some(hwnd),
                None,
                w!("Windows.UI.Composition.DesktopWindowContentBridge"),
                PCWSTR::null(),
            )
            .or_else(|_| FindWindowExW(Some(hwnd), None, w!("ToolbarWindow32"), PCWSTR::null()))
            .unwrap_or(hwnd);
            let Ok(root) = automation.ElementFromHandle(bridge) else {
                return false;
            };
            let Ok(condition) = automation.CreateTrueCondition() else {
                return false;
            };
            let Ok(elements) = root.FindAll(TreeScope_Descendants, &condition) else {
                return false;
            };
            (0..elements.Length().unwrap_or(0)).any(|index| {
                elements.GetElement(index).is_ok_and(|element| {
                    element
                        .CurrentAutomationId()
                        .is_ok_and(|id| id == "NotifyItemIcon")
                })
            })
        };
        if let Ok(hwnd) = find_overflow() {
            if !IsWindowVisible(hwnd).as_bool() {
                self.overflow = Some(hwnd);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            if has_icons(hwnd) {
                return Ok(());
            }
        }
        // Windows 11's tray controls live in a child XAML bridge, not in the
        // Shell_TrayWnd automation subtree itself. Invoking the chevron also
        // populates the overflow; ShowWindow on its host alone does not.
        let bridge = FindWindowExW(
            Some(self.taskbar),
            None,
            w!("Windows.UI.Composition.DesktopWindowContentBridge"),
            PCWSTR::null(),
        )
        .unwrap_or(self.taskbar);
        let root = automation
            .ElementFromHandle(bridge)
            .map_err(|e| e.to_string())?;
        let condition = automation
            .CreateTrueCondition()
            .map_err(|e| e.to_string())?;
        let elements = root
            .FindAll(TreeScope_Descendants, &condition)
            .map_err(|e| e.to_string())?;
        // Only invoke the known overflow chevron. Win+B/Space can accidentally
        // activate another app when all notification icons are promoted.
        for index in 0..elements.Length().unwrap_or(0) {
            let Ok(element) = elements.GetElement(index) else {
                continue;
            };
            if element
                .CurrentAutomationId()
                .is_ok_and(|s| s == "SystemTrayIcon")
                && element
                    .CurrentName()
                    .is_ok_and(|s| s.to_string().starts_with("Show Hidden Icons"))
            {
                let pattern = element
                    .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                    .map_err(|e| e.to_string())?;
                if element
                    .CurrentName()
                    .is_ok_and(|name| name.to_string().ends_with("Hide"))
                {
                    // Explorer remembers an open chevron even after Bloom hides
                    // its host; close that stale state before reopening it.
                    pattern.Invoke().map_err(|e| e.to_string())?;
                }
                pattern.Invoke().map_err(|e| e.to_string())?;
                for _ in 0..20 {
                    if let Ok(hwnd) = find_overflow() {
                        self.overflow = Some(hwnd);
                        if IsWindowVisible(hwnd).as_bool() && has_icons(hwnd) {
                            return Ok(());
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                break;
            }
        }
        Err(
            "Windows could not expose this app's tray icon. Open the system tray and try again."
                .into(),
        )
    }
}

impl Drop for TrayExposure {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        unsafe {
            if let Some(hwnd) = self.overflow {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            // Exit/settings cleanup owns the final visible state if it changed
            // while UIA was working; never undo a taskbar restoration.
            if self.was_hidden
                && !crate::state::SHUTTING_DOWN.load(Ordering::Relaxed)
                && crate::state::NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed)
            {
                let _ = ShowWindow(self.taskbar, SW_HIDE);
                SetWindowLongW(self.taskbar, GWL_EXSTYLE, self.style);
                let _ = SetWindowPos(
                    self.taskbar,
                    None,
                    self.rect.left,
                    self.rect.top,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            if self.was_hidden
                && (crate::state::SHUTTING_DOWN.load(Ordering::Relaxed)
                    || !crate::state::NATIVE_TASKBAR_HIDDEN.load(Ordering::Relaxed))
            {
                crate::utils::set_taskbar_visibility(true, true);
            }
            crate::state::TRAY_INTERACTION_ACTIVE.store(false, Ordering::SeqCst);
        }
    }
}

unsafe fn tray_order() -> Result<HashMap<String, usize>, String> {
    let root = open_key(HKEY_CURRENT_USER, "Control Panel\\NotifyIconSettings")
        .ok_or("Windows tray settings are unavailable.")?;
    let bytes =
        value(root.0, "UIOrderList", REG_BINARY).ok_or("Windows tray order is unavailable.")?;
    let (entries, remainder) = bytes.as_chunks::<8>();
    if !remainder.is_empty() {
        return Err("Windows tray order is invalid.".into());
    }
    Ok(entries
        .iter()
        .enumerate()
        .map(|(index, bytes)| (u64::from_le_bytes(*bytes).to_string(), index))
        .collect())
}

fn normalized_label(label: &str) -> String {
    label
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

unsafe fn matches_icon_label(label: &str, target: &Icon) -> bool {
    let name = normalized_label(label);
    let friendly = normalized_label(&crate::commands::friendly_process_name(&target.path));
    let stem = std::path::Path::new(&target.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(normalized_label)
        .unwrap_or_default();
    (!friendly.is_empty() && name.contains(&friendly)) || (!stem.is_empty() && name.contains(&stem))
}

/// Shell_NotifyIconGetRect points at the chevron for hidden icons. The live
/// registry order and the overflow's UI Automation button order coincide;
/// require equal counts and reject any contradictory name matches. An icon's
/// tooltip is app-defined and often differs from its executable description
/// (for example PhoneExperienceHost.exe displays "Phone Link - Disconnected"),
/// so a missing name match is not by itself an identity failure.
unsafe fn find_overflow_icon(
    automation: &IUIAutomation,
    icons: &[Icon],
    target: &Icon,
) -> Result<IUIAutomationElement3, String> {
    let order = tray_order()?;
    let mut hidden: Vec<_> = icons.iter().filter(|icon| !icon.promoted).collect();
    hidden.sort_by_key(|icon| order.get(&icon.key).copied().unwrap_or(usize::MAX));
    if hidden.iter().any(|icon| !order.contains_key(&icon.key)) {
        return Err("Windows tray order changed. Please try again.".into());
    }
    let position = hidden
        .iter()
        .position(|icon| icon.key == target.key)
        .ok_or("This app's tray icon is not in the overflow.")?;
    let hwnd = FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), PCWSTR::null())
        .or_else(|_| FindWindowW(w!("NotifyIconOverflowWindow"), PCWSTR::null()))
        .map_err(|_| "Windows tray overflow is unavailable.")?;
    let bridge = FindWindowExW(
        Some(hwnd),
        None,
        w!("Windows.UI.Composition.DesktopWindowContentBridge"),
        PCWSTR::null(),
    )
    .or_else(|_| FindWindowExW(Some(hwnd), None, w!("ToolbarWindow32"), PCWSTR::null()))
    .unwrap_or(hwnd);
    let root = automation
        .ElementFromHandle(bridge)
        .map_err(|e| e.to_string())?;
    let condition = automation
        .CreateTrueCondition()
        .map_err(|e| e.to_string())?;
    let elements = root
        .FindAll(TreeScope_Descendants, &condition)
        .map_err(|e| e.to_string())?;
    let mut buttons = Vec::new();
    for index in 0..elements.Length().unwrap_or(0) {
        let Ok(element) = elements.GetElement(index) else {
            continue;
        };
        if element
            .CurrentAutomationId()
            .is_ok_and(|id| id == "NotifyItemIcon")
            && element.CurrentControlType().ok() == Some(UIA_ButtonControlTypeId)
        {
            buttons.push(element);
        }
    }
    if buttons.len() != hidden.len() {
        // Some overflow icons can't be attributed to a process Bloom can read,
        // so positions don't line up. Fall back to the button whose label
        // names the target app, but only when exactly one does and it names
        // no other known icon; otherwise opening a menu could hit another app.
        let labelled: Vec<_> = buttons
            .iter()
            .filter(|button| {
                let label = button.CurrentName().unwrap_or_default().to_string();
                matches_icon_label(&label, target)
                    && !hidden
                        .iter()
                        .any(|icon| icon.key != target.key && matches_icon_label(&label, icon))
            })
            .collect();
        if let [button] = labelled[..] {
            return button.cast().map_err(|e| e.to_string());
        }
        return Err("Windows tray icons changed while opening the menu. Please try again.".into());
    }
    // Name matches provide cross-checks of Explorer's ordered UI tree, but
    // cannot be required for every icon because tooltip text is app-defined.
    // An icon without a name (an app with no tooltip, or one that just
    // re-registered after Explorer restarted) simply can't be cross-checked;
    // it must not block the icon the user clicked.
    for (button_index, button) in buttons.iter().enumerate() {
        let label = button.CurrentName().unwrap_or_default().to_string();
        if label.trim().is_empty() {
            continue;
        }
        let matches: Vec<_> = hidden
            .iter()
            .enumerate()
            .filter(|(_, icon)| matches_icon_label(&label, icon))
            .map(|(index, _)| index)
            .collect();
        if matches.len() == 1 && matches[0] != button_index {
            return Err("Windows tray icon order changed. Please try again.".into());
        }
    }
    buttons.remove(position).cast().map_err(|e| e.to_string())
}

unsafe fn find_promoted_icon(
    automation: &IUIAutomation,
    target: &Icon,
) -> Result<IUIAutomationElement3, String> {
    let icon = Shell_NotifyIconGetRect(&target.identifier)
        .map_err(|_| "This tray icon is no longer running.")?;
    let taskbar = crate::tray_host::explorer_taskbar().ok_or("Windows taskbar is unavailable.")?;
    let bridge = FindWindowExW(
        Some(taskbar),
        None,
        w!("Windows.UI.Composition.DesktopWindowContentBridge"),
        PCWSTR::null(),
    )
    .unwrap_or(taskbar);
    let root = automation
        .ElementFromHandle(bridge)
        .map_err(|e| e.to_string())?;
    let condition = automation
        .CreateTrueCondition()
        .map_err(|e| e.to_string())?;
    let elements = root
        .FindAll(TreeScope_Descendants, &condition)
        .map_err(|e| e.to_string())?;
    let dpi = GetDpiForWindow(taskbar).max(96) as f64;
    let center_x = (icon.left + icon.right) as f64 / 2.0;
    let center_y = (icon.top + icon.bottom) as f64 / 2.0;
    let mut matches = Vec::new();
    for index in 0..elements.Length().unwrap_or(0) {
        let Ok(element) = elements.GetElement(index) else {
            continue;
        };
        if !element
            .CurrentAutomationId()
            .is_ok_and(|id| id == "SystemTrayIcon")
        {
            continue;
        }
        let Ok(bounds) = element.CurrentBoundingRectangle() else {
            continue;
        };
        if bounds.right - bounds.left > 128 || bounds.bottom - bounds.top > 128 {
            continue;
        }
        let hit = |x: f64, y: f64| {
            x >= bounds.left as f64
                && x < bounds.right as f64
                && y >= bounds.top as f64
                && y < bounds.bottom as f64
        };
        if hit(center_x, center_y) || hit(center_x * 96.0 / dpi, center_y * 96.0 / dpi) {
            matches.push(element);
        }
    }
    // The icon's position already identifies exactly one button, so it's
    // used even when its app set no tooltip for it.
    if matches.len() != 1 {
        return Err("Windows could not identify this app's tray icon.".into());
    }
    matches.remove(0).cast().map_err(|e| e.to_string())
}

/// Visible top-level windows.
unsafe fn visible_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            windows.push(hwnd);
        }
        BOOL(1)
    }
    let mut windows = Vec::new();
    let _ = EnumWindows(Some(collect), LPARAM(&mut windows as *mut _ as isize));
    windows
}

/// Apps open their tray menu at their notification icon, which stays in the
/// hidden taskbar's corner while Bloom replaces the taskbar. Waits briefly for
/// the menu the app opens and moves it to `anchor` (the dock icon that was
/// clicked), above the pointer and kept on that monitor. The menu is found by
/// its process's executable: many icons are identified by GUID and carry no
/// window to take the process from.
unsafe fn move_new_menu_to(app_path: &str, before: &[HWND], anchor: POINT) {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(25));
        let Some(menu) = visible_windows().into_iter().find(|hwnd| {
            if before.contains(hwnd) {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(*hwnd, Some(&mut pid));
            crate::commands::process_image_path(pid)
                .is_some_and(|path| path.eq_ignore_ascii_case(app_path))
        }) else {
            continue;
        };
        let mut rect = RECT::default();
        if GetWindowRect(menu, &mut rect).is_err() {
            return;
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let area = info.rcMonitor;
        let x = anchor
            .x
            .clamp(area.left, (area.right - width).max(area.left));
        let y = (anchor.y - height).clamp(area.top, (area.bottom - height).max(area.top));
        let _ = SetWindowPos(
            menu,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        return;
    }
}

pub fn interact(key: &str, context_menu: bool) -> Result<(), String> {
    if crate::tray_host::has_icon(key) {
        let button = if context_menu {
            crate::tray_host::Button::Right
        } else {
            crate::tray_host::Button::Left
        };
        match crate::tray_host::click(key, button) {
            // Bloom has no click message for it: click it in Explorer's tray.
            Err(error) if error == crate::tray_host::NO_CLICK_MESSAGE => {}
            other => return other,
        }
    }
    interact_in_explorer(key, context_menu)
}

/// Clicks an icon in Explorer's own tray (through Windows' accessibility
/// interface), found by its key or, for an icon Bloom's tray knows, by its
/// window and id or its GUID.
fn interact_in_explorer(key: &str, context_menu: bool) -> Result<(), String> {
    let identity = crate::tray_host::identity(key);
    unsafe {
        let _com = ComApartment::new()?;
        let windows = process_windows();
        let icons = live_icons(&windows, true);
        let same_icon = |icon: &Icon| {
            icon.key == key
                || identity.is_some_and(|(hwnd, uid, guid)| {
                    let id = &icon.identifier;
                    guid.is_some_and(|guid| id.guidItem == guid)
                        || (id.hWnd.0 as isize == hwnd && id.uID == uid)
                })
        };
        let icon = icons
            .iter()
            .find(|icon| same_icon(icon))
            .ok_or(if identity.is_some() {
                crate::tray_host::NO_CLICK_MESSAGE
            } else {
                "This tray icon is no longer running."
            })?;
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        if let Ok(options) = automation.cast::<IUIAutomation2>() {
            let _ = options.SetConnectionTimeout(1000);
            let _ = options.SetTransactionTimeout(2000);
        }
        let mut exposure = TrayExposure::new()?;
        let element = if icon.promoted {
            find_promoted_icon(&automation, icon)?
        } else {
            exposure.expose_overflow(&automation)?;
            find_overflow_icon(&automation, &icons, icon)?
        };
        Shell_NotifyIconGetRect(&icon.identifier)
            .map_err(|_| "This tray icon is no longer running.")?;
        // The notch stays open while the menu this opens is open.
        let mut clicked_at = POINT::default();
        let _ = GetCursorPos(&mut clicked_at);
        let excluded = crate::tray_host::bloom_processes();
        let menus_before = crate::tray_host::open_menus(clicked_at, &excluded);
        let watch = || {
            crate::tray_host::watch_interaction(
                key.to_string(),
                clicked_at,
                excluded.clone(),
                menus_before.clone(),
                context_menu,
            )
        };
        if context_menu {
            // The pointer is on the dock icon that was right-clicked.
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let before = visible_windows();
            element
                .ShowContextMenu()
                .map_err(|_| "Windows could not open this app's native tray menu.".to_string())?;
            move_new_menu_to(&icon.path, &before, cursor);
            watch();
            Ok(())
        } else {
            let element: IUIAutomationElement = element.cast().map_err(|e| e.to_string())?;
            let pattern = element
                .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                .map_err(|e| e.to_string())?;
            pattern
                .Invoke()
                .map_err(|_| "Windows could not activate this tray icon.".to_string())?;
            watch();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "read-only check against the current Windows desktop"]
    fn live_tray_discovery() {
        let apps = enumerate();
        for app in &apps {
            assert!(!app.tray_ids.is_empty());
            assert!(!app.path.is_empty());
            println!("{}: {} live tray icon(s)", app.name, app.tray_ids.len());
        }
        println!("{} live tray apps", apps.len());
    }
    #[test]
    fn accepts_braced_guid_and_rejects_bad_identity() {
        assert!(parse_guid("{699E423D-73DA-42BA-B94E-881FF8672467}").is_some());
        assert!(parse_guid("not-an-icon").is_none());
    }

    #[test]
    fn explorer_hosted_system_icons_are_not_app_trays() {
        assert!(is_explorer_hosted_icon("C:\\Windows\\explorer.exe"));
        assert!(is_explorer_hosted_icon("C:\\WINDOWS\\EXPLORER.EXE"));
        assert!(!is_explorer_hosted_icon(
            "C:\\Windows\\System32\\SecurityHealthSystray.exe"
        ));
        assert!(!is_explorer_hosted_icon("C:\\Apps\\Discord.exe"));
    }
}
