//! Local inference runtime.
//!
//! Kavach talks to a model the user is already running on their own machine — Ollama on
//! 11434, or a llama.cpp server on 8080. There is no HTTP client dependency and no TLS,
//! because there is no network: the connection is a TCP socket to 127.0.0.1, and a
//! non-loopback host is refused before a socket is opened.
//!
//! The whole module is testable without a model: the tests stand up a real TCP server that
//! speaks the same protocol.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Nothing here may exceed this; a model server that streams forever must not hang a build.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
const IO_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub enum RuntimeError {
    NotLoopback(String),
    Connect(String),
    Io(String),
    Http(String),
    Protocol(String),
    TooLarge,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::NotLoopback(h) => {
                write!(f, "refusing to contact {h}: Kavach only talks to a model on this machine")
            }
            RuntimeError::Connect(m) => write!(f, "cannot reach the local model server: {m}"),
            RuntimeError::Io(m) => write!(f, "connection error: {m}"),
            RuntimeError::Http(m) => write!(f, "the local model server returned an error: {m}"),
            RuntimeError::Protocol(m) => write!(f, "unexpected response from the model server: {m}"),
            RuntimeError::TooLarge => write!(f, "the model server response exceeded {MAX_RESPONSE_BYTES} bytes"),
        }
    }
}

impl std::error::Error for RuntimeError {}

// ------------------------------------------------------------------ minimal JSON

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn index(&self, i: usize) -> Option<&Value> {
        match self {
            Value::Array(items) => items.get(i),
            _ => None,
        }
    }

    /// Walk a dotted path, e.g. `choices.0.message.content`.
    pub fn path(&self, path: &str) -> Option<&Value> {
        let mut current = self;
        for part in path.split('.') {
            current = match part.parse::<usize>() {
                Ok(i) => current.index(i)?,
                Err(_) => current.get(part)?,
            };
        }
        Some(current)
    }

    pub fn path_str(&self, path: &str) -> Option<&str> {
        self.path(path).and_then(Value::as_str)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { bytes: input.as_bytes(), pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), RuntimeError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(RuntimeError::Protocol(format!("expected '{}' at byte {}", byte as char, self.pos)))
        }
    }

    fn parse_value(&mut self) -> Result<Value, RuntimeError> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(Value::String(self.parse_string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            other => Err(RuntimeError::Protocol(format!("unexpected {:?} at byte {}", other.map(|b| b as char), self.pos))),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, RuntimeError> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(RuntimeError::Protocol(format!("expected {word}")))
        }
    }

    fn parse_object(&mut self) -> Result<Value, RuntimeError> {
        self.expect(b'{')?;
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(pairs));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(pairs));
                }
                _ => return Err(RuntimeError::Protocol("expected ',' or '}'".into())),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Value, RuntimeError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(RuntimeError::Protocol("expected ',' or ']'".into())),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, RuntimeError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self.peek().ok_or_else(|| RuntimeError::Protocol("unterminated string".into()))?;
            self.pos += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.peek().ok_or_else(|| RuntimeError::Protocol("unterminated escape".into()))?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self
                                .bytes
                                .get(self.pos..self.pos + 4)
                                .ok_or_else(|| RuntimeError::Protocol("truncated \\u escape".into()))?;
                            let hex = std::str::from_utf8(hex).map_err(|_| RuntimeError::Protocol("bad \\u".into()))?;
                            let code = u32::from_str_radix(hex, 16).map_err(|_| RuntimeError::Protocol("bad \\u".into()))?;
                            self.pos += 4;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => return Err(RuntimeError::Protocol(format!("bad escape \\{}", other as char))),
                    }
                }
                _ => {
                    // Collect the full UTF-8 sequence for this byte.
                    let start = self.pos - 1;
                    let width = utf8_width(byte);
                    let end = (start + width).min(self.bytes.len());
                    match std::str::from_utf8(&self.bytes[start..end]) {
                        Ok(chunk) => out.push_str(chunk),
                        Err(_) => out.push('\u{fffd}'),
                    }
                    self.pos = end;
                }
            }
        }
    }

    fn parse_number(&mut self) -> Result<Value, RuntimeError> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| RuntimeError::Protocol("bad number".into()))?;
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| RuntimeError::Protocol(format!("bad number {text:?}")))
    }
}

fn utf8_width(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

pub fn parse_json(input: &str) -> Result<Value, RuntimeError> {
    let mut parser = Parser::new(input);
    let value = parser.parse_value()?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(RuntimeError::Protocol(format!("trailing data at byte {}", parser.pos)));
    }
    Ok(value)
}

// ------------------------------------------------------------------ loopback HTTP

/// A model server Kavach can reach on this machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Runtime {
    pub kind: RuntimeKind,
    pub host: String,
    pub port: u16,
    pub model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    /// Ollama: `POST /api/generate`, `GET /api/tags`.
    Ollama,
    /// llama.cpp server and anything else OpenAI-compatible: `POST /v1/chat/completions`.
    OpenAiCompatible,
}

impl RuntimeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeKind::Ollama => "ollama",
            RuntimeKind::OpenAiCompatible => "openai-compatible",
        }
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

