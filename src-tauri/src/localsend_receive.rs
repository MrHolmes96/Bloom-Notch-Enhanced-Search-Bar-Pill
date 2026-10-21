//! LocalSend into the shelf: files and text sent from a phone or another
//! device with LocalSend land in the notch's shelf.
//!
//! Receiving is off until turned on with the shelf's "LocalSend" button, and
//! turns itself off again after a while without a transfer (set in the
//! shelf's settings, `bloom-shelf-localsend-receive-minutes`), and whenever
//! Bloom starts. Nothing of it runs while it is off.
//!
//! While on, Bloom is a LocalSend device as LocalSend's protocol describes it
//! (v2, github.com/localsend/protocol):
//!
//! - It answers on LocalSend's port over HTTPS (TLS with a certificate of its
//!   own, made once and kept in Bloom's data folder; its SHA-256 is the
//!   device's fingerprint), so everything sent to it is encrypted.
//! - It says it's there on LocalSend's multicast group as it turns on, and
//!   answers devices that look for others there (with `register`, else on
//!   the group). Devices that look by asking each address get their answer
//!   from `register` too.
//! - `prepare-upload` is accepted at once (receiving is on because it was
//!   asked for), `upload` saves each file in the Downloads folder (a folder
//!   sent keeps its name and inside), and `cancel` ends a transfer. Text sent
//!   as a message goes to the shelf as text. A second device sending while a
//!   transfer runs is told the PC is busy, as LocalSend does.
//!
//! The network part is plain Rust (std sockets and rustls) and is tested on
//! any system; the parts that need Windows or Tauri are marked `cfg(windows)`.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// LocalSend's port.
pub const PORT: u16 = 53317;
/// LocalSend's multicast group.
const MULTICAST: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);
const API: &str = "/api/localsend/v2";
/// A transfer nothing happened in for this long is over.
const SESSION_IDLE: Duration = Duration::from_secs(5 * 60);
/// Requests answered at once, at most.
const MAX_CONNECTIONS: usize = 16;
/// Largest request (other than a file) taken in, in bytes.
const MAX_JSON: u64 = 4 * 1024 * 1024;
/// Longest wait for the other device mid-request.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// What the receiver needs from the rest of Bloom.
pub trait Host: Send + Sync + 'static {
    /// Where received files go (the Downloads folder).
    fn folder(&self) -> PathBuf;
    /// Files (or folders) saved and text received, from the named device.
    fn received(&self, paths: Vec<PathBuf>, texts: Vec<String>, from: &str);
    /// Something was received or is on its way: receiving stays on.
    fn activity(&self);
    /// Answers a device that announced itself, over its own HTTP(S) port.
    /// False when it couldn't be reached that way.
    fn register_with(&self, ip: &str, port: u16, https: bool, info: &Value) -> bool;
}

/// This PC as LocalSend shows it.
pub fn device_info(alias: &str, fingerprint: &str, port: u16) -> Value {
    json!({
        "alias": alias,
        "version": "2.1",
        "deviceModel": "Windows",
        "deviceType": "desktop",
        "fingerprint": fingerprint,
        "port": port,
        "protocol": "https",
        "download": false,
    })
}

// ---------------------------------------------------------------- identity

/// The certificate the PC answers with, its private key, and its SHA-256
/// (LocalSend's fingerprint).
pub struct Identity {
    pub cert: Vec<u8>,
    pub key: Vec<u8>,
    pub fingerprint: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn fingerprint_of(cert: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, cert).as_ref())
}

/// The PC's certificate, made the first time and kept in `folder`, so other
/// devices know it as the same device every time.
pub fn identity(folder: &Path) -> Result<Identity, String> {
    let cert_path = folder.join("localsend-cert.der");
    let key_path = folder.join("localsend-key.der");
    if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        if tls_config(&cert, &key).is_ok() {
            let fingerprint = fingerprint_of(&cert);
            return Ok(Identity {
                cert,
                key,
                fingerprint,
            });
        }
    }
    let made = rcgen::generate_simple_self_signed(vec!["localsend".to_string()])
        .map_err(|e| format!("couldn't make LocalSend's certificate: {e}"))?;
    let cert = made.cert.der().to_vec();
    let key = made.signing_key.serialize_der();
    let _ = std::fs::create_dir_all(folder);
    let _ = std::fs::write(&cert_path, &cert);
    let _ = std::fs::write(&key_path, &key);
    let fingerprint = fingerprint_of(&cert);
    Ok(Identity {
        cert,
        key,
        fingerprint,
    })
}

