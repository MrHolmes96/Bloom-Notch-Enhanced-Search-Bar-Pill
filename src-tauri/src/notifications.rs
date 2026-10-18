//! Notifications (Settings > Notifications).
//!
//! Windows style keeps Windows' own pop-ups and can move them to another
//! corner. Bloom style shows them as Bloom cards (Bloom's colours and opacity,
//! adjustable rounding) in a window of their own and hides Windows' pop-up.
//!
//! Bloom reads the notifications with Windows' notification listener. Its
//! change event needs a packaged app, so Bloom reads it the moment Windows'
//! own pop-up appears or changes (which is exactly when a pop-up is due), and
//! otherwise only every few seconds. Windows' pop-up is only hidden once that
//! listener works, and it is put back whenever Bloom style is turned off,
//! Bloom exits, or Bloom starts again.
//!
//! Every notification Bloom shows as a card is also kept in Bloom's own
//! history (with what clicking it does), so it stays in Bloom's notification
//! center until it is clicked or cleared, even when Windows lets go of it
//! sooner (an app that keeps nothing in Windows' center, a Windows update), and
//! clicking it still works then.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
    GWL_EXSTYLE, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED,
    WS_EX_TRANSPARENT,
};

use crate::utils::get_setting_str;

static APP: OnceLock<AppHandle> = OnceLock::new();
/// Bloom style is working, so Windows' pop-up may be hidden.
static LISTENER_OK: AtomicBool = AtomicBool::new(false);
/// The pop-up window Bloom hid, and its extended style before that.
static HIDDEN_TOAST: AtomicIsize = AtomicIsize::new(0);
static HIDDEN_ORIGINAL_STYLE: AtomicIsize = AtomicIsize::new(0);
static STATUS: Mutex<&'static str> = Mutex::new("off");
static ACCESS_ASKED: AtomicBool = AtomicBool::new(false);

fn style(app: &AppHandle) -> String {
    get_setting_str(app, "bloom-notify-style").unwrap_or_else(|| "windows".into())
}

fn position(app: &AppHandle) -> String {
    get_setting_str(app, "bloom-notify-position").unwrap_or_else(|| "default".into())
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_status(status: &'static str) {
    let changed = STATUS
        .lock()
        .map(|mut s| {
            let changed = *s != status;
            *s = status;
            changed
        })
        .unwrap_or(false);
    if changed {
        if let Some(app) = APP.get() {
            let _ = app.emit("notifications-status", status);
        }
    }
}

// ---------------------------------------------------------------- Windows' pop-up

/// The pop-up's window title in each Windows display language, from TopNotify
/// (GPL-3.0, github.com/SamsidParty/TopNotify). Matching the title keeps the
/// Notification Center, which is the same kind of window, untouched.
const TOAST_TITLES: &[&str] = &[
    "New notification",
    "Nuwe kennisgewing",
    "አዲስ ማሳወቂያ",
    "নতুন জাননী",
    "Yeni bildiriş",
    "Новае апавяшчэнне",
    "নতুন বিজ্ঞপ্তি",
    "Novo obavještenje",
    "Notificació nova",
    "ᎢᏤᎢ ᎧᏃᎮᏓ",
    "Hysbysiad newydd",
    "اعلان جدید",
    "Bagong notification",
    "Fógra nua",
    "Brath ùr",
    "નવી સૂચના",
    "नई अधिसूचना",
    "Նոր ծանուցում",
    "Ný tilkynning",
    "ახალი შეტყობინება",
    "Жаңа хабарландыру",
    "ការ\u{200B}ជា\u{200B}ន្ដេីង\u{200B}ថ្មី",
    "ಹೊಸ ಪ್ರಕಟಣೆ",
    "नवी अधिसुचोवणी",
    "Nei Notifikatioun",
    "ການແຈ້ງເຕືອນໃໝ່",
    "Whakamōhiotanga hōu",
    "Ново известување",
    "പുതിയ അറിയിപ്പ്",
    "नवीन सूचना",
    "Pemberitahuan baharu",
    "Notifika ġdida",
    "नयाँ सूचना",
    "Nytt varsel",
    "ନୂତନ ବିଜ୍ଞପ୍ତି",
    "ਨਵੀਂ ਸੂਚਨਾ",
    "Musuq willana",
    "Njoftim i ri",
    "Ново обавјешчење",
    "Ново обавештење",
    "புதிய அறிவிப்பு",
    "కొత్త నోటిఫికేషన్",
    "Яңа белдерү",
    "يېڭى ئۇقتۇرۇش",
    "نئی اطلاع",
    "Yangi xabarnoma",
    "إعلام جديد",
    "Ново известие",
    "Nové oznámení",
    "Ny meddelelse",
    "Neue Benachrichtigung",
    "Νέα ειδοποίηση",
    "Nueva notificación",
    "Uus teatis",
    "Jakinarazpen berria",
    "Uusi ilmoitus",
    "Nouvelle notification",
    "Nova notificación",
    "הודעה חדשה",
    "Nova obavijesti",
    "Új értesítés",
    "Pemberitahuan baru",
    "Nuova notifica",
    "新しい通知",
    "새 알림",
    "Naujas pranešimas",
    "Jauns paziņojums",
    "Ny varsling",
    "Nieuwe melding",
    "Nowe powiadomienie",
    "Nova notificação",
    "Notificare nouă",
    "Новое уведомление",
    "Nové oznámenie",
    "Novo obvestilo",
    "Novo obaveštenje",
    "Nytt meddelande",
    "การแจ้งให้ทราบใหม่",
    "Yeni bildirim",
    "Нове сповіщення",
    "Thông báo mới",
    "新通知",
    "Notificación nueva",
    "Ny avisering",
    "Ny melding",
    "नई सूचना",
    "새로운 알림",
    "null",
];

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
}

