//! What a Windows notification does when it is clicked, and its buttons.
//!
//! Windows' notification listener hands out a notification's text but not its
//! content: the launch arguments, the buttons and reply boxes. Windows keeps
//! that content in its notification database, which this reads (read only)
//! with Windows' own SQLite, `winsqlite3.dll`. Clicking a notification or one
//! of its buttons then activates it the way Windows does:
//! - "protocol" notifications open their link (Settings pages, `https:` and
//!   app links);
//! - desktop apps that registered a notification activator get its COM call
//!   with the arguments and any typed reply, exactly as from Windows' own pop-up;
//! - Store apps are activated with the arguments;
//! - anything else opens its app, as before.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use windows::core::{GUID, HSTRING, PCWSTR};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A button on a notification.
#[derive(Clone, Debug, Serialize)]
pub struct ToastButton {
    pub label: String,
    /// The text box whose text this button sends, if any.
    pub input: Option<String>,
    #[serde(skip)]
    arguments: String,
    #[serde(skip)]
    activation: String,
}

/// A text box on a notification (quick reply).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToastInput {
    pub id: String,
    pub placeholder: String,
}

#[derive(Clone, Debug, Default)]
pub struct ToastContent {
    launch: String,
    activation: String,
    pub buttons: Vec<ToastButton>,
    pub inputs: Vec<ToastInput>,
}

/// A notification's content as Bloom keeps it with its notification history,
/// so clicking it still works after Windows has let go of it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavedContent {
    launch: String,
    activation: String,
    buttons: Vec<SavedButton>,
    inputs: Vec<ToastInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedButton {
    label: String,
    input: Option<String>,
    arguments: String,
    activation: String,
}

impl From<&ToastContent> for SavedContent {
    fn from(content: &ToastContent) -> Self {
        SavedContent {
            launch: content.launch.clone(),
            activation: content.activation.clone(),
            buttons: content
                .buttons
                .iter()
                .map(|b| SavedButton {
                    label: b.label.clone(),
                    input: b.input.clone(),
                    arguments: b.arguments.clone(),
                    activation: b.activation.clone(),
                })
                .collect(),
            inputs: content.inputs.clone(),
        }
    }
}

impl From<&SavedContent> for ToastContent {
    fn from(saved: &SavedContent) -> Self {
        ToastContent {
            launch: saved.launch.clone(),
            activation: saved.activation.clone(),
            buttons: saved
                .buttons
                .iter()
                .map(|b| ToastButton {
                    label: b.label.clone(),
                    input: b.input.clone(),
                    arguments: b.arguments.clone(),
                    activation: b.activation.clone(),
                })
                .collect(),
            inputs: saved.inputs.clone(),
        }
    }
}

// ---------------------------------------------------------------- reading the database

type Db = *mut c_void;
type Stmt = *mut c_void;

type OpenFn = unsafe extern "system" fn(*const u8, *mut Db, i32, *const u8) -> i32;
type PrepareFn = unsafe extern "system" fn(Db, *const u8, i32, *mut Stmt, *mut *const u8) -> i32;
type StepFn = unsafe extern "system" fn(Stmt) -> i32;
type ColumnIntFn = unsafe extern "system" fn(Stmt, i32) -> i64;
type ColumnPtrFn = unsafe extern "system" fn(Stmt, i32) -> *const u8;
type ColumnBytesFn = unsafe extern "system" fn(Stmt, i32) -> i32;
type CloseFn = unsafe extern "system" fn(Db) -> i32;
type BusyFn = unsafe extern "system" fn(Db, i32) -> i32;
type AnyFn = unsafe extern "system" fn() -> isize;

struct Sqlite {
    open_v2: OpenFn,
    prepare_v2: PrepareFn,
    step: StepFn,
    column_int64: ColumnIntFn,
    column_blob: ColumnPtrFn,
    column_bytes: ColumnBytesFn,
    column_text: ColumnPtrFn,
    finalize: StepFn,
    close_v2: CloseFn,
    busy_timeout: BusyFn,
}

const SQLITE_OK: i32 = 0;
const SQLITE_ROW: i32 = 100;
const SQLITE_OPEN_READONLY: i32 = 0x1;