pub fn tls_config(cert: &[u8], key: &[u8]) -> Result<Arc<rustls::ServerConfig>, String> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())),
        )
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(config))
}

// ---------------------------------------------------------------- names on disk

/// A name sent by the other device as a path inside the Downloads folder:
/// its folders and file name, with nothing that could reach outside it or
/// that Windows doesn't allow in a name.
pub fn safe_relative(name: &str) -> Option<PathBuf> {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let mut path = PathBuf::new();
    let mut parts = 0;
    for part in name.split(['/', '\\']) {
        let cleaned: String = part
            .chars()
            .map(|c| {
                if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
        if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
            continue;
        }
        let stem = cleaned.split('.').next().unwrap_or_default().to_lowercase();
        let cleaned = if RESERVED.contains(&stem.as_str()) {
            format!("_{cleaned}")
        } else {
            cleaned
        };
        path.push(cleaned.chars().take(200).collect::<String>());
        parts += 1;
        if parts >= 16 {
            break;
        }
    }
    (parts > 0).then_some(path)
}

/// `path`, or "name (1).ext", "name (2).ext"… when that is taken.
pub fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..10_000)
        .map(|n| parent.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

// ---------------------------------------------------------------- HTTP

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let byte = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|digits| u8::from_str_radix(digits, 16).ok());
                match byte {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A request's first line and headers.
pub fn parse_head(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    let (path, query_text) = target.split_once('?').unwrap_or((target, ""));
    let query = query_text
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(key), decode(value))
        })
        .collect();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    Some(Request {
        method,
        path: path.to_string(),
        query,
        headers,
    })
}

/// Reads up to the end of a request's headers; what came after them is the
/// start of its body.
fn read_head(stream: &mut impl Read) -> std::io::Result<Option<(Request, Vec<u8>)>> {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
            let rest = buffer[end + 4..].to_vec();
            return Ok(parse_head(&head).map(|request| (request, rest)));
        }
        if buffer.len() > 32 * 1024 {
            return Ok(None);
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// The bytes after the headers, then the rest of the stream.
struct Prefixed<'a, R: Read> {
    start: Vec<u8>,
    at: usize,
    inner: &'a mut R,
}

impl<R: Read> Read for Prefixed<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.at < self.start.len() {
            let count = (self.start.len() - self.at).min(buffer.len());
            buffer[..count].copy_from_slice(&self.start[self.at..self.at + count]);
            self.at += count;
            return Ok(count);
        }
        self.inner.read(buffer)
    }
}

/// A body sent in chunks (`Transfer-Encoding: chunked`).
struct Chunked<R: Read> {
    inner: R,
    left: u64,
    finished: bool,
}

impl<R: Read> Chunked<R> {
    fn line(&mut self) -> std::io::Result<String> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if self.inner.read(&mut byte)? == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
            if line.len() > 1024 {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
        }
        Ok(String::from_utf8_lossy(&line).trim().to_string())
    }
}

impl<R: Read> Read for Chunked<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.finished || buffer.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            let size_line = self.line()?;
            let size = size_line.split(';').next().unwrap_or_default();
            self.left = u64::from_str_radix(size.trim(), 16)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
            if self.left == 0 {
                // Trailers, up to the empty line.
                while !self.line()?.is_empty() {}
                self.finished = true;
                return Ok(0);
            }
        }
        let want = (self.left as usize).min(buffer.len());
        let read = self.inner.read(&mut buffer[..want])?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        self.left -= read as u64;
        if self.left == 0 {
            // The line end after the chunk.
            self.line()?;
        }
        Ok(read)
    }
}

/// The request's body as a reader: as long as its length says, or in chunks.
fn body<'a, R: Read + 'a>(
    request: &Request,
    start: Vec<u8>,
    stream: &'a mut R,
) -> Box<dyn Read + 'a> {
    let prefixed = Prefixed {
        start,
        at: 0,
        inner: stream,
    };
    let chunked = request
        .headers
        .get("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));
    if chunked {
        Box::new(Chunked {
            inner: prefixed,
            left: 0,
            finished: false,
        })
    } else {
        let length = request
            .headers
            .get("content-length")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        Box::new(prefixed.take(length))
    }
}

