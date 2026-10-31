//! Blitzit's lists and tasks kept on this PC, so the notch works with or
//! without a connection.
//!
//! - Every change made in the notch (a new task, a task ticked off, a task
//!   moved to another column) goes into a queue saved to disk first, and is
//!   shown at once. A worker sends the queue to Blitzit in order; with no
//!   connection it keeps it and tries again every few seconds, so nothing
//!   made offline is lost, not even across a restart.
//! - The lists and tasks last read from Blitzit are saved too, so the notch
//!   shows them instantly and offline; the notch asks Blitzit for fresh ones
//!   while it's open (Blitzit has no way to tell Bloom about changes made in
//!   its own app, so they're read again every few seconds).
//! - A new task that may have reached Blitzit before the connection dropped
//!   is looked up by its title before it's sent again, so it isn't added twice.
//!
//! Neither file is in Settings > About > Export Backup: the tasks live in
//! Blitzit itself, and a restored queue on another PC (without this PC's
//! sign-in) could send old changes a second time.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::blitzit::{self, Fail};
use super::mcp::McpError;
use super::tools::{self, Item, Kind, ListRef};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BoardList {
    pub id: String,
    pub name: String,
}

/// What Blitzit's connection lets Bloom do, as last seen.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct Caps {
    add: bool,
    lists: bool,
    tasks: bool,
    complete: bool,
    moving: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    #[serde(default)]
    lists: Vec<BoardList>,
    /// By list id ("" when Blitzit has no lists to choose from).
    #[serde(default)]
    tasks: HashMap<String, Vec<Item>>,
    #[serde(default)]
    columns: Vec<String>,
    /// Unknown until Blitzit has answered once.
    #[serde(default)]
    caps: Option<Caps>,
    /// A task made here, by the id it had here, and its id in Blitzit.
    #[serde(default)]
    made: HashMap<String, String>,
    #[serde(default)]
    synced_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OpKind {
    Add,
    Complete,
    Move,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Op {
    /// "local-…": a new task's id until Blitzit gives it one.
    id: String,
    kind: OpKind,
    #[serde(default)]
    title: String,
    #[serde(default)]
    task_id: String,
    #[serde(default)]
    column: Option<String>,
    /// A new task ticked off before it reached Blitzit.
    #[serde(default)]
    done: bool,
    #[serde(default)]
    list_id: Option<String>,
    #[serde(default)]
    list_name: Option<String>,
    /// Times it was sent already (it may have reached Blitzit).
    #[serde(default)]
    tries: u32,
    #[serde(default)]
    at: i64,
}

impl Op {
    fn list_key(&self) -> String {
        self.list_id.clone().unwrap_or_default()
    }
}

#[derive(Default)]
struct Store {
    cache: Cache,
    queue: Vec<Op>,
}

static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
/// Only one send of the queue at a time, so nothing is sent twice.
static FLUSHING: Mutex<()> = Mutex::new(());
static OFFLINE: AtomicBool = AtomicBool::new(false);
static KICK: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("integrations"))
}

fn read<T: for<'de> Deserialize<'de> + Default>(app: &AppHandle, name: &str) -> T {
    dir(app)
        .and_then(|d| std::fs::read(d.join(name)).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Written whole to a side file first, so a crash mid-write never leaves a
/// broken file behind.
fn write<T: Serialize>(app: &AppHandle, name: &str, value: &T) {
    let Some(dir) = dir(app) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let Ok(bytes) = serde_json::to_vec(value) else {
        return;
    };
    let side = dir.join(format!("{name}.new"));
    if std::fs::write(&side, bytes).is_ok() {
        let _ = std::fs::rename(&side, dir.join(name));
    }
}

const CACHE_FILE: &str = "blitzit-cache.json";
const QUEUE_FILE: &str = "blitzit-queue.json";

fn store(app: &AppHandle) -> &'static Mutex<Store> {
    STORE.get_or_init(|| {
        Mutex::new(Store {
            cache: read(app, CACHE_FILE),
            queue: read(app, QUEUE_FILE),
        })
    })
}

fn with_store<T>(app: &AppHandle, f: impl FnOnce(&mut Store) -> T) -> T {
    let mut guard = store(app).lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

fn save_queue(app: &AppHandle, store: &Store) {
    write(app, QUEUE_FILE, &store.queue);
}

fn save_cache(app: &AppHandle, store: &Store) {
    write(app, CACHE_FILE, &store.cache);
}

fn log(line: &str) {
    use std::io::Write;
    let Some(path) = crate::utils::data_file("blitzit-sync.log") else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 32 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {line}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        );
    }
}

