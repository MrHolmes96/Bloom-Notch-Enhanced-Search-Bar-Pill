//! Blitzit in the notch.
//!
//! Tasks go through Blitzit's own official connection for other apps (its MCP
//! server, the one Claude, ChatGPT and Raycast use), signed in once in the
//! browser with a Blitzit account. The sign-in is stored encrypted for this
//! Windows user only. Starting Blitz mode happens in the Blitzit app on this
//! PC: Bloom brings it forward and presses its "Blitzit now" button.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::mcp::{Client, McpError};
use super::oauth::{self, RefreshError, Session};
use super::tools::{self, Item, Kind, ListRef};

/// Blitzit 3 and newer; older versions answer on `integration-prd`.
pub const DEFAULT_SERVER: &str = "https://core.blitzit.app/v3/mcp";
const SERVER_SETTING: &str = "bloom-blitzit-server";

struct Connection {
    session: Session,
    client: Client,
}

static CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);

fn store_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    Some(
        app.path()
            .app_config_dir()
            .ok()?
            .join("integrations")
            .join("blitzit.bin"),
    )
}

fn load_session(app: &AppHandle) -> Option<Session> {
    let bytes = std::fs::read(store_path(app)?).ok()?;
    let plain = super::secret::unprotect(&bytes).ok()?;
    serde_json::from_slice(&plain).ok()
}

