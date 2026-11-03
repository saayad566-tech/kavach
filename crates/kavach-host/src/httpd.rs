//! A minimal HTTP server so Kavach runs in a browser, not only inside Tauri.
//!
//! Why this exists: the desktop shell needs a system webview, which a build machine may not
//! have. Without this, the platform cannot run anywhere at all. With it, one process serves the
//! UI and the whole command surface over loopback.
//!
//! Deliberate choices, each with a cost:
//!   * **std only.** No async runtime, no framework. One thread per connection. The host crate
//!     stays dependency-free and auditable, at the price of hand-parsing HTTP.
//!   * **Loopback only.** The listener binds `127.0.0.1`. There is no auth, so it must not be
//!     reachable from anywhere else.
//!   * **The same commands.** `/api/<command>` dispatches to `commands::*` — the identical
//!     surface the Tauri shell calls, so the two hosts cannot drift.
//!
//! The parsing and routing live here as ordinary functions with tests; the binary is a thin
//! shell around `handle`.

use std::path::{Component, Path, PathBuf};

use crate::commands;
use crate::runtime::{parse_json, Value};

/// A parsed request. Only what this server needs: method, path, body.
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    /// The request line is not `METHOD SP PATH SP HTTP/x.y`.
    Malformed,
    /// A `Content-Length` larger than the cap.
    TooLarge,
}

/// Refuse a body larger than this. The largest legitimate payload is a project prompt.
const MAX_BODY: usize = 1 << 20;

/// Parse the head of an HTTP request and return the request plus the byte offset where the
/// body starts.
///
/// Written defensively: every index is checked, because this input comes off a socket.
pub fn parse_head(head: &str) -> Result<(String, String, usize), ParseError> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or(ParseError::Malformed)?;
    let mut parts = request_line.split(' ');
    let method = parts.next().filter(|m| !m.is_empty()).ok_or(ParseError::Malformed)?;
    let path = parts.next().filter(|p| !p.is_empty()).ok_or(ParseError::Malformed)?;
    // A request line without a version is HTTP/0.9. We do not speak it, and guessing at a
    // half-parsed request is how parsers get exploited.
    let version = parts.next().ok_or(ParseError::Malformed)?;
    if !version.starts_with("HTTP/") || !path.starts_with('/') {
        return Err(ParseError::Malformed);
    }

    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else { continue };
        if name.trim().eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse().map_err(|_| ParseError::Malformed)?;
        }
    }
    if content_length > MAX_BODY {
        return Err(ParseError::TooLarge);
    }

    // The path may carry a query string; routing ignores it.
    let path_only = path.split('?').next().unwrap_or(path).to_string();
    Ok((method.to_string(), path_only, content_length))
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Response {
    fn json(status: u16, body: String) -> Response {
        Response { status, content_type: "application/json; charset=utf-8", body }
    }

    fn text(status: u16, content_type: &'static str, body: String) -> Response {
        Response { status, content_type, body }
    }

    pub fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Payload Too Large",
            500 => "Internal Server Error",
            _ => "OK",
        }
    }
}

fn error_json(message: &str) -> String {
    commands::json_err(message, "http")
}

