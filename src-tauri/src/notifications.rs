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

// ---------------------------------------------------------------- how long Windows' pop-up stays

/// Windows' hidden pop-up stays this long (seconds) while Bloom's cards
/// stand in for it: Windows' own longest choice (5 minutes). Apps built on
/// Electron (Discord, Slack, WhatsApp and many more) forget a notification
/// the moment its pop-up closes, by its time running out or being
/// dismissed: from then on a click, even in Windows' own notification
/// center, can only open the app, never the message. So the hidden pop-up is
/// kept after its card goes (time up), and a click on the notification in
/// Bloom's center within these minutes is a click on it, landing on the
/// exact message. Dismissing the card (its X) closes it at once.
const POPUP_SECONDS: u32 = 300;
/// The time earlier Bloom versions kept the pop-up (their value is Bloom's
/// too, never the user's).
const EARLIER_POPUP_SECONDS: [u32; 1] = [60];
/// SPI_GETMESSAGEDURATION / SPI_SETMESSAGEDURATION: Settings > Accessibility >
/// Visual effects > "Dismiss notifications after this amount of time".
const SPI_GETMESSAGEDURATION: u32 = 0x2016;
const SPI_SETMESSAGEDURATION: u32 = 0x2017;
/// Set once Windows refused to close a pop-up for Bloom: its own time is put
/// back for the rest of this run, so pop-ups never pile up.
static KEEP_POPUPS_OFF: AtomicBool = AtomicBool::new(false);

/// Where the user's own pop-up time is kept while Bloom's is in use (also
/// after a crash, until it is put back).
fn duration_backup() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(std::env::var_os("APPDATA")?)
        .join(crate::data_folders::IDENTIFIER);
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("notification-duration.txt"))
}

fn message_duration() -> Option<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SYSTEM_PARAMETERS_INFO_ACTION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut seconds = 0u32;
    unsafe {
        SystemParametersInfoW(
            SYSTEM_PARAMETERS_INFO_ACTION(SPI_GETMESSAGEDURATION),
            0,
            Some(&mut seconds as *mut u32 as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    (seconds > 0).then_some(seconds)
}

fn set_message_duration(seconds: u32) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SYSTEM_PARAMETERS_INFO_ACTION,
    };
    unsafe {
        // The value goes in the pointer argument itself.
        SystemParametersInfoW(
            SYSTEM_PARAMETERS_INFO_ACTION(SPI_SETMESSAGEDURATION),
            0,
            Some(seconds as usize as *mut _),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
        .is_ok()
    }
}

/// Lets Windows' hidden pop-up stay while Bloom's card does (the user's own
/// time is kept to put back). Only while Bloom's cards replace the pop-ups.
fn keep_popups_for_cards() {
    if KEEP_POPUPS_OFF.load(Ordering::Relaxed) {
        return;
    }
    let Some(backup) = duration_backup() else {
        return;
    };
    if backup.exists() {
        // Kept by an earlier Bloom, for a shorter time: Bloom's time now.
        if message_duration().is_some_and(|now| EARLIER_POPUP_SECONDS.contains(&now)) {
            set_message_duration(POPUP_SECONDS);
        }
        return;
    }
    let Some(own) = message_duration() else {
        return;
    };
    if own >= POPUP_SECONDS {
        return;
    }
    if std::fs::write(&backup, own.to_string()).is_ok() && !set_message_duration(POPUP_SECONDS) {
        let _ = std::fs::remove_file(&backup);
    }
}

/// Puts the user's own pop-up time back (Bloom style off, Bloom quitting, or
/// left over from a run that crashed).
fn put_back_popup_time() {
    let Some(backup) = duration_backup() else {
        return;
    };
    let Some(own) = std::fs::read_to_string(&backup)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
    else {
        return;
    };
    // Only Bloom's own value is replaced: one the user changed meanwhile stays.
    if message_duration()
        .is_some_and(|now| now == POPUP_SECONDS || EARLIER_POPUP_SECONDS.contains(&now))
    {
        set_message_duration(own);
    }
    let _ = std::fs::remove_file(&backup);
}

/// Bloom's card for a notification was dismissed (its X): Windows' hidden
/// pop-up for it closes too. A card whose time ran out leaves the pop-up up,
/// for clicks from Bloom's center (see `POPUP_SECONDS`).
#[tauri::command]
pub async fn notification_card_closed(title: String, body: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if on_windows_popup(&title, &body, PopupAct::Dismiss) == PopupOutcome::Failed {
            // Windows won't close it for Bloom: back to Windows' own time,
            // so hidden pop-ups never pile up and hold new ones back.
            KEEP_POPUPS_OFF.store(true, Ordering::Relaxed);
            put_back_popup_time();
            trace("  Windows' own pop-up time is used again for this run");
        }
    })
    .await;
}