fn set_offline(offline: bool, why: &str) {
    if OFFLINE.swap(offline, Ordering::Relaxed) != offline {
        log(&if offline {
            format!("offline: {why}")
        } else {
            "back online".to_string()
        });
    }
}

fn kick() {
    let (flag, signal) = &KICK;
    if let Ok(mut kicked) = flag.lock() {
        *kicked = true;
        signal.notify_all();
    }
}

fn is_done_column(column: &str) -> bool {
    matches!(
        column
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase()
            .as_str(),
        "done" | "completed" | "complete" | "finished"
    )
}

// ---------------------------------------------------------------- the board

#[derive(Clone, Debug, Serialize)]
pub struct BoardTask {
    id: String,
    name: String,
    done: bool,
    column: String,
    /// Not in Blitzit yet (waiting for a connection, or being sent).
    pending: bool,
}

#[derive(Serialize)]
pub struct Board {
    connected: bool,
    lists: Vec<BoardList>,
    /// The list shown.
    list: Option<BoardList>,
    tasks: Vec<BoardTask>,
    /// Columns a task can go in (Today, This week, …), when Blitzit says.
    columns: Vec<String>,
    can_add: bool,
    can_complete: bool,
    can_move: bool,
    /// Blitzit couldn't be reached: this is the copy saved on this PC.
    offline: bool,
    /// Something else went wrong.
    problem: Option<String>,
    /// Changes waiting to reach Blitzit.
    pending: usize,
    synced_at: Option<i64>,
}

fn board(app: &AppHandle, list_id: Option<String>, problem: Option<String>) -> Board {
    let connected = blitzit::is_connected(app);
    with_store(app, |store| {
        let cache = &store.cache;
        let list = match list_id.filter(|id| !id.is_empty()) {
            Some(id) => Some(cache.lists.iter().find(|l| l.id == id).cloned().unwrap_or(
                BoardList {
                    name: id.clone(),
                    id,
                },
            )),
            None => cache.lists.first().cloned(),
        };
        let key = list.as_ref().map(|l| l.id.clone()).unwrap_or_default();
        let mut tasks: Vec<BoardTask> = cache
            .tasks
            .get(&key)
            .map(|items| {
                items
                    .iter()
                    .map(|i| BoardTask {
                        id: i.id.clone(),
                        name: i.name.clone(),
                        done: i.done,
                        column: i.column.clone(),
                        pending: false,
                    })
                    .collect()
            })
            .unwrap_or_default();
        // What's waiting to reach Blitzit, shown as it will be.
        for op in store.queue.iter().filter(|op| op.list_key() == key) {
            let real = cache.made.get(&op.task_id).cloned();
            let target = |t: &BoardTask| t.id == op.task_id || real.as_ref() == Some(&t.id);
            match op.kind {
                OpKind::Add => tasks.push(BoardTask {
                    id: op.id.clone(),
                    name: op.title.clone(),
                    done: op.done,
                    column: op.column.clone().unwrap_or_default(),
                    pending: true,
                }),
                OpKind::Complete => {
                    if let Some(task) = tasks.iter_mut().find(|t| target(t)) {
                        task.done = true;
                        task.pending = true;
                    }
                }
                OpKind::Move => {
                    if let Some(task) = tasks.iter_mut().find(|t| target(t)) {
                        task.column = op.column.clone().unwrap_or_default();
                        task.pending = true;
                    }
                }
            }
        }
        let caps = cache.caps;
        Board {
            connected,
            lists: cache.lists.clone(),
            list,
            tasks,
            columns: cache.columns.clone(),
            can_add: caps.is_none_or(|c| c.add),
            can_complete: caps.is_none_or(|c| c.complete),
            can_move: caps.is_none_or(|c| c.moving),
            offline: OFFLINE.load(Ordering::Relaxed),
            problem,
            pending: store.queue.len(),
            synced_at: cache.synced_at,
        }
    })
}