/// A string field from the JSON body, or an empty string.
fn field(body: &Value, key: &str) -> String {
    body.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn flag(body: &Value, key: &str) -> bool {
    matches!(body.get(key), Some(Value::Bool(true)))
}

fn number(body: &Value, key: &str, fallback: u64) -> u64 {
    match body.get(key) {
        Some(Value::Number(n)) if *n >= 0.0 => *n as u64,
        _ => fallback,
    }
}

fn string_list(body: &Value, key: &str) -> Vec<String> {
    match body.get(key) {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(String::from).collect(),
        _ => Vec::new(),
    }
}

/// Dispatch a command name and JSON body to the host.
///
/// Every name in `commands::COMMAND_NAMES` must appear here; `dispatch_covers_every_command`
/// enforces that, so the HTTP surface cannot silently fall behind the IPC one.
pub fn dispatch(command: &str, body: &Value, workspace: &str) -> Option<String> {
    let out = match command {
        // The Tauri shell answers this from its app-data directory; here the server decides.
        "workspace_root" => commands::json_ok_path(workspace),

        "probe_hardware" => commands::probe_hardware(&or_default(&field(body, "workdir"), workspace)),
        "disk_usage" => commands::disk_usage(&or_default(&field(body, "path"), workspace)),
        "probe_runtime" => commands::probe_runtime(),

        "run_command" => commands::run_command(
            &or_default(&field(body, "cwd"), workspace),
            &field(body, "command"),
            &string_list(body, "args"),
            number(body, "timeoutMs", 120_000),
        ),

        "write_project_file" => commands::write_file(
            &or_default(&field(body, "root"), workspace),
            &field(body, "relPath"),
            &field(body, "content"),
        ),
        "read_project_file" => commands::read_file(
            &or_default(&field(body, "root"), workspace),
            &field(body, "relPath"),
        ),
        "list_files" => commands::list_files(&or_default(&field(body, "root"), workspace)),
        "delete_project_file" => commands::delete_file(
            &or_default(&field(body, "root"), workspace),
            &field(body, "relPath"),
        ),

        "hash_file" => commands::hash(&field(body, "path")),
        "verify_model" => commands::verify_model(&field(body, "path"), &field(body, "expectedSha256")),
        "install_model" => commands::install_model(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "modelId"),
            &field(body, "sourceFile"),
            &field(body, "destinationDir"),
            &field(body, "expectedSha256"),
            flag(body, "licenceAccepted"),
            &field(body, "licenceName"),
        ),
        "model_inspect" => commands::model_inspect(&field(body, "path")),
        "model_catalog" => commands::model_catalog(&or_default(&field(body, "workspace"), workspace)),
        "models_list" => commands::models_list(&or_default(&field(body, "workspace"), workspace)),
        "model_activate" => commands::model_activate(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "id"),
        ),
        "model_forget" => commands::model_forget(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "id"),
        ),

        "extract_requirements" => commands::extract_requirements(&field(body, "prompt")),
        "create_project" => commands::create_project(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "prompt"),
        ),
        "build_project" => commands::build_project(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "projectId"),
            &field(body, "prompt"),
            flag(body, "useLocalModel"),
        ),
        "security_scan" => commands::security_scan(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "projectId"),
        ),
        "export_project" => commands::export_project(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "projectId"),
            &field(body, "destination"),
        ),
        "list_projects" => commands::list_projects(&or_default(&field(body, "workspace"), workspace)),
        "list_checkpoints" => commands::list_checkpoints(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "projectId"),
        ),
        "restore_checkpoint" => commands::restore_checkpoint(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "projectId"),
            &field(body, "checkpointId"),
        ),
        "create_checkpoint" => commands::create_checkpoint(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "projectId"),
            &field(body, "label"),
        ),
        "project_memory" => commands::project_memory(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "projectId"),
        ),
        "agents_catalog" => commands::agents_catalog(
            &or_default(&field(body, "workspace"), workspace),
            &field(body, "task"),
        ),

        _ => return None,
    };
    Some(out)
}

fn or_default(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

/// Resolve a URL path inside the UI directory, refusing anything that escapes it.
///
/// `..` is rejected rather than normalised: a request that contains one is not a file this
/// server serves, and silently resolving it invites a traversal bug.
pub fn safe_static_path(root: &Path, url_path: &str) -> Option<PathBuf> {
    let relative = url_path.trim_start_matches('/');
    let relative = if relative.is_empty() { "index.html" } else { relative };

    let mut candidate = PathBuf::from(root);
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => candidate.push(part),
            // Anything else — parent dirs, roots, prefixes — is refused.
            _ => return None,
        }
    }
    Some(candidate)
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