/// Makes the pop-up invisible and click-through (the way TopNotify fades it).
fn hide_toast(hwnd: HWND) {
    keep_popups_for_cards();
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
    put_back_popup_time();
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
    put_back_popup_time();
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

/// Cards that came before the card window's page could show them (the window
/// is made a moment after Bloom starts), and whether that page is ready.
static CARDS_WAITING: Mutex<(bool, Vec<BloomNotification>)> = Mutex::new((false, Vec::new()));

/// Shows a card, or keeps it (the last few) until the card window's page is
/// ready, making the window if it isn't there yet.
fn show_card(app: &AppHandle, item: BloomNotification) {
    if let Ok(mut waiting) = CARDS_WAITING.lock() {
        if !waiting.0 {
            waiting.1.push(item);
            let extra = waiting.1.len().saturating_sub(5);
            waiting.1.drain(..extra);
            drop(waiting);
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = crate::later_windows::ensure(&app, "notifications");
            });
            return;
        }
    }
    let _ = app.emit_to("notifications", "bloom-notification", item);
}

/// The card window's page is listening: the cards that came before it.
#[tauri::command]
pub fn notifications_ready() -> Vec<BloomNotification> {
    match CARDS_WAITING.lock() {
        Ok(mut waiting) => {
            waiting.0 = true;
            std::mem::take(&mut waiting.1)
        }
        Err(_) => Vec::new(),
    }
}

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
        show_card(app, item);
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
    let uses_bloom = style(&app) == "bloom" || uses_bloom_center(&app);
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

// ---------------------------------------------------------------- clicking

/// What a notification click did, step by step, kept in
/// `notification-clicks.log` in Bloom's settings folder (the last few hundred
/// lines), to see why an app didn't open what a notification is about.
pub(crate) fn trace(line: &str) {
    use std::io::Write;
    let Some(path) = APP
        .get()
        .and_then(|app| app.path().app_config_dir().ok())
        .map(|dir| dir.join("notification-clicks.log"))
    else {
        return;
    };
    // Kept short: the newer half once it passes 64 KB.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let half = text.len() / 2;
            let start = text[half..].find('\n').map_or(half, |i| half + i + 1);
            let _ = std::fs::write(&path, &text[start..]);
        }
    }
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let stamp = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    );
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{stamp} {line}");
    }
}