fn save_session(app: &AppHandle, session: &Session) -> Result<(), String> {
    let path = store_path(app).ok_or("No settings folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let plain = serde_json::to_vec(session).map_err(|e| e.to_string())?;
    let sealed = super::secret::protect(&plain)?;
    std::fs::write(path, sealed).map_err(|e| e.to_string())
}

fn forget(app: &AppHandle) {
    if let Some(path) = store_path(app) {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(mut connection) = CONNECTION.lock() {
        *connection = None;
    }
}

fn server(app: &AppHandle) -> String {
    crate::utils::get_setting_str(app, SERVER_SETTING)
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("https://"))
        .unwrap_or_else(|| DEFAULT_SERVER.to_string())
}

const SIGNED_OUT: &str = "Signed out of Blitzit. Connect it again in Settings > Integrations";

/// Runs `f` with a ready connection: signed in, token fresh, tools known. A
/// rejected token is refreshed once; a reset connection is opened again.
fn with_client<T>(
    app: &AppHandle,
    mut f: impl FnMut(&mut Client, &str) -> Result<T, McpError>,
) -> Result<T, String> {
    let mut guard = CONNECTION.lock().map_err(|_| "Busy")?;
    if guard.is_none() {
        let session = load_session(app).ok_or("Blitzit isn't connected yet")?;
        *guard = Some(Connection {
            client: Client::new(&session.server),
            session,
        });
    }
    let connection = guard.as_mut().ok_or("Blitzit isn't connected yet")?;

    let refresh = |connection: &mut Connection| -> Result<(), String> {
        match oauth::refresh(&mut connection.session) {
            Ok(()) => {
                let _ = save_session(app, &connection.session);
                connection.client.ready = false;
                Ok(())
            }
            Err(RefreshError::Rejected(_)) => Err(SIGNED_OUT.to_string()),
            Err(RefreshError::Offline(message)) => Err(message),
        }
    };

    let soon = crate::utils::get_now_ms() + 60_000;
    if connection.session.expires_at.is_some_and(|at| at < soon)
        && connection.session.refresh_token.is_some()
    {
        if let Err(message) = refresh(connection) {
            if message == SIGNED_OUT {
                drop(guard);
                forget(app);
            }
            return Err(message);
        }
    }

    let mut refreshed = false;
    for _ in 0..3 {
        let token = connection.session.access_token.clone();
        let result = if connection.client.ready {
            f(&mut connection.client, &token)
        } else {
            connection
                .client
                .initialize(&token)
                .and_then(|_| f(&mut connection.client, &token))
        };
        match result {
            Ok(value) => return Ok(value),
            Err(McpError::Unauthorized) if !refreshed => {
                refreshed = true;
                if let Err(message) = refresh(connection) {
                    if message == SIGNED_OUT {
                        drop(guard);
                        forget(app);
                    }
                    return Err(message);
                }
            }
            Err(McpError::Unauthorized) => {
                drop(guard);
                forget(app);
                return Err(SIGNED_OUT.to_string());
            }
            Err(McpError::SessionGone) => continue,
            Err(McpError::Failed(message)) => return Err(message),
        }
    }
    Err("Blitzit didn't answer. Try again in a moment".into())
}

fn tool_name(client: &Client, kind: Kind) -> Option<String> {
    tools::pick(&client.tools, kind).map(|t| t.name.clone())
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
pub struct BlitzitStatus {
    connected: bool,
    server: String,
    /// What Blitzit's connection offers, by tool name.
    tools: Vec<String>,
    can_add: bool,
    can_read_lists: bool,
    can_read_tasks: bool,
    can_complete: bool,
    /// Columns a new task can go in, when Blitzit says.
    columns: Vec<String>,
    /// The Blitzit app is installed or running on this PC.
    app_found: bool,
}

fn status_now(app: &AppHandle, check: bool) -> BlitzitStatus {
    let connected = store_path(app).is_some_and(|p| p.is_file());
    let mut tools = Vec::new();
    let mut flags = [false; 4];
    let mut columns = Vec::new();
    if connected {
        let read = |client: &mut Client| {
            (
                client
                    .tools
                    .iter()
                    .map(|t| t.name.clone())
                    .collect::<Vec<_>>(),
                [
                    tool_name(client, Kind::CreateTask).is_some(),
                    tool_name(client, Kind::Lists).is_some(),
                    tool_name(client, Kind::Tasks).is_some(),
                    tool_name(client, Kind::CompleteTask).is_some(),
                ],
                tools::pick(&client.tools, Kind::CreateTask)
                    .map(tools::column_choices)
                    .unwrap_or_default(),
            )
        };
        let cached = CONNECTION.lock().ok().and_then(|mut c| {
            c.as_mut()
                .filter(|c| c.client.ready)
                .map(|c| read(&mut c.client))
        });
        let found = match cached {
            Some(found) => Some(found),
            None if check => with_client(app, |client, _| Ok(read(client))).ok(),
            None => None,
        };
        if let Some((names, found_flags, found_columns)) = found {
            tools = names;
            flags = found_flags;
            columns = found_columns;
        }
    }
    BlitzitStatus {
        connected: store_path(app).is_some_and(|p| p.is_file()),
        server: server(app),
        tools,
        can_add: flags[0],
        can_read_lists: flags[1],
        can_read_tasks: flags[2],
        can_complete: flags[3],
        columns,
        app_found: find_window().is_some() || installed_app().is_some(),
    }
}

/// Whether Blitzit is connected, and what it offers. `check` asks Blitzit.
#[tauri::command]
pub async fn blitzit_status(app: AppHandle, check: Option<bool>) -> BlitzitStatus {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || status_now(&handle, check.unwrap_or(false)))
        .await
        .unwrap_or(BlitzitStatus {
            connected: false,
            server: server(&app),
            tools: Vec::new(),
            can_add: false,
            can_read_lists: false,
            can_read_tasks: false,
            can_complete: false,
            columns: Vec::new(),
            app_found: false,
        })
}

/// Signs in to Blitzit in the browser and connects.
#[tauri::command]
pub async fn blitzit_connect(app: AppHandle) -> Result<BlitzitStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = oauth::sign_in(&server(&app), "Blitzit")?;
        save_session(&app, &session)?;
        if let Ok(mut connection) = CONNECTION.lock() {
            *connection = Some(Connection {
                client: Client::new(&session.server),
                session,
            });
        }
        let status = status_now(&app, true);
        // Tell the notch and Settings it is connected.
        use tauri::Emitter;
        let _ = app.emit("blitzit-changed", ());
        Ok(status)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stops a sign-in that is waiting for the browser.
#[tauri::command]
pub fn blitzit_cancel_connect() {
    oauth::cancel();
}

#[tauri::command]
pub fn blitzit_disconnect(app: AppHandle) {
    forget(&app);
    use tauri::Emitter;
    let _ = app.emit("blitzit-changed", ());
}

#[derive(Serialize)]
pub struct BlitzitList {
    id: String,
    name: String,
}

#[tauri::command]
pub async fn blitzit_lists(app: AppHandle) -> Result<Vec<BlitzitList>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_client(&app, |client, token| {
            let tool = tools::pick(&client.tools, Kind::Lists)
                .cloned()
                .ok_or_else(|| McpError::Failed("Blitzit doesn't share its lists".into()))?;
            let args = tools::lists_args(&tool)?;
            let answer = client.call(token, &tool.name, args)?;
            Ok(tools::items(&answer)
                .into_iter()
                .map(|item| BlitzitList {
                    id: item.id,
                    name: item.name,
                })
                .collect())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn list_ref(id: Option<String>, name: Option<String>) -> Option<ListRef> {
    id.filter(|i| !i.is_empty()).map(|id| ListRef {
        name: name.unwrap_or_else(|| id.clone()),
        id,
    })
}

#[tauri::command]
pub async fn blitzit_tasks(
    app: AppHandle,
    list_id: Option<String>,
    list_name: Option<String>,
) -> Result<Vec<Item>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let list = list_ref(list_id, list_name);
        with_client(&app, |client, token| {
            let tool = tools::pick(&client.tools, Kind::Tasks)
                .cloned()
                .ok_or_else(|| McpError::Failed("Blitzit doesn't share its tasks".into()))?;
            let args = tools::tasks_args(&tool, list.as_ref())?;
            let answer = client.call(token, &tool.name, args)?;
            Ok(tools::items(&answer))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn blitzit_add_task(
    app: AppHandle,
    title: String,
    list_id: Option<String>,
    list_name: Option<String>,
    column: Option<String>,
) -> Result<(), String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Write the task first".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let list = list_ref(list_id, list_name);
        with_client(&app, |client, token| {
            let tool = tools::pick(&client.tools, Kind::CreateTask)
                .cloned()
                .ok_or_else(|| McpError::Failed("Blitzit can't add tasks from here".into()))?;
            let args = tools::create_args(&tool, &title, list.as_ref(), column.as_deref())?;
            client.call(token, &tool.name, args).map(|_| ())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn blitzit_complete_task(
    app: AppHandle,
    task_id: String,
    list_id: Option<String>,
    list_name: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let list = list_ref(list_id, list_name);
        with_client(&app, |client, token| {
            let tool = tools::pick(&client.tools, Kind::CompleteTask)
                .cloned()
                .ok_or_else(|| McpError::Failed("Blitzit can't tick tasks off from here".into()))?;
            let args = tools::complete_args(&tool, &task_id, list.as_ref())?;
            client.call(token, &tool.name, args).map(|_| ())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------- the Blitzit app on this PC

fn is_blitzit(path: &str) -> bool {
    std::path::Path::new(path)
        .file_stem()
        .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("blitzit"))
}

/// Blitzit's main window: its largest titled window, shown or in the tray.
fn find_window() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowThreadProcessId,
    };
    unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> windows::core::BOOL {
        let list = &mut *(list.0 as *mut Vec<HWND>);
        list.push(hwnd);
        true.into()
    }
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize));
        let mut best: Option<(i64, HWND)> = None;
        let mut checked: std::collections::HashMap<u32, bool> = Default::default();
        for hwnd in all {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let ours = *checked.entry(pid).or_insert_with(|| {
                crate::commands::process_image_path(pid).is_some_and(|p| is_blitzit(&p))
            });
            if !ours || GetWindowTextLengthW(hwnd) == 0 {
                continue;
            }
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let area = ((rect.right - rect.left) as i64) * ((rect.bottom - rect.top) as i64);
            if best.is_none_or(|(a, _)| area > a) {
                best = Some((area, hwnd));
            }
        }
        best.map(|(_, hwnd)| hwnd)
    }
}

fn installed_app() -> Option<String> {
    let apps = crate::state::INSTALLED_APPS_CACHE.get()?.lock().ok()?;
    apps.iter()
        .find(|a| {
            a.name.eq_ignore_ascii_case("blitzit") || a.name.to_lowercase().starts_with("blitzit")
        })
        .map(|a| a.path.clone())
}

/// Brings Blitzit forward, starting it when it isn't running.
fn show_app() -> Result<windows::Win32::Foundation::HWND, String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };
    let hwnd = match find_window() {
        Some(hwnd) => hwnd,
        None => {
            let path = installed_app().ok_or("Blitzit isn't installed on this PC")?;
            crate::commands::launch_path(&path);
            let mut found = None;
            for _ in 0..60 {
                std::thread::sleep(std::time::Duration::from_millis(250));
                if let Some(hwnd) = find_window() {
                    found = Some(hwnd);
                    break;
                }
            }
            found.ok_or("Blitzit didn't open")?
        }
    };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(hwnd)
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Presses the button in `hwnd` whose text is one of `labels`, the way a
/// screen reader would; a click on it is the last resort.
unsafe fn press_button(
    hwnd: windows::Win32::Foundation::HWND,
    labels: &[&str],
) -> Result<bool, String> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation, IUIAutomationInvokePattern,
        IUIAutomationLegacyIAccessiblePattern, TreeScope_Descendants, UIA_InvokePatternId,
        UIA_LegacyIAccessiblePatternId,
    };
    let automation: IUIAutomation =
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
    let wanted: Vec<String> = labels.iter().map(|l| squash(l)).collect();
    // Chromium builds its accessibility tree on first request, so look again
    // for a moment.
    for _ in 0..12 {
        let root = automation
            .ElementFromHandle(hwnd)
            .map_err(|e| e.to_string())?;
        let condition = automation
            .CreateTrueCondition()
            .map_err(|e| e.to_string())?;
        if let Ok(elements) = root.FindAll(TreeScope_Descendants, &condition) {
            for index in 0..elements.Length().unwrap_or(0) {
                let Ok(element) = elements.GetElement(index) else {
                    continue;
                };
                let name = squash(&element.CurrentName().unwrap_or_default().to_string());
                if name.is_empty() || !wanted.contains(&name) {
                    continue;
                }
                if let Ok(invoke) =
                    element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                {
                    if invoke.Invoke().is_ok() {
                        return Ok(true);
                    }
                }
                if let Ok(legacy) = element
                    .GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                        UIA_LegacyIAccessiblePatternId,
                    )
                {
                    if legacy.DoDefaultAction().is_ok() {
                        return Ok(true);
                    }
                }
                if let Ok(rect) = element.CurrentBoundingRectangle() {
                    if rect.right > rect.left && rect.bottom > rect.top {
                        click_at((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
                        return Ok(true);
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Ok(false)
}

/// One left click at a screen point; the pointer goes back where it was.
unsafe fn click_at(x: i32, y: i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEINPUT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};
    let mut before = POINT::default();
    let _ = GetCursorPos(&mut before);
    let _ = SetCursorPos(x, y);
    let event = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    SendInput(
        &[event(MOUSEEVENTF_LEFTDOWN), event(MOUSEEVENTF_LEFTUP)],
        std::mem::size_of::<INPUT>() as i32,
    );
    std::thread::sleep(std::time::Duration::from_millis(30));
    let _ = SetCursorPos(before.x, before.y);
}

/// Starts Blitz mode: Blitzit comes forward and its "Blitzit now" is pressed.
#[tauri::command]
pub async fn blitzit_start() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let hwnd = show_app()?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        if unsafe { press_button(hwnd, &["Blitzit now", "Blitz now", "Blitzit Now!"]) }? {
            Ok("started".into())
        } else {
            Ok("opened".into())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Brings the Blitzit app forward.
#[tauri::command]
pub async fn blitzit_open() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| show_app().map(|_| ()))
        .await
        .map_err(|e| e.to_string())?
}
