//! Signing in to an app's MCP server the standard way (the MCP authorization
//! spec: OAuth 2.1 with PKCE, dynamic client registration and a sign-in page
//! in the browser that returns to Bloom on this PC only).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::http;
use super::secret;

/// A signed-in account on one server, kept encrypted on disk.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub server: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Milliseconds since 1970.
    pub expires_at: Option<i64>,
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_method: String,
    pub resource: String,
}

struct ServerInfo {
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
    scopes: Vec<String>,
    resource: String,
}

static CANCEL: AtomicBool = AtomicBool::new(false);

/// Stops a sign-in that is waiting for the browser.
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// `key="value"` pairs of a WWW-Authenticate header.
fn auth_params(header: &str) -> Vec<(String, String)> {
    let mut params = Vec::new();
    let mut rest = header.trim();
    if let Some(stripped) = rest.strip_prefix("Bearer") {
        rest = stripped;
    }
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().trim_start_matches(',').trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let (value, remaining) = if let Some(quoted) = after.strip_prefix('"') {
            match quoted.find('"') {
                Some(end) => (quoted[..end].to_string(), &quoted[end + 1..]),
                None => (quoted.to_string(), ""),
            }
        } else {
            let end = after.find(',').unwrap_or(after.len());
            (after[..end].trim().to_string(), &after[end..])
        };
        params.push((key.to_lowercase(), value));
        rest = remaining;
    }
    params
}

fn get_json(url: &str) -> Option<Value> {
    let response = http::send("GET", url, &[("Accept", "application/json")], None).ok()?;
    if !response.ok() {
        return None;
    }
    response.json().ok().filter(|v| v.is_object())
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn discover(server: &str) -> Result<ServerInfo, String> {
    let origin = http::origin(server).ok_or("Not a web address")?;
    let path = http::path_of(server);

    // The server says where its sign-in details are when asked without one.
    let probe = http::send(
        "POST",
        server,
        &[
            ("Content-Type", "application/json"),
            ("Accept", "application/json, text/event-stream"),
        ],
        Some(
            json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
                "protocolVersion": super::mcp::PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "Bloom", "version": env!("CARGO_PKG_VERSION")}
            }})
            .to_string()
            .as_bytes(),
        ),
    )?;
    let challenge = probe
        .header("WWW-Authenticate")
        .map(auth_params)
        .unwrap_or_default();
    let param = |name: &str| {
        challenge
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };

    let mut candidates = Vec::new();
    if let Some(url) = param("resource_metadata") {
        candidates.push(url);
    }
    if path != "/" {
        candidates.push(format!(
            "{}/.well-known/oauth-protected-resource{}",
            origin, path
        ));
    }
    candidates.push(format!("{}/.well-known/oauth-protected-resource", origin));
    let resource_info = candidates.iter().find_map(|url| get_json(url));

    let issuer = resource_info
        .as_ref()
        .and_then(|info| info.get("authorization_servers"))
        .and_then(|list| list.as_array())
        .and_then(|list| list.first())
        .and_then(|v| v.as_str())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| origin.clone());
    let resource = resource_info
        .as_ref()
        .and_then(|info| text(info, "resource"))
        .unwrap_or_else(|| server.to_string());
    let mut scopes: Vec<String> = param("scope")
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    if scopes.is_empty() {
        scopes = resource_info
            .as_ref()
            .and_then(|info| info.get("scopes_supported"))
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
    }

    let issuer_origin = http::origin(&issuer).unwrap_or_else(|| origin.clone());
    let issuer_path = http::path_of(&issuer);
    let mut metadata_urls = Vec::new();
    if issuer_path != "/" {
        metadata_urls.push(format!(
            "{}/.well-known/oauth-authorization-server{}",
            issuer_origin, issuer_path
        ));
        metadata_urls.push(format!(
            "{}/.well-known/openid-configuration{}",
            issuer_origin, issuer_path
        ));
        metadata_urls.push(format!("{}/.well-known/openid-configuration", issuer));
    } else {
        metadata_urls.push(format!(
            "{}/.well-known/oauth-authorization-server",
            issuer_origin
        ));
        metadata_urls.push(format!(
            "{}/.well-known/openid-configuration",
            issuer_origin
        ));
    }
    let metadata = metadata_urls.iter().find_map(|url| get_json(url));

    Ok(match metadata {
        Some(meta) => ServerInfo {
            authorization_endpoint: text(&meta, "authorization_endpoint")
                .ok_or("The sign-in server gave no sign-in page")?,
            token_endpoint: text(&meta, "token_endpoint")
                .ok_or("The sign-in server gave no token address")?,
            registration_endpoint: text(&meta, "registration_endpoint"),
            scopes,
            resource,
        },
        // Servers from before discovery: the standard addresses on the server.
        None => ServerInfo {
            authorization_endpoint: format!("{}/authorize", issuer_origin),
            token_endpoint: format!("{}/token", issuer_origin),
            registration_endpoint: Some(format!("{}/register", issuer_origin)),
            scopes,
            resource,
        },
    })
}