fn read_json(reader: &mut dyn Read) -> Option<Value> {
    let mut bytes = Vec::new();
    reader.take(MAX_JSON).read_to_end(&mut bytes).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn respond(stream: &mut impl Write, status: u16, body: Option<&Value>) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    let bytes = body
        .map(|value| serde_json::to_vec(value).unwrap_or_default())
        .unwrap_or_default();
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(&bytes)?;
    stream.flush()
}

// ---------------------------------------------------------------- transfers

struct Incoming {
    name: String,
    size: u64,
    token: String,
    done: bool,
}

struct Session {
    id: String,
    from: String,
    files: HashMap<String, Incoming>,
    /// A folder sent, by its name, as it is saved (made unique).
    folders: HashMap<String, PathBuf>,
    /// What went to the shelf already (a folder goes once).
    shown: HashSet<PathBuf>,
    touched: Instant,
}

impl Session {
    fn finished(&self) -> bool {
        self.files.values().all(|file| file.done)
    }
}

fn random_id() -> String {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 16];
    let _ = ring::rand::SystemRandom::new().fill(&mut bytes);
    hex(&bytes)
}

struct Shared {
    host: Arc<dyn Host>,
    info: Value,
    fingerprint: String,
    session: Mutex<Option<Session>>,
}

/// Text sent as a message: one "file" of plain text whose contents come
/// with it (as LocalSend sends a message), so nothing needs uploading.
fn as_text(file: &Value) -> Option<String> {
    let kind = file
        .get("fileType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let preview = file.get("preview").and_then(Value::as_str)?;
    let size = file.get("size").and_then(Value::as_u64).unwrap_or(u64::MAX);
    (kind.starts_with("text/plain") && preview.len() as u64 == size).then(|| preview.to_string())
}

fn prepare(shared: &Shared, request: &Value) -> (u16, Option<Value>) {
    let from = request
        .pointer("/info/alias")
        .and_then(Value::as_str)
        .unwrap_or("A device")
        .to_string();
    let Some(files) = request.get("files").and_then(Value::as_object) else {
        return (400, None);
    };
    let Ok(mut current) = shared.session.lock() else {
        return (500, None);
    };
    if current
        .as_ref()
        .is_some_and(|session| !session.finished() && session.touched.elapsed() < SESSION_IDLE)
    {
        return (
            409,
            Some(json!({ "message": "Blocked by another session" })),
        );
    }
    shared.host.activity();
    let mut texts = Vec::new();
    let mut incoming = HashMap::new();
    for (id, file) in files {
        if let Some(text) = as_text(file) {
            texts.push(text);
            continue;
        }
        let name = file
            .get("fileName")
            .and_then(Value::as_str)
            .unwrap_or("file")
            .to_string();
        let size = file.get("size").and_then(Value::as_u64).unwrap_or(0);
        incoming.insert(
            id.clone(),
            Incoming {
                name,
                size,
                token: random_id(),
                done: false,
            },
        );
    }
    if !texts.is_empty() {
        shared.host.received(Vec::new(), texts, &from);
    }
    if incoming.is_empty() {
        *current = None;
        return (204, None);
    }
    let tokens: serde_json::Map<String, Value> = incoming
        .iter()
        .map(|(id, file)| (id.clone(), Value::String(file.token.clone())))
        .collect();
    let session = Session {
        id: random_id(),
        from,
        files: incoming,
        folders: HashMap::new(),
        shown: HashSet::new(),
        touched: Instant::now(),
    };
    let reply = json!({ "sessionId": session.id, "files": tokens });
    *current = Some(session);
    (200, Some(reply))
}

/// Where an upload goes, and what of it the shelf shows (the file, or the
/// folder it came in).
fn destination(shared: &Shared, session: &mut Session, name: &str) -> Option<(PathBuf, PathBuf)> {
    let relative = safe_relative(name)?;
    let folder = shared.host.folder();
    let mut parts = relative.components();
    let first = parts.next()?.as_os_str().to_string_lossy().into_owned();
    let rest: PathBuf = parts.collect();
    if rest.as_os_str().is_empty() {
        let path = unique(&folder.join(&first));
        return Some((path.clone(), path));
    }
    let top = session
        .folders
        .entry(first.clone())
        .or_insert_with(|| unique(&folder.join(&first)))
        .clone();
    Some((unique(&top.join(rest)), top))
}

fn upload(shared: &Shared, request: &Request, reader: &mut dyn Read) -> u16 {
    let session_id = request.query.get("sessionId").cloned().unwrap_or_default();
    let file_id = request.query.get("fileId").cloned().unwrap_or_default();
    let token = request.query.get("token").cloned().unwrap_or_default();
    // Look the file up and pick its place, without holding the transfer
    // while its contents arrive.
    let (target, shown, size, from) = {
        let Ok(mut current) = shared.session.lock() else {
            return 500;
        };
        let Some(session) = current.as_mut().filter(|session| session.id == session_id) else {
            return 403;
        };
        let Some((name, size)) = session
            .files
            .get(&file_id)
            .filter(|file| file.token == token && !file.done)
            .map(|file| (file.name.clone(), file.size))
        else {
            return 403;
        };
        session.touched = Instant::now();
        let Some((target, shown)) = destination(shared, session, &name) else {
            return 400;
        };
        (target, shown, size, session.from.clone())
    };
    shared.host.activity();
    if let Some(parent) = target.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return 500;
        }
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    else {
        return 500;
    };
    let copied = std::io::copy(&mut reader.take(size), &mut file);
    let complete = matches!(copied, Ok(count) if count == size) && file.flush().is_ok();
    drop(file);
    if !complete {
        let _ = std::fs::remove_file(&target);
        return 400;
    }
    let show = {
        let Ok(mut current) = shared.session.lock() else {
            return 200;
        };
        let mut show = false;
        if let Some(session) = current.as_mut().filter(|session| session.id == session_id) {
            if let Some(entry) = session.files.get_mut(&file_id) {
                entry.done = true;
            }
            session.touched = Instant::now();
            show = session.shown.insert(shown.clone());
            if session.finished() {
                *current = None;
            }
        }
        show
    };
    if show {
        shared.host.received(vec![shown], Vec::new(), &from);
    }
    shared.host.activity();
    200
}