fn window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
}

fn process_file_name(hwnd: HWND) -> String {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return String::new();
    }
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buffer = [0u16; 512];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        path.rsplit('\\').next().unwrap_or_default().to_string()
    }
}

fn is_toast_window(hwnd: HWND) -> bool {
    class_name(hwnd) == "Windows.UI.Core.CoreWindow"
        && TOAST_TITLES.contains(&window_title(hwnd).as_str())
        && process_file_name(hwnd).eq_ignore_ascii_case("ShellExperienceHost.exe")
}

fn find_toast_window() -> Option<HWND> {
    unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> windows::core::BOOL {
        if class_name(hwnd) == "Windows.UI.Core.CoreWindow" && is_toast_window(hwnd) {
            *(found.0 as *mut isize) = hwnd.0 as isize;
            return false.into();
        }
        true.into()
    }
    let mut found: isize = 0;
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut isize as isize));
    }
    (found != 0).then_some(HWND(found as *mut _))
}

/// Makes the pop-up invisible and click-through (the way TopNotify fades it).
fn hide_toast(hwnd: HWND) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if HIDDEN_TOAST.load(Ordering::SeqCst) != hwnd.0 as isize {
            HIDDEN_ORIGINAL_STYLE.store(style, Ordering::SeqCst);
            HIDDEN_TOAST.store(hwnd.0 as isize, Ordering::SeqCst);
        }
        let wanted = style | WS_EX_LAYERED.0 as isize | WS_EX_TRANSPARENT.0 as isize;
        if style != wanted {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        }
        let _ = SetLayeredWindowAttributes(hwnd, Default::default(), 0, LWA_ALPHA);
    }
}

/// Puts back the pop-up Bloom hid, if any.
pub fn restore_native_toasts() {
    let hidden = HIDDEN_TOAST.swap(0, Ordering::SeqCst);
    if hidden == 0 {
        return;
    }
    let hwnd = HWND(hidden as *mut _);
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, Default::default(), 255, LWA_ALPHA);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            HIDDEN_ORIGINAL_STYLE.load(Ordering::SeqCst),
        );
    }
}

/// Clears a hidden pop-up left by an earlier run that ended without cleaning
/// up (a crash), before anything else.
pub(crate) fn repair_leftover() {
    if let Some(hwnd) = find_toast_window() {
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let ours = WS_EX_LAYERED.0 as isize | WS_EX_TRANSPARENT.0 as isize;
            if style & ours == ours {
                let _ = SetLayeredWindowAttributes(hwnd, Default::default(), 255, LWA_ALPHA);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style & !ours);
            }
        }
    }
}

fn work_area(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info)
            .as_bool()
            .then_some(info.rcWork)
    }
}