fn sqlite() -> Option<&'static Sqlite> {
    static API: OnceLock<Option<Sqlite>> = OnceLock::new();
    API.get_or_init(|| unsafe { load_sqlite() }).as_ref()
}

unsafe fn load_sqlite() -> Option<Sqlite> {
    use windows::core::PCSTR;
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    let module = LoadLibraryExW(
        windows::core::w!("winsqlite3.dll"),
        None,
        LOAD_LIBRARY_SEARCH_SYSTEM32,
    )
    .ok()?;
    macro_rules! symbol {
        ($name:literal, $kind:ty) => {
            std::mem::transmute::<AnyFn, $kind>(GetProcAddress(
                module,
                PCSTR(concat!($name, "\0").as_ptr()),
            )?)
        };
    }
    Some(Sqlite {
        open_v2: symbol!("sqlite3_open_v2", OpenFn),
        prepare_v2: symbol!("sqlite3_prepare_v2", PrepareFn),
        step: symbol!("sqlite3_step", StepFn),
        column_int64: symbol!("sqlite3_column_int64", ColumnIntFn),
        column_blob: symbol!("sqlite3_column_blob", ColumnPtrFn),
        column_bytes: symbol!("sqlite3_column_bytes", ColumnBytesFn),
        column_text: symbol!("sqlite3_column_text", ColumnPtrFn),
        finalize: symbol!("sqlite3_finalize", StepFn),
        close_v2: symbol!("sqlite3_close_v2", CloseFn),
        busy_timeout: symbol!("sqlite3_busy_timeout", BusyFn),
    })
}

/// One stored toast: its database id, app, arrival time and XML.
struct Row {
    id: i64,
    aumid: String,
    /// Milliseconds since 1970.
    arrived: i64,
    xml: String,
}

fn database_path() -> Option<std::path::PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let path = std::path::PathBuf::from(local)
        .join("Microsoft")
        .join("Windows")
        .join("Notifications")
        .join("wpndatabase.db");
    path.is_file().then_some(path)
}