struct Client {
    id: String,
    secret: Option<String>,
    method: String,
}

fn register(info: &ServerInfo, redirect_uri: &str) -> Result<Client, String> {
    let endpoint = info
        .registration_endpoint
        .as_deref()
        .ok_or("This server doesn't let apps register for sign-in")?;
    let mut body = json!({
        "client_name": "Bloom",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    if !info.scopes.is_empty() {
        body["scope"] = Value::from(info.scopes.join(" "));
    }
    let response = http::send(
        "POST",
        endpoint,
        &[
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
        ],
        Some(body.to_string().as_bytes()),
    )?;
    let answer = response.json()?;
    if !response.ok() {
        return Err(text(&answer, "error_description")
            .or_else(|| text(&answer, "error"))
            .unwrap_or_else(|| format!("Registering Bloom failed ({})", response.status)));
    }
    Ok(Client {
        id: text(&answer, "client_id").ok_or("No app id came back from registering")?,
        secret: text(&answer, "client_secret"),
        method: text(&answer, "token_endpoint_auth_method").unwrap_or_else(|| "none".into()),
    })
}

/// Why a token request failed.
pub enum RefreshError {
    /// The server said no: signing in again is needed.
    Rejected(String),
    /// The server couldn't be reached.
    Offline(String),
}

impl RefreshError {
    fn message(self) -> String {
        match self {
            RefreshError::Rejected(m) | RefreshError::Offline(m) => m,
        }
    }
}

/// Posts a token request, with the app's secret the way it registered.
fn token_request(
    endpoint: &str,
    client_id: &str,
    client_secret: Option<&str>,
    method: &str,
    mut fields: Vec<(&str, String)>,
) -> Result<Value, RefreshError> {
    let mut headers = vec![
        (
            "Content-Type",
            "application/x-www-form-urlencoded".to_string(),
        ),
        ("Accept", "application/json".to_string()),
    ];
    match (client_secret, method) {
        (Some(secret_value), "client_secret_basic") => {
            use base64::Engine;
            let pair = format!("{}:{}", http::encode(client_id), http::encode(secret_value));
            headers.push((
                "Authorization",
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(pair)
                ),
            ));
        }
        (Some(secret_value), _) => {
            fields.push(("client_id", client_id.to_string()));
            fields.push(("client_secret", secret_value.to_string()));
        }
        (None, _) => fields.push(("client_id", client_id.to_string())),
    }
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let header_refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let response = http::send(
        "POST",
        endpoint,
        &header_refs,
        Some(http::form(&pairs).as_bytes()),
    )
    .map_err(RefreshError::Offline)?;
    if response.status >= 500 {
        return Err(RefreshError::Offline(format!(
            "The sign-in server is having trouble ({})",
            response.status
        )));
    }
    let answer = response.json().map_err(RefreshError::Rejected)?;
    if !response.ok() || text(&answer, "access_token").is_none() {
        return Err(RefreshError::Rejected(
            text(&answer, "error_description")
                .or_else(|| text(&answer, "error"))
                .unwrap_or_else(|| format!("Signing in failed ({})", response.status)),
        ));
    }
    Ok(answer)
}

fn apply_tokens(session: &mut Session, answer: &Value) {
    if let Some(token) = text(answer, "access_token") {
        session.access_token = token;
    }
    if let Some(token) = text(answer, "refresh_token") {
        session.refresh_token = Some(token);
    }
    session.expires_at = answer
        .get("expires_in")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .map(|seconds| crate::utils::get_now_ms() + seconds * 1000);
}

/// A new access token from the refresh token.
pub fn refresh(session: &mut Session) -> Result<(), RefreshError> {
    let refresh_token = session
        .refresh_token
        .clone()
        .ok_or_else(|| RefreshError::Rejected("Signed out: sign in again".into()))?;
    let mut fields = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token),
    ];
    if !session.resource.is_empty() {
        fields.push(("resource", session.resource.clone()));
    }
    let answer = token_request(
        &session.token_endpoint,
        &session.client_id,
        session.client_secret.as_deref(),
        &session.auth_method,
        fields,
    )?;
    apply_tokens(session, &answer);
    Ok(())
}

