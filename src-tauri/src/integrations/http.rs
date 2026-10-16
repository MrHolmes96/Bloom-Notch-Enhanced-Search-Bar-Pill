//! A small HTTP client on Windows' own WinHTTP (system proxy settings, system
//! certificates, nothing bundled).

use std::ffi::c_void;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_RAW_HEADERS_CRLF,
    WINHTTP_QUERY_STATUS_CODE,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
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

pub struct Response {
    pub status: u32,
    headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&self.body).map_err(|_| {
            let text = self.text();
            format!(
                "Unexpected answer ({}): {}",
                self.status,
                text.chars().take(160).collect::<String>()
            )
        })
    }

    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

struct Target {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn split_url(url: &str) -> Result<Target, String> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(format!("Not a web address: {}", url));
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    let path = path.split('#').next().unwrap_or_default();
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => (
            host.to_string(),
            port.parse().map_err(|_| "Bad port".to_string())?,
        ),
        _ => (authority.to_string(), if secure { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(format!("Not a web address: {}", url));
    }
    Ok(Target {
        secure,
        host,
        port,
        path: if path.is_empty() {
            "/".into()
        } else if path.starts_with('?') {
            format!("/{}", path)
        } else {
            path.to_string()
        },
    })
}

/// Sends a request and reads the answer. `done` may end reading early once
/// what has arrived is enough (a server-sent event stream left open).
pub fn send_until(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    timeout_ms: i32,
    mut done: impl FnMut(&[u8]) -> bool,
) -> Result<Response, String> {
    let target = split_url(url)?;
    unsafe {
        let agent = wide("Bloom");
        let session = Handle(WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return Err("Windows' web connection isn't available".into());
        }
        let _ = WinHttpSetTimeouts(session.0, 5000, 8000, timeout_ms, timeout_ms);
        let host = wide(&target.host);
        let connection = Handle(WinHttpConnect(
            session.0,
            PCWSTR(host.as_ptr()),
            target.port,
            0,
        ));
        if connection.0.is_null() {
            return Err(format!("Couldn't reach {}", target.host));
        }
        let verb = wide(method);
        let path = wide(&target.path);
        let request = Handle(WinHttpOpenRequest(
            connection.0,
            PCWSTR(verb.as_ptr()),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            if target.secure {
                WINHTTP_FLAG_SECURE
            } else {
                WINHTTP_OPEN_REQUEST_FLAGS(0)
            },
        ));
        if request.0.is_null() {
            return Err("Couldn't start the request".into());
        }
        let header_text: String = headers
            .iter()
            .map(|(name, value)| format!("{}: {}\r\n", name, value))
            .collect();
        let header_wide: Vec<u16> = header_text.encode_utf16().collect();
        let body = body.unwrap_or_default();
        WinHttpSendRequest(
            request.0,
            (!header_wide.is_empty()).then_some(header_wide.as_slice()),
            (!body.is_empty()).then_some(body.as_ptr() as *const c_void),
            body.len() as u32,
            body.len() as u32,
            0,
        )
        .map_err(|_| format!("Couldn't reach {}", target.host))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|_| format!("No answer from {}", target.host))?;

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
        .map_err(|e| e.to_string())?;

        let mut size = 0u32;
        let _ = WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            PCWSTR::null(),
            None,
            &mut size,
            std::ptr::null_mut(),
        );
        let mut raw = vec![0u16; (size as usize).div_ceil(2) + 1];
        let mut parsed = Vec::new();
        if size > 0
            && WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                PCWSTR::null(),
                Some(raw.as_mut_ptr() as *mut c_void),
                &mut size,
                std::ptr::null_mut(),
            )
            .is_ok()
        {
            let text = String::from_utf16_lossy(&raw[..(size as usize / 2).min(raw.len())]);
            for line in text.lines().skip(1) {
                if let Some((name, value)) = line.split_once(':') {
                    parsed.push((name.trim().to_string(), value.trim().to_string()));
                }
            }
        }

        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            if WinHttpQueryDataAvailable(request.0, &mut available).is_err() {
                if body.is_empty() {
                    return Err(format!("The answer from {} broke off", target.host));
                }
                break;
            }
            if available == 0 || body.len() > 8 * 1024 * 1024 {
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
            if done(&body) {
                break;
            }
        }
        Ok(Response {
            status,
            headers: parsed,
            body,
        })
    }
}

pub fn send(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<Response, String> {
    send_until(method, url, headers, body, 20_000, |_| false)
}

/// Percent-encodes a value for a URL or a form.
pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The scheme and host of a URL: `https://example.com`.
pub fn origin(url: &str) -> Option<String> {
    let target = split_url(url).ok()?;
    let scheme = if target.secure { "https" } else { "http" };
    let default = if target.secure { 443 } else { 80 };
    Some(if target.port == default {
        format!("{}://{}", scheme, target.host)
    } else {
        format!("{}://{}:{}", scheme, target.host, target.port)
    })
}

/// The path of a URL, without its query: `/v3/mcp`.
pub fn path_of(url: &str) -> String {
    split_url(url)
        .map(|t| t.path.split('?').next().unwrap_or("/").to_string())
        .unwrap_or_else(|_| "/".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_split() {
        let t = split_url("https://core.blitzit.app/v3/mcp?x=1").unwrap();
        assert!(t.secure);
        assert_eq!(t.host, "core.blitzit.app");
        assert_eq!(t.port, 443);
        assert_eq!(t.path, "/v3/mcp?x=1");
        let t = split_url("http://localhost:8080").unwrap();
        assert_eq!(
            (t.host.as_str(), t.port, t.path.as_str()),
            ("localhost", 8080, "/")
        );
        assert_eq!(origin("https://a.b/c/d").as_deref(), Some("https://a.b"));
        assert_eq!(path_of("https://a.b/c/d?e"), "/c/d");
        assert_eq!(form(&[("a b", "c&d")]), "a%20b=c%26d");
    }
}