/// Route one request. Pure except for reading a static file, which is why it is testable.
pub fn route(request: &Request, ui_root: &Path, workspace: &str) -> Response {
    if let Some(command) = request.path.strip_prefix("/api/") {
        if request.method != "POST" {
            return Response::json(405, error_json("use POST for the command API"));
        }
        let body = if request.body.trim().is_empty() {
            Value::Object(Vec::new())
        } else {
            match parse_json(&request.body) {
                Ok(value) => value,
                Err(e) => {
                    return Response::json(400, error_json(&format!("the body is not valid JSON: {e}")))
                }
            }
        };
        return match dispatch(command, &body, workspace) {
            Some(json) => Response::json(200, json),
            None => Response::json(404, error_json(&format!("no command called {command}"))),
        };
    }

    if request.method != "GET" {
        return Response::json(405, error_json("static files are served over GET"));
    }

    match safe_static_path(ui_root, &request.path) {
        None => Response::json(404, error_json("not found")),
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(body) => Response::text(200, content_type_for(&path), body),
            Err(_) => Response::json(404, error_json("not found")),
        },
    }
}

/// Read one request from a stream and write one response.
pub fn handle(stream: &mut std::net::TcpStream, ui_root: &Path, workspace: &str) -> std::io::Result<()> {
    use std::io::Read;

    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    let head_end;
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        raw.extend_from_slice(&buffer[..read]);
        if let Some(position) = find_head_end(&raw) {
            head_end = position;
            break;
        }
        if raw.len() > MAX_BODY + 8192 {
            return write_response(stream, &Response::json(413, error_json("request too large")));
        }
    }

    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let (method, path, content_length) = match parse_head(&head) {
        Ok(parts) => parts,
        Err(ParseError::TooLarge) => {
            return write_response(stream, &Response::json(413, error_json("request too large")))
        }
        Err(ParseError::Malformed) => {
            return write_response(stream, &Response::json(400, error_json("malformed request")))
        }
    };

    // Read the rest of the body if it has not all arrived.
    let body_start = head_end + 4;
    while raw.len() < body_start + content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&buffer[..read]);
    }
    let body = String::from_utf8_lossy(&raw[body_start..(body_start + content_length).min(raw.len())]).to_string();

    let request = Request { method, path, body };
    let response = route(&request, ui_root, workspace);
    write_response(stream, &response)
}