/// Reads one list's tasks from Blitzit.
fn read_tasks(
    app: &AppHandle,
    client: &mut super::mcp::Client,
    token: &str,
    list: Option<&ListRef>,
) -> Result<Vec<Item>, McpError> {
    let tool = blitzit::pick(app, client, Kind::Tasks)
        .ok_or_else(|| McpError::Failed("Blitzit doesn't share its tasks".into()))?;
    let args = tools::tasks_args(&tool, list)?;
    let answer = client.call(token, &tool.name, args)?;
    Ok(tools::items(&answer))
}

/// Sends what's waiting, then reads the lists and the shown list's tasks
/// from Blitzit. Gives the list shown, and what went wrong if anything did.
fn refresh(
    app: &AppHandle,
    list_id: Option<String>,
    with_lists: bool,
) -> (Option<String>, Option<String>) {
    let failures = {
        let _sending = FLUSHING.lock().unwrap_or_else(|e| e.into_inner());
        flush_locked(app)
    };
    if !failures.is_empty() {
        let _ = app.emit("blitzit-sync-failed", failures);
    }
    let known_lists = with_store(app, |s| s.cache.lists.clone());
    let fetch_lists = with_lists || known_lists.is_empty();
    let result = blitzit::with_client_ex(app, |client, token| {
        let caps = Caps {
            add: blitzit::pick(app, client, Kind::CreateTask).is_some(),
            lists: blitzit::pick(app, client, Kind::Lists).is_some(),
            tasks: blitzit::pick(app, client, Kind::Tasks).is_some(),
            complete: blitzit::pick(app, client, Kind::CompleteTask).is_some(),
            moving: blitzit::pick(app, client, Kind::MoveTask).is_some(),
        };
        let columns = blitzit::pick(app, client, Kind::CreateTask)
            .map(|t| tools::column_choices(&t))
            .unwrap_or_default();
        let lists = if caps.lists && fetch_lists {
            let tool = blitzit::pick(app, client, Kind::Lists)
                .ok_or_else(|| McpError::Failed("Blitzit doesn't share its lists".into()))?;
            let args = tools::lists_args(&tool)?;
            let answer = client.call(token, &tool.name, args)?;
            Some(
                tools::items(&answer)
                    .into_iter()
                    .map(|i| BoardList {
                        id: i.id,
                        name: i.name,
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        let all = lists.clone().unwrap_or_else(|| known_lists.clone());
        let chosen = list_id
            .clone()
            .filter(|id| !id.is_empty() && (all.is_empty() || all.iter().any(|l| &l.id == id)))
            .or_else(|| all.first().map(|l| l.id.clone()));
        let list = chosen.as_ref().map(|id| ListRef {
            id: id.clone(),
            name: all
                .iter()
                .find(|l| &l.id == id)
                .map(|l| l.name.clone())
                .unwrap_or_else(|| id.clone()),
        });
        let tasks = if caps.tasks {
            Some(read_tasks(app, client, token, list.as_ref())?)
        } else {
            None
        };
        Ok((caps, columns, lists, chosen, tasks))
    });
    match result {
        Ok((caps, columns, lists, chosen, tasks)) => {
            set_offline(false, "");
            with_store(app, |store| {
                let cache = &mut store.cache;
                cache.caps = Some(caps);
                cache.columns = columns;
                if let Some(lists) = lists {
                    cache.lists = lists;
                }
                if let Some(tasks) = tasks {
                    cache
                        .tasks
                        .insert(chosen.clone().unwrap_or_default(), tasks);
                }
                // Lists that are gone take their tasks with them.
                let ids: Vec<String> = cache.lists.iter().map(|l| l.id.clone()).collect();
                if !ids.is_empty() {
                    cache.tasks.retain(|k, _| ids.contains(k));
                }
                cache.synced_at = Some(crate::utils::get_now_ms());
                save_cache(app, store);
            });
            (chosen, None)
        }
        Err(Fail::Offline(why)) => {
            set_offline(true, &why);
            (list_id, None)
        }
        Err(Fail::SignedOut(m)) | Err(Fail::Other(m)) => (list_id, Some(m)),
    }
}

/// The board for a list (the first list when none is given). `fresh` asks
/// Blitzit first (sending what's waiting too); otherwise it's the copy kept
/// on this PC, which answers at once.
#[tauri::command]
pub async fn blitzit_board(
    app: AppHandle,
    list_id: Option<String>,
    fresh: Option<bool>,
    with_lists: Option<bool>,
) -> Board {
    let handle = app.clone();
    let wanted = list_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if fresh.unwrap_or(false) && blitzit::is_connected(&handle) {
            let (chosen, problem) = refresh(&handle, list_id, with_lists.unwrap_or(false));
            board(&handle, chosen, problem)
        } else {
            board(&handle, list_id, None)
        }
    })
    .await
    .unwrap_or_else(|_| board(&app, wanted, None))
}

// ---------------------------------------------------------------- changes

fn new_local_id() -> String {
    format!(
        "local-{}-{}",
        crate::utils::get_now_ms(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn queue(app: &AppHandle, op: Op) {
    log(&format!(
        "queued {:?} {}",
        op.kind,
        if op.title.is_empty() {
            &op.task_id
        } else {
            &op.title
        }
    ));
    with_store(app, |store| {
        store.queue.push(op);
        save_queue(app, store);
    });
    kick();
}

/// Adds a task to a list (and column). It shows at once; it reaches Blitzit
/// now, or once there's a connection.
#[tauri::command]
pub fn blitzit_add_task(
    app: AppHandle,
    title: String,
    list_id: Option<String>,
    list_name: Option<String>,
    column: Option<String>,
) -> Result<BoardTask, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Write the task first".into());
    }
    if !blitzit::is_connected(&app) {
        return Err("Connect Blitzit in Settings > Integrations first".into());
    }
    if with_store(&app, |s| s.cache.caps.is_some_and(|c| !c.add)) {
        return Err("Blitzit can't add tasks from here".into());
    }
    let column = column.filter(|c| !c.is_empty());
    let op = Op {
        id: new_local_id(),
        kind: OpKind::Add,
        title: title.clone(),
        task_id: String::new(),
        column: column.clone(),
        done: false,
        list_id: list_id.filter(|l| !l.is_empty()),
        list_name,
        tries: 0,
        at: crate::utils::get_now_ms(),
    };
    let task = BoardTask {
        id: op.id.clone(),
        name: title,
        done: false,
        column: column.unwrap_or_default(),
        pending: true,
    };
    queue(&app, op);
    Ok(task)
}

/// A change to a task that hasn't reached Blitzit yet, made to the waiting
/// new task itself. False when it was already sent (or is being sent).
fn change_waiting_task(app: &AppHandle, local_id: &str, change: impl FnOnce(&mut Op)) -> bool {
    with_store(app, |store| {
        let Some(op) = store
            .queue
            .iter_mut()
            .find(|op| op.id == local_id && op.kind == OpKind::Add && op.tries == 0)
        else {
            return false;
        };
        change(op);
        save_queue(app, store);
        true
    })
}

#[tauri::command]
pub fn blitzit_complete_task(
    app: AppHandle,
    task_id: String,
    list_id: Option<String>,
    list_name: Option<String>,
) -> Result<(), String> {
    if with_store(&app, |s| s.cache.caps.is_some_and(|c| !c.complete)) {
        return Err("Blitzit can't tick tasks off from here".into());
    }
    if task_id.starts_with("local-") && change_waiting_task(&app, &task_id, |op| op.done = true) {
        kick();
        return Ok(());
    }
    queue(
        &app,
        Op {
            id: new_local_id(),
            kind: OpKind::Complete,
            title: String::new(),
            task_id,
            column: None,
            done: true,
            list_id: list_id.filter(|l| !l.is_empty()),
            list_name,
            tries: 0,
            at: crate::utils::get_now_ms(),
        },
    );
    Ok(())
}

/// Moves a task to another column of its list (Done ticks it off).
#[tauri::command]
pub fn blitzit_move_task(
    app: AppHandle,
    task_id: String,
    list_id: Option<String>,
    list_name: Option<String>,
    column: String,
) -> Result<(), String> {
    if is_done_column(&column) {
        return blitzit_complete_task(app, task_id, list_id, list_name);
    }
    if task_id.starts_with("local-") {
        let wanted = column.clone();
        if change_waiting_task(&app, &task_id, |op| op.column = Some(wanted)) {
            kick();
            return Ok(());
        }
    }
    if with_store(&app, |s| s.cache.caps.is_some_and(|c| !c.moving)) {
        return Err(
            "Blitzit's connection doesn't let Bloom move tasks. Pick its move action in Settings > Integrations > Blitzit if it has one"
                .into(),
        );
    }
    let list_id = list_id.filter(|l| !l.is_empty());
    // A move still waiting is changed rather than sent twice.
    let merged = with_store(&app, |store| {
        let found = store
            .queue
            .iter_mut()
            .find(|op| op.kind == OpKind::Move && op.task_id == task_id && op.tries == 0);
        if let Some(op) = found {
            op.column = Some(column.clone());
            save_queue(&app, store);
            true
        } else {
            false
        }
    });
    if merged {
        kick();
        return Ok(());
    }
    queue(
        &app,
        Op {
            id: new_local_id(),
            kind: OpKind::Move,
            title: String::new(),
            task_id,
            column: Some(column),
            done: false,
            list_id,
            list_name,
            tries: 0,
            at: crate::utils::get_now_ms(),
        },
    );
    Ok(())
}

// ---------------------------------------------------------------- sending

/// A task's id in Blitzit: a task made here goes by the id Blitzit gave it.
fn real_id(app: &AppHandle, id: &str) -> Option<String> {
    if !id.starts_with("local-") {
        return Some(id.to_string());
    }
    with_store(app, |s| s.cache.made.get(id).cloned())
}

/// Sends one change. For a new task, gives its id in Blitzit when known.
fn send(app: &AppHandle, op: &Op) -> Result<Option<String>, Fail> {
    let list = blitzit::list_ref(op.list_id.clone(), op.list_name.clone());
    match op.kind {
        OpKind::Add => {
            // Kept across the connection's own retries: made once, never twice.
            let mut made: Option<Option<String>> = None;
            blitzit::with_client_ex(app, |client, token| {
                if made.is_none() && op.tries > 0 {
                    // It may have reached Blitzit last time.
                    if let Ok(items) = read_tasks(app, client, token, list.as_ref()) {
                        if let Some(found) = items
                            .into_iter()
                            .find(|i| tools::same_title(&i.name, &op.title))
                        {
                            log(&format!("\"{}\" was already in Blitzit", op.title));
                            made = Some(Some(found.id));
                        }
                    }
                }
                if made.is_none() {
                    let tool = blitzit::pick(app, client, Kind::CreateTask).ok_or_else(|| {
                        McpError::Failed("Blitzit can't add tasks from here".into())
                    })?;
                    let args =
                        tools::create_args(&tool, &op.title, list.as_ref(), op.column.as_deref())?;
                    let answer = client.call(token, &tool.name, args)?;
                    let mut id = tools::created_item(&answer, &op.title).map(|i| i.id);
                    if id.is_none() {
                        // Blitzit didn't say: find it in the list.
                        id = read_tasks(app, client, token, list.as_ref())
                            .ok()
                            .and_then(|items| {
                                items
                                    .into_iter()
                                    .rev()
                                    .find(|i| !i.done && tools::same_title(&i.name, &op.title))
                                    .map(|i| i.id)
                            });
                    }
                    made = Some(id);
                }
                let id = made.clone().flatten();
                if op.done && id.is_none() {
                    log(&format!(
                        "\"{}\" was added, but Blitzit didn't say which task it is to tick it off",
                        op.title
                    ));
                }
                if let Some(id) = id.clone().filter(|_| op.done) {
                    let tool = blitzit::pick(app, client, Kind::CompleteTask).ok_or_else(|| {
                        McpError::Failed("Blitzit can't tick tasks off from here".into())
                    })?;
                    let args = tools::complete_args(&tool, &id, list.as_ref())?;
                    client.call(token, &tool.name, args)?;
                }
                Ok(id)
            })
        }
        OpKind::Complete | OpKind::Move => {
            let id = real_id(app, &op.task_id).ok_or_else(|| {
                Fail::Other("Blitzit didn't say which task the new one is".into())
            })?;
            blitzit::with_client_ex(app, |client, token| {
                if op.kind == OpKind::Complete {
                    let tool = blitzit::pick(app, client, Kind::CompleteTask).ok_or_else(|| {
                        McpError::Failed("Blitzit can't tick tasks off from here".into())
                    })?;
                    let args = tools::complete_args(&tool, &id, list.as_ref())?;
                    client.call(token, &tool.name, args)?;
                } else {
                    let tool = blitzit::pick(app, client, Kind::MoveTask).ok_or_else(|| {
                        McpError::Failed("Blitzit can't move tasks from here".into())
                    })?;
                    let column = op.column.clone().unwrap_or_default();
                    let args = tools::move_args(&tool, &id, list.as_ref(), &column)?;
                    client.call(token, &tool.name, args)?;
                }
                Ok(None)
            })
        }
    }
}

/// The change, made to the copy kept here, once Blitzit took it.
fn apply(cache: &mut Cache, op: &Op, made: Option<String>) {
    let tasks = cache.tasks.entry(op.list_key()).or_default();
    match op.kind {
        OpKind::Add => {
            let id = made.clone().unwrap_or_else(|| op.id.clone());
            if !tasks.iter().any(|t| t.id == id) {
                tasks.push(Item {
                    id,
                    name: op.title.clone(),
                    done: op.done,
                    column: op.column.clone().unwrap_or_default(),
                    list: op.list_name.clone().unwrap_or_default(),
                });
            }
            if let Some(real) = made {
                cache.made.insert(op.id.clone(), real);
                // Only the latest few are needed (for changes still waiting).
                if cache.made.len() > 200 {
                    let oldest = cache.made.keys().min().cloned();
                    if let Some(oldest) = oldest {
                        cache.made.remove(&oldest);
                    }
                }
            }
        }
        OpKind::Complete | OpKind::Move => {
            let real = cache.made.get(&op.task_id).cloned();
            if let Some(task) = tasks
                .iter_mut()
                .find(|t| t.id == op.task_id || real.as_ref() == Some(&t.id))
            {
                if op.kind == OpKind::Complete {
                    task.done = true;
                } else {
                    task.column = op.column.clone().unwrap_or_default();
                }
            }
        }
    }
}

fn describe(op: &Op, why: &str) -> String {
    match op.kind {
        OpKind::Add => format!("Couldn't add \"{}\" to Blitzit: {why}", op.title),
        OpKind::Complete => format!("Couldn't tick a task off in Blitzit: {why}"),
        OpKind::Move => format!("Couldn't move a task in Blitzit: {why}"),
    }
}

/// Sends the queue in order until it's empty or there's no connection.
/// Changes Blitzit refused are dropped (and said). Needs `FLUSHING`.
fn flush_locked(app: &AppHandle) -> Vec<String> {
    let mut failures = Vec::new();
    let mut touched: Vec<Option<ListRef>> = Vec::new();
    // Each one is counted as sent before it goes (it may reach Blitzit even
    // if the answer doesn't come back).
    let next = || {
        with_store(app, |store| {
            let op = store.queue.first_mut()?;
            let before = op.clone();
            op.tries += 1;
            save_queue(app, store);
            Some(before)
        })
    };
    while let Some(op) = next() {
        match send(app, &op) {
            Ok(made) => {
                set_offline(false, "");
                log(&format!("sent {:?} {}", op.kind, op.id));
                with_store(app, |store| {
                    // Changes made to it while it was being sent wait as
                    // their own (by the id it had here), right behind it.
                    store.queue.retain(|o| o.id != op.id);
                    apply(&mut store.cache, &op, made.clone());
                    save_queue(app, store);
                    save_cache(app, store);
                });
                let list = blitzit::list_ref(op.list_id.clone(), op.list_name.clone());
                if !touched
                    .iter()
                    .any(|l| l.as_ref().map(|l| &l.id) == list.as_ref().map(|l| &l.id))
                {
                    touched.push(list);
                }
            }
            Err(Fail::Offline(why)) => {
                set_offline(true, &why);
                break;
            }
            Err(Fail::SignedOut(why)) => {
                // Kept until Blitzit is connected again.
                log(&format!("waiting for sign-in: {why}"));
                break;
            }
            Err(Fail::Other(why)) => {
                log(&format!("refused {:?} {}: {why}", op.kind, op.id));
                failures.push(describe(&op, &why));
                with_store(app, |store| {
                    store.queue.retain(|o| o.id != op.id);
                    save_queue(app, store);
                });
            }
        }
    }
    // The lists changed are read again, so the copy here is Blitzit's own.
    for list in touched {
        let key = list.as_ref().map(|l| l.id.clone()).unwrap_or_default();
        if let Ok(items) = blitzit::with_client_ex(app, |client, token| {
            read_tasks(app, client, token, list.as_ref())
        }) {
            with_store(app, |store| {
                store.cache.tasks.insert(key, items);
                store.cache.synced_at = Some(crate::utils::get_now_ms());
                save_cache(app, store);
            });
        }
    }
    failures
}

/// Sends what's waiting whenever something is added, and every few seconds
/// while there's no connection.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        let waiting = with_store(&app, |s| !s.queue.is_empty());
        {
            let (flag, signal) = &KICK;
            let guard = flag.lock().unwrap_or_else(|e| e.into_inner());
            let wait = if waiting {
                Duration::from_secs(15)
            } else {
                Duration::from_secs(300)
            };
            let (mut kicked, _) = signal
                .wait_timeout_while(guard, wait, |kicked| !*kicked)
                .unwrap_or_else(|e| e.into_inner());
            *kicked = false;
        }
        if !blitzit::is_connected(&app) || with_store(&app, |s| s.queue.is_empty()) {
            continue;
        }
        let failures = {
            let _sending = FLUSHING.lock().unwrap_or_else(|e| e.into_inner());
            flush_locked(&app)
        };
        if !failures.is_empty() {
            let _ = app.emit("blitzit-sync-failed", failures);
        }
        let _ = app.emit("blitzit-synced", ());
    });
}

