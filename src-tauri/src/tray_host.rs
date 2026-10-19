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
//! AppBar messages that carry a screen area (asking for or setting the strip a
//! bar reserves, as Bloom's dock and notch do) keep it in shared memory made
//! for the window they reach first, which Explorer can't open; that memory is
//! made again for Explorer, and Explorer's answer copied back, as Cairo does.
//! Without this the bars register but never reserve their strip, and
//! maximized windows go underneath them.

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
    HWND_MESSAGE, HWND_TOPMOST, MSG, SMTO_ABORTIFHUNG, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, WM_COMMAND, WM_CONTEXTMENU, WM_COPYDATA, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_TIMER, WM_USER, WNDCLASSEXW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// `TRAYNOTIFYDATAW.dwSignature`.
const SIGNATURE: u32 = 0x3475_3423;
/// `COPYDATASTRUCT.dwData` of a tray icon message.
const COPYDATA_TRAY: usize = 1;
/// `COPYDATASTRUCT.dwData` of an AppBar message (`SHAppBarMessage`).
const COPYDATA_APPBAR: usize = 0;

/// `APPBARDATA` as `SHAppBarMessage` sends it to the taskbar (32- and 64-bit
/// programs share this layout).
#[repr(C)]
#[derive(Clone, Copy)]
struct AppBarData3264 {
    size: u32,
    hwnd: u32,
    callback_message: u32,
    edge: u32,
    rect: windows::Win32::Foundation::RECT,
    lparam: i64,
}

/// The whole AppBar message: the bar's data, which message, and the shared
/// memory the answer goes back in (none for most messages).
#[repr(C)]
#[derive(Clone, Copy)]
struct AppBarMessage3264 {
    data: AppBarData3264,
    message: u32,
    _padding1: u32,
    shared_memory: i64,
    source_process: u32,
    _padding2: u32,
}

const _: () = assert!(std::mem::size_of::<AppBarData3264>() == 40);
const _: () = assert!(std::mem::size_of::<AppBarMessage3264>() == 64);

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

/// Posted to the host to announce a change from another thread (timers
/// belong to the window's own thread). Above `WM_USER`'s range, which goes
/// on to Explorer.
const MSG_ANNOUNCE: u32 = 0x8000 + 0x42;

const TIMER_ZORDER: usize = 1;
const TIMER_CHANGED: usize = 2;
const TIMER_RESYNC: usize = 3;
/// How often Bloom checks that apps still reach its window first.
const ZORDER_CHECK_MS: u32 = 2000;
/// Changes are announced to the notch at most this often.
const CHANGED_DELAY_MS: u32 = 120;

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
    /// Hidden from Windows' tray (`NIS_HIDDEN`), as Tray Icon Manager hides
    /// icons: still running, and shown under the notch's hidden icons.
    pub hidden: bool,
    /// The app gave it a picture (an icon without one shows nothing).
    pub has_picture: bool,
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