fn find_head_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_response(stream: &mut std::net::TcpStream, response: &Response) -> std::io::Result<()> {
    use std::io::Write;

    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        response.status,
        response.reason(),
        response.content_type,
        response.body.as_bytes().len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(response.body.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: &str) -> Value {
        parse_json(json).expect("test fixture must be valid JSON")
    }

    #[test]
    fn a_request_line_and_headers_parse() {
        let head = "POST /api/list_projects HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n";
        let (method, path, length) = parse_head(head).unwrap();
        assert_eq!(method, "POST");
        assert_eq!(path, "/api/list_projects");
        assert_eq!(length, 2);
    }

    #[test]
    fn a_query_string_is_dropped_from_the_path() {
        let (_, path, _) = parse_head("GET /app.js?v=2 HTTP/1.1\r\n").unwrap();
        assert_eq!(path, "/app.js");
    }

    #[test]
    fn malformed_requests_are_rejected_not_guessed() {
        assert_eq!(parse_head("").unwrap_err(), ParseError::Malformed);
        assert_eq!(parse_head("GET").unwrap_err(), ParseError::Malformed);
        assert_eq!(parse_head("GET /only-a-path").unwrap_err(), ParseError::Malformed);
        // A relative path is not something a browser sends.
        assert_eq!(parse_head("GET app.js HTTP/1.1\r\n").unwrap_err(), ParseError::Malformed);
        // A non-numeric length must not be treated as zero.
        assert_eq!(
            parse_head("POST /api/x HTTP/1.1\r\nContent-Length: abc\r\n").unwrap_err(),
            ParseError::Malformed
        );
    }

    #[test]
    fn an_oversized_body_is_refused_before_it_is_read() {
        let head = format!("POST /api/x HTTP/1.1\r\nContent-Length: {}\r\n", MAX_BODY + 1);
        assert_eq!(parse_head(&head).unwrap_err(), ParseError::TooLarge);
    }

    #[test]
    fn the_static_path_cannot_escape_its_root() {
        let root = Path::new("/srv/ui");
        assert_eq!(safe_static_path(root, "/app.js"), Some(PathBuf::from("/srv/ui/app.js")));
        assert_eq!(safe_static_path(root, "/"), Some(PathBuf::from("/srv/ui/index.html")));
        // Traversal, absolute paths and parent components are all refused.
        assert_eq!(safe_static_path(root, "/../../etc/passwd"), None);
        assert_eq!(safe_static_path(root, "/.."), None);
        // A doubled slash is not an escape: it resolves to a path *inside* the root, which
        // simply will not exist. The dangerous case is the one above, and it is refused.
        assert_eq!(safe_static_path(root, "//etc/passwd"), Some(PathBuf::from("/srv/ui/etc/passwd")));
    }

    #[test]
    fn every_registered_command_dispatches() {
        // The HTTP surface must not fall behind the IPC surface.
        let empty = Value::Object(Vec::new());
        for name in crate::COMMAND_NAMES {
            assert!(
                dispatch(name, &empty, "/tmp/ws").is_some(),
                "command {name} is registered but the HTTP server cannot dispatch it"
            );
        }
    }

    #[test]
    fn an_unknown_command_returns_none() {
        assert!(dispatch("drop_everything", &Value::Object(Vec::new()), "/tmp").is_none());
    }

    #[test]
    fn workspace_root_answers_with_the_server_workspace() {
        let response = route(
            &Request { method: "POST".into(), path: "/api/workspace_root".into(), body: String::new() },
            Path::new("/srv/ui"),
            "/tmp/ws",
        );
        assert_eq!(response.status, 200);
        assert!(response.body.contains("/tmp/ws"), "{}", response.body);
    }

    #[test]
    fn a_bad_method_and_a_bad_body_are_reported_as_such() {
        let ui = Path::new("/srv/ui");
        let get_api = route(&Request { method: "GET".into(), path: "/api/x".into(), body: String::new() }, ui, "/tmp");
        assert_eq!(get_api.status, 405);

        let bad_json = route(
            &Request { method: "POST".into(), path: "/api/list_projects".into(), body: "{not json".into() },
            ui,
            "/tmp",
        );
        assert_eq!(bad_json.status, 400);
        assert!(bad_json.body.contains("not valid JSON"), "{}", bad_json.body);

        let unknown = route(
            &Request { method: "POST".into(), path: "/api/nope".into(), body: String::new() },
            ui,
            "/tmp",
        );
        assert_eq!(unknown.status, 404);
    }

    #[test]
    fn a_missing_static_file_is_a_404_not_a_panic() {
        let response = route(
            &Request { method: "GET".into(), path: "/nope.js".into(), body: String::new() },
            Path::new("/definitely/not/here"),
            "/tmp",
        );
        assert_eq!(response.status, 404);
    }

    #[test]
    fn arguments_are_read_from_the_body() {
        let value = body(r#"{"workspace":"/tmp/ws","prompt":"build an app","useLocalModel":true,"timeoutMs":5000,"args":["-c","echo hi"]}"#);
        assert_eq!(field(&value, "workspace"), "/tmp/ws");
        assert_eq!(field(&value, "prompt"), "build an app");
        assert!(flag(&value, "useLocalModel"));
        assert!(!flag(&value, "missing"));
        assert_eq!(number(&value, "timeoutMs", 1), 5000);
        assert_eq!(number(&value, "missing", 7), 7);
        assert_eq!(string_list(&value, "args"), vec!["-c".to_string(), "echo hi".to_string()]);
        // A missing field is empty, never a panic.
        assert_eq!(field(&value, "absent"), "");
    }

    #[test]
    fn an_empty_workspace_falls_back_to_the_server_one() {
        let value = body("{}");
        assert_eq!(or_default(&field(&value, "workspace"), "/srv/ws"), "/srv/ws");
    }
}