/// Decodes a stored payload: UTF-8, or UTF-16 with or without a byte order mark.
fn payload_text(bytes: &[u8]) -> String {
    let utf16 = bytes.starts_with(&[0xFF, 0xFE])
        || (bytes.len() >= 4 && bytes[1] == 0 && bytes[3] == 0 && bytes[0] == b'<');
    if utf16 {
        let start = if bytes.starts_with(&[0xFF, 0xFE]) {
            2
        } else {
            0
        };
        let units: Vec<u16> = bytes[start..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let text = String::from_utf8_lossy(bytes);
    text.trim_start_matches('\u{feff}').to_string()
}

unsafe fn query_rows(api: &Sqlite, path: &std::path::Path) -> Result<Vec<Row>, i32> {
    let mut location = path.to_string_lossy().into_owned().into_bytes();
    location.push(0);
    let mut db: Db = std::ptr::null_mut();
    let status = (api.open_v2)(
        location.as_ptr(),
        &mut db,
        SQLITE_OPEN_READONLY,
        std::ptr::null(),
    );
    if status != SQLITE_OK {
        if !db.is_null() {
            (api.close_v2)(db);
        }
        return Err(status);
    }
    (api.busy_timeout)(db, 400);
    let sql = b"SELECT n.Id, h.PrimaryId, n.ArrivalTime, n.Payload FROM Notification n \
JOIN NotificationHandler h ON h.RecordId = n.HandlerId WHERE lower(n.Type) = 'toast'\0";
    let mut stmt: Stmt = std::ptr::null_mut();
    let status = (api.prepare_v2)(db, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut());
    if status != SQLITE_OK {
        (api.close_v2)(db);
        return Err(status);
    }
    let mut rows = Vec::new();
    let mut status;
    loop {
        status = (api.step)(stmt);
        if status != SQLITE_ROW {
            break;
        }
        let aumid_ptr = (api.column_text)(stmt, 1);
        let aumid = if aumid_ptr.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(aumid_ptr as *const std::ffi::c_char)
                .to_string_lossy()
                .into_owned()
        };
        let length = (api.column_bytes)(stmt, 3).max(0) as usize;
        let blob = (api.column_blob)(stmt, 3);
        let xml = if blob.is_null() || length == 0 {
            String::new()
        } else {
            payload_text(std::slice::from_raw_parts(blob, length))
        };
        // Windows counts 100 ns steps from 1601.
        let filetime = (api.column_int64)(stmt, 2);
        rows.push(Row {
            id: (api.column_int64)(stmt, 0),
            aumid,
            arrived: (filetime - 116_444_736_000_000_000) / 10_000,
            xml,
        });
    }
    (api.finalize)(stmt);
    (api.close_v2)(db);
    // 101 = SQLITE_DONE
    if status == 101 {
        Ok(rows)
    } else {
        Err(status)
    }
}

/// Every stored toast. When Windows has the database locked, a copy is read.
fn stored_toasts() -> Vec<Row> {
    let (Some(api), Some(path)) = (sqlite(), database_path()) else {
        return Vec::new();
    };
    if let Ok(rows) = unsafe { query_rows(api, &path) } {
        return rows;
    }
    let copy_dir = std::env::temp_dir().join("bloom-notifications");
    if std::fs::create_dir_all(&copy_dir).is_err() {
        return Vec::new();
    }
    let copy = copy_dir.join("wpndatabase.db");
    let wal = path.with_file_name("wpndatabase.db-wal");
    let copy_wal = copy_dir.join("wpndatabase.db-wal");
    let _ = std::fs::remove_file(copy_dir.join("wpndatabase.db-shm"));
    let _ = std::fs::remove_file(&copy_wal);
    if std::fs::copy(&path, &copy).is_err() {
        return Vec::new();
    }
    if wal.is_file() {
        let _ = std::fs::copy(&wal, &copy_wal);
    }
    let rows = unsafe { query_rows(api, &copy) }.unwrap_or_default();
    let _ = std::fs::remove_file(&copy);
    let _ = std::fs::remove_file(&copy_wal);
    let _ = std::fs::remove_file(copy_dir.join("wpndatabase.db-shm"));
    rows
}

fn parse(xml: &str) -> Option<ToastContent> {
    use windows::core::Interface;
    use windows::Data::Xml::Dom::{XmlDocument, XmlElement};
    let document = XmlDocument::new().ok()?;
    document.LoadXml(&HSTRING::from(xml)).ok()?;
    let attribute = |element: &XmlElement, name: &str| {
        element
            .GetAttribute(&HSTRING::from(name))
            .map(|v| v.to_string())
            .unwrap_or_default()
    };
    let toast: XmlElement = document
        .SelectSingleNode(&HSTRING::from("/toast"))
        .ok()?
        .cast()
        .ok()?;
    let mut content = ToastContent {
        launch: attribute(&toast, "launch"),
        activation: attribute(&toast, "activationType"),
        ..Default::default()
    };
    if let Ok(inputs) = document.SelectNodes(&HSTRING::from("/toast/actions/input")) {
        for index in 0..inputs.Length().unwrap_or(0) {
            let Some(input) = inputs
                .Item(index)
                .ok()
                .and_then(|n| n.cast::<XmlElement>().ok())
            else {
                continue;
            };
            if attribute(&input, "type") != "text" {
                continue;
            }
            content.inputs.push(ToastInput {
                id: attribute(&input, "id"),
                placeholder: attribute(&input, "placeHolderContent"),
            });
        }
    }
    if let Ok(actions) = document.SelectNodes(&HSTRING::from("/toast/actions/action")) {
        for index in 0..actions.Length().unwrap_or(0) {
            let Some(action) = actions
                .Item(index)
                .ok()
                .and_then(|n| n.cast::<XmlElement>().ok())
            else {
                continue;
            };
            // Right-click menu entries aren't buttons on the pop-up either.
            if attribute(&action, "placement") == "contextMenu" {
                continue;
            }
            let label = attribute(&action, "content");
            if label.trim().is_empty() {
                continue;
            }
            let input = attribute(&action, "hint-inputId");
            content.buttons.push(ToastButton {
                label,
                input: (!input.is_empty()).then_some(input),
                arguments: attribute(&action, "arguments"),
                activation: attribute(&action, "activationType"),
            });
        }
    }
    content.buttons.truncate(5);
    Some(content)
}

static CONTENT: OnceLock<Mutex<HashMap<u32, Option<ToastContent>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<u32, Option<ToastContent>>> {
    CONTENT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The listener's notification `id` from `aumid`, created at `time` (ms).
pub struct Wanted<'a> {
    pub id: u32,
    pub aumid: &'a str,
    pub time: i64,
}

/// The stored content of each wanted notification, by its listener id.
/// Notifications already looked up are answered from memory.
pub fn content_for(wanted: &[Wanted]) -> HashMap<u32, ToastContent> {
    let mut found = HashMap::new();
    let missing: Vec<&Wanted> = {
        let Ok(cache) = cache().lock() else {
            return found;
        };
        wanted
            .iter()
            .filter(|w| match cache.get(&w.id) {
                Some(Some(content)) => {
                    found.insert(w.id, content.clone());
                    false
                }
                Some(None) => false,
                None => true,
            })
            .collect()
    };
    if missing.is_empty() {
        return found;
    }
    let rows = stored_toasts();
    let Ok(mut cache) = cache().lock() else {
        return found;
    };
    for want in missing {
        // The database id is the listener's id; arrival time and app are the
        // check, and the fallback should the ids ever differ.
        let row = rows
            .iter()
            .find(|r| r.id == want.id as i64 && r.aumid.eq_ignore_ascii_case(want.aumid))
            .or_else(|| {
                rows.iter()
                    .filter(|r| r.aumid.eq_ignore_ascii_case(want.aumid))
                    .filter(|r| (r.arrived - want.time).abs() <= 3000)
                    .min_by_key(|r| (r.arrived - want.time).abs())
            });
        let content = row.and_then(|r| parse(&r.xml));
        if let Some(content) = &content {
            found.insert(want.id, content.clone());
        }
        // Not found yet (Windows writes it a moment later): looked up again
        // next time rather than remembered as missing.
        if content.is_some() || row.is_some() {
            cache.insert(want.id, content);
        }
    }
    // Ids are never reused within a session; keep memory bounded anyway.
    if cache.len() > 512 {
        cache.clear();
    }
    found
}

// ---------------------------------------------------------------- activating

/// What clicking the notification itself does.
pub fn activate(aumid: &str, content: &ToastContent) -> Result<(), String> {
    run(aumid, &content.launch, &content.activation, &[])
}

/// What one of its buttons does; `typed` holds the text of its reply boxes.
pub fn press(
    aumid: &str,
    content: &ToastContent,
    button: usize,
    typed: &HashMap<String, String>,
) -> Result<bool, String> {
    let button = content.buttons.get(button).ok_or("That button is gone")?;
    // Windows' own "dismiss" and "snooze" only close the notification.
    if button.activation == "system" {
        return Ok(false);
    }
    let inputs: Vec<(String, String)> = content
        .inputs
        .iter()
        .map(|i| (i.id.clone(), typed.get(&i.id).cloned().unwrap_or_default()))
        .collect();
    run(aumid, &button.arguments, &button.activation, &inputs)?;
    Ok(true)
}

fn looks_like_link(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    !rest.is_empty()
        && scheme.len() > 1
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
}

fn run(
    aumid: &str,
    arguments: &str,
    activation: &str,
    inputs: &[(String, String)],
) -> Result<(), String> {
    if activation == "protocol" {
        return open_link(arguments);
    }
    if let Some(clsid) = activator(aumid) {
        return unsafe { com_activate(&clsid, aumid, arguments, inputs) };
    }
    if aumid.contains('!') {
        return unsafe { activate_package(aumid, arguments) };
    }
    if looks_like_link(arguments) {
        return open_link(arguments);
    }
    // Windows' own notices ("Windows.SystemToast.…") aren't apps to open.
    if aumid.starts_with("Windows.SystemToast.") {
        return Ok(());
    }
    open_app(aumid)
}

fn shell_open(file: &str, parameters: Option<&str>) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    crate::foreground::grant();
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = wide("open");
    let file = wide(file);
    let parameters = parameters.map(wide);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            parameters
                .as_ref()
                .map(|p| PCWSTR(p.as_ptr()))
                .unwrap_or(PCWSTR::null()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err("Windows couldn't open it".to_string())
    }
}