/// Explorer's taskbar window: the one of class `Shell_TrayWnd` that belongs to
/// the process showing the desktop (Explorer), so neither Bloom's own window
/// of that class nor one another program makes (a tray tool) is taken for it.
/// Without a desktop, the first one that isn't Bloom's.
pub fn explorer_taskbar() -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::GetShellWindow;
    unsafe {
        let own = std::process::id();
        let mut shell = 0u32;
        let desktop = GetShellWindow();
        if !desktop.0.is_null() {
            GetWindowThreadProcessId(desktop, Some(&mut shell));
        }
        let mut first_other: Option<HWND> = None;
        let mut after: Option<HWND> = None;
        for _ in 0..16 {
            let Ok(hwnd) = FindWindowExW(None, after, w!("Shell_TrayWnd"), PCWSTR::null()) else {
                break;
            };
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != own {
                if shell == 0 || pid == shell {
                    return Some(hwnd);
                }
                first_other.get_or_insert(hwnd);
            }
            after = Some(hwnd);
        }
        first_other
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

/// A hidden state another program set for an icon Bloom hasn't seen yet.
struct StateSeen {
    id: String,
    hwnd: isize,
    uid: u32,
    hidden: bool,
    /// When the app was last asked to add its icons (or the state was seen).
    at: i64,
}

static STATES_SEEN: Mutex<Vec<StateSeen>> = Mutex::new(Vec::new());

unsafe fn remember_state(id: &str, hwnd: isize, uid: u32, hidden: bool) {
    let now = now_ms();
    let Ok(mut seen) = STATES_SEEN.lock() else {
        return;
    };
    seen.retain(|entry| now - entry.at < 10 * 60_000);
    let asked_lately = seen.iter().any(|entry| {
        (entry.id == id || (hwnd != 0 && entry.hwnd == hwnd)) && now - entry.at < 60_000
    });
    match seen.iter_mut().find(|entry| entry.id == id) {
        Some(entry) => {
            entry.hidden = hidden;
            if !asked_lately {
                entry.at = now;
            }
        }
        None => seen.push(StateSeen {
            id: id.to_string(),
            hwnd,
            uid,
            hidden,
            at: now,
        }),
    }
    if !asked_lately && hwnd != 0 {
        ask_to_add_again(hwnd);
    }
}

/// The state remembered for an icon now added, used once.
fn remembered_state(id: &str, hwnd: isize, uid: u32) -> Option<bool> {
    let mut seen = STATES_SEEN.lock().ok()?;
    let index = seen.iter().position(|entry| {
        entry.id == id || (hwnd != 0 && entry.hwnd == hwnd && entry.uid == uid)
    })?;
    Some(seen.remove(index).hidden)
}

/// Records what one `Shell_NotifyIcon` call says, given Explorer's answer to
/// it (`None` when Explorer isn't there to answer). Runs while the app waits,
/// so it only copies what it needs.
unsafe fn record(message: u32, nid: &NotifyIconData32, accepted: Option<bool>) -> bool {
    let hwnd = handle(nid.hwnd);
    let guid = (nid.flags & NIF_GUID != 0 && nid.guid != GUID::zeroed()).then_some(nid.guid);
    let id = match &guid {
        Some(guid) => format!("g:{}", guid_text(guid)),
        None => format!("w:{:x}:{}", hwnd, nid.uid),
    };
    let Ok(mut icons) = ICONS.lock() else {
        return false;
    };
    // The same icon, however this message names it: by its GUID, or by its
    // window and id (not every app sends the GUID with every change), so a
    // change never makes a second copy of an icon without its click message.
    let index = icons.iter().position(|icon| icon.id == id).or_else(|| {
        (hwnd != 0).then_some(()).and_then(|_| {
            icons
                .iter()
                .position(|icon| icon.hwnd == hwnd && icon.uid == nid.uid)
        })
    });
    if let (Some(index), Some(_)) = (index, &guid) {
        icons[index].id = id.clone();
    }
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
                // while Explorer's window was in front) still adds it, unless
                // Explorer turned the change down: then Windows has no such
                // icon either (an app changing an icon it removed, or trying
                // a change before adding), and Bloom shows none.
                None if message == NIM_ADD
                    || (nid.flags & NIF_ICON != 0 && accepted != Some(false)) =>
                {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
                    if pid == std::process::id() {
                        return false;
                    }
                    // First seen being changed rather than added (it was added
                    // while Explorer's window was in front), so without the
                    // message its clicks go to: ask the app to add it again.
                    if message != NIM_ADD && nid.flags & NIF_MESSAGE == 0 {
                        ask_to_add_again(hwnd);
                    }
                    let path = crate::commands::any_process_path(pid).unwrap_or_default();
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
                None => {
                    // Another program (Tray Icon Manager) hiding or showing an
                    // icon Bloom hasn't seen: kept for when the app adds it,
                    // which it is asked to do.
                    if nid.flags & NIF_STATE != 0 && nid.state_mask & NIS_HIDDEN != 0 {
                        remember_state(&id, hwnd, nid.uid, nid.state & NIS_HIDDEN != 0);
                    }
                    return false;
                }
            };
            if message == NIM_ADD {
                if let Some(hidden) = remembered_state(&icons[index].id, hwnd, nid.uid) {
                    icons[index].hidden = hidden;
                }
            }
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

/// Passes an AppBar message that carries shared memory on to Explorer, with
/// that memory made again for Explorer and its answer (the strip it allows a
/// bar, or where the taskbar is) copied back for the program that asked.
/// `None` when the message has no shared memory, so it is passed on as it is.
unsafe fn forward_appbar(wparam: WPARAM, data: &COPYDATASTRUCT) -> Option<LRESULT> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::UI::Shell::{SHAllocShared, SHFreeShared, SHLockShared, SHUnlockShared};

    if data.cbData as usize != std::mem::size_of::<AppBarMessage3264>() || data.lpData.is_null() {
        return None;
    }
    let mut message = std::ptr::read_unaligned(data.lpData as *const AppBarMessage3264);
    if message.shared_memory == 0 {
        return None;
    }
    let Some(target) = explorer_target() else {
        // Explorer isn't there (restarting): nothing can answer.
        return Some(LRESULT(0));
    };
    let mut explorer = 0u32;
    GetWindowThreadProcessId(target, Some(&mut explorer));
    if explorer == 0 {
        return None;
    }

    let size = std::mem::size_of::<AppBarData3264>();
    let asked = HANDLE(message.shared_memory as isize as *mut core::ffi::c_void);
    let original = SHLockShared(asked, message.source_process);
    if original.is_null() {
        return None;
    }
    let copy = SHAllocShared(Some(original as *const _), size as u32, explorer);
    if copy.is_invalid() {
        let _ = SHUnlockShared(original);
        return None;
    }
    message.shared_memory = copy.0 as isize as i64;
    message.source_process = explorer;
    let mut onward = COPYDATASTRUCT {
        dwData: COPYDATA_APPBAR,
        cbData: std::mem::size_of::<AppBarMessage3264>() as u32,
        lpData: &mut message as *mut AppBarMessage3264 as *mut _,
    };
    let mut result = 0usize;
    let sent = SendMessageTimeoutW(
        target,
        WM_COPYDATA,
        wparam,
        LPARAM(&mut onward as *mut COPYDATASTRUCT as isize),
        SMTO_ABORTIFHUNG | SMTO_NORMAL,
        4000,
        Some(&mut result),
    );
    // Explorer writes its answer into the copy.
    let answer = SHLockShared(copy, explorer);
    if !answer.is_null() {
        std::ptr::copy_nonoverlapping(answer as *const u8, original as *mut u8, size);
        let _ = SHUnlockShared(answer);
    }
    let _ = SHUnlockShared(original);
    let _ = SHFreeShared(copy, explorer);
    Some(LRESULT(if sent.0 != 0 { result as isize } else { 0 }))
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
    SYNCED_AT.store(now_ms(), Ordering::Relaxed);
    let _ = SendNotifyMessageW(HWND_BROADCAST, message, WPARAM(0), LPARAM(BLOOM_MARK));
    // A broadcast reaches only ordinary windows; some apps keep their tray
    // icon on a message-only one, so those are asked one by one.
    let own = std::process::id();
    let mut previous = None;
    for _ in 0..4096 {
        let Ok(window) =
            FindWindowExW(Some(HWND_MESSAGE), previous, PCWSTR::null(), PCWSTR::null())
        else {
            break;
        };
        previous = Some(window);
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid != own && pid != 0 {
            let _ = PostMessageW(Some(window), message, WPARAM(0), LPARAM(BLOOM_MARK));
        }
    }
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
        if data.dwData == COPYDATA_APPBAR {
            if let Some(result) = forward_appbar(wparam, data) {
                return result;
            }
        }
        let mut tray: Option<TrayNotifyData> = None;
        if data.dwData == COPYDATA_TRAY && data.cbData as usize >= 8 && !data.lpData.is_null() {
            let mut copy: TrayNotifyData = std::mem::zeroed();
            let length = (data.cbData as usize).min(std::mem::size_of::<TrayNotifyData>());
            std::ptr::copy_nonoverlapping(
                data.lpData as *const u8,
                &mut copy as *mut TrayNotifyData as *mut u8,
                length,
            );
            if copy.signature == SIGNATURE {
                tray = Some(copy);
            }
        }
        // Explorer answers first: what it takes is what Windows' tray has,
        // and Bloom's copy follows that.
        let answer = forward(msg, wparam, lparam);
        if let Some(tray) = tray {
            if record(tray.message, &tray.nid, answer.map(|r| r.0 != 0)) {
                announce_change(hwnd);
            }
        }
        return match answer {
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

    if msg == MSG_ANNOUNCE {
        announce_change(hwnd);
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
                    // reached it instead: get back in front. What was missed
                    // meanwhile is caught up icon by icon (`tray::reconcile`),
                    // not by asking every app to add its icons again, which
                    // some answer with a second copy of their icon.
                    raise(hwnd);
                }
                if prune() {
                    announce_change(hwnd);
                }
                // Icons Windows' tray removed or added without Bloom hearing.
                crate::tray::reconcile_in_background();
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
    // Also needed by the older way of clicking, when the host is off.
    let _ = APP.set(app.clone());
    if crate::utils::get_setting_str(&app, "bloom-tray-live").as_deref() == Some("false") {
        return;
    }
    std::thread::Builder::new()
        .name("bloom-tray-host".into())
        .spawn(move || unsafe {
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
            // Apps' taskbar progress (and the rest of `ITaskbarList`) is sent
            // to the window this window names, so Bloom names one of its own.
            crate::taskbar_progress::start(&app, module.into(), hwnd);
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

/// Every tray icon of other programs, with its picture, also the ones hidden
/// from Windows' tray (each says whether it is).
pub fn snapshot() -> Vec<HostedIcon> {
    if prune() {
        announce();
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
            .filter(|icon| icon.image.is_none())
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
        .map(|icon| HostedIcon {
            id: icon.id.clone(),
            path: icon.path.clone(),
            pid: icon.pid,
            tooltip: icon.tooltip.clone(),
            image: icon.image.clone().flatten(),
            uid: icon.uid,
            guid: icon.id.strip_prefix("g:").map(str::to_string),
            window: icon.hwnd,
            hidden: icon.hidden,
            has_picture: icon.picture.is_some(),
        })
        .collect()
}

/// Every tray icon the host has, without pictures (quick).
pub fn snapshot_ids() -> Vec<HostedIcon> {
    let Ok(icons) = ICONS.lock() else {
        return Vec::new();
    };
    icons
        .iter()
        .map(|icon| HostedIcon {
            id: icon.id.clone(),
            path: icon.path.clone(),
            pid: icon.pid,
            tooltip: icon.tooltip.clone(),
            image: None,
            uid: icon.uid,
            guid: icon.id.strip_prefix("g:").map(str::to_string),
            window: icon.hwnd,
            hidden: icon.hidden,
            has_picture: icon.picture.is_some(),
        })
        .collect()
}

/// Drops an icon Explorer's tray no longer has (removed while Explorer's
/// window was in front, so Bloom never heard), as Windows' tray did.
pub fn forget(id: &str) -> bool {
    let Ok(mut icons) = ICONS.lock() else {
        return false;
    };
    let before = icons.len();
    icons.retain(|icon| icon.id != id);
    let gone = icons.len() != before;
    drop(icons);
    if gone {
        announce();
    }
    gone
}

/// Bloom's settings folder (for the tray's icon list file).
pub fn config_dir() -> Option<std::path::PathBuf> {
    use tauri::Manager;
    APP.get()?.path().app_config_dir().ok()
}

/// Asks the app that owns this window to add its tray icons again.
pub fn ask_icons_again(hwnd: isize) {
    unsafe { ask_to_add_again(hwnd) }
}

/// The host is up and apps have had time to add their icons since Bloom
/// last asked them to.
pub fn settled() -> bool {
    ready() && now_ms() - SYNCED_AT.load(Ordering::Relaxed) > 5000
}

/// Tells the notch's tray that its icons changed. Any thread.
pub fn announce() {
    let host = HWND(HOST.load(Ordering::Relaxed) as *mut _);
    if !host.0.is_null() {
        unsafe {
            let _ = PostMessageW(Some(host), MSG_ANNOUNCE, WPARAM(0), LPARAM(0));
        }
    }
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
/// Bloom and the processes it started (its WebView2 browser processes, which
/// draw its tooltips as windows of their own): never an app's menu.
pub(crate) fn bloom_processes() -> Vec<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let own = std::process::id();
    let mut found = vec![own];
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return found;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ParentProcessID == own {
                    found.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    found
}

/// Whether a window can actually be seen: shown, not cloaked, not faded out
/// to nothing, and on a screen. Some apps keep a closed menu's window shown
/// but invisible (faded out, or moved off the screen), and that must not
/// count as an open menu.
fn really_shown(hwnd: HWND) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetLayeredWindowAttributes, GetWindowLongW, GetWindowRect, IsWindowVisible, GWL_EXSTYLE,
        LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, WS_EX_LAYERED,
    };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED.0 != 0 {
            let mut alpha = 255u8;
            let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
            if GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), Some(&mut flags)).is_ok()
                && flags.0 & LWA_ALPHA.0 != 0
                && alpha == 0
            {
                return false;
            }
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err()
            || rect.right <= rect.left
            || rect.bottom <= rect.top
            || MonitorFromRect(&rect, MONITOR_DEFAULTTONULL).is_invalid()
        {
            return false;
        }
        true
    }
}

/// The menus on screen that a tray click at `point` may have opened, by
/// handle: Windows' own menus (`#32768`), and the menus apps draw themselves
/// (a tool or top-most pop-up without a caption, of menu width, that takes
/// clicks) opening at the pointer, in the app's process or a helper's
/// (Steam's web helper). An app's window or panel opened elsewhere is not a
/// menu, so it never holds the notch open.
pub(crate) fn open_menus(point: POINT, excluded: &[u32]) -> Vec<isize> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowLongW, GetWindowRect, GWL_EXSTYLE, GWL_STYLE,
        WS_CAPTION, WS_EX_TRANSPARENT,
    };
    struct Search<'a> {
        point: POINT,
        excluded: &'a [u32],
        found: Vec<isize>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let search = &mut *(param.0 as *mut Search);
        if !really_shown(hwnd) {
            return true.into();
        }
        let mut class = [0u16; 16];
        let length = GetClassNameW(hwnd, &mut class) as usize;
        if &class[..length.min(class.len())]
            == "#32768".encode_utf16().collect::<Vec<_>>().as_slice()
        {
            search.found.push(hwnd.0 as isize);
            return true.into();
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_POPUP.0 == 0
            || style & WS_CAPTION.0 == WS_CAPTION.0
            || ex_style & WS_EX_TRANSPARENT.0 != 0
            || ex_style & (WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) == 0
        {
            return true.into();
        }
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut owner));
        if search.excluded.contains(&owner) {
            return true.into();
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return true.into();
        }
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let reach = 32 * dpi / 96;
        let menu_sized = width > 8 && height > 8 && width <= 720 * dpi / 96;
        let at_pointer = search.point.x >= rect.left - reach
            && search.point.x <= rect.right + reach
            && search.point.y >= rect.top - reach
            && search.point.y <= rect.bottom + reach;
        if menu_sized && at_pointer {
            search.found.push(hwnd.0 as isize);
        }
        true.into()
    }
    let mut search = Search {
        point,
        excluded,
        found: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

/// The tray menus open right now, per click being watched, so Bloom keeps
/// them in front of the notch: the notch puts itself back on top whenever it
/// changes whether it takes clicks, which happens as the pointer moves from
/// the icon into the menu.
static OPEN_MENUS: Mutex<Vec<(u64, Vec<isize>)>> = Mutex::new(Vec::new());
static WATCH_SERIAL: AtomicU64 = AtomicU64::new(1);

fn raise_menu(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::SWP_NOOWNERZORDER;
    let window = HWND(hwnd as *mut _);
    unsafe {
        if IsWindow(Some(window)).as_bool() {
            let _ = SetWindowPos(
                window,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            );
        }
    }
}

/// Puts the open tray menus back in front of Bloom's windows. Called whenever
/// Bloom raises one of its windows.
pub fn raise_open_menus() {
    let menus: Vec<isize> = match OPEN_MENUS.lock() {
        Ok(open) if !open.is_empty() => open.iter().flat_map(|(_, m)| m.clone()).collect(),
        _ => return,
    };
    for hwnd in menus {
        raise_menu(hwnd);
    }
}

/// Whether the pointer is on one of `menus` (or within a few pixels of it).
fn pointer_on_any(menus: &[isize]) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return true;
    }
    menus.iter().any(|&menu| {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(HWND(menu as *mut _), &mut rect) }.is_ok()
            && cursor.x >= rect.left - 12
            && cursor.x <= rect.right + 12
            && cursor.y >= rect.top - 12
            && cursor.y <= rect.bottom + 12
    })
}

