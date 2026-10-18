//! The dock's Recycle Bin, modelled on Tray Bin (a MiniBin replacement): an
//! icon that fills up bar by bar with the bin's total size, a choice of what a
//! double click does, and the shell's confirm / sound / progress options when
//! emptying.
//!
//! Bloom draws its own bin by default: five bars, one more for each size step
//! (under 100 MB, 100-500 MB, 500 MB-1 GB, 1-5 GB, 5 GB and up), sharp at any
//! dock size. Tray Bin's icons (MiniBin's artwork, four steps) can be used
//! instead: they are not shipped with Bloom but copied once out of an
//! installed Tray Bin (`TrayBin.exe` keeps them as RCDATA resources 10-14 and
//! 20-24) into Bloom's config folder, so they stay after Tray Bin is
//! uninstalled. The Windows bin icons are the third choice.
//!
//! The icon updates the moment the bin changes: Bloom watches each drive's
//! `$Recycle.Bin` folder for this user with Windows' change notifications, so
//! nothing polls.

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

const MB: i64 = 1024 * 1024;
/// Bin size at which 2, 3, 4 and 5 bars take over; any item at all shows 1.
const LEVEL_STEPS: [i64; 4] = [100 * MB, 500 * MB, 1024 * MB, 5 * 1024 * MB];
/// The fullest level: 5 bars.
pub const FULL: usize = 5;
const LEVEL_NAMES: [&str; 5] = ["0", "25", "50", "75", "100"];
/// Which of Tray Bin's five icons (empty, 25, 50, 75, 100 %) shows each of
/// Bloom's six levels: 5 GB and up is its full icon.
const TRAYBIN_ICON_FOR_LEVEL: [usize; 6] = [0, 1, 2, 3, 3, 4];
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

/// Which icons the bin uses: Bloom's own ("bloom", the default), Tray Bin's
/// ("traybin") or Windows' ("windows").
pub fn icon_style(app: &AppHandle) -> String {
    match get_setting_str(app, "bloom-dock-bin-icons").as_deref() {
        Some("traybin") => "traybin".into(),
        Some("windows") => "windows".into(),
        _ => "bloom".into(),
    }
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

/// Tray Bin's program: where its installer registered it (the folder can be
/// changed while installing), else its default folder.
fn traybin_exe() -> Option<PathBuf> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\TrayBin");
    let value = wide("DisplayIcon");
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let registered = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
        .is_ok()
    }
    .then(|| {
        let length = (size as usize / 2).min(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..length]);
        // "C:\...\TrayBin.exe" or "C:\...\TrayBin.exe,0".
        let path = text
            .trim_end_matches('\0')
            .split(',')
            .next()
            .unwrap_or_default();
        PathBuf::from(path.trim().trim_matches('"'))
    })
    .filter(|p| p.is_file());
    registered.or_else(|| {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let exe = PathBuf::from(local)
            .join("Programs")
            .join("TrayBin")
            .join("TrayBin.exe");
        exe.is_file().then_some(exe)
    })
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

/// Bloom's own bin at a fill level (0-5) as an SVG data URI, in the dock's
/// text colour for a dark or a light dock: a bin with its lid and handle and
/// five bar slots, filled from the bottom up.
pub fn bloom_icon(level: usize, dark_background: bool) -> String {
    let ink = if dark_background {
        "#ffffff"
    } else {
        "#1d1d1f"
    };
    let mut bars = String::new();
    if level > 0 {
        // Five bars from the bottom of the body up; the body narrows towards
        // its foot, and so do the bars.
        for index in 0..FULL {
            let bottom = 52.0 - index as f64 * 6.0;
            let top = bottom - 4.0;
            let middle = (top + bottom) / 2.0;
            let edge = 17.0 + 2.6 * ((middle - 20.0) / 34.4) + 4.6;
            let filled = index < level;
            bars.push_str(&format!(
                r#"<rect x="{x:.2}" y="{top:.2}" width="{w:.2}" height="4" rx="1.6" fill="{ink}" opacity="{o}"/>"#,
                x = edge,
                w = 64.0 - 2.0 * edge,
                o = if filled { "1" } else { "0.22" },
            ));
        }
    }
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g fill="none" stroke="{ink}" stroke-width="3.2" stroke-linejoin="round" stroke-linecap="round"><path d="M26 12V9.6a2.2 2.2 0 0 1 2.2-2.2h7.6A2.2 2.2 0 0 1 38 9.6V12"/><path d="M17 20h30l-2.6 34.4a4 4 0 0 1-4 3.6H23.6a4 4 0 0 1-4-3.6Z"/></g><rect x="12" y="12.4" width="40" height="4.4" rx="2.2" fill="{ink}"/>{bars}</svg>"#
    );
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    )
}

/// (for a dark background, for a light background) at one of Bloom's levels
/// (0-5), or None when Tray Bin's icons are neither copied yet nor installed.
pub fn traybin_icons(app: &AppHandle, level: usize) -> Option<(String, String)> {
    let level = TRAYBIN_ICON_FOR_LEVEL[level.min(FULL)];
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

/// This user's SID ("S-1-5-21-..."), the name of their folder in each
/// drive's `$Recycle.Bin`.
fn user_sid() -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
        let mut buffer = vec![0u8; size.max(64) as usize];
        let read = GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr() as *mut _),
            buffer.len() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        read.ok()?;
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut text = windows::core::PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).ok()?;
        let sid = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut _)));
        sid
    }
}

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
        let sid = user_sid();
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
    fn tray_bin_icons_cover_every_level() {
        assert_eq!(TRAYBIN_ICON_FOR_LEVEL[0], 0);
        assert_eq!(TRAYBIN_ICON_FOR_LEVEL[FULL], 4);
        assert!(TRAYBIN_ICON_FOR_LEVEL.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn bloom_icon_fills_one_bar_per_level() {
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
        assert_eq!(decode(bloom_icon(0, true)).matches("<rect").count(), 1);
        for level in 1..=FULL {
            let svg = decode(bloom_icon(level, true));
            assert_eq!(svg.matches(r#"opacity="1""#).count(), level);
            assert_eq!(svg.matches(r#"opacity="0.22""#).count(), FULL - level);
        }
        assert!(decode(bloom_icon(3, false)).contains("#1d1d1f"));
    }
}