fn cancel(shared: &Shared, request: &Request) {
    let session_id = request.query.get("sessionId").cloned().unwrap_or_default();
    if let Ok(mut current) = shared.session.lock() {
        if current
            .as_ref()
            .is_some_and(|session| session.id == session_id)
        {
            *current = None;
        }
    }
}

/// Answers one request on a connection, then closes it.
fn serve(shared: &Shared, stream: &mut (impl Read + Write)) -> std::io::Result<()> {
    let Some((request, start)) = read_head(stream)? else {
        return Ok(());
    };
    let route = request
        .path
        .strip_prefix(API)
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();
    let mut reader = body(&request, start, stream);
    let (status, reply) = match (request.method.as_str(), route.as_str()) {
        ("POST", "/register") => {
            let _ = read_json(&mut reader);
            (200, Some(shared.info.clone()))
        }
        ("GET", "/info") => (200, Some(shared.info.clone())),
        ("POST", "/prepare-upload") => match read_json(&mut reader) {
            Some(value) => prepare(shared, &value),
            None => (400, None),
        },
        ("POST", "/upload") => (upload(shared, &request, &mut reader), None),
        ("POST", "/cancel") => {
            cancel(shared, &request);
            (200, None)
        }
        _ => (404, None),
    };
    drop(reader);
    respond(stream, status, reply.as_ref())
}

fn connection(shared: Arc<Shared>, tls: Arc<rustls::ServerConfig>, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_nodelay(true);
    let Ok(connection) = rustls::ServerConnection::new(tls) else {
        return;
    };
    let mut secure = rustls::StreamOwned::new(connection, stream);
    let _ = serve(&shared, &mut secure);
    secure.conn.send_close_notify();
    let _ = secure.flush();
}

// ---------------------------------------------------------------- finding each other

fn announcement(info: &Value, announce: bool) -> Vec<u8> {
    let mut message = info.clone();
    message["announce"] = Value::Bool(announce);
    // LocalSend before protocol v2 read this name.
    message["announcement"] = Value::Bool(announce);
    serde_json::to_vec(&message).unwrap_or_default()
}

