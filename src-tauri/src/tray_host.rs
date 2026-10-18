//! Bloom's own copy of the notification area, so the tray in the notch is
//! Windows' tray drawn in the notch: every icon as it is right now, updated
//! the moment an app changes it, and clicks that reach the app at once.
//!
//! Apps add and change their tray icons with `Shell_NotifyIcon`, which hands
//! the icon to the first window of class `Shell_TrayWnd` (`WM_COPYDATA`).
//! Bloom keeps a hidden, top-most window of that class, records each icon
//! (its window, id, callback message, picture, tooltip), and passes every
//! message on unchanged to Explorer's own taskbar. Explorer's tray therefore
//! stays complete, so nothing changes for Windows or when Bloom quits. This is
//! how RetroBar and Cairo show the tray alongside Explorer.
//!
//! At start Bloom broadcasts `TaskbarCreated`, the message Explorer sends when
//! it restarts, so running apps add their icons again and Bloom sees them.
//! Clicking a tile sends the app the same callback messages Explorer sends for
//! a click on its icon, so its menu or window opens right away, at the pointer.
//!
//! The taskbar's other traffic sent to that class (AppBar messages, icon
//! positions, commands) is passed straight on to Explorer and answered by it.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::core::{w, GUID, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::{
    CopyIcon, CreateWindowExW, DefWindowProcW, DestroyIcon, DispatchMessageW, FindWindowExW,
    FindWindowW, GetMessageW, GetWindowThreadProcessId, InSendMessage, IsWindow, KillTimer,
    PostMessageW, RegisterClassExW, RegisterWindowMessageW, SendMessageTimeoutW,
    SendNotifyMessageW, SetTimer, SetWindowPos, TranslateMessage, HICON, HWND_BROADCAST,
    HWND_TOPMOST, MSG, SMTO_ABORTIFHUNG, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    WM_COMMAND, WM_CONTEXTMENU, WM_COPYDATA, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WM_USER, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

/// `TRAYNOTIFYDATAW.dwSignature`.
const SIGNATURE: u32 = 0x3475_3423;
/// `COPYDATASTRUCT.dwData` of a tray icon message.
const COPYDATA_TRAY: usize = 1;

const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETVERSION: u32 = 4;
const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;
const NIF_STATE: u32 = 0x08;
const NIF_GUID: u32 = 0x20;
const NIS_HIDDEN: u32 = 0x01;
/// `NIN_SELECT`: a version 3+ icon was clicked.
const NIN_SELECT: u32 = WM_USER;

const TIMER_ZORDER: usize = 1;
const TIMER_CHANGED: usize = 2;
const TIMER_RESYNC: usize = 3;
/// How often Bloom checks that apps still reach its window first.
const ZORDER_CHECK_MS: u32 = 2000;
/// Changes are announced to the notch at most this often.
const CHANGED_DELAY_MS: u32 = 120;
/// At most one "add your icons again" broadcast this often after start.
const RESYNC_MIN_GAP_MS: i64 = 30_000;

/// `NOTIFYICONDATAW` as `Shell_NotifyIcon` sends it to the taskbar: window and
/// icon handles are 32-bit so 32- and 64-bit programs share one layout.
#[repr(C)]
#[derive(Clone, Copy)]
struct NotifyIconData32 {
    cb_size: u32,
    hwnd: u32,
    uid: u32,
    flags: u32,
    callback: u32,
    icon: u32,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: GUID,
    balloon_icon: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TrayNotifyData {
    signature: u32,
    message: u32,
    nid: NotifyIconData32,
}

/// Where Windows loaded an icon from, so it can be loaded again larger.
#[derive(Clone)]
enum IconSource {
    Id(String, u16),
    Name(String, String),
}

impl IconSource {
    fn module(&self) -> &str {
        match self {
            IconSource::Id(module, _) | IconSource::Name(module, _) => module,
        }
    }