/// Whether one of Bloom's windows is in front of `menu`.
fn bloom_in_front_of(menu: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, IsWindowVisible, GW_HWNDPREV};
    let own = std::process::id();
    let mut above = HWND(menu as *mut _);
    for _ in 0..64 {
        above = match unsafe { GetWindow(above, GW_HWNDPREV) } {
            Ok(hwnd) if !hwnd.0.is_null() => hwnd,
            _ => return false,
        };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(above, Some(&mut owner)) };
        if owner == own && unsafe { IsWindowVisible(above) }.as_bool() {
            return true;
        }
    }
    false
}

/// Watches what a tray click opened. While a menu it opened is on screen the
/// notch stays open, however long that is. A right click holds the notch open
/// at once (`pinned`), as a menu is coming; a left click only once a menu
/// actually appears (`tray-menu-open`), so opening an app's window lets the
/// notch close as usual. `tray-interaction-end` says when the menu is gone.
/// `before` is what was on screen before the click.
pub(crate) fn watch_interaction(
    id: String,
    point: POINT,
    excluded: Vec<u32>,
    before: Vec<isize>,
    pinned: bool,
) {
    std::thread::spawn(move || {
        let serial = WATCH_SERIAL.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let mut seen = false;
        let mut announced = false;
        // When the pointer was last on one of the menus. A menu nobody has
        // pointed at for a while lets the notch go: it may be one an app keeps
        // around after closing it in a way that can't be told from open.
        let mut last_on_menu = std::time::Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let opened: Vec<isize> = open_menus(point, &excluded)
                .into_iter()
                .filter(|hwnd| !before.contains(hwnd))
                .collect();
            let open = !opened.is_empty();
            if open && pointer_on_any(&opened) {
                last_on_menu = std::time::Instant::now();
            }
            if open && last_on_menu.elapsed().as_secs() >= 8 {
                break;
            }
            // In front of the notch, from the moment it appears.
            for &menu in &opened {
                if !seen || bloom_in_front_of(menu) {
                    raise_menu(menu);
                }
            }
            if let Ok(mut menus) = OPEN_MENUS.lock() {
                menus.retain(|(watch, _)| *watch != serial);
                if open {
                    menus.push((serial, opened));
                }
            }
            if open && !seen && !pinned {
                if let Some(app) = APP.get() {
                    let _ = app.emit_to("main", "tray-menu-open", id.clone());
                }
                announced = true;
            }
            seen |= open;
            let elapsed = started.elapsed();
            // No menu at all: the click did something else.
            if !open && (seen || elapsed.as_millis() > 1500) {
                break;
            }
            if elapsed.as_secs() > 600 {
                break;
            }
        }
        if let Ok(mut menus) = OPEN_MENUS.lock() {
            menus.retain(|(watch, _)| *watch != serial);
        }
        if pinned || announced {
            if let Some(app) = APP.get() {
                let _ = app.emit_to("main", "tray-interaction-end", id);
            }
        }
    });
}