/// The PC's address on the home network (the one its traffic goes out by).
fn own_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Nothing is sent: connecting a UDP socket only picks the route.
    socket.connect("1.1.1.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

fn discovery(shared: Arc<Shared>, stop: Arc<AtomicBool>, port: u16) {
    let Ok(socket) = UdpSocket::bind(("0.0.0.0", port)) else {
        return;
    };
    let interface = own_address().unwrap_or(Ipv4Addr::UNSPECIFIED);
    if socket.join_multicast_v4(&MULTICAST, &interface).is_err()
        && socket
            .join_multicast_v4(&MULTICAST, &Ipv4Addr::UNSPECIFIED)
            .is_err()
    {
        return;
    }
    let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
    let here = announcement(&shared.info, true);
    let group = (MULTICAST, port);
    let _ = socket.send_to(&here, group);
    let started = Instant::now();
    let mut repeated = false;
    let mut buffer = vec![0u8; 64 * 1024];
    while !stop.load(Ordering::Acquire) {
        // Once more a moment later: a single packet can get lost.
        if !repeated && started.elapsed() > Duration::from_secs(2) {
            repeated = true;
            let _ = socket.send_to(&here, group);
        }
        let Ok((count, from)) = socket.recv_from(&mut buffer) else {
            continue;
        };
        let Ok(message) = serde_json::from_slice::<Value>(&buffer[..count]) else {
            continue;
        };
        if message.get("fingerprint").and_then(Value::as_str) == Some(shared.fingerprint.as_str()) {
            continue;
        }
        let asks = message.get("announce").and_then(Value::as_bool) == Some(true)
            || message.get("announcement").and_then(Value::as_bool) == Some(true);
        if !asks {
            continue;
        }
        let ip = from.ip().to_string();
        let their_port = message
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(port);
        let https = message.get("protocol").and_then(Value::as_str) != Some("http");
        let shared = shared.clone();
        let reply = announcement(&shared.info, false);
        let answer = socket.try_clone();
        std::thread::spawn(move || {
            if !shared
                .host
                .register_with(&ip, their_port, https, &shared.info)
            {
                if let Ok(answer) = answer {
                    let _ = answer.send_to(&reply, group);
                }
            }
        });
    }
    let _ = socket.leave_multicast_v4(&MULTICAST, &interface);
}

// ---------------------------------------------------------------- the receiver

/// Receiving, while it runs.
pub struct Receiver {
    stop: Arc<AtomicBool>,
    port: u16,
}

impl Receiver {
    /// Starts answering on `port` (LocalSend's own in Bloom). Fails when the
    /// port is taken (LocalSend itself open on this PC, say).
    pub fn start(
        host: Arc<dyn Host>,
        identity: &Identity,
        alias: &str,
        port: u16,
    ) -> Result<Receiver, String> {
        let tls = tls_config(&identity.cert, &identity.key)?;
        let listener = TcpListener::bind(("0.0.0.0", port)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                "LocalSend's port is in use on this PC: close the LocalSend app here first"
                    .to_string()
            } else {
                format!("couldn't start receiving: {e}")
            }
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            host,
            info: device_info(alias, &identity.fingerprint, port),
            fingerprint: identity.fingerprint.clone(),
            session: Mutex::new(None),
        });
        {
            let stop = stop.clone();
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("bloom-localsend-receive".into())
                .spawn(move || {
                    let busy = Arc::new(AtomicUsize::new(0));
                    for stream in listener.incoming() {
                        if stop.load(Ordering::Acquire) {
                            break;
                        }
                        let Ok(stream) = stream else {
                            continue;
                        };
                        if busy.load(Ordering::Acquire) >= MAX_CONNECTIONS {
                            continue;
                        }
                        busy.fetch_add(1, Ordering::AcqRel);
                        let (shared, tls, done) = (shared.clone(), tls.clone(), busy.clone());
                        let spawned = std::thread::Builder::new()
                            .name("bloom-localsend-request".into())
                            .spawn(move || {
                                connection(shared, tls, stream);
                                done.fetch_sub(1, Ordering::AcqRel);
                            });
                        if spawned.is_err() {
                            busy.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
        }
        {
            let stop = stop.clone();
            let _ = std::thread::Builder::new()
                .name("bloom-localsend-discovery".into())
                .spawn(move || discovery(shared, stop, port));
        }
        Ok(Receiver { stop, port })
    }

    /// Stops answering: the threads end within a second.
    pub fn stop(self) {
        self.stop.store(true, Ordering::Release);
        // Wakes the listener waiting for a connection.
        let _ = TcpStream::connect_timeout(
            &std::net::SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)),
            Duration::from_millis(300),
        );
    }
}

// ---------------------------------------------------------------- Bloom (Windows)

#[cfg(windows)]
pub use bloom::our_identity;

/// Turns receiving into the shelf on or off; the new state also goes to the
/// notch as `localsend-receive`.
#[cfg(windows)]
#[tauri::command]
pub async fn localsend_receive(app: tauri::AppHandle, on: bool) -> bloom::ReceiveStatus {
    bloom::receive(app, on).await
}

/// Whether receiving is on, and until when.
#[cfg(windows)]
#[tauri::command]
pub fn localsend_receive_status() -> bloom::ReceiveStatus {
    bloom::status(None)
}

#[cfg(windows)]
mod bloom {
    use super::*;
    use serde::Serialize;
    use std::sync::atomic::AtomicI64;
    use tauri::{AppHandle, Emitter, Manager};

    static RECEIVER: Mutex<Option<Receiver>> = Mutex::new(None);
    /// When receiving turns itself off, in ms since 1970 (0: it doesn't).
    static UNTIL: AtomicI64 = AtomicI64::new(0);
    /// Bumped by every start, so an older auto-off timer ends.
    static RUN: AtomicUsize = AtomicUsize::new(0);

    const MINUTES_KEY: &str = "bloom-shelf-localsend-receive-minutes";

    #[derive(Clone, Serialize)]
    pub struct ReceiveStatus {
        on: bool,
        /// When it turns itself off (ms since 1970), 0 for never.
        until: i64,
        /// Why it couldn't turn on.
        error: Option<String>,
    }

    pub fn status(error: Option<String>) -> ReceiveStatus {
        let on = RECEIVER.lock().is_ok_and(|receiver| receiver.is_some());
        ReceiveStatus {
            on,
            until: if on { UNTIL.load(Ordering::Acquire) } else { 0 },
            error,
        }
    }

    fn announce(app: &AppHandle, error: Option<String>) -> ReceiveStatus {
        let now = status(error);
        let _ = app.emit_to("main", "localsend-receive", now.clone());
        now
    }

    fn minutes(app: &AppHandle) -> i64 {
        crate::utils::get_setting_str(app, MINUTES_KEY)
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(10)
            .clamp(0, 24 * 60)
    }

    /// Received files in the shelf: sent to the notch.
    #[derive(Clone, Serialize)]
    struct Received {
        paths: Vec<String>,
        texts: Vec<String>,
        from: String,
    }

    struct Bloom {
        app: AppHandle,
        downloads: PathBuf,
    }

    impl Host for Bloom {
        fn folder(&self) -> PathBuf {
            self.downloads.clone()
        }

        fn received(&self, paths: Vec<PathBuf>, texts: Vec<String>, from: &str) {
            let _ = self.app.emit_to(
                "main",
                "localsend-received",
                Received {
                    paths: paths
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect(),
                    texts,
                    from: from.to_string(),
                },
            );
        }

        fn activity(&self) {
            let minutes = minutes(&self.app);
            if minutes > 0 {
                UNTIL.store(
                    crate::utils::get_now_ms() + minutes * 60_000,
                    Ordering::Release,
                );
            }
        }

        fn register_with(&self, ip: &str, port: u16, https: bool, info: &Value) -> bool {
            crate::localsend::register_with(ip, port, https, info)
        }
    }

    fn downloads() -> Option<PathBuf> {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{
            FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
        };
        unsafe {
            let path = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
            let text = path.to_string().ok();
            CoTaskMemFree(Some(path.0 as *const core::ffi::c_void));
            text.filter(|p| !p.is_empty()).map(PathBuf::from)
        }
    }

    /// This PC's LocalSend identity (made once), also used when sending.
    pub fn our_identity(app: &AppHandle) -> Option<&'static Identity> {
        static IDENTITY: std::sync::OnceLock<Option<Identity>> = std::sync::OnceLock::new();
        IDENTITY
            .get_or_init(|| {
                let folder = app.path().app_config_dir().ok()?;
                identity(&folder).ok()
            })
            .as_ref()
    }

    fn turn_on(app: &AppHandle) -> Result<(), String> {
        if RECEIVER.lock().is_ok_and(|receiver| receiver.is_some()) {
            Host::activity(&Bloom {
                app: app.clone(),
                downloads: PathBuf::new(),
            });
            return Ok(());
        }
        let identity = our_identity(app).ok_or("couldn't make LocalSend's certificate")?;
        let downloads = downloads().ok_or("couldn't find the Downloads folder")?;
        let host = Arc::new(Bloom {
            app: app.clone(),
            downloads,
        });
        let alias = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Bloom".into());
        let receiver = Receiver::start(host.clone(), identity, &alias, PORT)?;
        UNTIL.store(0, Ordering::Release);
        host.activity();
        if let Ok(mut slot) = RECEIVER.lock() {
            *slot = Some(receiver);
        }
        // Turns itself off once nothing has come for the time set.
        let run = RUN.fetch_add(1, Ordering::AcqRel) + 1;
        let app = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            if RUN.load(Ordering::Acquire) != run {
                return;
            }
            let until = UNTIL.load(Ordering::Acquire);
            if until > 0 && crate::utils::get_now_ms() >= until {
                turn_off();
                announce(&app, None);
                return;
            }
        });
        Ok(())
    }

    fn turn_off() {
        RUN.fetch_add(1, Ordering::AcqRel);
        UNTIL.store(0, Ordering::Release);
        let receiver = RECEIVER.lock().ok().and_then(|mut slot| slot.take());
        if let Some(receiver) = receiver {
            receiver.stop();
        }
    }

    pub async fn receive(app: AppHandle, on: bool) -> ReceiveStatus {
        tauri::async_runtime::spawn_blocking(move || {
            let error = if on {
                turn_on(&app).err()
            } else {
                turn_off();
                None
            };
            announce(&app, error)
        })
        .await
        .unwrap_or_else(|e| status(Some(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_stay_inside_downloads() {
        assert_eq!(safe_relative("photo.jpg"), Some(PathBuf::from("photo.jpg")));
        assert_eq!(
            safe_relative("Trip/day 1/a.jpg"),
            Some(["Trip", "day 1", "a.jpg"].iter().collect())
        );
        assert_eq!(
            safe_relative("../../Windows/system.ini"),
            Some(["Windows", "system.ini"].iter().collect())
        );
        assert_eq!(
            safe_relative("C:\\x.txt"),
            Some(["C_", "x.txt"].iter().collect())
        );
        assert_eq!(safe_relative("con.txt"), Some(PathBuf::from("_con.txt")));
        assert_eq!(safe_relative("a?b*.txt "), Some(PathBuf::from("a_b_.txt")));
        assert_eq!(safe_relative("../.."), None);
        assert_eq!(safe_relative(""), None);
    }

    #[test]
    fn requests_are_read() {
        let request = parse_head(
            "POST /api/localsend/v2/upload?sessionId=a%20b&fileId=1&token=x+y HTTP/1.1\r\nHost: pc\r\nContent-Length: 5",
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/api/localsend/v2/upload");
        assert_eq!(request.query["sessionId"], "a b");
        assert_eq!(request.query["token"], "x y");
        assert_eq!(request.headers["content-length"], "5");
    }

    #[test]
    fn chunked_bodies_are_read() {
        let mut stream: &[u8] = b"4\r\nWiki\r\n6;x=y\r\npedia \r\n0\r\n\r\n";
        let request = parse_head("POST / HTTP/1.1\r\nTransfer-Encoding: chunked").unwrap();
        let mut text = String::new();
        body(&request, b"".to_vec(), &mut stream)
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "Wikipedia ");
    }

    #[test]
    fn messages_are_text() {
        let message = json!({ "fileType": "text/plain", "preview": "hello", "size": 5 });
        assert_eq!(as_text(&message).as_deref(), Some("hello"));
        let file = json!({ "fileType": "text/plain", "size": 5000 });
        assert_eq!(as_text(&file), None);
    }
}