    /// The same source with its file as a full path: a bare file name is
    /// looked for next to the program that owns the icon, never elsewhere,
    /// so another file of the same name can't be picked up.
    fn located(self, program: &str) -> Option<IconSource> {
        let module = self.module();
        let path = if std::path::Path::new(module).is_absolute() {
            std::path::PathBuf::from(module)
        } else {
            std::path::Path::new(program).parent()?.join(module)
        };
        if !path.is_file() {
            return None;
        }
        let path = path.to_string_lossy().into_owned();
        Some(match self {
            IconSource::Id(_, id) => IconSource::Id(path, id),
            IconSource::Name(_, name) => IconSource::Name(path, name),
        })
    }
}

struct Icon {
    id: String,
    hwnd: isize,
    uid: u32,
    callback: u32,
    version: u32,
    tooltip: String,
    hidden: bool,
    /// Bloom's own copy of the icon (apps free theirs right after sending it).
    picture: Option<isize>,
    source: Option<IconSource>,
    /// The picture as a PNG data URI, made when first asked for.
    image: Option<Option<String>>,
    /// Bumped whenever the picture changes.
    serial: u64,
    pid: u32,
    path: String,
}

impl Drop for Icon {
    fn drop(&mut self) {
        if let Some(picture) = self.picture.take() {
            unsafe {
                let _ = DestroyIcon(HICON(picture as *mut _));
            }
        }
    }
}

/// One tray icon, as the notch shows it.
#[derive(Clone, Serialize)]
pub struct HostedIcon {
    pub id: String,
    pub path: String,
    pub pid: u32,
    pub tooltip: String,
    pub image: Option<String>,
    pub uid: u32,
    /// The icon's GUID ("{...}"), for icons that have one.
    pub guid: Option<String>,
    pub window: isize,
}

static ICONS: Mutex<Vec<Icon>> = Mutex::new(Vec::new());
static HOST: AtomicIsize = AtomicIsize::new(0);
static EXPLORER: AtomicIsize = AtomicIsize::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// Sent with Bloom's own `TaskbarCreated` broadcasts (as `lParam`, which apps
/// ignore), so Bloom tells them from Explorer's when they come back to it.
const BLOOM_MARK: isize = 0x426C_6F6F;
/// Explorer's taskbar window as last seen, to notice it being (re)created.
static EXPLORER_SEEN: AtomicIsize = AtomicIsize::new(0);
static LAST_RESYNC: AtomicI64 = AtomicI64::new(0);
/// When apps were last asked to add their icons again.
static SYNCED_AT: AtomicI64 = AtomicI64::new(0);
static SERIAL: AtomicU64 = AtomicU64::new(1);
static APP: OnceLock<AppHandle> = OnceLock::new();

fn now_ms() -> i64 {
    crate::utils::get_now_ms()
}

/// True once the host is up and apps have had a moment to add their icons.
pub fn ready() -> bool {
    RUNNING.load(Ordering::Relaxed)
        && SYNCED_AT.load(Ordering::Relaxed) != 0
        && now_ms() - SYNCED_AT.load(Ordering::Relaxed) > 800
}

/// Explorer's taskbar window, skipping Bloom's own window of the same class.
pub fn explorer_taskbar() -> Option<HWND> {
    unsafe {
        let own = std::process::id();
        let mut after: Option<HWND> = None;
        for _ in 0..16 {
            let hwnd = FindWindowExW(None, after, w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != own {
                return Some(hwnd);
            }
            after = Some(hwnd);
        }
        None
    }
}

fn explorer_target() -> Option<HWND> {
    let cached = HWND(EXPLORER.load(Ordering::Relaxed) as *mut _);
    if !cached.is_invalid() && unsafe { IsWindow(Some(cached)) }.as_bool() {
        return Some(cached);
    }
    let found = explorer_taskbar()?;
    EXPLORER.store(found.0 as isize, Ordering::Relaxed);
    Some(found)
}

fn text(units: &[u16]) -> String {
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

fn guid_text(guid: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

/// Handles are 32-bit in the message; Windows sign-extends them.
fn handle(value: u32) -> isize {
    value as i32 as isize
}

unsafe fn icon_source(icon: HICON) -> Option<IconSource> {
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfoExW, ICONINFOEXW};
    let mut info = ICONINFOEXW {
        cbSize: std::mem::size_of::<ICONINFOEXW>() as u32,
        ..Default::default()
    };
    if !GetIconInfoExW(icon, &mut info).as_bool() {
        return None;
    }
    if !info.hbmMask.is_invalid() {
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(info.hbmMask.into());
    }
    if !info.hbmColor.is_invalid() {
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(info.hbmColor.into());
    }
    let module = text(&info.szModName);
    if module.is_empty() {
        return None;
    }
    let name = text(&info.szResName);
    if !name.is_empty() {
        Some(IconSource::Name(module, name))
    } else if info.wResID != 0 {
        Some(IconSource::Id(module, info.wResID))
    } else {
        None
    }
}

/// Records what one `Shell_NotifyIcon` call says. Runs while the app waits,
/// so it only copies what it needs.
unsafe fn record(message: u32, nid: &NotifyIconData32) -> bool {
    let hwnd = handle(nid.hwnd);
    let guid = (nid.flags & NIF_GUID != 0 && nid.guid != GUID::zeroed()).then_some(nid.guid);
    let id = match &guid {
        Some(guid) => format!("g:{}", guid_text(guid)),
        None => format!("w:{:x}:{}", hwnd, nid.uid),
    };
    let Ok(mut icons) = ICONS.lock() else {
        return false;
    };
    let index = icons.iter().position(|icon| icon.id == id);
    match message {
        NIM_DELETE => {
            if let Some(index) = index {
                icons.remove(index);
                return true;
            }
            false
        }
        NIM_SETVERSION => {
            if let Some(index) = index {
                icons[index].version = nid.version;
            }
            false
        }
        NIM_ADD | NIM_MODIFY => {
            let index = match index {
                Some(index) => index,
                // A change for an icon Bloom never saw added (it was added
                // while Explorer's window was in front) still adds it.
                None if message == NIM_ADD || nid.flags & NIF_ICON != 0 => {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
                    if pid == std::process::id() {
                        return false;
                    }
                    let path = crate::commands::process_image_path(pid).unwrap_or_default();
                    icons.push(Icon {
                        id,
                        hwnd,
                        uid: nid.uid,
                        callback: 0,
                        version: 0,
                        tooltip: String::new(),
                        hidden: false,
                        picture: None,
                        source: None,
                        image: None,
                        serial: 0,
                        pid,
                        path,
                    });
                    icons.len() - 1
                }
                None => return false,
            };
            let icon = &mut icons[index];
            icon.hwnd = hwnd;
            icon.uid = nid.uid;
            if nid.flags & NIF_MESSAGE != 0 {
                icon.callback = nid.callback;
            }
            if nid.flags & NIF_TIP != 0 {
                icon.tooltip = text(&nid.tip)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .into();
            }
            if nid.flags & NIF_STATE != 0 && nid.state_mask & NIS_HIDDEN != 0 {
                icon.hidden = nid.state & NIS_HIDDEN != 0;
            }
            if nid.flags & NIF_ICON != 0 {
                let source = HICON(handle(nid.icon) as *mut _);
                if let Some(old) = icon.picture.take() {
                    let _ = DestroyIcon(HICON(old as *mut _));
                }
                icon.picture = if source.is_invalid() {
                    None
                } else {
                    CopyIcon(source).ok().map(|copy| copy.0 as isize)
                };
                icon.source = if source.is_invalid() {
                    None
                } else {
                    icon_source(source)
                };
                icon.image = None;
                icon.serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            }
            true
        }
        _ => false,
    }
}

/// Passes a message on to Explorer's taskbar and returns its answer.
unsafe fn forward(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let target = explorer_target()?;
    if msg == WM_COPYDATA || InSendMessage().as_bool() {
        let mut result = 0usize;
        let sent = SendMessageTimeoutW(
            target,
            msg,
            wparam,
            lparam,
            SMTO_ABORTIFHUNG | SMTO_NORMAL,
            4000,
            Some(&mut result),
        );
        (sent.0 != 0).then_some(LRESULT(result as isize))
    } else {
        PostMessageW(Some(target), msg, wparam, lparam).ok()?;
        Some(LRESULT(0))
    }
}

unsafe fn announce_change(hwnd: HWND) {
    // One announcement per burst (an animated icon changes many times a second).
    SetTimer(Some(hwnd), TIMER_CHANGED, CHANGED_DELAY_MS, None);
}

/// Asks every running app to add its tray icons again, as after an Explorer
/// restart, and remembers the broadcast so Bloom ignores it itself.
unsafe fn request_icons() {
    let message = TASKBAR_CREATED.load(Ordering::Relaxed);
    if message == 0 {
        return;
    }
    LAST_RESYNC.store(now_ms(), Ordering::Relaxed);
    SYNCED_AT.store(now_ms(), Ordering::Relaxed);
    let _ = SendNotifyMessageW(HWND_BROADCAST, message, WPARAM(0), LPARAM(BLOOM_MARK));
}

unsafe fn raise(hwnd: HWND) {
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOPMOST),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
}

/// Drops icons whose window is gone (an app that crashed never deletes them).
fn prune() -> bool {
    let Ok(mut icons) = ICONS.lock() else {
        return false;
    };
    let before = icons.len();
    icons.retain(|icon| unsafe { IsWindow(Some(HWND(icon.hwnd as *mut _))) }.as_bool());
    icons.len() != before
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_COPYDATA && lparam.0 != 0 {
        let data = &*(lparam.0 as *const COPYDATASTRUCT);
        let mut recorded = false;
        if data.dwData == COPYDATA_TRAY && data.cbData as usize >= 8 && !data.lpData.is_null() {
            let mut tray: TrayNotifyData = std::mem::zeroed();
            let length = (data.cbData as usize).min(std::mem::size_of::<TrayNotifyData>());
            std::ptr::copy_nonoverlapping(
                data.lpData as *const u8,
                &mut tray as *mut TrayNotifyData as *mut u8,
                length,
            );
            if tray.signature == SIGNATURE {
                recorded = record(tray.message, &tray.nid);
            }
        }
        if recorded {
            announce_change(hwnd);
        }
        return match forward(msg, wparam, lparam) {
            Some(result) => result,
            // Explorer isn't there (restarting): the icon is still Bloom's.
            None if data.dwData == COPYDATA_TRAY => LRESULT(1),
            None => LRESULT(0),
        };
    }

    let taskbar_created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if taskbar_created != 0 && msg == taskbar_created {
        if lparam.0 != BLOOM_MARK {
            // Explorer restarted: its new taskbar is in front of Bloom's
            // window, so apps re-add their icons there. Get back in front,
            // then ask them once more.
            EXPLORER.store(0, Ordering::Relaxed);
            SetTimer(Some(hwnd), TIMER_RESYNC, 1500, None);
        }
        return LRESULT(0);
    }

    if msg == WM_TIMER {
        match wparam.0 {
            TIMER_CHANGED => {
                let _ = KillTimer(Some(hwnd), TIMER_CHANGED);
                if let Some(app) = APP.get() {
                    // Only the notch shows the tray.
                    let _ = app.emit_to("main", "tray-changed", ());
                }
            }
            TIMER_RESYNC => {
                let _ = KillTimer(Some(hwnd), TIMER_RESYNC);
                raise(hwnd);
                if prune() {
                    announce_change(hwnd);
                }
                request_icons();
            }
            TIMER_ZORDER => {
                // Explorer's taskbar was created (Bloom started before it, at
                // sign-in) or recreated: apps added their icons there, so
                // ask them again once it has settled.
                let explorer = explorer_taskbar().map(|h| h.0 as isize).unwrap_or(0);
                let seen = EXPLORER_SEEN.swap(explorer, Ordering::Relaxed);
                if explorer != 0 && explorer != seen {
                    EXPLORER.store(0, Ordering::Relaxed);
                    SetTimer(Some(hwnd), TIMER_RESYNC, 1500, None);
                }
                let first = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).unwrap_or_default();
                if first != hwnd {
                    // Explorer's taskbar came in front (it was shown), so apps
                    // reached it instead; get back in front and catch up.
                    raise(hwnd);
                    if now_ms() - LAST_RESYNC.load(Ordering::Relaxed) > RESYNC_MIN_GAP_MS {
                        request_icons();
                    }
                }
                if prune() {
                    announce_change(hwnd);
                }
            }
            _ => {}
        }
        return LRESULT(0);
    }

    // Commands and private messages apps send the taskbar go on to Explorer.
    if msg == WM_COMMAND || (WM_USER..0x8000).contains(&msg) {
        if let Some(result) = forward(msg, wparam, lparam) {
            return result;
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Starts the host on a thread of its own. Does nothing when it is turned off
/// in settings ("bloom-tray-live" = "false").
pub fn start(app: AppHandle) {
    if crate::utils::get_setting_str(&app, "bloom-tray-live").as_deref() == Some("false") {
        return;
    }
    let _ = APP.set(app);
    std::thread::Builder::new()
        .name("bloom-tray-host".into())
        .spawn(|| unsafe {
            use windows::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows::Win32::UI::WindowsAndMessaging::{
                ChangeWindowMessageFilterEx, MSGFLT_ALLOW,
            };
            let message = RegisterWindowMessageW(w!("TaskbarCreated"));
            TASKBAR_CREATED.store(message, Ordering::Relaxed);
            let Ok(module) = GetModuleHandleW(None) else {
                return;
            };
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                hInstance: module.into(),
                lpszClassName: w!("Shell_TrayWnd"),
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 {
                return;
            }
            let Ok(hwnd) = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("Shell_TrayWnd"),
                w!(""),
                WS_POPUP,
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
            // Programs running at a lower integrity level (sandboxed ones) can
            // reach Explorer's tray, so they must reach this one too.
            let _ = ChangeWindowMessageFilterEx(hwnd, WM_COPYDATA, MSGFLT_ALLOW, None);
            if message != 0 {
                let _ = ChangeWindowMessageFilterEx(hwnd, message, MSGFLT_ALLOW, None);
            }
            raise(hwnd);
            HOST.store(hwnd.0 as isize, Ordering::Relaxed);
            EXPLORER_SEEN.store(
                explorer_taskbar().map(|h| h.0 as isize).unwrap_or(0),
                Ordering::Relaxed,
            );
            RUNNING.store(true, Ordering::Relaxed);
            request_icons();
            SetTimer(Some(hwnd), TIMER_ZORDER, ZORDER_CHECK_MS, None);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            RUNNING.store(false, Ordering::Relaxed);
        })
        .ok();
}

unsafe fn load_from_source(source: &IconSource, size: i32) -> Option<HICON> {
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{
        LoadLibraryExW, LOAD_LIBRARY_AS_DATAFILE, LOAD_LIBRARY_AS_IMAGE_RESOURCE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_DEFAULTCOLOR};
    let (module_path, name) = match source {
        IconSource::Id(path, _) => (path, None),
        IconSource::Name(path, name) => (path, Some(name)),
    };
    let wide: Vec<u16> = module_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let module = LoadLibraryExW(
        PCWSTR(wide.as_ptr()),
        None,
        LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
    )
    .ok()?;
    let name_wide: Option<Vec<u16>> =
        name.map(|n| n.encode_utf16().chain(std::iter::once(0)).collect());
    let resource = match (&name_wide, source) {
        (Some(wide), _) => PCWSTR(wide.as_ptr()),
        (None, IconSource::Id(_, id)) => PCWSTR(*id as usize as *const u16),
        _ => PCWSTR::null(),
    };
    let loaded = LoadImageW(
        Some(module.into()),
        resource,
        IMAGE_ICON,
        size,
        size,
        LR_DEFAULTCOLOR,
    )
    .ok()
    .map(|handle| HICON(handle.0));
    let _ = FreeLibrary(module);
    loaded
}

/// The icon's picture as a PNG: loaded large from where Windows got it when
/// that is known (sharp at the notch's size), else the app's own picture.
unsafe fn render(picture: Option<HICON>, source: Option<&IconSource>) -> Option<String> {
    if let Some(source) = source {
        if let Some(large) = load_from_source(source, 64) {
            let png = crate::utils::icon_to_base64(large);
            let _ = DestroyIcon(large);
            if png.is_some() {
                return png;
            }
        }
    }
    picture.and_then(|p| crate::utils::icon_to_base64(p))
}

/// Every visible tray icon of other programs, with its picture.
pub fn snapshot() -> Vec<HostedIcon> {
    if prune() {
        let host = HWND(HOST.load(Ordering::Relaxed) as *mut _);
        if !host.is_invalid() {
            unsafe { announce_change(host) };
        }
    }
    // Pictures are made outside the lock, from copies, so the host never
    // waits on them; an icon changed meanwhile keeps its newer picture.
    type Wanted = (String, u64, Option<isize>, Option<IconSource>);
    let wanted: Vec<Wanted> = {
        let Ok(icons) = ICONS.lock() else {
            return Vec::new();
        };
        icons
            .iter()
            .filter(|icon| icon.image.is_none() && !icon.hidden)
            .map(|icon| {
                let copy = icon.picture.and_then(|p| unsafe {
                    CopyIcon(HICON(p as *mut _)).ok().map(|c| c.0 as isize)
                });
                let source = icon
                    .source
                    .clone()
                    .and_then(|source| source.located(&icon.path));
                (icon.id.clone(), icon.serial, copy, source)
            })
            .collect()
    };
    let mut made = Vec::new();
    for (id, serial, copy, source) in wanted {
        let image = unsafe { render(copy.map(|c| HICON(c as *mut _)), source.as_ref()) };
        if let Some(copy) = copy {
            unsafe {
                let _ = DestroyIcon(HICON(copy as *mut _));
            }
        }
        made.push((id, serial, image));
    }
    let Ok(mut icons) = ICONS.lock() else {
        return Vec::new();
    };
    for (id, serial, image) in made {
        if let Some(icon) = icons.iter_mut().find(|i| i.id == id && i.serial == serial) {
            icon.image = Some(image);
        }
    }
    icons
        .iter()
        .filter(|icon| !icon.hidden)
        .map(|icon| HostedIcon {
            id: icon.id.clone(),
            path: icon.path.clone(),
            pid: icon.pid,
            tooltip: icon.tooltip.clone(),
            image: icon.image.clone().flatten(),
            uid: icon.uid,
            guid: icon.id.strip_prefix("g:").map(str::to_string),
            window: icon.hwnd,
        })
        .collect()
}

pub fn has_icon(id: &str) -> bool {
    ICONS
        .lock()
        .map(|icons| icons.iter().any(|icon| icon.id == id))
        .unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Double,
}

/// Menus and flyouts on screen: Windows' own menus, and pop-up windows (no
/// title bar) of the icon's program.
fn open_popups(pid: u32) -> Vec<isize> {
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowLongW, IsWindowVisible, GWL_STYLE, WS_CAPTION,
    };
    struct Search {
        pid: u32,
        found: Vec<isize>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let search = &mut *(param.0 as *mut Search);
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let mut class = [0u16; 16];
        let length = GetClassNameW(hwnd, &mut class) as usize;
        let menu = &class[..length.min(class.len())]
            == "#32768".encode_utf16().collect::<Vec<_>>().as_slice();
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut owner));
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let popup =
            owner == search.pid && style & WS_POPUP.0 != 0 && style & WS_CAPTION.0 != WS_CAPTION.0;
        if menu || popup {
            search.found.push(hwnd.0 as isize);
        }
        true.into()
    }
    let mut search = Search {
        pid,
        found: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

/// Tells the notch when what a click opened (the app's menu or flyout) is
/// gone again, so the notch stays open until then. `before` is what was on
/// screen before the click.
fn watch_interaction(id: String, pid: u32, before: Vec<isize>) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut seen = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let open = open_popups(pid).iter().any(|hwnd| !before.contains(hwnd));
            seen |= open;
            let elapsed = started.elapsed().as_millis();
            if (!open && (seen || elapsed > 1500)) || elapsed > 120_000 {
                break;
            }
        }
        if let Some(app) = APP.get() {
            let _ = app.emit("tray-interaction-end", id);
        }
    });
}