/// Top-left corner for a window of this size in the chosen corner.
fn corner_origin(area: RECT, width: i32, height: i32, corner: &str, margin: i32) -> (i32, i32) {
    let left = area.left + margin;
    let right = area.right - width - margin;
    let top = area.top + margin;
    let bottom = area.bottom - height - margin;
    match corner {
        "top-left" => (left, top),
        "top-right" => (right, top),
        "bottom-left" => (left, bottom),
        _ => (right, bottom),
    }
}

/// Moves Windows' pop-up to the chosen corner of its screen.
fn move_toast(hwnd: HWND, corner: &str) {
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let Some(area) = work_area(hwnd) else {
        return;
    };
    let (x, y) = corner_origin(
        area,
        rect.right - rect.left,
        rect.bottom - rect.top,
        corner,
        0,
    );
    if rect.left != x || rect.top != y {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

/// Brings the pop-up in line with the settings.
fn apply_to_toast(hwnd: HWND) {
    let Some(app) = APP.get() else {
        return;
    };
    if style(app) == "bloom" && LISTENER_OK.load(Ordering::SeqCst) {
        hide_toast(hwnd);
        return;
    }
    if HIDDEN_TOAST.load(Ordering::SeqCst) == hwnd.0 as isize {
        restore_native_toasts();
    }
    let corner = position(app);
    if corner != "default" {
        move_toast(hwnd, &corner);
    }
}

fn apply_now() {
    match find_toast_window() {
        Some(hwnd) => apply_to_toast(hwnd),
        None => restore_native_toasts(),
    }
}

/// Called when a Settings > Notifications value changes.
pub fn settings_changed() {
    std::thread::spawn(apply_now);
}

unsafe extern "system" fn on_win_event(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // OBJID_WINDOW = 0: the window itself, not a caret or cursor.
    if id_object != 0 || hwnd.0.is_null() {
        return;
    }
    if class_name(hwnd) != "Windows.UI.Core.CoreWindow" || !is_toast_window(hwnd) {
        return;
    }
    apply_to_toast(hwnd);
    // Windows is showing (or changing) a pop-up: a notification is due now.
    wake_listener();
}

/// Watches for Windows' pop-up appearing or being moved, on a thread of its own.
fn start_toast_watcher() {
    std::thread::spawn(|| unsafe {
        use windows::Win32::UI::Accessibility::SetWinEventHook;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, TranslateMessage, EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_SHOW, MSG, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        };
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let _show = SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            None,
            Some(on_win_event),
            0,
            0,
            flags,
        );
        let _moved = SetWinEventHook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(on_win_event),
            0,
            0,
            flags,
        );
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

// ---------------------------------------------------------------- Bloom cards

#[derive(Clone, Serialize)]
pub struct BloomNotification {
    id: u32,
    app: String,
    aumid: String,
    icon: Option<String>,
    title: String,
    body: String,
    /// When it arrived, in milliseconds since 1970.
    time: i64,
    /// Its buttons and reply boxes, as on Windows' own pop-up.
    buttons: Vec<crate::toast::ToastButton>,
    inputs: Vec<crate::toast::ToastInput>,
}

/// Adds each notification's buttons and reply boxes, read from Windows'
/// notification database in one go, and returns the content found.
fn attach_content(items: &mut [BloomNotification]) -> HashMap<u32, crate::toast::ToastContent> {
    if items.is_empty() {
        return HashMap::new();
    }
    let wanted: Vec<crate::toast::Wanted> = items
        .iter()
        .map(|item| crate::toast::Wanted {
            id: item.id,
            aumid: &item.aumid,
            time: item.time,
        })
        .collect();
    let content = crate::toast::content_for(&wanted);
    for item in items.iter_mut() {
        if let Some(found) = content.get(&item.id) {
            item.buttons = found.buttons.clone();
            item.inputs = found.inputs.clone();
        }
    }
    content
}

fn hstring(value: windows::core::Result<windows::core::HSTRING>) -> String {
    value.map(|s| s.to_string()).unwrap_or_default()
}

static ICONS: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

fn app_icon(aumid: &str) -> Option<String> {
    if aumid.is_empty() {
        return None;
    }
    let cache = ICONS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(icon) = cache.lock().ok()?.get(aumid) {
        return icon.clone();
    }
    let icon = crate::commands::icon_from_aumid(aumid);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(aumid.to_string(), icon.clone());
    }
    icon
}

fn read_notification(
    notification: &windows::UI::Notifications::UserNotification,
) -> Option<BloomNotification> {
    use windows::UI::Notifications::KnownNotificationBindings;
    let id = notification.Id().ok()?;
    let info = notification.AppInfo().ok();
    let app = info
        .as_ref()
        .and_then(|i| i.DisplayInfo().ok())
        .map(|d| hstring(d.DisplayName()))
        .unwrap_or_default();
    let aumid = info
        .as_ref()
        .map(|i| hstring(i.AppUserModelId()))
        .unwrap_or_default();
    let binding = notification
        .Notification()
        .ok()?
        .Visual()
        .ok()?
        .GetBinding(&KnownNotificationBindings::ToastGeneric().ok()?)
        .ok()?;
    let texts: Vec<String> = binding
        .GetTextElements()
        .ok()?
        .into_iter()
        .map(|t| hstring(t.Text()))
        .filter(|t| !t.trim().is_empty())
        .collect();
    let title = texts.first().cloned().unwrap_or_else(|| app.clone());
    let body = texts.iter().skip(1).cloned().collect::<Vec<_>>().join("\n");
    // Windows counts 100 ns steps from 1601; the cards want Unix milliseconds.
    let time = notification
        .CreationTime()
        .map(|t| (t.UniversalTime - 116_444_736_000_000_000) / 10_000)
        .unwrap_or_else(|_| crate::utils::get_now_ms());
    Some(BloomNotification {
        id,
        icon: app_icon(&aumid),
        app,
        aumid,
        title,
        body,
        time,
        buttons: Vec::new(),
        inputs: Vec::new(),
    })
}

fn read_dword(subkey: &str, value: &str) -> Option<u32> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let subkey = wide(subkey);
    let value = wide(value);
    let mut data = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    Some(data)
}

