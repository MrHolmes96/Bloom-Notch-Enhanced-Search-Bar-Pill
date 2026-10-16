//! A small client for an app's MCP server over Streamable HTTP: JSON-RPC
//! requests posted to one address, answered as JSON or as a server-sent event
//! stream.

use serde::Serialize;
use serde_json::{json, Value};

use super::http;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug)]
pub enum McpError {
    /// The sign-in expired or was withdrawn.
    Unauthorized,
    /// The server forgot this connection; start a new one.
    SessionGone,
    Failed(String),
}

impl From<String> for McpError {
    fn from(message: String) -> Self {
        McpError::Failed(message)
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Unauthorized => write!(f, "Signed out: sign in again"),
            McpError::SessionGone => write!(f, "The connection was reset"),
            McpError::Failed(message) => write!(f, "{}", message),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(skip)]
    pub schema: Value,
}

pub struct Client {
    url: String,
    session_id: Option<String>,
    protocol: String,
    next_id: u64,
    pub tools: Vec<Tool>,
    pub ready: bool,
}

/// The JSON-RPC messages in a server-sent event stream.
fn sse_messages(body: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(body);
    let mut messages = Vec::new();
    for event in text.replace("\r\n", "\n").split("\n\n") {
        let data: Vec<&str> = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|d| d.strip_prefix(' ').unwrap_or(d))
            .collect();
        if data.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&data.join("\n")) {
            match value {
                Value::Array(batch) => messages.extend(batch),
                single => messages.push(single),
            }
        }
    }
    messages
}

fn answer_for(messages: Vec<Value>, id: u64) -> Option<Value> {
    messages.into_iter().find(|m| {
        m.get("id").and_then(|v| v.as_u64()) == Some(id)
            && (m.get("result").is_some() || m.get("error").is_some())
    })
}

impl Client {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            session_id: None,
            protocol: PROTOCOL_VERSION.to_string(),
            next_id: 1,
            tools: Vec::new(),
            ready: false,
        }
    }

    fn post(
        &mut self,
        token: &str,
        message: Value,
        id: Option<u64>,
    ) -> Result<Option<Value>, McpError> {
        let bearer = format!("Bearer {}", token);
        let mut headers: Vec<(&str, &str)> = vec![
            ("Content-Type", "application/json"),
            ("Accept", "application/json, text/event-stream"),
            ("Authorization", &bearer),
        ];
        if self.ready {
            headers.push(("MCP-Protocol-Version", &self.protocol));
        }
        let session = self.session_id.clone();
        if let Some(session) = session.as_deref() {
            headers.push(("Mcp-Session-Id", session));
        }
        let body = message.to_string();
        let response = http::send_until(
            "POST",
            &self.url,
            &headers,
            Some(body.as_bytes()),
            30_000,
            |so_far| {
                // An event stream may stay open after the answer; stop once it's in.
                id.is_some_and(|id| {
                    (so_far.ends_with(b"\n\n") || so_far.ends_with(b"\r\n\r\n"))
                        && answer_for(sse_messages(so_far), id).is_some()
                })
            },
        )?;
        if response.status == 401 || response.status == 403 {
            return Err(McpError::Unauthorized);
        }
        if response.status == 404 && self.session_id.is_some() {
            self.session_id = None;
            self.ready = false;
            return Err(McpError::SessionGone);
        }
        if let Some(session) = response.header("Mcp-Session-Id") {
            self.session_id = Some(session.to_string());
        }
        let Some(id) = id else {
            return if response.ok() {
                Ok(None)
            } else {
                Err(McpError::Failed(format!(
                    "The server answered {}",
                    response.status
                )))
            };
        };
        if !response.ok() {
            let detail = response.text();
            return Err(McpError::Failed(format!(
                "The server answered {}{}",
                response.status,
                if detail.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", detail.chars().take(160).collect::<String>())
                }
            )));
        }
        let is_stream = response
            .header("Content-Type")
            .is_some_and(|t| t.to_ascii_lowercase().contains("text/event-stream"));
        let answer = if is_stream {
            answer_for(sse_messages(&response.body), id)
        } else {
            match response.json()? {
                Value::Array(batch) => answer_for(batch, id),
                single => Some(single),
            }
        }
        .ok_or_else(|| McpError::Failed("The server sent no answer".into()))?;
        if let Some(error) = answer.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("The server refused");
            return Err(McpError::Failed(message.to_string()));
        }
        Ok(answer.get("result").cloned())
    }

    fn request(&mut self, token: &str, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        Ok(self.post(token, message, Some(id))?.unwrap_or(Value::Null))
    }

    /// Opens the connection and learns what the server can do.
    pub fn initialize(&mut self, token: &str) -> Result<(), McpError> {
        self.ready = false;
        self.session_id = None;
        let result = self.request(
            token,
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "Bloom", "version": env!("CARGO_PKG_VERSION")}
            }),
        )?;
        if let Some(version) = result.get("protocolVersion").and_then(|v| v.as_str()) {
            self.protocol = version.to_string();
        }
        self.ready = true;
        let _ = self.post(
            token,
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            None,
        );
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let page = self.request(token, "tools/list", params)?;
            for tool in page
                .get("tools")
                .and_then(|t| t.as_array())
                .cloned()
                .unwrap_or_default()
            {
                let Some(name) = tool.get("name").and_then(|n| n.as_str()) else {
                    continue;
                };
                tools.push(Tool {
                    name: name.to_string(),
                    description: tool
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    schema: tool.get("inputSchema").cloned().unwrap_or(Value::Null),
                });
            }
            cursor = page
                .get("nextCursor")
                .and_then(|c| c.as_str())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        self.tools = tools;
        Ok(())
    }

    /// Calls a tool and returns what it answered: its structured content, or
    /// its text (parsed as JSON when it is JSON).
    pub fn call(&mut self, token: &str, name: &str, arguments: Value) -> Result<Value, McpError> {
        let result = self.request(
            token,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        let text: String = result
            .get("content")
            .and_then(|c| c.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if result.get("isError").and_then(|e| e.as_bool()) == Some(true) {
            return Err(McpError::Failed(if text.trim().is_empty() {
                "Blitzit refused that".into()
            } else {
                text.chars().take(200).collect()
            }));
        }
        if let Some(structured) = result.get("structuredContent") {
            if !structured.is_null() {
                return Ok(structured.clone());
            }
        }
        Ok(serde_json::from_str::<Value>(text.trim()).unwrap_or(Value::String(text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_streams_parse() {
        let body =
            b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"ok\":true}}\n\n";
        let answer = answer_for(sse_messages(body), 3).unwrap();
        assert_eq!(answer["result"]["ok"], true);
        assert!(answer_for(sse_messages(body), 4).is_none());
    }
}
