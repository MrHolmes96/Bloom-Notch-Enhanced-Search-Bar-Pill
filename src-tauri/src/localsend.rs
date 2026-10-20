//! LocalSend from the shelf: files and text picked in the shelf go straight to
//! a nearby device running LocalSend, without opening the LocalSend app.
//!
//! This follows LocalSend's protocol (v2, github.com/localsend/protocol), as
//! the sending side only, so Bloom never listens on the network:
//!
//! - Finding devices: while the shelf's LocalSend picker is open, Bloom asks
//!   every address of the home network (the /24 of the PC's own address) at
//!   LocalSend's port, `POST /api/localsend/v2/register`, which is LocalSend's
//!   own fallback discovery. Devices answer with their name and kind. Nothing
//!   runs when the picker is closed, and asking only goes out, so Windows'
//!   firewall has nothing to ask about.
//! - Sending: `prepare-upload` with the files' names and sizes (the other
//!   device asks its user to accept), then one `upload` per file with its
//!   contents, `cancel` when stopped. LocalSend's own self-signed HTTPS (or
//!   plain HTTP when its encryption is off), over Windows' WinHTTP.
//!
//! Progress goes to the notch as `localsend-progress`, found devices as
//! `localsend-device`.

use std::ffi::c_void;
use std::io::Read;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts, WinHttpWriteData, ERROR_WINHTTP_INVALID_SERVER_RESPONSE,
    ERROR_WINHTTP_SECURE_FAILURE, SECURITY_FLAG_IGNORE_CERT_CN_INVALID,
    SECURITY_FLAG_IGNORE_CERT_DATE_INVALID, SECURITY_FLAG_IGNORE_CERT_WRONG_USAGE,
    SECURITY_FLAG_IGNORE_UNKNOWN_CA, WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_SECURITY_FLAGS, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE,
};

/// LocalSend's port.
const PORT: u16 = 53317;
const API: &str = "/api/localsend/v2";
/// Devices are asked this many at a time.
const SCAN_THREADS: usize = 48;
/// How long the other device has to accept, as LocalSend's own sender allows.
const ACCEPT_WAIT_MS: i32 = 5 * 60 * 1000;
/// File contents go in pieces this big.
const CHUNK: usize = 256 * 1024;

/// A nearby device running LocalSend.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub alias: String,
    #[serde(rename = "deviceModel")]
    pub device_model: Option<String>,
    /// "mobile", "desktop", "web", "headless" or "server".
    #[serde(rename = "deviceType")]
    pub device_type: String,
    pub ip: String,
    pub port: u16,
    pub https: bool,
}

/// Something from the shelf to send: a file or folder, or text.
#[derive(Clone, Deserialize)]
pub struct SendItem {
    pub path: Option<String>,
    pub text: Option<String>,
}