/// Forgets the lists, tasks and waiting changes (another account may connect).
pub fn clear(app: &AppHandle) {
    with_store(app, |store| {
        *store = Store::default();
        save_queue(app, store);
        save_cache(app, store);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(kind: OpKind, task_id: &str, column: Option<&str>) -> Op {
        Op {
            id: "local-1-1".into(),
            kind,
            title: "Gym".into(),
            task_id: task_id.into(),
            column: column.map(str::to_string),
            done: false,
            list_id: Some("L1".into()),
            list_name: Some("Work".into()),
            tries: 0,
            at: 0,
        }
    }

    #[test]
    fn sent_changes_show_in_the_copy_kept_here() {
        let mut cache = Cache::default();
        apply(
            &mut cache,
            &op(OpKind::Add, "", Some("TODAY")),
            Some("T9".into()),
        );
        assert_eq!(cache.tasks["L1"][0].id, "T9");
        assert_eq!(cache.made["local-1-1"], "T9");
        // A change to the task made here finds it by either id.
        apply(
            &mut cache,
            &op(OpKind::Move, "local-1-1", Some("BACKLOG")),
            None,
        );
        assert_eq!(cache.tasks["L1"][0].column, "BACKLOG");
        apply(&mut cache, &op(OpKind::Complete, "T9", None), None);
        assert!(cache.tasks["L1"][0].done);
    }

    #[test]
    fn done_columns_are_known() {
        assert!(is_done_column("DONE"));
        assert!(is_done_column("Completed"));
        assert!(!is_done_column("TODAY"));
    }

    #[test]
    fn old_queue_files_still_read() {
        let raw = r#"[{"id":"local-5-1","kind":"add","title":"Call mum","at":1}]"#;
        let queue: Vec<Op> = serde_json::from_str(raw).unwrap();
        assert_eq!(queue[0].kind, OpKind::Add);
        assert_eq!(queue[0].tries, 0);
    }
}
