//! The dock's Recycle Bin, modelled on Tray Bin (a MiniBin replacement): five
//! fill-level icons chosen by the bin's total size, a choice of what a double
//! click does, and the shell's confirm / sound / progress options when emptying.
//!
//! The fill-level icons are MiniBin's artwork, so they are not shipped with
//! Bloom. They are copied once out of an installed Tray Bin (`TrayBin.exe`
//! keeps them as RCDATA resources 10-14 and 20-24) into Bloom's config folder,
//! so they stay after Tray Bin is uninstalled. Without them the dock falls back
//! to the standard Windows bin icons.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

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

/// Bin size (MB) at which the 2, 3 and 4 bar icons take over. Same defaults as
/// Tray Bin; any item at all shows 1 bar.
const THRESHOLDS_MB: [i64; 3] = [100, 500, 2048];
const LEVEL_NAMES: [&str; 5] = ["0", "25", "50", "75", "100"];
/// Tray Bin's resource ids: white set (dark backgrounds) and black set.
const WHITE_BASE: u16 = 10;
const BLACK_BASE: u16 = 20;

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

/// 0 = empty, 1-4 = bars, from the total size like Tray Bin.
pub fn level(state: BinState, only_empty_full: bool) -> usize {
    if state.items <= 0 {
        return 0;
    }
    if only_empty_full {
        return 4;
    }
    let mb = state.bytes / (1024 * 1024);
    if mb >= THRESHOLDS_MB[2] {
        4
    } else if mb >= THRESHOLDS_MB[1] {
        3
    } else if mb >= THRESHOLDS_MB[0] {
        2
    } else {
        1
    }
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

pub fn uses_traybin_icons(app: &AppHandle) -> bool {
    get_setting_str(app, "bloom-dock-bin-icons").as_deref() != Some("windows")
}

pub fn only_empty_full(app: &AppHandle) -> bool {
    setting_on(app, "bloom-dock-bin-only-empty-full", false)
}

// ---------------------------------------------------------------- icons

fn icon_dir(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("recycle-bin-icons"))
}

fn icon_file(dir: &Path, dark_background: bool, level: usize) -> PathBuf {
    let set = if dark_background { "white" } else { "black" };
    dir.join(format!("{}_{}.ico", set, LEVEL_NAMES[level]))
}

fn traybin_exe() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let exe = PathBuf::from(local)
        .join("Programs")
        .join("TrayBin")
        .join("TrayBin.exe");
    exe.is_file().then_some(exe)
}

fn looks_like_ico(bytes: &[u8]) -> bool {
    bytes.len() > 22 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 1 && bytes[3] == 0
}

/// Reads one RCDATA resource (a whole .ico file) from a module loaded as data.
unsafe fn read_rcdata(module: windows::Win32::Foundation::HMODULE, id: u16) -> Option<Vec<u8>> {
    use windows::Win32::System::LibraryLoader::{
        FindResourceW, LoadResource, LockResource, SizeofResource,
    };
    // MAKEINTRESOURCE(id) and RT_RCDATA (10).
    let resource = FindResourceW(
        Some(module),
        PCWSTR(id as usize as *const u16),
        PCWSTR(10usize as *const u16),
    );
    if resource.is_invalid() {
        return None;
    }
    let size = SizeofResource(Some(module), resource) as usize;
    let handle = LoadResource(Some(module), resource).ok()?;
    let data = LockResource(handle) as *const u8;
    if data.is_null() || size == 0 {
        return None;
    }
    Some(std::slice::from_raw_parts(data, size).to_vec())
}

/// Copies Tray Bin's ten icons into `dir`. All or nothing.
fn import_traybin_icons(dir: &Path) -> bool {
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{
        LoadLibraryExW, LOAD_LIBRARY_AS_DATAFILE, LOAD_LIBRARY_AS_IMAGE_RESOURCE,
    };
    let Some(exe) = traybin_exe() else {
        return false;
    };
    let wide: Vec<u16> = exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    unsafe {
        let Ok(module) = LoadLibraryExW(
            PCWSTR(wide.as_ptr()),
            None,
            LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        ) else {
            return false;
        };
        for (base, dark) in [(WHITE_BASE, true), (BLACK_BASE, false)] {
            for level in 0..5 {
                match read_rcdata(module, base + level as u16) {
                    Some(bytes) if looks_like_ico(&bytes) => {
                        files.push((icon_file(dir, dark, level), bytes))
                    }
                    _ => {
                        let _ = FreeLibrary(module);
                        return false;
                    }
                }
            }
        }
        let _ = FreeLibrary(module);
    }
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    files
        .into_iter()
        .all(|(path, bytes)| std::fs::write(path, bytes).is_ok())
}