/// Bumped by every new search and by closing the picker, so an older
/// search's threads stop.
static SEARCH: AtomicU64 = AtomicU64::new(0);
static SEARCHING: AtomicBool = AtomicBool::new(false);
static FOUND: Mutex<Vec<Device>> = Mutex::new(Vec::new());
static CANCELLED: AtomicBool = AtomicBool::new(false);
/// The request being waited on, closed to stop a send at once.
static CURRENT: AtomicIsize = AtomicIsize::new(0);
static FINGERPRINT: OnceLock<String> = OnceLock::new();

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Bloom as LocalSend shows it to the other device: the PC's name.
fn our_info() -> Value {
    let alias = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Bloom".into());
    let fingerprint = FINGERPRINT.get_or_init(|| {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        format!("bloom-{:x}{:x}", seed, std::process::id())
    });
    json!({
        "alias": alias,
        "version": "2.1",
        "deviceModel": "Windows",
        "deviceType": "desktop",
        "fingerprint": fingerprint,
        "port": PORT,
        "protocol": "https",
        "download": false,
    })
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// Why a request failed: no LocalSend there, one that speaks the other
/// protocol (HTTP for HTTPS), or another reason (with its text).
enum Failure {
    Unreachable,
    WrongProtocol,
    Other(String),
}

/// Fills a buffer with the next piece of a file, saying how much it put in.
type Fill<'a> = dyn FnMut(&mut [u8]) -> std::io::Result<usize> + 'a;

struct Answer {
    status: u32,
    body: Vec<u8>,
}

/// One HTTP(S) request to a device. `body` is sent whole, or `stream` writes
/// `length` bytes in pieces (a file); `on_request` sees the open request so
/// a send can be stopped by closing it.
#[allow(clippy::too_many_arguments)]
unsafe fn request(
    session: &Handle,
    device_ip: &str,
    port: u16,
    https: bool,
    path: &str,
    body: Option<&[u8]>,
    stream: Option<(u64, &mut Fill)>,
    remember: bool,
) -> Result<Answer, Failure> {
    let host = wide(device_ip);
    let connection = Handle(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), port, 0));
    if connection.0.is_null() {
        return Err(Failure::Unreachable);
    }
    let verb = wide("POST");
    let object = wide(path);
    let request = Handle(WinHttpOpenRequest(
        connection.0,
        PCWSTR(verb.as_ptr()),
        PCWSTR(object.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        std::ptr::null(),
        if https {
            WINHTTP_FLAG_SECURE
        } else {
            WINHTTP_OPEN_REQUEST_FLAGS(0)
        },
    ));
    if request.0.is_null() {
        return Err(Failure::Other("couldn't start the request".into()));
    }
    if https {
        // LocalSend's certificates are its own (self-signed), as LocalSend
        // itself accepts them.
        let flags = SECURITY_FLAG_IGNORE_UNKNOWN_CA
            | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID
            | SECURITY_FLAG_IGNORE_CERT_CN_INVALID
            | SECURITY_FLAG_IGNORE_CERT_WRONG_USAGE;
        let _ = WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_SECURITY_FLAGS,
            Some(&flags.to_le_bytes()),
        );
    }
    if remember {
        CURRENT.store(request.0 as isize, Ordering::Release);
    }
    let result = (|| {
        let failed = |error: windows::core::Error| {
            let code = (error.code().0 as u32) & 0xFFFF;
            if code == ERROR_WINHTTP_SECURE_FAILURE || code == ERROR_WINHTTP_INVALID_SERVER_RESPONSE
            {
                Failure::WrongProtocol
            } else if CANCELLED.load(Ordering::Acquire) {
                Failure::Other("stopped".into())
            } else {
                Failure::Unreachable
            }
        };
        let headers: Vec<u16> = "Content-Type: application/json\r\n"
            .encode_utf16()
            .collect();
        match (body, stream) {
            (Some(body), _) => WinHttpSendRequest(
                request.0,
                Some(&headers),
                Some(body.as_ptr() as *const c_void),
                body.len() as u32,
                body.len() as u32,
                0,
            )
            .map_err(failed)?,
            (None, Some((length, fill))) => {
                let headers: Vec<u16> = "Content-Type: application/octet-stream\r\n"
                    .encode_utf16()
                    .collect();
                WinHttpSendRequest(
                    request.0,
                    Some(&headers),
                    None,
                    0,
                    length.min(u32::MAX as u64) as u32,
                    0,
                )
                .map_err(failed)?;
                let mut buffer = vec![0u8; CHUNK];
                let mut left = length;
                while left > 0 {
                    if CANCELLED.load(Ordering::Acquire) {
                        return Err(Failure::Other("stopped".into()));
                    }
                    let want = (left as usize).min(CHUNK);
                    let got = fill(&mut buffer[..want])
                        .map_err(|e| Failure::Other(format!("couldn't read the file: {e}")))?;
                    if got == 0 {
                        return Err(Failure::Other("the file got shorter while sending".into()));
                    }
                    let mut written = 0u32;
                    WinHttpWriteData(
                        request.0,
                        Some(buffer.as_ptr() as *const c_void),
                        got as u32,
                        &mut written,
                    )
                    .map_err(failed)?;
                    left -= got as u64;
                }
            }
            (None, None) => {
                WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(failed)?
            }
        }
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(failed)?;
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(failed)?;
        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            if WinHttpQueryDataAvailable(request.0, &mut available).is_err()
                || available == 0
                || body.len() > 1024 * 1024
            {
                break;
            }
            let mut chunk = vec![0u8; available as usize];
            let mut read = 0u32;
            if WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut c_void,
                available,
                &mut read,
            )
            .is_err()
                || read == 0
            {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
        }
        Ok(Answer { status, body })
    })();
    // Stopped meanwhile: `localsend_cancel` closed the request already.
    if remember && CURRENT.swap(0, Ordering::AcqRel) != request.0 as isize {
        std::mem::forget(request);
    }
    result
}

unsafe fn session(connect_ms: i32, wait_ms: i32) -> Option<Handle> {
    let agent = wide("Bloom LocalSend");
    let session = Handle(WinHttpOpen(
        PCWSTR(agent.as_ptr()),
        WINHTTP_ACCESS_TYPE_NO_PROXY,
        PCWSTR::null(),
        PCWSTR::null(),
        0,
    ));
    if session.0.is_null() {
        return None;
    }
    let _ = WinHttpSetTimeouts(session.0, 0, connect_ms, wait_ms, wait_ms);
    Some(session)
}