/// POST a JSON body to a loopback endpoint and return the response body.
///
/// Hand-rolled HTTP/1.1 because the host has no dependencies. It understands
/// `Content-Length`; a chunked response is rejected rather than mis-parsed.
pub fn http_post(host: &str, port: u16, path: &str, body: &str) -> Result<String, RuntimeError> {
    if !is_loopback(host) {
        return Err(RuntimeError::NotLoopback(host.to_string()));
    }
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| RuntimeError::Connect(e.to_string()))?
        .next()
        .ok_or_else(|| RuntimeError::Connect(format!("{host}:{port} did not resolve")))?;

    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| RuntimeError::Connect(format!("{host}:{port}: {e}")))?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok();

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\nAccept: application/json\r\n\r\n{body}",
        body.as_bytes().len()
    );
    stream.write_all(request.as_bytes()).map_err(|e| RuntimeError::Io(e.to_string()))?;
    stream.flush().ok();

    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&chunk[..n]);
                if raw.len() > MAX_RESPONSE_BYTES {
                    return Err(RuntimeError::TooLarge);
                }
            }
            Err(e) => return Err(RuntimeError::Io(e.to_string())),
        }
    }

    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| RuntimeError::Protocol("response had no header terminator".into()))?;

    let status_line = head.lines().next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| RuntimeError::Protocol(format!("bad status line {status_line:?}")))?;
    if status >= 400 {
        return Err(RuntimeError::Http(format!("HTTP {status}: {}", body.chars().take(300).collect::<String>())));
    }
    if head.to_lowercase().contains("transfer-encoding: chunked") {
        return Err(RuntimeError::Protocol("chunked responses are not supported".into()));
    }
    Ok(body.to_string())
}

/// GET a loopback endpoint (used for discovery).
pub fn http_get(host: &str, port: u16, path: &str) -> Result<String, RuntimeError> {
    if !is_loopback(host) {
        return Err(RuntimeError::NotLoopback(host.to_string()));
    }
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| RuntimeError::Connect(e.to_string()))?
        .next()
        .ok_or_else(|| RuntimeError::Connect(format!("{host}:{port} did not resolve")))?;
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| RuntimeError::Connect(format!("{host}:{port}: {e}")))?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).map_err(|e| RuntimeError::Io(e.to_string()))?;

    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&chunk[..n]);
                if raw.len() > MAX_RESPONSE_BYTES {
                    return Err(RuntimeError::TooLarge);
                }
            }
            Err(e) => return Err(RuntimeError::Io(e.to_string())),
        }
    }
    let text = String::from_utf8_lossy(&raw).to_string();
    text.split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .ok_or_else(|| RuntimeError::Protocol("response had no header terminator".into()))
}

/// Ask the model server for the model names it is serving.
pub fn list_models(runtime: &Runtime) -> Result<Vec<String>, RuntimeError> {
    match runtime.kind {
        RuntimeKind::Ollama => {
            let body = http_get(&runtime.host, runtime.port, "/api/tags")?;
            let json = parse_json(&body)?;
            let mut names = Vec::new();
            if let Some(Value::Array(models)) = json.get("models") {
                for model in models {
                    if let Some(name) = model.path_str("name") {
                        names.push(name.to_string());
                    }
                }
            }
            Ok(names)
        }
        RuntimeKind::OpenAiCompatible => {
            let body = http_get(&runtime.host, runtime.port, "/v1/models")?;
            let json = parse_json(&body)?;
            let mut names = Vec::new();
            if let Some(Value::Array(models)) = json.get("data") {
                for model in models {
                    if let Some(name) = model.path_str("id") {
                        names.push(name.to_string());
                    }
                }
            }
            Ok(names)
        }
    }
}

/// Generate a completion. Returns the text, or an error the caller can report.
pub fn generate(runtime: &Runtime, prompt: &str, max_tokens: u32) -> Result<String, RuntimeError> {
    if !is_loopback(&runtime.host) {
        return Err(RuntimeError::NotLoopback(runtime.host.clone()));
    }
    match runtime.kind {
        RuntimeKind::Ollama => {
            let body = format!(
                "{{\"model\":{},\"prompt\":{},\"stream\":false,\"options\":{{\"num_predict\":{max_tokens},\"temperature\":0.2}}}}",
                json_string(&runtime.model),
                json_string(prompt)
            );
            let response = http_post(&runtime.host, runtime.port, "/api/generate", &body)?;
            let json = parse_json(&response)?;
            json.path_str("response")
                .map(|s| s.to_string())
                .ok_or_else(|| RuntimeError::Protocol("Ollama response had no \"response\" field".into()))
        }
        RuntimeKind::OpenAiCompatible => {
            let body = format!(
                "{{\"model\":{},\"messages\":[{{\"role\":\"user\",\"content\":{}}}],\"max_tokens\":{max_tokens},\"temperature\":0.2,\"stream\":false}}",
                json_string(&runtime.model),
                json_string(prompt)
            );
            let response = http_post(&runtime.host, runtime.port, "/v1/chat/completions", &body)?;
            let json = parse_json(&response)?;
            json.path_str("choices.0.message.content")
                .map(|s| s.to_string())
                .ok_or_else(|| RuntimeError::Protocol("OpenAI response had no choices[0].message.content".into()))
        }
    }
}

