//! Web search suggestions (off unless turned on in the settings), fetched with
//! Windows' own HTTP client. All three sources answer in the OpenSearch
//! suggestion format: `["query", ["suggestion", ...]]`.

use std::ffi::c_void;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts, INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
    WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn encode(query: &str) -> String {
    let mut out = String::new();
    for byte in query.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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

/// A small HTTPS GET; returns the body when the server answers 200.
unsafe fn https_get(host: &str, path: &str) -> Option<Vec<u8>> {
    let agent = wide("Bloom");
    let session = Handle(WinHttpOpen(
        PCWSTR(agent.as_ptr()),
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
        PCWSTR::null(),
        PCWSTR::null(),
        0,
    ));
    if session.0.is_null() {
        return None;
    }
    let _ = WinHttpSetTimeouts(session.0, 2000, 2000, 2000, 3000);
    let host_w = wide(host);
    let connection = Handle(WinHttpConnect(
        session.0,
        PCWSTR(host_w.as_ptr()),
        INTERNET_DEFAULT_HTTPS_PORT,
        0,
    ));
    if connection.0.is_null() {
        return None;
    }
    let verb = wide("GET");
    let path_w = wide(path);
    let request = Handle(WinHttpOpenRequest(
        connection.0,
        PCWSTR(verb.as_ptr()),
        PCWSTR(path_w.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        std::ptr::null(),
        WINHTTP_FLAG_SECURE,
    ));
    if request.0.is_null() {
        return None;
    }
    WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
    WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;

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
    .ok()?;
    if status != 200 {
        return None;
    }

    let mut body = Vec::new();
    loop {
        let mut available = 0u32;
        WinHttpQueryDataAvailable(request.0, &mut available).ok()?;
        if available == 0 || body.len() > 256 * 1024 {
            break;
        }
        let mut chunk = vec![0u8; available as usize];
        let mut read = 0u32;
        WinHttpReadData(
            request.0,
            chunk.as_mut_ptr() as *mut c_void,
            available,
            &mut read,
        )
        .ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read as usize]);
    }
    Some(body)
}

fn parse_suggestions(body: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(body);
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get(1).and_then(|list| list.as_array()).cloned())
        .map(|list| {
            list.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Search suggestions from "google", "bing" or "duckduckgo".
#[tauri::command]
pub async fn launcher_web_suggestions(source: String, query: String) -> Vec<String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Vec::new();
    }
    tauri::async_runtime::spawn_blocking(move || {
        let q = encode(&query);
        let (host, path) = match source.as_str() {
            "bing" => ("api.bing.com", format!("/osjson.aspx?query={q}")),
            "duckduckgo" => ("duckduckgo.com", format!("/ac/?type=list&q={q}")),
            _ => (
                "suggestqueries.google.com",
                format!("/complete/search?client=firefox&ie=utf-8&oe=utf-8&q={q}"),
            ),
        };
        unsafe { https_get(host, &path) }
            .map(|body| parse_suggestions(&body))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_search_suggestions() {
        let body = br#"["blo",["bloom","blog","blood"]]"#;
        assert_eq!(parse_suggestions(body), vec!["bloom", "blog", "blood"]);
        assert!(parse_suggestions(b"nope").is_empty());
        assert_eq!(encode("a b&c"), "a%20b%26c");
    }
}
