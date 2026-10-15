//! File and folder search through the Windows Search index, the same way
//! Flow Launcher's Explorer plugin does it: the index's query helper turns
//! what you typed into Windows Search SQL, and OLE DB runs it against the
//! SystemIndex catalog.
//!
//! The query runs on one worker thread that keeps its connection to the
//! indexer open. When queries pile up while you type, only the newest runs.

use serde::Serialize;
use std::ffi::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use windows::core::{IUnknown, Interface, GUID, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Search::{
    CSearchManager, IAccessor, ICommand, ICommandText, IDBCreateCommand, IDBCreateSession,
    IDBInitialize, IDataInitialize, IRowset, ISearchManager, ISearchQueryHelper,
    DBACCESSOR_ROWDATA, DBBINDING, DBMEMOWNER_CLIENTOWNED, DBPARAMIO_NOTPARAM, DBPART_LENGTH,
    DBPART_STATUS, DBPART_VALUE, DBSTATUS_S_OK, DBTYPE_WSTR, HACCESSOR, MSDAINITIALIZE,
};

/// The default SQL dialect for ICommandText.
const DBGUID_DEFAULT: GUID = GUID::from_u128(0xc8b521fb_5cf3_11ce_ade5_00aa0044773d);
/// Characters per string column; longer paths come back truncated and are skipped.
const COLUMN_CHARS: usize = 1024;
const COLUMNS: usize = 3;

#[derive(Serialize, Clone)]
pub struct IndexHit {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

#[repr(C)]
struct Column {
    status: u32,
    _pad: u32,
    length: usize,
    value: [u16; COLUMN_CHARS],
}

#[repr(C)]
struct Row {
    columns: [Column; COLUMNS],
}

enum Mode {
    /// File and folder names, with the indexer's own query syntax.
    Name,
    /// Words inside files.
    Content,
}

struct Request {
    query: String,
    mode: Mode,
    max: u32,
    excluded: Vec<String>,
    reply: Sender<Result<Vec<IndexHit>, String>>,
}

static WORKER: OnceLock<Mutex<Sender<Request>>> = OnceLock::new();

fn worker() -> &'static Mutex<Sender<Request>> {
    WORKER.get_or_init(|| {
        let (tx, rx) = channel::<Request>();
        std::thread::spawn(move || run_worker(rx));
        Mutex::new(tx)
    })
}

struct Connection {
    helper: ISearchQueryHelper,
    session_factory: IDBCreateSession,
}

unsafe fn connect() -> Result<Connection, String> {
    let manager: ISearchManager = CoCreateInstance(&CSearchManager, None, CLSCTX_ALL)
        .map_err(|_| "Windows Search is turned off".to_string())?;
    let catalog = manager
        .GetCatalog(windows::core::w!("SystemIndex"))
        .map_err(|e| e.message())?;
    let helper = catalog.GetQueryHelper().map_err(|e| e.message())?;
    let connection_string = helper.ConnectionString().map_err(|e| e.message())?;

    let init: IDataInitialize =
        CoCreateInstance(&MSDAINITIALIZE, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message())?;
    let mut source: Option<IUnknown> = None;
    let result = init.GetDataSource(
        None,
        CLSCTX_INPROC_SERVER.0,
        PCWSTR(connection_string.0),
        &IDBInitialize::IID,
        &mut source,
    );
    CoTaskMemFree(Some(connection_string.0 as *const c_void));
    result.map_err(|e| e.message())?;
    let source = source.ok_or("no data source")?;
    let db: IDBInitialize = source.cast().map_err(|e| e.message())?;
    db.Initialize().map_err(|e| e.message())?;
    let session_factory: IDBCreateSession = db.cast().map_err(|e| e.message())?;
    Ok(Connection {
        helper,
        session_factory,
    })
}

fn run_worker(rx: Receiver<Request>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut connection: Option<Connection> = None;
    while let Ok(mut request) = rx.recv() {
        // Typing queues a query per keystroke; answer the stale ones empty.
        while let Ok(newer) = rx.try_recv() {
            let _ = request.reply.send(Ok(Vec::new()));
            request = newer;
        }
        if connection.is_none() {
            connection = unsafe { connect() }
                .map_err(|e| {
                    let _ = request.reply.send(Err(e));
                })
                .ok();
            if connection.is_none() {
                continue;
            }
        }
        let conn = connection.as_ref().unwrap();
        let result = unsafe { search(conn, &request) };
        // A broken connection (the indexer restarted) is rebuilt next time.
        if result.is_err() {
            connection = None;
        }
        let _ = request.reply.send(result);
    }
}

/// Escapes text for a single-quoted Windows Search SQL string.
fn sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

/// Flow's clean-up of a query before the indexer sees it: runs of special
/// characters become spaces, so "my-file_v2" still finds things.
fn clean_query(s: &str) -> String {
    let special = "@#&*_;,%|!(){}[]^~?\\\"/:=-";
    let replaced: String = s
        .chars()
        .map(|c| if special.contains(c) { ' ' } else { c })
        .collect();
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

unsafe fn build_sql(conn: &Connection, request: &Request) -> Result<String, String> {
    let helper = &conn.helper;
    let columns = "System.ItemPathDisplay, System.FileName, System.ItemType";
    match request.mode {
        Mode::Name => {
            let query = clean_query(&request.query);
            if query.is_empty() {
                return Err("empty query".into());
            }
            helper
                .SetQueryMaxResults(request.max as i32)
                .map_err(|e| e.message())?;
            helper
                .SetQuerySelectColumns(&windows::core::HSTRING::from(columns))
                .map_err(|e| e.message())?;
            helper
                .SetQueryContentProperties(windows::core::w!("System.FileName"))
                .map_err(|e| e.message())?;
            helper
                .SetQueryWhereRestrictions(windows::core::w!("AND scope='file:'"))
                .map_err(|e| e.message())?;
            helper
                .SetQuerySorting(windows::core::w!("System.Search.Rank DESC"))
                .map_err(|e| e.message())?;
            let sql = helper
                .GenerateSQLFromUserQuery(&windows::core::HSTRING::from(query))
                .map_err(|e| e.message())?;
            let text = sql.to_string().map_err(|e| e.to_string());
            CoTaskMemFree(Some(sql.0 as *const c_void));
            text
        }
        Mode::Content => {
            let words = request.query.trim();
            if words.is_empty() {
                return Err("empty query".into());
            }
            Ok(format!(
                "SELECT TOP {} {columns} FROM SystemIndex WHERE FREETEXT('{}') AND scope='file:' ORDER BY System.Search.Rank DESC",
                request.max,
                sql_string(words)
            ))
        }
    }
}

unsafe fn read_column(column: &Column) -> Option<String> {
    // Truncated paths are skipped: opening half a path would be wrong.
    if column.status as i32 != DBSTATUS_S_OK.0 {
        return None;
    }
    let chars = (column.length / 2).min(COLUMN_CHARS);
    let text = String::from_utf16_lossy(&column.value[..chars]);
    let text = text.trim_end_matches('\0').to_string();
    (!text.is_empty()).then_some(text)
}

unsafe fn search(conn: &Connection, request: &Request) -> Result<Vec<IndexHit>, String> {
    let sql = build_sql(conn, request)?;
    let session = conn
        .session_factory
        .CreateSession(None, &IDBCreateCommand::IID)
        .map_err(|e| e.message())?;
    let command_factory: IDBCreateCommand = session.cast().map_err(|e| e.message())?;
    let command = command_factory
        .CreateCommand(None, &ICommandText::IID)
        .map_err(|e| e.message())?;
    let text: ICommandText = command.cast().map_err(|e| e.message())?;
    text.SetCommandText(&DBGUID_DEFAULT, &windows::core::HSTRING::from(sql))
        .map_err(|e| e.message())?;
    let command: ICommand = text.cast().map_err(|e| e.message())?;
    let mut rowset: Option<IUnknown> = None;
    command
        .Execute(None, &IRowset::IID, None, None, Some(&mut rowset))
        .map_err(|e| e.message())?;
    let Some(rowset) = rowset else {
        return Ok(Vec::new());
    };
    let rowset: IRowset = rowset.cast().map_err(|e| e.message())?;
    let accessor: IAccessor = rowset.cast().map_err(|e| e.message())?;

    let base = std::mem::offset_of!(Row, columns);
    let column_size = std::mem::size_of::<Column>();
    let bindings: Vec<DBBINDING> = (0..COLUMNS)
        .map(|i| {
            let at = base + i * column_size;
            DBBINDING {
                iOrdinal: i + 1,
                obValue: at + std::mem::offset_of!(Column, value),
                obLength: at + std::mem::offset_of!(Column, length),
                obStatus: at + std::mem::offset_of!(Column, status),
                dwPart: (DBPART_VALUE.0 | DBPART_LENGTH.0 | DBPART_STATUS.0) as u32,
                dwMemOwner: DBMEMOWNER_CLIENTOWNED.0 as u32,
                eParamIO: DBPARAMIO_NOTPARAM.0 as u32,
                cbMaxLen: COLUMN_CHARS * 2,
                wType: DBTYPE_WSTR.0 as u16,
                ..Default::default()
            }
        })
        .collect();
    let mut handle = HACCESSOR::default();
    accessor
        .CreateAccessor(
            DBACCESSOR_ROWDATA.0 as u32,
            COLUMNS,
            bindings.as_ptr(),
            std::mem::size_of::<Row>(),
            &mut handle,
            None,
        )
        .map_err(|e| e.message())?;

    let excluded: Vec<String> = request
        .excluded
        .iter()
        .map(|p| p.trim_end_matches('\\').to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let mut hits = Vec::new();
    let mut row: Box<Row> = Box::new(std::mem::zeroed());
    'fetch: loop {
        let mut obtained = 0usize;
        let mut rows: *mut usize = std::ptr::null_mut();
        // The generated wrapper ties the row count to a slice length; call
        // the vtable directly with a provider-allocated row array instead.
        let hr = (Interface::vtable(&rowset).GetNextRows)(
            rowset.as_raw(),
            0,
            0,
            32,
            &mut obtained,
            &mut rows,
        );
        if hr.is_err() || obtained == 0 || rows.is_null() {
            if !rows.is_null() {
                CoTaskMemFree(Some(rows as *const c_void));
            }
            break;
        }
        let handles = std::slice::from_raw_parts(rows, obtained);
        for &hrow in handles {
            std::ptr::write_bytes(&mut *row as *mut Row, 0, 1);
            if rowset
                .GetData(hrow, handle, &mut *row as *mut Row as *mut c_void)
                .is_err()
            {
                continue;
            }
            let Some(path) = read_column(&row.columns[0]) else {
                continue;
            };
            let lower = path.to_lowercase();
            if excluded
                .iter()
                .any(|e| lower == *e || lower.starts_with(&format!("{e}\\")))
            {
                continue;
            }
            let name = read_column(&row.columns[1])
                .unwrap_or_else(|| path.rsplit('\\').next().unwrap_or(&path).to_string());
            let is_dir = read_column(&row.columns[2]).is_some_and(|t| t == "Directory");
            hits.push(IndexHit { path, name, is_dir });
        }
        let _ = rowset.ReleaseRows(
            obtained,
            rows,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        CoTaskMemFree(Some(rows as *const c_void));
        if hits.len() >= request.max as usize {
            break 'fetch;
        }
    }
    let _ = accessor.ReleaseAccessor(handle, None);
    hits.truncate(request.max as usize);
    Ok(hits)
}

/// Searches the Windows index. `mode` is "name" (file and folder names) or
/// "content" (words inside files). Paths under `excluded` are left out.
#[tauri::command]
pub async fn launcher_search_index(
    query: String,
    mode: String,
    max: u32,
    excluded: Vec<String>,
) -> Result<Vec<IndexHit>, String> {
    let (reply, wait) = channel();
    let request = Request {
        query,
        mode: if mode == "content" {
            Mode::Content
        } else {
            Mode::Name
        },
        max: max.clamp(1, 500),
        excluded,
        reply,
    };
    worker()
        .lock()
        .map_err(|_| "search worker unavailable".to_string())?
        .send(request)
        .map_err(|_| "search worker stopped".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        wait.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| "Windows Search did not answer".to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_queries_like_flow() {
        assert_eq!(clean_query("my-file_v2"), "my file v2");
        assert_eq!(clean_query("  report  2026 "), "report 2026");
        assert_eq!(clean_query("@#!"), "");
        assert_eq!(sql_string("it's"), "it''s");
    }
}