const DONE_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Bloom</title>\
<style>body{margin:0;height:100vh;display:flex;align-items:center;justify-content:center;\
background:#111;color:#eee;font:16px 'Segoe UI',sans-serif}div{text-align:center}\
p{color:#999;font-size:14px}</style></head><body><div><h2>NAME is connected to Bloom</h2>\
<p>You can close this tab.</p></div></body></html>";

const FAILED_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Bloom</title>\
<style>body{margin:0;height:100vh;display:flex;align-items:center;justify-content:center;\
background:#111;color:#eee;font:16px 'Segoe UI',sans-serif}div{text-align:center}\
p{color:#999;font-size:14px}</style></head><body><div><h2>Signing in didn't finish</h2>\
<p>Go back to Bloom and try again.</p></div></body></html>";

fn reply(stream: &mut TcpStream, status: &str, page: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        page.len(),
        page
    );
    let _ = stream.flush();
}

fn query_value(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name).then(|| decode(value))
    })
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 3 <= bytes.len() => {
                let byte = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match byte {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Waits for the browser to come back with the sign-in code.
fn wait_for_code(
    listeners: &[TcpListener],
    state: &str,
    name: &str,
    deadline: Instant,
) -> Result<String, String> {
    for listener in listeners {
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    }
    loop {
        if CANCEL.load(Ordering::SeqCst) {
            return Err("Signing in was stopped".into());
        }
        if Instant::now() > deadline {
            return Err("Signing in took too long. Try again".into());
        }
        let mut accepted = None;
        for listener in listeners {
            if let Ok((stream, _)) = listener.accept() {
                accepted = Some(stream);
                break;
            }
        }
        let Some(mut stream) = accepted else {
            std::thread::sleep(Duration::from_millis(60));
            continue;
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let mut buffer = [0u8; 8192];
        let read = stream.read(&mut buffer).unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..read]);
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_string();
        let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        if path != "/callback" {
            reply(&mut stream, "404 Not Found", "");
            continue;
        }
        if query_value(query, "state").as_deref() != Some(state) {
            reply(&mut stream, "400 Bad Request", FAILED_PAGE);
            continue;
        }
        if let Some(error) = query_value(query, "error") {
            reply(&mut stream, "200 OK", FAILED_PAGE);
            return Err(query_value(query, "error_description").unwrap_or(error));
        }
        match query_value(query, "code") {
            Some(code) => {
                reply(&mut stream, "200 OK", &DONE_PAGE.replace("NAME", name));
                return Ok(code);
            }
            None => {
                reply(&mut stream, "200 OK", FAILED_PAGE);
                return Err("The sign-in page sent no code".into());
            }
        }
    }
}

fn open_browser(url: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb = wide("open");
    let file = wide(url);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err("Couldn't open the browser".into())
    }
}

/// Signs in to `server` in the browser and returns the session. `name` is the
/// app's name for the page shown when it is done.
pub fn sign_in(server: &str, name: &str) -> Result<Session, String> {
    CANCEL.store(false, Ordering::SeqCst);
    let info = discover(server)?;

    // The browser comes back to this PC only, on a free port.
    let v4 = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = v4.local_addr().map_err(|e| e.to_string())?.port();
    let mut listeners = vec![v4];
    if let Ok(v6) = TcpListener::bind(("::1", port)) {
        listeners.push(v6);
    }
    let redirect_uri = format!("http://localhost:{}/callback", port);
    let client = register(&info, &redirect_uri)?;

    let verifier = secret::base64url(&secret::random_bytes(32));
    let challenge = secret::base64url(&secret::sha256(verifier.as_bytes()));
    let state = secret::base64url(&secret::random_bytes(16));
    let mut params = vec![
        ("response_type", "code".to_string()),
        ("client_id", client.id.clone()),
        ("redirect_uri", redirect_uri.clone()),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256".to_string()),
        ("state", state.clone()),
        ("resource", info.resource.clone()),
    ];
    if !info.scopes.is_empty() {
        params.push(("scope", info.scopes.join(" ")));
    }
    let pairs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let separator = if info.authorization_endpoint.contains('?') {
        '&'
    } else {
        '?'
    };
    open_browser(&format!(
        "{}{}{}",
        info.authorization_endpoint,
        separator,
        http::form(&pairs)
    ))?;

    let code = wait_for_code(
        &listeners,
        &state,
        name,
        Instant::now() + Duration::from_secs(300),
    )?;
    let answer = token_request(
        &info.token_endpoint,
        &client.id,
        client.secret.as_deref(),
        &client.method,
        vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
            ("resource", info.resource.clone()),
        ],
    )
    .map_err(RefreshError::message)?;
    let mut session = Session {
        server: server.to_string(),
        token_endpoint: info.token_endpoint,
        client_id: client.id,
        client_secret: client.secret,
        auth_method: client.method,
        resource: info.resource,
        ..Default::default()
    };
    apply_tokens(&mut session, &answer);
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_params_parse() {
        let params = auth_params(
            r#"Bearer error="invalid_token", resource_metadata="https://a.b/.well-known/oauth-protected-resource", scope="read write""#,
        );
        assert!(params.contains(&(
            "resource_metadata".into(),
            "https://a.b/.well-known/oauth-protected-resource".into()
        )));
        assert!(params.contains(&("scope".into(), "read write".into())));
    }

    #[test]
    fn query_values_decode() {
        assert_eq!(
            query_value("code=a%2Fb+c&state=x", "code").as_deref(),
            Some("a/b c")
        );
        assert_eq!(query_value("code=a&state=x", "state").as_deref(), Some("x"));
        assert_eq!(decode("100%"), "100%");
    }
}
