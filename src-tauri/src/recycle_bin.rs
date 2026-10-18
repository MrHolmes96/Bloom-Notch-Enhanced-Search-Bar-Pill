//! The dock's Recycle Bin, modelled on Tray Bin (a MiniBin replacement): an
//! icon that fills up bar by bar with the bin's total size, a choice of what a
//! double click does, and the shell's confirm / sound / progress options when
//! emptying.
//!
//! The icon is Tray Bin's bin, drawn by Bloom on Tray Bin's own 16 px grid so
//! it stays sharp at the dock's size. Where Tray Bin has four bars, this one
//! has five, one more for each size step: under 100 MB 1, 100-500 MB 2,
//! 500 MB-1 GB 3, 1-5 GB 4, 5 GB and up 5 (full). The Windows bin icons are
//! the other choice.
//!
//! The icon updates the moment the bin changes: Bloom watches each drive's
//! `$Recycle.Bin` folder for this user with Windows' change notifications, so
//! nothing polls.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use base64::Engine;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{PCSTR, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, StrFormatByteSizeW, SHERB_NOCONFIRMATION,
    SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHQUERYRBINFO,
};

use crate::utils::get_setting_str;

/// Event sent when the bin's contents change, so the dock can redraw it.
pub const CHANGED_EVENT: &str = "recycle-bin-changed";

const MB: i64 = 1024 * 1024;
/// Bin size at which 2, 3, 4 and 5 bars take over; any item at all shows 1.
const LEVEL_STEPS: [i64; 4] = [100 * MB, 500 * MB, 1024 * MB, 5 * 1024 * MB];
/// The fullest level: 5 bars.
pub const FULL: usize = 5;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct BinState {
    pub items: i64,
    pub bytes: i64,
}

struct Cached {
    state: BinState,
    at_ms: i64,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

fn query_now() -> BinState {
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    if unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.is_ok() {
        BinState {
            items: info.i64NumItems,
            bytes: info.i64Size,
        }
    } else {
        BinState::default()
    }
}

/// The bin's contents, re-queried at most every 1.5 s: the query walks every
/// volume's bin, and the dock and the watcher both ask.
pub fn query() -> BinState {
    let now = crate::utils::get_now_ms();
    if let Ok(guard) = CACHE.lock() {
        if let Some(cached) = guard.as_ref() {
            if now - cached.at_ms < 1500 {
                return cached.state;
            }
        }
    }
    let state = query_now();
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(Cached { state, at_ms: now });
    }
    state
}

fn invalidate() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

/// 0 = empty, 1-5 bars from the total size: under 100 MB 1, 100-500 MB 2,
/// 500 MB-1 GB 3, 1-5 GB 4, 5 GB and up 5 (full).
pub fn level(state: BinState, only_empty_full: bool) -> usize {
    if state.items <= 0 {
        return 0;
    }
    if only_empty_full {
        return FULL;
    }
    1 + LEVEL_STEPS
        .iter()
        .filter(|&&step| state.bytes >= step)
        .count()
}

/// "4,88 MB" with the shell's own rounding and decimal separator.
fn format_size(bytes: i64) -> String {
    let mut buffer = [0u16; 64];
    if !unsafe { StrFormatByteSizeW(bytes, &mut buffer) }.is_null() {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        return String::from_utf16_lossy(&buffer[..end]);
    }
    format!("{} bytes", bytes)
}

/// Tooltip line under the name: "Empty" or "4,88 MB · 12 items".
pub fn detail(state: BinState) -> String {
    if state.items <= 0 {
        return "Empty".into();
    }
    let noun = if state.items == 1 { "item" } else { "items" };
    format!("{} · {} {}", format_size(state.bytes), state.items, noun)
}

fn setting_on(app: &AppHandle, key: &str, default: bool) -> bool {
    get_setting_str(app, key).map_or(default, |v| v != "false")
}

/// Which icons the bin uses: Tray Bin's ("traybin", the default) or Windows'
/// ("windows").
pub fn icon_style(app: &AppHandle) -> String {
    match get_setting_str(app, "bloom-dock-bin-icons").as_deref() {
        Some("windows") => "windows".into(),
        _ => "traybin".into(),
    }
}

pub fn only_empty_full(app: &AppHandle) -> bool {
    setting_on(app, "bloom-dock-bin-only-empty-full", false)
}

// ---------------------------------------------------------------- icons

/// Tray Bin's bin at a fill level (0-5) as an SVG data URI: white for a dark
/// dock, black for a light one, as Tray Bin's two sets are. Its 16 px grid:
/// the handle, the lid, the walls and the foot, and the bars stacked up from
/// the bottom with a line between each (Tray Bin's four, and a fifth above).
pub fn traybin_icon(level: usize, dark_background: bool) -> String {
    let ink = if dark_background {
        "#ffffff"
    } else {
        "#000000"
    };
    let mut shapes = String::from(
        r#"<rect x="5" y="1" width="6" height="1"/><rect x="2" y="2" width="12" height="1"/><rect x="3" y="3" width="1" height="11"/><rect x="12" y="3" width="1" height="11"/><rect x="4" y="14" width="8" height="1"/>"#,
    );
    for bar in 0..level.min(FULL) {
        shapes.push_str(&format!(
            r#"<rect x="5" y="{}" width="6" height="1"/>"#,
            12 - 2 * bar
        ));
    }
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" shape-rendering="crispEdges"><g fill="{ink}">{shapes}</g></svg>"#
    );
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    )
}