/// The PC's address on the home network (the one its traffic goes out by).
fn own_address() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    // Nothing is sent: connecting a UDP socket only picks the route.
    socket.connect("1.1.1.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// Asks one address whether LocalSend runs there.
unsafe fn ask(session: &Handle, ip: Ipv4Addr) -> Option<Device> {
    let address = ip.to_string();
    let body = serde_json::to_vec(&our_info()).ok()?;
    let path = format!("{API}/register");
    let (answer, https) = match request(
        session,
        &address,
        PORT,
        true,
        &path,
        Some(&body),
        None,
        false,
    ) {
        Ok(answer) => (answer, true),
        // LocalSend with its encryption turned off.
        Err(Failure::WrongProtocol) => (
            request(
                session,
                &address,
                PORT,
                false,
                &path,
                Some(&body),
                None,
                false,
            )
            .ok()?,
            false,
        ),
        Err(_) => return None,
    };
    if !(200..300).contains(&answer.status) {
        return None;
    }
    let info: Value = serde_json::from_slice(&answer.body).ok()?;
    let alias = info.get("alias")?.as_str()?.trim().to_string();
    if alias.is_empty() {
        return None;
    }
    Some(Device {
        alias,
        device_model: info
            .get("deviceModel")
            .and_then(Value::as_str)
            .map(str::to_string),
        device_type: info
            .get("deviceType")
            .and_then(Value::as_str)
            .unwrap_or("desktop")
            .to_string(),
        ip: address,
        port: PORT,
        https,
    })
}

/// Starts looking for devices (unless already looking) and returns the ones
/// found so far. Found devices arrive as `localsend-device`, the end of the
/// search as `localsend-search-done`.
#[tauri::command]
pub fn localsend_find_devices(app: AppHandle) -> Vec<Device> {
    let found = FOUND.lock().map(|f| f.clone()).unwrap_or_default();
    if SEARCHING.swap(true, Ordering::AcqRel) {
        return found;
    }
    let search = SEARCH.fetch_add(1, Ordering::AcqRel) + 1;
    if let Ok(mut list) = FOUND.lock() {
        list.clear();
    }
    std::thread::spawn(move || {
        let Some(own) = own_address() else {
            SEARCHING.store(false, Ordering::Release);
            let _ = app.emit_to("main", "localsend-search-done", "No network connection");
            return;
        };
        let [a, b, c, mine] = own.octets();
        let next = Arc::new(AtomicUsize::new(1));
        let workers: Vec<_> = (0..SCAN_THREADS)
            .map(|_| {
                let next = next.clone();
                let app = app.clone();
                std::thread::spawn(move || unsafe {
                    let Some(session) = session(700, 1500) else {
                        return;
                    };
                    loop {
                        let host = next.fetch_add(1, Ordering::AcqRel);
                        if host > 254 || SEARCH.load(Ordering::Acquire) != search {
                            break;
                        }
                        if host == mine as usize {
                            continue;
                        }
                        let ip = Ipv4Addr::new(a, b, c, host as u8);
                        if let Some(device) = ask(&session, ip) {
                            if SEARCH.load(Ordering::Acquire) != search {
                                break;
                            }
                            let new = FOUND.lock().is_ok_and(|mut list| {
                                if list.iter().any(|d| d.ip == device.ip) {
                                    return false;
                                }
                                list.push(device.clone());
                                true
                            });
                            if new {
                                let _ = app.emit_to("main", "localsend-device", device);
                            }
                        }
                    }
                })
            })
            .collect();
        for worker in workers {
            let _ = worker.join();
        }
        if SEARCH.load(Ordering::Acquire) == search {
            SEARCHING.store(false, Ordering::Release);
            let _ = app.emit_to("main", "localsend-search-done", "");
        }
    });
    found
}

/// The picker closed: any search stops.
#[tauri::command]
pub fn localsend_stop_finding() {
    SEARCH.fetch_add(1, Ordering::AcqRel);
    SEARCHING.store(false, Ordering::Release);
}

/// Stops a send under way.
#[tauri::command]
pub fn localsend_cancel() {
    CANCELLED.store(true, Ordering::Release);
    let current = CURRENT.swap(0, Ordering::AcqRel);
    if current != 0 {
        // Ends the request the send is waiting on.
        unsafe {
            let _ = WinHttpCloseHandle(current as *mut c_void);
        }
    }
}

/// One file to send, as LocalSend names it (a folder's files carry the
/// folder in their name, "Holiday/IMG_1.jpg", which LocalSend recreates).
struct Outgoing {
    id: String,
    name: String,
    size: u64,
    source: Source,
}

enum Source {
    File(std::path::PathBuf),
    Text(String),
}

fn file_type(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or_default().to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "bmp" => "image/bmp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "txt" | "md" | "log" => "text/plain",
        "zip" => "application/zip",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

/// Everything to send: files, every file in a folder (named under the
/// folder), and text as a text message.
fn outgoing(items: &[SendItem]) -> Result<Vec<Outgoing>, String> {
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<Outgoing>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if out.len() >= 10_000 {
                return;
            }
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                walk(base, &path, out);
            } else if let Ok(relative) = path.strip_prefix(base.parent().unwrap_or(base)) {
                out.push(Outgoing {
                    id: String::new(),
                    name: relative.to_string_lossy().replace('\\', "/"),
                    size: meta.len(),
                    source: Source::File(path),
                });
            }
        }
    }
    let mut out = Vec::new();
    for item in items {
        if let Some(text) = item.text.as_ref().filter(|t| !t.is_empty()) {
            out.push(Outgoing {
                id: String::new(),
                name: "message.txt".into(),
                size: text.len() as u64,
                source: Source::Text(text.clone()),
            });
            continue;
        }
        let Some(path) = item.path.as_ref() else {
            continue;
        };
        let path = std::path::PathBuf::from(path);
        let meta = std::fs::metadata(&path).map_err(|_| {
            format!(
                "{} isn't there any more",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        })?;
        if meta.is_dir() {
            walk(&path, &path, &mut out);
        } else {
            out.push(Outgoing {
                id: String::new(),
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into()),
                size: meta.len(),
                source: Source::File(path),
            });
        }
    }
    for (index, file) in out.iter_mut().enumerate() {
        file.id = format!("bloom{index}");
    }
    Ok(out)
}