/// The icon as a PNG data URI at its own size (16 px); the dock scales it up
/// with nearest-neighbour so the pixel art stays crisp.
fn ico_to_data_uri(bytes: &[u8]) -> Option<String> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico).ok()?;
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

static ICONS: OnceLock<Mutex<HashMap<(bool, usize), String>>> = OnceLock::new();
static IMPORT_TRIED: AtomicBool = AtomicBool::new(false);

fn have_all_icons(dir: &Path) -> bool {
    (0..5).all(|level| {
        icon_file(dir, true, level).is_file() && icon_file(dir, false, level).is_file()
    })
}

/// (for a dark background, for a light background), or None when Tray Bin's
/// icons are neither copied yet nor installed.
pub fn traybin_icons(app: &AppHandle, level: usize) -> Option<(String, String)> {
    let cache = ICONS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let (Some(dark), Some(light)) = (guard.get(&(true, level)), guard.get(&(false, level))) {
            return Some((dark.clone(), light.clone()));
        }
    }
    let dir = icon_dir(app)?;
    if !have_all_icons(&dir) {
        // Looked for once per run; installing Tray Bin later is picked up by
        // `refresh_traybin_icons` from the settings page.
        if IMPORT_TRIED.swap(true, Ordering::Relaxed) || !import_traybin_icons(&dir) {
            return None;
        }
    }
    let dark = ico_to_data_uri(&std::fs::read(icon_file(&dir, true, level)).ok()?)?;
    let light = ico_to_data_uri(&std::fs::read(icon_file(&dir, false, level)).ok()?)?;
    if let Ok(mut guard) = cache.lock() {
        guard.insert((true, level), dark.clone());
        guard.insert((false, level), light.clone());
    }
    Some((dark, light))
}

/// Whether Tray Bin's icons can be used, copying them in again if Tray Bin has
/// been installed since. Used by the settings page.
#[tauri::command]
pub async fn recycle_bin_icons_available(app: AppHandle) -> bool {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(dir) = icon_dir(&app) else {
            return false;
        };
        if have_all_icons(&dir) {
            return true;
        }
        IMPORT_TRIED.store(true, Ordering::Relaxed);
        import_traybin_icons(&dir)
    })
    .await
    .unwrap_or(false)
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

/// Polls the bin every second while the dock shows it and tells the dock when
/// the contents change, so files deleted anywhere update the icon right away.
pub fn ensure_watcher(app: &AppHandle) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last = query_now();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let shown = get_setting_str(&app, "bloom-dock-extras-recycle-bin").as_deref()
                != Some("false")
                && matches!(
                    get_setting_str(&app, "bloom-dock-extras-position").as_deref(),
                    Some("left") | Some("right")
                );
            if !shown {
                continue;
            }
            let now = query_now();
            if now != last {
                last = now;
                if let Ok(mut guard) = CACHE.lock() {
                    *guard = Some(Cached {
                        state: now,
                        at_ms: crate::utils::get_now_ms(),
                    });
                }
                let _ = app.emit(CHANGED_EVENT, ());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_traybin_thresholds() {
        let mb = 1024 * 1024;
        let state = |items, bytes| BinState { items, bytes };
        assert_eq!(level(state(0, 0), false), 0);
        assert_eq!(level(state(1, 10), false), 1);
        assert_eq!(level(state(3, 100 * mb), false), 2);
        assert_eq!(level(state(3, 500 * mb), false), 3);
        assert_eq!(level(state(3, 2048 * mb), false), 4);
        assert_eq!(level(state(1, 10), true), 4);
    }
}