// ---------------------------------------------------------------- emptying

type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
type FlushMenuThemes = unsafe extern "system" fn();

/// Puts this process in dark or light mode (or back to default) so the
/// shell's confirmation and progress dialogs match the Windows theme, the way
/// Explorer's do. Undocumented uxtheme exports by ordinal (135, 136), the same
/// calls Tray Bin makes; Windows 10 1903 and later.
unsafe fn set_app_mode(mode: i32) {
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    if windows_build() < 18362 {
        return;
    }
    let name: Vec<u16> = "uxtheme.dll"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let Ok(module) = LoadLibraryExW(PCWSTR(name.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
    else {
        return;
    };
    if let Some(set) = GetProcAddress(module, PCSTR(135usize as *const u8)) {
        let set: SetPreferredAppMode = std::mem::transmute(set);
        set(mode);
    }
    if let Some(flush) = GetProcAddress(module, PCSTR(136usize as *const u8)) {
        let flush: FlushMenuThemes = std::mem::transmute(flush);
        flush();
    }
}

fn windows_build() -> u32 {
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    read_registry_string(
        HKEY_LOCAL_MACHINE,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
        "CurrentBuildNumber",
    )
    .and_then(|s| s.trim().parse().ok())
    .unwrap_or(0)
}

fn read_registry_string(
    root: windows::Win32::System::Registry::HKEY,
    subkey: &str,
    value: &str,
) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let subkey: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let value: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buffer = [0u16; 128];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..end]))
}

fn apps_use_dark_theme() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value: Vec<u16> = "AppsUseLightTheme"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    status.is_ok() && data == 0
}

static EMPTYING: AtomicBool = AtomicBool::new(false);

/// Tells the dock to redraw the bin now and again over the next few seconds:
/// the shell finishes emptying asynchronously.
fn announce_change_burst(app: AppHandle) {
    std::thread::spawn(move || {
        for delay in [0u64, 400, 1000, 2500, 5000] {
            std::thread::sleep(std::time::Duration::from_millis(delay));
            invalidate();
            let _ = app.emit(CHANGED_EVENT, ());
        }
    });
}

/// Empties the bin with the confirm / sound / progress options from settings.
#[tauri::command]
pub fn empty_recycle_bin(app: AppHandle) {
    if EMPTYING.swap(true, Ordering::SeqCst) {
        return;
    }
    let confirm = setting_on(&app, "bloom-dock-bin-confirm", true);
    let sound = setting_on(&app, "bloom-dock-bin-sound", true);
    let progress = setting_on(&app, "bloom-dock-bin-progress", true);
    let owner = app
        .get_webview_window("dock")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize)
        .unwrap_or(0);

    std::thread::spawn(move || {
        let mut flags = 0u32;
        if !confirm {
            flags |= SHERB_NOCONFIRMATION;
        }
        if !progress {
            flags |= SHERB_NOPROGRESSUI;
        }
        if !sound {
            flags |= SHERB_NOSOUND;
        }
        unsafe {
            // 2 = force dark, 3 = force light, 0 = back to default.
            set_app_mode(if apps_use_dark_theme() { 2 } else { 3 });
            // Cancelling the confirmation returns an error; nothing to report.
            let _ = SHEmptyRecycleBinW(Some(HWND(owner as *mut _)), PCWSTR::null(), flags);
            set_app_mode(0);
        }
        EMPTYING.store(false, Ordering::SeqCst);
        announce_change_burst(app);
    });
}

// ---------------------------------------------------------------- watcher

static WATCHING: AtomicBool = AtomicBool::new(false);

/// The folders whose changes mean the bin changed: this user's folder in
/// `$Recycle.Bin` on every fixed or removable drive (or `$Recycle.Bin` itself
/// until that folder exists, so its creation is noticed).
fn bin_folders(sid: Option<&str>) -> Vec<(PathBuf, bool)> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    let mut folders = Vec::new();
    let drives = unsafe { GetLogicalDrives() };
    for index in 0..26u32 {
        if drives & (1 << index) == 0 {
            continue;
        }
        let root = format!("{}:\\", (b'A' + index as u8) as char);
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        // 2 = removable, 3 = fixed.
        let kind = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
        if kind != 2 && kind != 3 {
            continue;
        }
        let bin = PathBuf::from(&root).join("$Recycle.Bin");
        match sid.map(|sid| bin.join(sid)) {
            Some(own) if own.is_dir() => folders.push((own, true)),
            _ if bin.is_dir() => folders.push((bin, false)),
            _ => {}
        }
    }
    folders
}