fn json_string(value: &str) -> String {
    crate::json::escape(value).pipe(|escaped| format!("\"{escaped}\""))
}

trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}
impl<T> Pipe for T {}

/// Probe the conventional local ports.
///
/// Never starts anything: if the user has not started a model server, none is found.
pub fn discover(host: &str, ports: &[(u16, RuntimeKind)], preferred_model: Option<&str>) -> Option<Runtime> {
    for (port, kind) in ports {
        let probe = Runtime { kind: *kind, host: host.to_string(), port: *port, model: String::new() };
        if let Ok(models) = list_models(&probe) {
            if models.is_empty() {
                continue;
            }
            let model = preferred_model
                .filter(|want| models.iter().any(|m| m == *want || m.starts_with(&format!("{want}:"))))
                .map(|m| m.to_string())
                .unwrap_or_else(|| models[0].clone());
            return Some(Runtime { model, ..probe });
        }
    }
    None
}

/// The ports Kavach looks at, in order.
pub fn default_ports() -> Vec<(u16, RuntimeKind)> {
    vec![(11434, RuntimeKind::Ollama), (8080, RuntimeKind::OpenAiCompatible)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn serve_once(response: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for stream in listener.incoming().take(1) {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        port
    }

    #[test]
    fn parses_nested_json() {
        let json = parse_json(r#"{"a":{"b":[1,2,{"c":"x\ny"}]},"ok":true,"n":null}"#).unwrap();
        assert_eq!(json.path_str("a.b.2.c"), Some("x\ny"));
        assert_eq!(json.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(json.get("n"), Some(&Value::Null));
    }

    #[test]
    fn parses_unicode_escapes_and_multibyte() {
        let json = parse_json(r#"{"s":"caf\u00e9 ☕"}"#).unwrap();
        assert_eq!(json.path_str("s"), Some("café ☕"));
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_json("{").is_err());
        assert!(parse_json("{\"a\":}").is_err());
        assert!(parse_json("{}extra").is_err());
        assert!(parse_json("{\"a\" 1}").is_err());
    }

    #[test]
    fn refuses_a_non_loopback_host() {
        let err = http_post("192.168.1.10", 11434, "/api/generate", "{}").unwrap_err();
        assert!(matches!(err, RuntimeError::NotLoopback(_)));
        assert!(err.to_string().contains("only talks to a model on this machine"));
    }

    #[test]
    fn speaks_ollama_over_a_real_socket() {
        let port = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 24\r\n\r\n{\"response\":\"hello there\"}",
        );
        let runtime = Runtime { kind: RuntimeKind::Ollama, host: "127.0.0.1".into(), port, model: "m".into() };
        let text = generate(&runtime, "say hello", 16).unwrap();
        assert_eq!(text, "hello there");
    }

    #[test]
    fn speaks_openai_compatible_over_a_real_socket() {
        let body = "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}";
        let response: &'static str = Box::leak(
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}", body.len(), body).into_boxed_str(),
        );
        let port = serve_once(response);
        let runtime = Runtime { kind: RuntimeKind::OpenAiCompatible, host: "127.0.0.1".into(), port, model: "m".into() };
        assert_eq!(generate(&runtime, "hi", 8).unwrap(), "ok");
    }

    #[test]
    fn surfaces_an_http_error() {
        let port = serve_once("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 5\r\n\r\nboom!");
        let runtime = Runtime { kind: RuntimeKind::Ollama, host: "127.0.0.1".into(), port, model: "m".into() };
        let err = generate(&runtime, "x", 8).unwrap_err();
        assert!(matches!(err, RuntimeError::Http(_)));
        assert!(err.to_string().contains("500"));
    }

    #[test]
    fn rejects_a_chunked_response_instead_of_misparsing_it() {
        let port = serve_once("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n");
        let err = http_post("127.0.0.1", port, "/api/generate", "{}").unwrap_err();
        assert!(matches!(err, RuntimeError::Protocol(_)));
        assert!(err.to_string().contains("chunked"));
    }

    #[test]
    fn discovery_finds_nothing_when_nothing_is_listening() {
        // A port nothing can be listening on.
        assert!(discover("127.0.0.1", &[(1, RuntimeKind::Ollama)], None).is_none());
    }

    #[test]
    fn discovery_finds_a_serving_runtime() {
        let body = "{\"models\":[{\"name\":\"qwen2.5-coder:7b\"},{\"name\":\"llama3.2:3b\"}]}";
        let response: &'static str = Box::leak(
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}", body.len(), body).into_boxed_str(),
        );
        let port = serve_once(response);
        let found = discover("127.0.0.1", &[(port, RuntimeKind::Ollama)], None).expect("should find the server");
        assert_eq!(found.model, "qwen2.5-coder:7b");
        assert_eq!(found.kind, RuntimeKind::Ollama);
    }
}