fn open_link(link: &str) -> Result<(), String> {
    if !looks_like_link(link) || link.to_ascii_lowercase().starts_with("file:") {
        return Err("This notification has nothing to open".to_string());
    }
    shell_open(link, None)
}

/// Opens the notification's app, as Bloom did before it knew more.
pub fn open_app(aumid: &str) -> Result<(), String> {
    if aumid.is_empty() {
        return Err("This notification has no app".to_string());
    }
    shell_open(
        "explorer.exe",
        Some(&format!("shell:AppsFolder\\{}", aumid)),
    )
}

unsafe fn com_activate(
    clsid: &GUID,
    aumid: &str,
    arguments: &str,
    inputs: &[(String, String)],
) -> Result<(), String> {
    use windows::Win32::System::Com::{
        CoAllowSetForegroundWindow, CoCreateInstance, CLSCTX_LOCAL_SERVER,
    };
    use windows::Win32::UI::Notifications::{
        INotificationActivationCallback, NOTIFICATION_USER_INPUT_DATA,
    };
    let callback: INotificationActivationCallback =
        CoCreateInstance(clsid, None, CLSCTX_LOCAL_SERVER)
            .map_err(|_| "The app didn't answer".to_string())?;
    // The app may bring its window forward, as it can from Windows' pop-up.
    crate::foreground::grant();
    let _ = CoAllowSetForegroundWindow(&callback, None);
    let keys: Vec<Vec<u16>> = inputs.iter().map(|(k, _)| wide(k)).collect();
    let values: Vec<Vec<u16>> = inputs.iter().map(|(_, v)| wide(v)).collect();
    let data: Vec<NOTIFICATION_USER_INPUT_DATA> = keys
        .iter()
        .zip(&values)
        .map(|(k, v)| NOTIFICATION_USER_INPUT_DATA {
            Key: PCWSTR(k.as_ptr()),
            Value: PCWSTR(v.as_ptr()),
        })
        .collect();
    let aumid = wide(aumid);
    let arguments = wide(arguments);
    callback
        .Activate(PCWSTR(aumid.as_ptr()), PCWSTR(arguments.as_ptr()), &data)
        .map_err(|_| "The app didn't take it".to_string())
}

