//! Model-authored code: accepted only when the project still passes its own tests.
//!
//! Each test stands up a real server on the real Ollama port and drives the real builder.
//! The four cases are the ones that decide whether this feature is safe to ship:
//!   1. good code that passes        -> accepted
//!   2. output that is not JSON      -> rejected, scaffold delivered
//!   3. code that breaks the tests   -> rolled back, scaffold still passes
//!   4. a path that escapes the root -> rejected

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use kavach_host::builder::build_project_with;
use kavach_host::runtime::{discover, default_ports, Runtime};

/// What the fake model should say when asked for code.
#[derive(Clone)]
enum CodeReply {
    Valid,
    NotJson,
    BreaksTests,
    Traversal,
}

/// One server for the whole binary, and one test at a time.
///
/// Every test needs the same fixed port, so they cannot run concurrently: an earlier
/// version spawned a server per test and the first one to bind served every test, which
/// made two of them pass and two fail for the wrong reasons.
fn serve_forever(reply: Arc<Mutex<CodeReply>>) {
    let listener = match TcpListener::bind(("127.0.0.1", 11434)) {
        Ok(l) => l,
        Err(_) => return,
    };
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut buf = vec![0u8; 16384];
        let read = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..read]).to_string();
        let current = reply.lock().unwrap_or_else(|p| p.into_inner()).clone();

        let body = if request.starts_with("GET /api/tags") {
            r#"{"models":[{"name":"qwen2.5-coder:7b"}]}"#.to_string()
        } else if request.contains("adding code") {
            let generated = match current {
                CodeReply::Valid => format!(
                    r#"{{"files":[{{"path":"src/summarise.js","content":{}}}]}}"#,
                    quote("export function count(rows) {\n  return Array.isArray(rows) ? rows.length : 0;\n}\n")
                ),
                CodeReply::NotJson => "I'm sorry, I can't help with that.".to_string(),
                CodeReply::BreaksTests => format!(
                    r#"{{"files":[{{"path":"test/broken.test.js","content":{}}}]}}"#,
                    quote("import test from 'node:test';\nimport assert from 'node:assert/strict';\ntest('this always fails', () => { assert.equal(1, 2); });\n")
                ),
                CodeReply::Traversal => format!(
                    r#"{{"files":[{{"path":"../../escaped.js","content":{}}}]}}"#,
                    quote("export const x = 1;\n")
                ),
            };
            format!("{{\"response\":{}}}", quote(&generated))
        } else {
            format!("{{\"response\":{}}}", quote("- Tracks invoices.\n- Has tests.\n"))
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }
}

/// Take the port, set the reply this test wants, and hand back a runtime.
fn runtime_for(reply: CodeReply) -> Option<(std::sync::MutexGuard<'static, ()>, Runtime)> {
    use std::sync::{MutexGuard, OnceLock};

    static LOCK: Mutex<()> = Mutex::new(());
    static REPLY: OnceLock<Arc<Mutex<CodeReply>>> = OnceLock::new();

    let guard: MutexGuard<'static, ()> = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let shared = REPLY.get_or_init(|| {
        let cell = Arc::new(Mutex::new(CodeReply::Valid));
        let for_thread = Arc::clone(&cell);
        // The server loop never returns, and `get_or_init` blocks until its closure
        // returns — so it must run on its own thread or the first caller hangs forever.
        thread::spawn(move || serve_forever(for_thread));
        thread::sleep(std::time::Duration::from_millis(250));
        cell
    });
    *shared.lock().unwrap_or_else(|p| p.into_inner()) = reply;
    thread::sleep(std::time::Duration::from_millis(120));

    discover("127.0.0.1", &default_ports(), None).map(|rt| (guard, rt))
}

fn quote(text: &str) -> String {
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
    let dir = std::env::temp_dir().join(format!("kavach-modelcode-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn stage_detail(report: &kavach_host::builder::DeliveryReport, stage: &str) -> String {
    report
        .stages
        .iter()
        .find(|s| s.stage == stage)
        .map(|s| s.detail.clone())
        .unwrap_or_else(|| format!("no {stage} stage"))
}

#[test]
fn valid_model_code_is_accepted_when_the_tests_still_pass() {
    let Some((_guard, runtime)) = runtime_for(CodeReply::Valid) else {
        eprintln!("SKIPPED: port 11434 unavailable");
        return;
    };
    let dir = temp_project("accept");
    let report = build_project_with("p-accept", &dir, "Build a web app to manage invoices and clients", Some(&runtime));

    assert_eq!(report.model_files, vec!["src/summarise.js".to_string()], "{}", stage_detail(&report, "model-code"));
    assert!(dir.join("src/summarise.js").exists());
    assert!(report.stages.iter().any(|s| s.stage == "verify" && s.ok), "{}", stage_detail(&report, "verify"));
    assert!(report.limitations.join(" ").contains("model-authored"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn non_json_output_is_rejected_and_the_scaffold_is_delivered() {
    let Some((_guard, runtime)) = runtime_for(CodeReply::NotJson) else {
        eprintln!("SKIPPED: port 11434 unavailable");
        return;
    };
    let dir = temp_project("notjson");
    let report = build_project_with("p-notjson", &dir, "Build a web app to manage invoices", Some(&runtime));

    assert!(report.model_files.is_empty());
    let detail = stage_detail(&report, "model-code");
    assert!(detail.contains("rejected"), "{detail}");
    assert!(report.stages.iter().any(|s| s.stage == "verify" && s.ok));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn code_that_breaks_the_tests_is_rolled_back() {
    let Some((_guard, runtime)) = runtime_for(CodeReply::BreaksTests) else {
        eprintln!("SKIPPED: port 11434 unavailable");
        return;
    };
    let dir = temp_project("rollback");
    let report = build_project_with("p-rollback", &dir, "Build a web app to manage invoices", Some(&runtime));

    assert!(report.model_files.is_empty(), "broken model code must not be accepted");
    let detail = stage_detail(&report, "model-code");
    assert!(detail.contains("rolled back"), "{detail}");
    assert!(detail.contains("scaffold alone passes"), "{detail}");
    assert!(!dir.join("test/broken.test.js").exists(), "the broken file must be deleted");
    // And the delivery is still good: the scaffold alone passes.
    assert!(report.stages.iter().any(|s| s.stage == "verify" && s.ok), "{}", stage_detail(&report, "verify"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_traversing_path_is_rejected() {
    let Some((_guard, runtime)) = runtime_for(CodeReply::Traversal) else {
        eprintln!("SKIPPED: port 11434 unavailable");
        return;
    };
    let dir = temp_project("traversal");
    let report = build_project_with("p-traversal", &dir, "Build a web app to manage invoices", Some(&runtime));

    assert!(report.model_files.is_empty());
    let detail = stage_detail(&report, "model-code");
    assert!(detail.contains("rejected"), "{detail}");
    assert!(!dir.parent().unwrap().join("escaped.js").exists());
    std::fs::remove_dir_all(&dir).ok();
}