/// Clicks an icon the way Explorer's tray does, with the pointer where the
/// user clicked, so the app opens its menu or window there.
pub fn click(id: &str, button: Button) -> Result<(), String> {
    let (hwnd, callback, uid, version, pid) = {
        let icons = ICONS.lock().map_err(|_| "The tray is busy.".to_string())?;
        let icon = icons
            .iter()
            .find(|icon| icon.id == id)
            .ok_or("This tray icon is no longer running.")?;
        (icon.hwnd, icon.callback, icon.uid, icon.version, icon.pid)
    };
    let window = HWND(hwnd as *mut _);
    if !unsafe { IsWindow(Some(window)) }.as_bool() {
        prune();
        return Err("This tray icon is no longer running.".into());
    }
    if callback == 0 {
        return Err("This tray icon doesn't respond to clicks.".into());
    }
    let mut cursor = POINT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor);
    }
    let before = open_popups(pid);
    // The app may bring its menu or window to the front.
    crate::foreground::grant_to(pid);
    let anchor = ((cursor.y as u32 & 0xFFFF) << 16) | (cursor.x as u32 & 0xFFFF);
    let send = |event: u32| unsafe {
        let (wparam, lparam) = if version > 3 {
            (
                anchor as usize,
                (((uid & 0xFFFF) << 16) | (event & 0xFFFF)) as isize,
            )
        } else {
            (uid as usize, event as isize)
        };
        let _ = SendNotifyMessageW(window, callback, WPARAM(wparam), LPARAM(lparam));
    };
    send(WM_MOUSEMOVE);
    match button {
        Button::Left => {
            send(WM_LBUTTONDOWN);
            send(WM_LBUTTONUP);
            // Documented for version 4; Explorer sends it to version 3 too.
            if version >= 3 {
                send(NIN_SELECT);
            }
        }
        Button::Double => {
            send(WM_LBUTTONDBLCLK);
            send(WM_LBUTTONUP);
        }
        Button::Right => {
            send(WM_RBUTTONDOWN);
            send(WM_RBUTTONUP);
            if version >= 3 {
                send(WM_CONTEXTMENU);
            }
        }
    }
    watch_interaction(id.to_string(), pid, before);
    Ok(())
}
