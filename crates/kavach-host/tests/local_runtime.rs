//! End-to-end proof that a local model is discovered and used.
//!
//! This stands up a server on the real Ollama port (11434) and drives the real builder
//! through it. Nothing is mocked: the host discovers the runtime, connects over a socket,
//! asks for a completion, writes the model's notes into the project, and the project's own
//! tests still pass.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use kavach_host::builder::build_project_with;
use kavach_host::runtime::{discover, default_ports};

const NOTES: &str = "- Tracks invoices and clients in a local JSON file.\n\
- Serves a small browser UI and a JSON API.\n\
- Has no dependencies and no network calls.\n\
- Ships with a test suite that runs under node --test.\n";

/// Serve `/api/tags` and `/api/generate` until the test finishes.
fn spawn_ollama(port: u16) -> Arc<AtomicUsize> {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => l,
            Err(_) => return,
        };
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buf = vec![0u8; 8192];
            let read = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..read]).to_string();

            let body = if request.starts_with("GET /api/tags") {
                r#"{"models":[{"name":"qwen2.5-coder:7b"},{"name":"llama3.2:3b"}]}"#.to_string()
            } else if request.starts_with("POST /api/generate") {
                format!("{{\"response\":{}}}", serde_escape(NOTES))
            } else {
                r#"{"error":"not found"}"#.to_string()
            };

            let status = if body.contains("\"error\"") { "404 Not Found" } else { "200 OK" };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    hits
}

fn serde_escape(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kavach-runtime-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_local_model_is_discovered_and_its_notes_are_delivered() {
    let hits = spawn_ollama(11434);
    // Give the listener a moment to bind.
    thread::sleep(std::time::Duration::from_millis(250));

    let found = match discover("127.0.0.1", &default_ports(), None) {
        Some(runtime) => runtime,
        None => {
            eprintln!("SKIPPED: port 11434 is unavailable in this environment");
            return;
        }
    };
    assert_eq!(found.kind.as_str(), "ollama");
    assert_eq!(found.model, "qwen2.5-coder:7b", "should pick the first model the server lists");

    let dir = temp_project("deliver");
    let report = build_project_with("p-runtime", &dir, "Build a web app to manage invoices and clients", Some(&found));

    assert!(report.runtime.as_deref().unwrap_or_default().contains("ollama"), "report must name the runtime");
    assert!(
        dir.join("docs/MODEL-NOTES.md").exists(),
        "the model's notes should have been written; stages: {:?}",
        report.stages
    );
    let notes = std::fs::read_to_string(dir.join("docs/MODEL-NOTES.md")).unwrap();
    assert!(notes.contains("Tracks invoices and clients"), "notes should contain the model text");

    // The generated project must still pass its own tests with the extra file present.
    let verify = report.stages.iter().find(|s| s.stage == "verify").expect("verify stage");
    assert!(verify.ok, "verify failed: {}", verify.detail);
    assert!(report.security_passed, "the model's notes must not trip the security scan");
    assert!(hits.load(Ordering::SeqCst) >= 2, "the server should have been asked for tags and a completion");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn without_a_runtime_the_delivery_says_so() {
    let dir = temp_project("noruntime");
    let report = build_project_with("p-offline", &dir, "Build a web app to manage invoices and clients", None);

    assert!(report.runtime.is_none());
    assert!(!dir.join("docs/MODEL-NOTES.md").exists());
    let limitations = report.limitations.join(" ");
    assert!(
        limitations.contains("No local model server was found"),
        "the report must state that no model was available: {limitations}"
    );
    // The app is still built and verified without a model.
    assert!(report.stages.iter().any(|s| s.stage == "verify" && s.ok));

    std::fs::remove_dir_all(&dir).ok();
}