#[derive(Clone, Serialize)]
struct Progress {
    /// "waiting" (for the other device to accept), "sending", "done",
    /// "declined", "failed" or "stopped".
    state: &'static str,
    sent: u64,
    total: u64,
    device: String,
    message: String,
}

fn report(
    app: &AppHandle,
    state: &'static str,
    sent: u64,
    total: u64,
    device: &str,
    message: &str,
) {
    let _ = app.emit_to(
        "main",
        "localsend-progress",
        Progress {
            state,
            sent,
            total,
            device: device.to_string(),
            message: message.to_string(),
        },
    );
}

/// What the other device's refusal means, in words.
fn refusal(status: u32) -> (&'static str, &'static str) {
    match status {
        403 => ("declined", "Declined on the other device"),
        401 => (
            "failed",
            "That device asks for a PIN: turn off its PIN in LocalSend to send from Bloom",
        ),
        409 => ("failed", "That device is busy with another transfer"),
        429 => (
            "failed",
            "That device is getting too many requests; try again shortly",
        ),
        _ => ("failed", "That device couldn't take the files"),
    }
}

/// Sends the items to the device. Runs until done; progress is reported
/// along the way and the end result returned.
#[tauri::command]
pub async fn localsend_send(
    app: AppHandle,
    device: Device,
    items: Vec<SendItem>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || send_now(&app, &device, &items))
        .await
        .map_err(|e| e.to_string())?
}

