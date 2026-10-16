//! Notifications (Settings > Notifications).
//!
//! Windows style keeps Windows' own pop-ups and can move them to another
//! corner. Bloom style shows them as Bloom cards (Bloom's colours and opacity,
//! adjustable rounding) in a window of their own and hides Windows' pop-up.
//!
//! Bloom reads the notifications with Windows' notification listener, polling
//! it because its change event needs a packaged app. Windows' pop-up is only
//! hidden once that listener works, and it is put back whenever Bloom style is
//! turned off, Bloom exits, or Bloom starts again.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
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
fn repair_leftover() {
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
                let _ = app.emit_to("notifications", "bloom-notification", item);
            }
        }
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
        loop {
            std::thread::sleep(std::time::Duration::from_millis(700));
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
    start_listener(app);
    std::thread::spawn(apply_now);
}

// ---------------------------------------------------------------- commands

/// "off", "ok", "denied" (Windows' privacy setting) or "unavailable".
#[tauri::command]
pub fn notifications_status() -> String {
    STATUS.lock().map(|s| s.to_string()).unwrap_or_default()
}

/// Opens the app a card came from and clears it from the notification
/// center, like clicking Windows' own pop-up.
#[tauri::command]
pub async fn notification_activate(id: u32, aumid: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if let Ok(listener) =
            windows::UI::Notifications::Management::UserNotificationListener::Current()
        {
            let _ = listener.RemoveNotification(id);
        }
        if aumid.is_empty() {
            crate::commands::open_notification_center();
            return Ok(());
        }
        let file = wide("explorer.exe");
        let args = wide(&format!("shell:AppsFolder\\{}", aumid));
        let verb = wide("open");
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(verb.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR(args.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as usize > 32 {
            Ok(())
        } else {
            Err("Couldn't open the app".to_string())
        }
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
        let wanted = style | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize;
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

/// Everything in Windows' notification center, newest first.
#[tauri::command]
pub async fn notification_center_list() -> Result<Vec<BloomNotification>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        with_listener(|listener| {
            use windows::UI::Notifications::NotificationKinds;
            let list = listener
                .GetNotificationsAsync(NotificationKinds::Toast)
                .and_then(|op| op.get())
                .map_err(|_| "unavailable".to_string())?;
            let mut items: Vec<BloomNotification> = list
                .into_iter()
                .filter_map(|n| read_notification(&n))
                .collect();
            items.sort_by_key(|item| std::cmp::Reverse(item.time));
            Ok(items)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn notification_remove(id: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_listener(|listener| listener.RemoveNotification(id).map_err(|e| e.to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn notification_clear_all() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
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
            let _ = window.set_size(tauri::PhysicalSize::new(
                width as u32,
                (work.bottom - work.top - margin * 2).max(200) as u32,
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

#[tauri::command]
pub fn notification_center_hide(app: AppHandle) {
    if let Some(window) = app.get_webview_window("notification-center") {
        if window.is_visible().unwrap_or(false) {
            CENTER_HIDDEN_AT.store(crate::utils::get_now_ms(), Ordering::SeqCst);
        }
        let _ = window.hide();
    }
}