/// Whether Windows would have shown this one as a pop-up: not in Do not
/// disturb, not over a full-screen app, and not an app whose banners are off.
fn windows_would_show(item: &BloomNotification) -> bool {
    use windows::UI::Notifications::{ToastNotificationManager, ToastNotificationMode};
    let base = "Software\\Microsoft\\Windows\\CurrentVersion\\Notifications\\Settings";
    if read_dword(base, "NOC_GLOBAL_SETTING_TOASTS_ENABLED") == Some(0) {
        return false;
    }
    if let Ok(mode) = ToastNotificationManager::GetDefault().and_then(|m| m.NotificationMode()) {
        if mode != ToastNotificationMode::Unrestricted {
            return false;
        }
    }
    if crate::state::CURRENT_FOREGROUND_FULLSCREEN.load(Ordering::Relaxed) {
        return false;
    }
    if !item.aumid.is_empty() {
        let key = format!("{}\\{}", base, item.aumid);
        if read_dword(&key, "ShowBanner") == Some(0) || read_dword(&key, "Enabled") == Some(0) {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------- Bloom's history

/// A notification Bloom showed, as kept in Bloom's notification center.
#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    id: u32,
    app: String,
    aumid: String,
    title: String,
    body: String,
    time: i64,
    content: Option<crate::toast::SavedContent>,
}

/// Kept at most this many, for at most this long.
const HISTORY_MAX: usize = 200;
const HISTORY_DAYS: i64 = 14;

static HISTORY: Mutex<Option<Vec<Saved>>> = Mutex::new(None);

fn history_path() -> Option<std::path::PathBuf> {
    Some(
        APP.get()?
            .path()
            .app_config_dir()
            .ok()?
            .join("notification-history.json"),
    )
}

/// Runs `change` on the history (read from disk the first time), then saves
/// it when `change` says it changed anything.
fn with_history<T>(change: impl FnOnce(&mut Vec<Saved>) -> (T, bool)) -> T {
    let mut guard = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    let list = guard.get_or_insert_with(|| {
        history_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Vec<Saved>>(&text).ok())
            .unwrap_or_default()
    });
    let (result, changed) = change(list);
    let oldest = crate::utils::get_now_ms() - HISTORY_DAYS * 24 * 60 * 60 * 1000;
    let before = list.len();
    list.retain(|item| item.time >= oldest);
    list.sort_by_key(|item| std::cmp::Reverse(item.time));
    list.truncate(HISTORY_MAX);
    if changed || list.len() != before {
        if let (Some(path), Ok(text)) = (history_path(), serde_json::to_string(&*list)) {
            let _ = std::fs::write(path, text);
        }
    }
    result
}

fn center_changed() {
    if let Some(app) = APP.get() {
        let _ = app.emit_to("notification-center", "notification-center-changed", ());
    }
}

/// Keeps the notifications just shown as cards, with what clicking them does.
fn remember(items: &[BloomNotification], contents: &HashMap<u32, crate::toast::ToastContent>) {
    if items.is_empty() {
        return;
    }
    with_history(|list| {
        for item in items {
            list.retain(|saved| saved.id != item.id);
            list.push(Saved {
                id: item.id,
                app: item.app.clone(),
                aumid: item.aumid.clone(),
                title: item.title.clone(),
                body: item.body.clone(),
                time: item.time,
                content: contents.get(&item.id).map(crate::toast::SavedContent::from),
            });
        }
        ((), true)
    });
    center_changed();
}

fn forget(id: u32) {
    let removed = with_history(|list| {
        let before = list.len();
        list.retain(|saved| saved.id != id);
        let removed = list.len() != before;
        (removed, removed)
    });
    if removed {
        center_changed();
    }
}

fn forget_all() {
    with_history(|list| {
        let had = !list.is_empty();
        list.clear();
        ((), had)
    });
    center_changed();
}

/// What clicking a notification does: from Windows' database while Windows
/// still has it, else from Bloom's history.
fn content_of(id: u32, aumid: &str, time: i64) -> Option<crate::toast::ToastContent> {
    if let Some(found) =
        crate::toast::content_for(&[crate::toast::Wanted { id, aumid, time }]).remove(&id)
    {
        return Some(found);
    }
    with_history(|list| {
        let saved = list
            .iter()
            .find(|saved| saved.id == id)
            .and_then(|saved| saved.content.as_ref())
            .map(crate::toast::ToastContent::from);
        (saved, false)
    })
}

fn saved_to_card(saved: &Saved) -> BloomNotification {
    let content = saved.content.as_ref().map(crate::toast::ToastContent::from);
    BloomNotification {
        id: saved.id,
        icon: app_icon(&saved.aumid),
        app: saved.app.clone(),
        aumid: saved.aumid.clone(),
        title: saved.title.clone(),
        body: saved.body.clone(),
        time: saved.time,
        buttons: content
            .as_ref()
            .map(|c| c.buttons.clone())
            .unwrap_or_default(),
        inputs: content.map(|c| c.inputs).unwrap_or_default(),
    }
}

// ---------------------------------------------------------------- listening

/// Set when Windows shows or changes its pop-up, so the listener reads now.
static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
/// How long the listener waits when nothing happens.
const IDLE_POLL_MS: u64 = 4000;

fn wake_listener() {
    if let Ok(mut woken) = WAKE.0.lock() {
        *woken = true;
        WAKE.1.notify_one();
    }
}

/// Waits until woken or `timeout` passes; true when woken.
fn wait_for_wake(timeout: std::time::Duration) -> bool {
    let Ok(guard) = WAKE.0.lock() else {
        std::thread::sleep(timeout);
        return false;
    };
    let (mut woken, _) = WAKE
        .1
        .wait_timeout_while(guard, timeout, |woken| !*woken)
        .unwrap_or_else(|e| e.into_inner());
    std::mem::replace(&mut *woken, false)
}

enum PollError {
    Denied,
    Unavailable,
}

fn poll(seen: &mut HashSet<u32>, primed: &mut bool, app: &AppHandle) -> Result<(), PollError> {
    use windows::UI::Notifications::Management::{
        UserNotificationListener, UserNotificationListenerAccessStatus,
    };
    use windows::UI::Notifications::NotificationKinds;
    let listener = UserNotificationListener::Current().map_err(|_| PollError::Unavailable)?;
    let mut access = listener
        .GetAccessStatus()
        .map_err(|_| PollError::Unavailable)?;
    // Asked once per run: Windows answers from its privacy setting.
    if access != UserNotificationListenerAccessStatus::Allowed
        && !ACCESS_ASKED.swap(true, Ordering::SeqCst)
    {
        if let Ok(asked) = listener.RequestAccessAsync().and_then(|op| op.get()) {
            access = asked;
        }
    }
    if access != UserNotificationListenerAccessStatus::Allowed {
        return Err(PollError::Denied);
    }
    let list = listener
        .GetNotificationsAsync(NotificationKinds::Toast)
        .and_then(|op| op.get())
        .map_err(|_| PollError::Unavailable)?;
    let mut current = HashSet::new();
    let mut fresh = Vec::new();
    for notification in list {
        let Ok(id) = notification.Id() else {
            continue;
        };
        current.insert(id);
        if !*primed || seen.contains(&id) {
            continue;
        }
        if let Some(item) = read_notification(&notification) {
            if windows_would_show(&item) {
                fresh.push(item);
            }
        }
    }
    let contents = attach_content(&mut fresh);
    remember(&fresh, &contents);
    for item in fresh {
        let _ = app.emit_to("notifications", "bloom-notification", item);
    }
    *seen = current;
    *primed = true;
    Ok(())
}

fn start_listener(app: AppHandle) {
    std::thread::spawn(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let mut seen = HashSet::new();
        let mut primed = false;
        // Reads still to do soon after a wake: the listener can lag Windows'
        // pop-up by a moment, and a burst of notifications arrives together.
        // The first read happens straight away, so Windows' pop-up is
        // replaced from the start.
        let mut follow_ups: Vec<u64> = vec![0];
        loop {
            let woken = match follow_ups.pop() {
                Some(delay) => {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    false
                }
                None => wait_for_wake(std::time::Duration::from_millis(IDLE_POLL_MS)),
            };
            if woken {
                follow_ups = vec![1200, 500, 200];
            }
            if style(&app) != "bloom" {
                if LISTENER_OK.swap(false, Ordering::SeqCst) {
                    apply_now();
                }
                primed = false;
                set_status("off");
                continue;
            }
            match poll(&mut seen, &mut primed, &app) {
                Ok(()) => {
                    if !LISTENER_OK.swap(true, Ordering::SeqCst) {
                        apply_now();
                    }
                    set_status("ok");
                }
                Err(error) => {
                    if LISTENER_OK.swap(false, Ordering::SeqCst) {
                        apply_now();
                    }
                    primed = false;
                    follow_ups.clear();
                    set_status(match error {
                        PollError::Denied => "denied",
                        PollError::Unavailable => "unavailable",
                    });
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            }
        }
    });
}

pub fn start(app: AppHandle) {
    let _ = APP.set(app.clone());
    repair_leftover();
    start_toast_watcher();
    let uses_bloom = style(&app) == "bloom"
        || get_setting_str(&app, "bloom-notification-center").as_deref() == Some("bloom");
    if uses_bloom {
        crate::toast::prewarm();
    }
    start_listener(app);
    std::thread::spawn(apply_now);
}

// ---------------------------------------------------------------- commands

/// "off", "ok", "denied" (Windows' privacy setting) or "unavailable".
#[tauri::command]
pub fn notifications_status() -> String {
    STATUS.lock().map(|s| s.to_string()).unwrap_or_default()
}

/// Does what clicking the notification in Windows does (opens its page, its
/// app, or hands the app its arguments) and clears it from the notification
/// center, as Windows does.
#[tauri::command]
pub async fn notification_activate(
    id: u32,
    aumid: String,
    time: Option<i64>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let content = content_of(id, &aumid, time.unwrap_or_default());
        let result = match &content {
            Some(content) => crate::toast::activate(&aumid, content),
            None if aumid.is_empty() => {
                crate::commands::open_notification_center();
                Ok(())
            }
            None => crate::toast::open_app(&aumid),
        };
        // The app didn't take the click (its activator is gone, say): open
        // the app itself, which is what clicking still means.
        let result = match result {
            Err(_) if !aumid.is_empty() && !aumid.starts_with("Windows.SystemToast.") => {
                crate::toast::open_app(&aumid)
            }
            other => other,
        };
        if let Ok(listener) =
            windows::UI::Notifications::Management::UserNotificationListener::Current()
        {
            let _ = listener.RemoveNotification(id);
        }
        forget(id);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One of a notification's buttons, with the text typed into its reply box.
#[tauri::command]
pub async fn notification_action(
    id: u32,
    aumid: String,
    time: Option<i64>,
    button: usize,
    typed: HashMap<String, String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let content =
            content_of(id, &aumid, time.unwrap_or_default()).ok_or("This notification is gone")?;
        let result = crate::toast::press(&aumid, &content, button, &typed);
        if let Ok(listener) =
            windows::UI::Notifications::Management::UserNotificationListener::Current()
        {
            let _ = listener.RemoveNotification(id);
        }
        forget(id);
        result.map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sizes the card window to its cards and puts it in the chosen corner of
/// the main screen, without taking focus. Height 0 hides it.
#[tauri::command]
pub fn notifications_set_size(app: AppHandle, width: f64, height: f64) {
    use windows::Win32::UI::WindowsAndMessaging::{
        ShowWindow, HWND_TOPMOST, SWP_SHOWWINDOW, SW_HIDE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let Some(window) = app.get_webview_window("notifications") else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = HWND(hwnd.0);
    if height <= 0.0 {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let (w, h) = (
        (width * scale).round() as i32,
        (height * scale).round() as i32,
    );
    let corner = match position(&app).as_str() {
        "default" => "bottom-right".to_string(),
        other => other.to_string(),
    };
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let mut wanted = style | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize;
        if CARD_TYPING.load(Ordering::Relaxed) {
            wanted &= !(WS_EX_NOACTIVATE.0 as isize);
        }
        if style != wanted {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        }
        let primary = windows::Win32::Graphics::Gdi::MonitorFromPoint(
            Default::default(),
            MONITOR_DEFAULTTOPRIMARY,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(primary, &mut info).as_bool() {
            return;
        }
        let (x, y) = corner_origin(info.rcWork, w, h, &corner, (8.0 * scale) as i32);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// A reply box on a card is being typed in.
static CARD_TYPING: AtomicBool = AtomicBool::new(false);

/// The cards never take the keyboard, so typing a reply needs it for a moment.
#[tauri::command]
pub fn notifications_allow_typing(app: AppHandle, typing: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, WS_EX_NOACTIVATE};
    let Some(window) = app.get_webview_window("notifications") else {
        return;
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = HWND(hwnd.0);
    CARD_TYPING.store(typing, Ordering::Relaxed);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if typing {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(hwnd);
            let _ = window.set_focus();
        } else {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE.0 as isize);
        }
    }
}

/// A sample card, for trying the look from Settings.
#[tauri::command]
pub fn notifications_test(app: AppHandle) {
    let _ = app.emit_to(
        "notifications",
        "bloom-notification",
        BloomNotification {
            id: 0,
            app: "Bloom".into(),
            aumid: String::new(),
            icon: None,
            title: "This is how notifications look".into(),
            body: "Change the corner and the rounding in Settings > Notifications.".into(),
            time: crate::utils::get_now_ms(),
            buttons: Vec::new(),
            inputs: Vec::new(),
        },
    );
}

#[tauri::command]
pub fn notifications_open_privacy() {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let uri = wide("ms-settings:privacy-notifications");
    let verb = wide("open");
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(uri.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_sit_inside_the_work_area() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1032,
        };
        assert_eq!(corner_origin(area, 400, 100, "top-right", 0), (1520, 0));
        assert_eq!(corner_origin(area, 400, 100, "top-left", 8), (8, 8));
        assert_eq!(corner_origin(area, 400, 100, "bottom-left", 0), (0, 932));
        assert_eq!(
            corner_origin(area, 400, 100, "bottom-right", 0),
            (1520, 932)
        );
    }
}

// ---------------------------------------------------------------- Bloom notification center

fn with_listener<T>(
    f: impl FnOnce(
        &windows::UI::Notifications::Management::UserNotificationListener,
    ) -> Result<T, String>,
) -> Result<T, String> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::UI::Notifications::Management::{
        UserNotificationListener, UserNotificationListenerAccessStatus,
    };
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let listener = UserNotificationListener::Current().map_err(|_| "unavailable".to_string())?;
    if listener.GetAccessStatus().ok() != Some(UserNotificationListenerAccessStatus::Allowed) {
        return Err("denied".to_string());
    }
    f(&listener)
}

/// Everything in Windows' notification center, plus every notification
/// Bloom showed as a card that hasn't been clicked or cleared, newest first.
#[tauri::command]
pub async fn notification_center_list() -> Result<Vec<BloomNotification>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let windows_items = with_listener(|listener| {
            use windows::UI::Notifications::NotificationKinds;
            let list = listener
                .GetNotificationsAsync(NotificationKinds::Toast)
                .and_then(|op| op.get())
                .map_err(|_| "unavailable".to_string())?;
            let mut items: Vec<BloomNotification> = list
                .into_iter()
                .filter_map(|n| read_notification(&n))
                .collect();
            let contents = attach_content(&mut items);
            // Fill in what clicking does for cards saved before Windows had
            // written it down.
            with_history(|saved| {
                let mut changed = false;
                for entry in saved.iter_mut().filter(|e| e.content.is_none()) {
                    if let Some(found) = contents.get(&entry.id) {
                        entry.content = Some(crate::toast::SavedContent::from(found));
                        changed = true;
                    }
                }
                ((), changed)
            });
            Ok(items)
        });
        let saved: Vec<BloomNotification> =
            with_history(|list| (list.iter().map(saved_to_card).collect(), false));
        let mut items = match windows_items {
            Ok(items) => items,
            // Windows' list can't be read (its privacy setting): Bloom's own
            // history still is, unless there is none to show.
            Err(error) if saved.is_empty() => return Err(error),
            Err(_) => Vec::new(),
        };
        for card in saved {
            if !items.iter().any(|item| item.id == card.id) {
                items.push(card);
            }
        }
        items.sort_by_key(|item| std::cmp::Reverse(item.time));
        Ok(items)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn notification_remove(id: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        forget(id);
        // Gone from Windows already is fine: it was only in Bloom's history.
        let _ =
            with_listener(|listener| listener.RemoveNotification(id).map_err(|e| e.to_string()));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn notification_clear_all() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        forget_all();
        with_listener(|listener| listener.ClearNotifications().map_err(|e| e.to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The notification button opens Bloom's center instead of Windows' when
/// that is chosen in Settings > Notifications. Returns false to let Windows'
/// open.
pub fn try_open_bloom_center() -> bool {
    let Some(app) = APP.get() else {
        return false;
    };
    if get_setting_str(app, "bloom-notification-center").as_deref() != Some("bloom") {
        return false;
    }
    let Some(window) = app.get_webview_window("notification-center") else {
        return false;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return true;
    }
    // Clicking the bell while the center is open first hides it (it loses
    // focus); that click must not open it again.
    if crate::utils::get_now_ms() - CENTER_HIDDEN_AT.load(Ordering::SeqCst) < 400 {
        return true;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    unsafe {
        let primary = windows::Win32::Graphics::Gdi::MonitorFromPoint(
            Default::default(),
            MONITOR_DEFAULTTOPRIMARY,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(primary, &mut info).as_bool() {
            let work = info.rcWork;
            let margin = (8.0 * scale) as i32;
            let width = (392.0 * scale) as i32;
            // As tall as its notifications were last time; it fits itself to
            // the current ones as soon as they are shown.
            let height = (CENTER_HEIGHT.load(Ordering::Relaxed) as f64 * scale) as i32;
            let _ = window.set_size(tauri::PhysicalSize::new(
                width as u32,
                height.clamp(120, (work.bottom - work.top - margin * 2).max(120)) as u32,
            ));
            let _ = window.set_position(tauri::PhysicalPosition::new(
                work.right - width - margin,
                work.top + margin,
            ));
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to("notification-center", "notification-center-open", ());
    true
}

static CENTER_HIDDEN_AT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
/// The center's last height, in CSS pixels.
static CENTER_HEIGHT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(160);

/// Fits the center to its notifications: it hangs from the top of the screen
/// and is only as tall as what it shows (the list scrolls past about six).
#[tauri::command]
pub fn notification_center_fit(app: AppHandle, height: f64) {
    if !height.is_finite() || height <= 0.0 {
        return;
    }
    CENTER_HEIGHT.store(height.round() as u32, Ordering::Relaxed);
    let Some(window) = app.get_webview_window("notification-center") else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(size) = window.outer_size() else {
        return;
    };
    let mut limit = i32::MAX;
    unsafe {
        let primary = windows::Win32::Graphics::Gdi::MonitorFromPoint(
            Default::default(),
            MONITOR_DEFAULTTOPRIMARY,
        );
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(primary, &mut info).as_bool() {
            limit = info.rcWork.bottom - info.rcWork.top - (16.0 * scale) as i32;
        }
    }
    let wanted = ((height * scale).round() as i32).clamp(1, limit.max(120)) as u32;
    if size.height != wanted {
        let _ = window.set_size(tauri::PhysicalSize::new(size.width, wanted));
    }
}

#[tauri::command]
pub fn notification_center_hide(app: AppHandle) {
    if let Some(window) = app.get_webview_window("notification-center") {
        if window.is_visible().unwrap_or(false) {
            CENTER_HIDDEN_AT.store(crate::utils::get_now_ms(), Ordering::SeqCst);
        }
        let _ = window.hide();
    }
}