/// An icon Bloom has no click message for even after asking (`tray::interact`
/// then clicks it in Explorer's own tray instead).
pub const NO_CLICK_MESSAGE: &str = "This tray icon doesn't respond to clicks.";

/// Asks one program to add its tray icons again, as it does after Explorer
/// restarts (`TaskbarCreated`), so Bloom gets what it missed.
unsafe fn ask_to_add_again(hwnd: isize) {
    let message = TASKBAR_CREATED.load(Ordering::Relaxed);
    if message != 0 {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut _)),
            message,
            WPARAM(0),
            LPARAM(BLOOM_MARK),
        );
    }
}

/// The message (and version) an icon's clicks go to, for an icon Bloom saw
/// changed but never added: the one the app gave another of its icons (apps
/// use one message for all their icons), or else the one it gives when asked
/// to add its icons again, which it does at once.
fn recover_callback(id: &str, hwnd: isize, uid: u32) -> Option<(u32, u32)> {
    let known = || -> Option<(u32, u32)> {
        let icons = ICONS.lock().ok()?;
        icons
            .iter()
            .filter(|icon| icon.callback != 0)
            .find(|icon| icon.id == id || (icon.hwnd == hwnd && icon.uid == uid))
            .or_else(|| {
                icons
                    .iter()
                    .find(|icon| icon.callback != 0 && icon.hwnd == hwnd)
            })
            .map(|icon| (icon.callback, icon.version))
    };
    if let Some(found) = known() {
        return Some(found);
    }
    unsafe {
        // In front first, so what the app sends comes to Bloom.
        let host = HWND(HOST.load(Ordering::Relaxed) as *mut _);
        if !host.0.is_null() {
            raise(host);
        }
        ask_to_add_again(hwnd);
    }
    for _ in 0..16 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Some(found) = known() {
            return Some(found);
        }
    }
    None
}

/// An icon's window, id and GUID, to find it in Explorer's own tray.
pub fn identity(id: &str) -> Option<(isize, u32, Option<GUID>)> {
    let icons = ICONS.lock().ok()?;
    let icon = icons.iter().find(|icon| icon.id == id)?;
    let guid = icon
        .id
        .strip_prefix("g:")
        .and_then(|text| GUID::try_from(text.trim_matches(|c| c == '{' || c == '}')).ok());
    Some((icon.hwnd, icon.uid, guid))
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
    let (callback, version) = if callback != 0 {
        (callback, version)
    } else {
        recover_callback(id, hwnd, uid).unwrap_or((0, version))
    };
    if callback == 0 {
        return Err(NO_CLICK_MESSAGE.into());
    }
    let mut cursor = POINT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor);
    }
    let excluded = bloom_processes();
    let before = open_menus(cursor, &excluded);
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
    watch_interaction(
        id.to_string(),
        cursor,
        excluded,
        before,
        matches!(button, Button::Right),
    );
    Ok(())
}