/// Whether another app's window came to the front within `timeout_ms` of a
/// click (not Bloom's, not Windows' notification or Start windows).
fn came_forward(before: HWND, timeout_ms: u64) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let process = |hwnd: HWND| {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid
    };
    let started = std::time::Instant::now();
    let before_process = process(before);
    loop {
        let now = unsafe { GetForegroundWindow() };
        if !now.is_invalid() && now != before {
            let pid = process(now);
            let shell = unsafe { crate::commands::process_image_path(pid) }.is_some_and(|path| {
                let file = path.rsplit('\\').next().unwrap_or_default().to_lowercase();
                matches!(
                    file.as_str(),
                    "shellexperiencehost.exe" | "startmenuexperiencehost.exe" | "searchhost.exe"
                )
            });
            if pid != 0 && pid != std::process::id() && pid != before_process && !shell {
                return true;
            }
        }
        if started.elapsed() >= std::time::Duration::from_millis(timeout_ms) {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
}

/// Letters and digits only, lower case, for comparing texts.
fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A notification's title and first body line: from Bloom's history, else
/// from Windows.
fn texts_of(id: u32) -> Option<(String, String)> {
    let saved = with_history(|list| {
        let found = list
            .iter()
            .find(|saved| saved.id == id)
            .map(|saved| (saved.title.clone(), saved.body.clone()));
        (found, false)
    });
    if saved.is_some() {
        return saved;
    }
    with_listener(|listener| {
        let notification = listener.GetNotification(id).map_err(|e| e.to_string())?;
        read_notification(&notification)
            .map(|item| (item.title, item.body))
            .ok_or_else(|| "unreadable".to_string())
    })
    .ok()
}

/// Clicks the notification in Windows' own pop-up, which is still there,
/// invisible behind Bloom's card, for as long as Windows shows it: through
/// Windows' accessibility interface, which does exactly what clicking it
/// there does. Apps that take the click inside the running app instead of
/// being launched with it (Discord and other Electron apps) only answer this
/// way, and then open what the notification is about (the chat, the page).
fn click_windows_popup(title: &str, body: &str) -> bool {
    on_windows_popup(title, body, PopupAct::Click) == PopupOutcome::Done
}

/// What to do to a notification in Windows' own (hidden) pop-up.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PopupAct {
    /// Click it: the app opens what it is about.
    Click,
    /// Close it with its own dismiss button, as when Windows' time is up.
    Dismiss,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PopupOutcome {
    Done,
    /// It isn't in the pop-up (already gone, or never shown there).
    Gone,
    /// It is there, but Windows didn't take the click.
    Failed,
}

fn on_windows_popup(title: &str, body: &str, act: PopupAct) -> PopupOutcome {
    let Some(hwnd) = find_toast_window() else {
        trace("  Windows' pop-up: not there any more");
        return PopupOutcome::Gone;
    };
    act_in_window(hwnd, "Windows' pop-up", title, body, act)
}

/// Finds a notification in `hwnd` (Windows' pop-up, or its notification
/// center) by its title and first line, and clicks or dismisses it there,
/// through Windows' accessibility interface, which does exactly what a click
/// there does.
fn act_in_window(hwnd: HWND, place: &str, title: &str, body: &str, act: PopupAct) -> PopupOutcome {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
        IUIAutomationLegacyIAccessiblePattern, TreeScope_Descendants, UIA_InvokePatternId,
        UIA_LegacyIAccessiblePatternId, UIA_TextControlTypeId,
    };
    let title = squash(title);
    let body = squash(body.lines().next().unwrap_or_default());
    if title.is_empty() && body.is_empty() {
        return PopupOutcome::Gone;
    }
    unsafe {
        let Ok(automation) =
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
        else {
            return PopupOutcome::Failed;
        };
        let Ok(root) = automation.ElementFromHandle(hwnd) else {
            return PopupOutcome::Failed;
        };
        let Ok(condition) = automation.CreateTrueCondition() else {
            return PopupOutcome::Failed;
        };
        let Ok(elements) = root.FindAll(TreeScope_Descendants, &condition) else {
            trace(&format!("  {place}: its contents can't be read"));
            return PopupOutcome::Failed;
        };
        trace(&format!(
            "  {place}: {} parts",
            elements.Length().unwrap_or(0)
        ));
        // The notification itself: the outermost element that names it
        // (its title, and its text when it has some) and can be clicked;
        // never its own buttons (dismiss, settings, its actions).
        let mut best: Option<(u8, IUIAutomationElement)> = None;
        for index in 0..elements.Length().unwrap_or(0) {
            let Ok(element) = elements.GetElement(index) else {
                continue;
            };
            let id = element
                .CurrentAutomationId()
                .map(|s| s.to_string())
                .unwrap_or_default();
            if matches!(
                id.as_str(),
                "DismissButton"
                    | "VerbButton"
                    | "MoreButton"
                    | "SettingsButton"
                    | "ExpandButton"
                    | "CollapseButton"
                    | "SnoozeButton"
            ) {
                continue;
            }
            if element
                .CurrentControlType()
                .is_ok_and(|t| t == UIA_TextControlTypeId)
            {
                continue;
            }
            let name = squash(
                &element
                    .CurrentName()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            );
            let has_title = !title.is_empty() && name.contains(&title);
            let has_body = !body.is_empty() && name.contains(&body);
            if !has_title && !has_body {
                continue;
            }
            // The toast itself rather than a list item or panel around it.
            let class = element
                .CurrentClassName()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let toast = class.contains("Toast") || id.contains("Toast");
            let score = has_title as u8 + has_body as u8 + 2 * toast as u8;
            let invokable = element
                .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                .is_ok();
            trace(&format!(
                "    part: type {} class \"{}\" id \"{}\" title {} text {} invoke {}",
                element
                    .CurrentControlType()
                    .map(|t| t.0)
                    .unwrap_or_default(),
                class,
                id,
                has_title,
                has_body,
                invokable
            ));
            if best.as_ref().is_some_and(|(top, _)| *top >= score) {
                continue;
            }
            let clickable = element
                .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                .is_ok()
                || element
                    .GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                        UIA_LegacyIAccessiblePatternId,
                    )
                    .is_ok_and(|p| p.CurrentDefaultAction().is_ok_and(|a| !a.is_empty()));
            if clickable {
                best = Some((score, element));
            }
        }
        let Some((_, element)) = best else {
            trace(&format!("  {place}: this notification isn't in it"));
            return PopupOutcome::Gone;
        };
        if act == PopupAct::Dismiss {
            return dismiss_in_popup(&automation, &element);
        }
        // The app may bring its window forward, as from Windows' pop-up.
        crate::foreground::grant();
        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        {
            if pattern.Invoke().is_ok() {
                trace(&format!("  {place}: clicked"));
                return PopupOutcome::Done;
            }
        }
        let done = element
            .GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                UIA_LegacyIAccessiblePatternId,
            )
            .is_ok_and(|p| p.DoDefaultAction().is_ok());
        trace(&if done {
            format!("  {place}: clicked (default action)")
        } else {
            format!("  {place}: the click didn't go through")
        });
        if done {
            PopupOutcome::Done
        } else {
            PopupOutcome::Failed
        }
    }
}