fn send_now(app: &AppHandle, device: &Device, items: &[SendItem]) -> Result<(), String> {
    CANCELLED.store(false, Ordering::Release);
    let files = outgoing(items)?;
    if files.is_empty() {
        return Err("Nothing to send".into());
    }
    let total: u64 = files.iter().map(|f| f.size).sum();
    let alias = device.alias.as_str();
    let mut listing = serde_json::Map::new();
    for file in &files {
        let mut meta = json!({
            "id": file.id,
            "fileName": file.name,
            "size": file.size,
            "fileType": match file.source {
                Source::Text(_) => "text/plain",
                Source::File(_) => file_type(&file.name),
            },
        });
        if let Source::Text(text) = &file.source {
            // LocalSend shows a text's preview as the message itself.
            meta["preview"] = Value::String(text.clone());
        }
        listing.insert(file.id.clone(), meta);
    }
    let body = serde_json::to_vec(&json!({ "info": our_info(), "files": listing }))
        .map_err(|e| e.to_string())?;

    report(app, "waiting", 0, total, alias, "");
    let finish = |state: &'static str, message: &str| -> Result<(), String> {
        report(app, state, 0, total, alias, message);
        if state == "done" {
            Ok(())
        } else {
            Err(message.to_string())
        }
    };
    unsafe {
        let Some(waiting) = session(3000, ACCEPT_WAIT_MS) else {
            return finish("failed", "Windows' network connection isn't available");
        };
        let answer = match request(
            &waiting,
            &device.ip,
            device.port,
            device.https,
            &format!("{API}/prepare-upload"),
            Some(&body),
            None,
            true,
        ) {
            Ok(answer) => answer,
            Err(_) if CANCELLED.load(Ordering::Acquire) => return finish("stopped", "Stopped"),
            Err(Failure::Other(text)) => return finish("failed", &text),
            Err(_) => return finish("failed", &format!("Couldn't reach {alias}")),
        };
        if answer.status == 204 {
            return finish("done", "Nothing needed sending");
        }
        if !(200..300).contains(&answer.status) {
            let (state, message) = refusal(answer.status);
            return finish(state, message);
        }
        let reply: Value = serde_json::from_slice(&answer.body).map_err(|e| e.to_string())?;
        let session_id = reply
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let tokens = reply
            .get("files")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        let Some(sending) = session(3000, 60_000) else {
            return finish("failed", "Windows' network connection isn't available");
        };
        let mut sent: u64 = 0;
        let mut last_report = std::time::Instant::now();
        for file in &files {
            // Files the other side turned down (it can accept some) are skipped.
            let Some(token) = tokens.get(&file.id).and_then(Value::as_str) else {
                sent += file.size;
                continue;
            };
            let path = format!(
                "{API}/upload?sessionId={}&fileId={}&token={}",
                crate::integrations::http::encode(&session_id),
                crate::integrations::http::encode(&file.id),
                crate::integrations::http::encode(token)
            );
            let base = sent;
            let mut done_here: u64 = 0;
            let result = match &file.source {
                Source::Text(text) => request(
                    &sending,
                    &device.ip,
                    device.port,
                    device.https,
                    &path,
                    Some(text.as_bytes()),
                    None,
                    true,
                ),
                Source::File(location) => {
                    let mut reader = match std::fs::File::open(location) {
                        Ok(reader) => reader,
                        Err(_) => {
                            cancel_session(device, &session_id);
                            return finish("failed", &format!("Couldn't open {}", file.name));
                        }
                    };
                    let mut fill = |buffer: &mut [u8]| -> std::io::Result<usize> {
                        let read = reader.read(buffer)?;
                        done_here += read as u64;
                        if last_report.elapsed().as_millis() >= 200 {
                            last_report = std::time::Instant::now();
                            report(app, "sending", base + done_here, total, alias, &file.name);
                        }
                        Ok(read)
                    };
                    request(
                        &sending,
                        &device.ip,
                        device.port,
                        device.https,
                        &path,
                        None,
                        Some((file.size, &mut fill)),
                        true,
                    )
                }
            };
            sent = base + file.size;
            match result {
                Ok(answer) if (200..300).contains(&answer.status) => {}
                _ if CANCELLED.load(Ordering::Acquire) => {
                    cancel_session(device, &session_id);
                    return finish("stopped", "Stopped");
                }
                Ok(answer) => {
                    cancel_session(device, &session_id);
                    let (state, message) = refusal(answer.status);
                    return finish(state, message);
                }
                Err(Failure::Other(text)) => {
                    cancel_session(device, &session_id);
                    return finish("failed", &text);
                }
                Err(_) => {
                    cancel_session(device, &session_id);
                    return finish("failed", &format!("Lost the connection to {alias}"));
                }
            }
            report(app, "sending", sent, total, alias, &file.name);
        }
        report(app, "done", total, total, alias, "");
        Ok(())
    }
}

/// Tells the other device the send stopped, so it doesn't wait for it.
fn cancel_session(device: &Device, session_id: &str) {
    if session_id.is_empty() {
        return;
    }
    unsafe {
        if let Some(session) = session(1500, 2000) {
            let _ = request(
                &session,
                &device.ip,
                device.port,
                device.https,
                &format!(
                    "{API}/cancel?sessionId={}",
                    crate::integrations::http::encode(session_id)
                ),
                None,
                None,
                false,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_types_follow_extensions() {
        assert_eq!(file_type("IMG_1.JPG"), "image/jpeg");
        assert_eq!(file_type("notes.txt"), "text/plain");
        assert_eq!(file_type("archive.tar.gz"), "application/octet-stream");
    }

    #[test]
    fn refusals_say_what_happened() {
        assert_eq!(refusal(403).0, "declined");
        assert_eq!(refusal(409).0, "failed");
    }
}