unsafe fn activate_package(aumid: &str, arguments: &str) -> Result<(), String> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
    use windows::Win32::UI::Shell::{
        ApplicationActivationManager, IApplicationActivationManager, AO_NONE,
    };
    let manager: IApplicationActivationManager =
        CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
            .map_err(|e| e.to_string())?;
    crate::foreground::grant();
    let aumid_w = wide(aumid);
    let arguments = wide(arguments);
    match manager.ActivateApplication(
        PCWSTR(aumid_w.as_ptr()),
        PCWSTR(arguments.as_ptr()),
        AO_NONE,
    ) {
        Ok(_) => Ok(()),
        Err(_) => open_app(aumid),
    }
}

// ---------------------------------------------------------------- finding an app's activator

fn read_registry_string(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    value: &str,
) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let key = wide(key);
    let value = wide(value);
    let mut buffer = [0u16; 128];
    let mut size = (buffer.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let length = (size as usize / 2).min(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..length]);
    Some(text.trim_end_matches('\0').trim().to_string())
}

fn parse_guid(text: &str) -> Option<GUID> {
    GUID::try_from(text.trim().trim_start_matches('{').trim_end_matches('}')).ok()
}

/// Apps say which COM class handles their notifications either in the
/// registry (`AppUserModelId\<id>\CustomActivator`) or on their Start menu
/// shortcut (System.AppUserModel.ToastActivatorCLSID).
fn activator(aumid: &str) -> Option<GUID> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    if aumid.is_empty() {
        return None;
    }
    let key = format!("Software\\Classes\\AppUserModelId\\{}", aumid);
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        if let Some(clsid) = read_registry_string(root, &key, "CustomActivator")
            .as_deref()
            .and_then(parse_guid)
        {
            return Some(clsid);
        }
    }
    shortcut_activators().get(&aumid.to_lowercase()).copied()
}