/// Re-reads the bin and tells the dock when anything changed.
fn recheck(app: &AppHandle, last: &mut BinState) {
    let now = query_now();
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(Cached {
            state: now,
            at_ms: crate::utils::get_now_ms(),
        });
    }
    if now != *last {
        *last = now;
        let _ = app.emit(CHANGED_EVENT, ());
    }
}

/// Watches the bin while the dock shows it and tells the dock the moment its
/// contents change, wherever files are deleted from. Windows' change
/// notifications wake it; it only re-reads the drive list now and then, in
/// case a drive was connected.
pub fn ensure_watcher(app: &AppHandle) {
    use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::Storage::FileSystem::{
        FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
        FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_SIZE,
    };
    use windows::Win32::System::Threading::WaitForMultipleObjects;
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let sid = crate::utils::current_user_sid();
        let mut last = query_now();
        loop {
            let shown =
                get_setting_str(&app, "bloom-dock-extras-recycle-bin").as_deref() != Some("false");
            if !shown {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }
            let folders = bin_folders(sid.as_deref());
            let handles: Vec<HANDLE> = folders
                .iter()
                .filter_map(|(folder, subtree)| {
                    let wide: Vec<u16> = folder
                        .to_string_lossy()
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    unsafe {
                        FindFirstChangeNotificationW(
                            PCWSTR(wide.as_ptr()),
                            *subtree,
                            FILE_NOTIFY_CHANGE_FILE_NAME
                                | FILE_NOTIFY_CHANGE_DIR_NAME
                                | FILE_NOTIFY_CHANGE_SIZE,
                        )
                    }
                    .ok()
                })
                .take(64)
                .collect();
            recheck(&app, &mut last);
            // Until the drive list is read again (a drive plugged in, this
            // user's bin folder created on a drive).
            let rescan_at = std::time::Instant::now() + std::time::Duration::from_secs(60);
            while std::time::Instant::now() < rescan_at {
                if handles.is_empty() {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    recheck(&app, &mut last);
                    continue;
                }
                let woke = unsafe { WaitForMultipleObjects(&handles, false, 15_000) };
                if woke == WAIT_TIMEOUT {
                    continue;
                }
                let index = woke.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
                if index >= handles.len() {
                    break;
                }
                // A delete or an empty is many changes in a row: let them
                // settle for a moment, then read the bin once.
                std::thread::sleep(std::time::Duration::from_millis(150));
                for handle in &handles {
                    unsafe {
                        let _ = FindNextChangeNotification(*handle);
                    }
                }
                recheck(&app, &mut last);
                if get_setting_str(&app, "bloom-dock-extras-recycle-bin").as_deref()
                    == Some("false")
                {
                    break;
                }
            }
            for handle in handles {
                unsafe {
                    let _ = FindCloseChangeNotification(handle);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_size_steps() {
        let state = |items, bytes| BinState { items, bytes };
        assert_eq!(level(state(0, 0), false), 0);
        assert_eq!(level(state(1, 10), false), 1);
        assert_eq!(level(state(3, 99 * MB), false), 1);
        assert_eq!(level(state(3, 100 * MB), false), 2);
        assert_eq!(level(state(3, 499 * MB), false), 2);
        assert_eq!(level(state(3, 500 * MB), false), 3);
        assert_eq!(level(state(3, 1023 * MB), false), 3);
        assert_eq!(level(state(3, 1024 * MB), false), 4);
        assert_eq!(level(state(3, 5 * 1024 * MB - 1), false), 4);
        assert_eq!(level(state(3, 5 * 1024 * MB), false), 5);
        assert_eq!(level(state(3, 40 * 1024 * MB), false), 5);
        assert_eq!(level(state(1, 10), true), 5);
    }

    #[test]
    fn tray_bin_icon_stacks_one_bar_per_level() {
        use base64::Engine;
        let decode = |uri: String| {
            let data = uri.split_once(',').unwrap().1.to_string();
            String::from_utf8(
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .unwrap(),
            )
            .unwrap()
        };
        // Handle, lid, two walls and the foot.
        assert_eq!(decode(traybin_icon(0, true)).matches("<rect").count(), 5);
        for level in 1..=FULL {
            let svg = decode(traybin_icon(level, true));
            assert_eq!(svg.matches("<rect").count(), 5 + level);
            assert!(svg.contains(&format!(r#"y="{}""#, 12 - 2 * (level - 1))));
        }
        // Never more than five bars, the top one just under the lid.
        assert_eq!(decode(traybin_icon(9, true)).matches("<rect").count(), 10);
        assert!(decode(traybin_icon(5, true)).contains(r#"y="4" width="6""#));
        assert!(decode(traybin_icon(3, false)).contains("#000000"));
        assert!(decode(traybin_icon(3, true)).contains("#ffffff"));
    }
}