/// Presses the dismiss button of the notification `element` stands for: the
/// nearest one at or above it (the button sits beside the notification's
/// text, inside the notification).
unsafe fn dismiss_in_popup(
    automation: &windows::Win32::UI::Accessibility::IUIAutomation,
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
) -> PopupOutcome {
    use windows::core::Interface;
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        IUIAutomationInvokePattern, TreeScope_Descendants, UIA_AutomationIdPropertyId,
        UIA_InvokePatternId,
    };
    let Ok(condition) = automation
        .CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from("DismissButton"))
    else {
        return PopupOutcome::Failed;
    };
    let Ok(walker) = automation.RawViewWalker() else {
        return PopupOutcome::Failed;
    };
    let mut current = Some(element.clone());
    for _ in 0..4 {
        let Some(node) = current.take() else {
            break;
        };
        if let Ok(button) = node.FindFirst(TreeScope_Descendants, &condition) {
            if let Ok(pattern) =
                button.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            {
                let done = pattern.Invoke().is_ok();
                trace(if done {
                    "  Windows' pop-up: dismissed"
                } else {
                    "  Windows' pop-up: its dismiss button didn't respond"
                });
                return if done {
                    PopupOutcome::Done
                } else {
                    PopupOutcome::Failed
                };
            }
        }
        current = walker
            .GetParentElement(&node)
            .ok()
            .filter(|p| p.as_raw() != node.as_raw());
    }
    trace("  Windows' pop-up: no dismiss button found");
    PopupOutcome::Failed
}

/// Whether Windows still has the notification (in its notification center).
fn windows_still_has(id: u32) -> bool {
    windows::UI::Notifications::Management::UserNotificationListener::Current()
        .and_then(|listener| listener.GetNotification(id))
        .is_ok()
}