/// Reads the Start menu shortcuts ahead of the first click, so activating a
/// notification never waits for it.
pub fn prewarm() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(20));
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let _ = shortcut_activators();
        let _ = sqlite();
    });
}

/// AppUserModelID (lower case) to notification activator, from the Start
/// menu shortcuts. Read once, and again when apps were installed since.
fn shortcut_activators() -> HashMap<String, GUID> {
    static FOUND: Mutex<Option<(i64, HashMap<String, GUID>)>> = Mutex::new(None);
    let now = crate::utils::get_now_ms();
    if let Ok(found) = FOUND.lock() {
        if let Some((at, map)) = found.as_ref() {
            if now - at < 120_000 {
                return map.clone();
            }
        }
    }
    let map = scan_shortcuts();
    if let Ok(mut found) = FOUND.lock() {
        *found = Some((now, map.clone()));
    }
    map
}

fn scan_shortcuts() -> HashMap<String, GUID> {
    use windows::Win32::UI::Shell::{FOLDERID_CommonPrograms, FOLDERID_Programs};
    let mut map = HashMap::new();
    let mut folders = Vec::new();
    for id in [FOLDERID_Programs, FOLDERID_CommonPrograms] {
        if let Some(path) = known_folder(&id) {
            folders.push(path);
        }
    }
    let mut seen = 0usize;
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let is_shortcut = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
            if !is_shortcut {
                continue;
            }
            seen += 1;
            if seen > 2000 {
                return map;
            }
            if let Some((aumid, clsid)) = shortcut_activator(&path) {
                map.insert(aumid.to_lowercase(), clsid);
            }
        }
    }
    map
}

fn known_folder(id: &GUID) -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const c_void));
        text.map(std::path::PathBuf::from)
    }
}

fn shortcut_activator(path: &std::path::Path) -> Option<(String, GUID)> {
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToGUID, PropVariantToStringAlloc,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreFromParsingName, GPS_DEFAULT,
    };
    const APP_USER_MODEL: GUID = GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3);
    let id_key = PROPERTYKEY {
        fmtid: APP_USER_MODEL,
        pid: 5,
    };
    let activator_key = PROPERTYKEY {
        fmtid: APP_USER_MODEL,
        pid: 26,
    };
    unsafe {
        let wide_path = wide(&path.to_string_lossy());
        let store: IPropertyStore =
            SHGetPropertyStoreFromParsingName(PCWSTR(wide_path.as_ptr()), None, GPS_DEFAULT)
                .ok()?;
        let mut clsid_value = store.GetValue(&activator_key).ok()?;
        let clsid = PropVariantToGUID(&clsid_value).ok();
        let _ = PropVariantClear(&mut clsid_value);
        let clsid = clsid.filter(|g| *g != GUID::zeroed())?;
        let mut id_value = store.GetValue(&id_key).ok()?;
        let aumid = PropVariantToStringAlloc(&id_value).ok().map(|p| {
            let text = String::from_utf16_lossy(p.as_wide());
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const c_void));
            text
        });
        let _ = PropVariantClear(&mut id_value);
        let aumid = aumid.filter(|a| !a.trim().is_empty())?;
        Some((aumid.trim().to_string(), clsid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_recognised() {
        assert!(looks_like_link("ms-settings:startupapps"));
        assert!(looks_like_link("https://example.com/a"));
        assert!(looks_like_link("windowsdefender://threat"));
        assert!(!looks_like_link("action=reply&id=4"));
        assert!(!looks_like_link("C:\\Windows"));
        assert!(!looks_like_link(""));
    }

    #[test]
    fn payloads_decode() {
        assert_eq!(payload_text(b"<toast/>"), "<toast/>");
        let utf16: Vec<u8> = "<toast/>"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(payload_text(&utf16), "<toast/>");
    }
}