/// Does what clicking the notification in Windows does (opens what it is
/// about: its chat or page, or its app with its arguments) and clears it from
/// the notification center, as Windows does. While Windows' own pop-up still
/// shows it, that pop-up is clicked, which works for every app; later Bloom
/// does what the notification says clicking does, or opens its app.
#[tauri::command]
pub async fn notification_activate(
    id: u32,
    aumid: String,
    time: Option<i64>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let before = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        let opens_an_app = !aumid.is_empty() && !aumid.starts_with("Windows.SystemToast.");
        trace(&format!("click: notification {id} from {aumid}"));
        // Windows' own pop-up first; it counts once something came forward
        // (an app that takes its clicks inside, like Discord, opens the chat).
        let texts = texts_of(id);
        let clicked_in_windows = texts
            .as_ref()
            .is_some_and(|(title, body)| click_windows_popup(title, body));
        // Windows' own notification center is never opened for a click:
        // with Bloom's center in use, Windows' must not show at all.
        if !clicked_in_windows {
            trace(if windows_still_has(id) {
                "  Windows still has it (not clicked in Windows' center)"
            } else {
                "  Windows no longer has it (the app took it back)"
            });
        }
        let opened_by_windows = clicked_in_windows && came_forward(before, 1800);
        if clicked_in_windows {
            trace(&format!("  app came forward: {opened_by_windows}"));
        }
        let result = if opened_by_windows {
            Ok(())
        } else {
            let content = content_of(id, &aumid, time.unwrap_or_default());
            match &content {
                Some(c) => trace(&format!(
                    "  what clicking does: launch \"{}\" activation \"{}\"",
                    c.launch, c.activation
                )),
                None => trace("  what clicking does: not found in Windows' notification store"),
            }
            let result = match &content {
                Some(content) => crate::toast::activate(&aumid, content),
                None if aumid.is_empty() => {
                    crate::commands::open_notification_center();
                    Ok(())
                }
                None => crate::toast::open_app(&aumid),
            };
            match result {
                // The app said yes but nothing opened (an activator that does
                // nothing for this one), or it didn't take the click at all:
                // open the app itself, which is what clicking still means.
                Ok(()) if opens_an_app && content.is_some() && !came_forward(before, 1500) => {
                    crate::toast::open_app(&aumid)
                }
                Err(_) if opens_an_app => crate::toast::open_app(&aumid),
                other => other,
            }
        };
        trace(&format!("  result: {result:?}"));
        // Opened another way: Windows' hidden pop-up for it closes now too.
        if !clicked_in_windows {
            if let Some((title, body)) = texts_of(id) {
                on_windows_popup(&title, &body, PopupAct::Dismiss);
            }
        }
        // Cleared once it did something, as in Windows; a click that did
        // nothing leaves it in the center.
        if result.is_ok() {
            if let Ok(listener) =
                windows::UI::Notifications::Management::UserNotificationListener::Current()
            {
                let _ = listener.RemoveNotification(id);
            }
            forget(id);
        }
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
        if let Some((title, body)) = texts_of(id) {
            on_windows_popup(&title, &body, PopupAct::Dismiss);
        }
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
/// A notice from Bloom itself (a Bluetooth device's battery running low), as
/// a card in the chosen corner.
pub fn show_bloom_notice(app: &AppHandle, title: &str, body: &str) {
    show_card(
        app,
        BloomNotification {
            id: 0,
            app: "Bloom".into(),
            aumid: String::new(),
            icon: None,
            title: title.into(),
            body: body.into(),
            time: crate::utils::get_now_ms(),
            buttons: Vec::new(),
            inputs: Vec::new(),
        },
    );
}

#[tauri::command]
pub fn notifications_test(app: AppHandle) {
    show_card(
        &app,
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

/// Bloom's center is the one the bell opens: when chosen, and by default
/// while notifications are Bloom cards, so every card is there too (apps such
/// as Discord take theirs out of Windows' center as soon as they go).
fn uses_bloom_center(app: &AppHandle) -> bool {
    match get_setting_str(app, "bloom-notification-center").as_deref() {
        Some("bloom") => true,
        Some("windows") => false,
        _ => style(app) == "bloom",
    }
}

/// The notification button opens Bloom's center instead of Windows' when
/// that is chosen in Settings > Notifications. Returns false to let Windows'
/// open.
pub fn try_open_bloom_center() -> bool {
    let Some(app) = APP.get() else {
        return false;
    };
    if !uses_bloom_center(app) {
        return false;
    }
    let Some(window) = app.get_webview_window("notification-center") else {
        // Not made yet (just after start): made now, and opened.
        let app = app.clone();
        std::thread::spawn(move || {
            if crate::later_windows::ensure(&app, "notification-center").is_some() {
                std::thread::sleep(std::time::Duration::from_millis(150));
                try_open_bloom_center();
            }
        });
        return true;
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
