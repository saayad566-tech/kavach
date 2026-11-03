//! Kavach as a full-stack web application: one process serves the UI and the command API.
//!
//!   cargo run --bin kavach-server --release
//!   KAVACH_PORT=4780 KAVACH_WORKSPACE=~/.kavach cargo run --bin kavach-server
//!
//! It binds loopback only. There is no authentication, so it must never be exposed.

use std::net::TcpListener;
use std::path::PathBuf;

use kavach_host::httpd;

/// Where the server keeps projects when the environment does not say.
fn default_workspace() -> PathBuf {
    if let Ok(explicit) = std::env::var("KAVACH_WORKSPACE") {
        if !explicit.trim().is_empty() {
            return PathBuf::from(explicit);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".kavach")
}

/// The UI directory: beside the repository in a source checkout, or beside the binary.
fn ui_root() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/ui"),
        PathBuf::from("apps/desktop/ui"),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("ui")))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|path| path.join("index.html").is_file())
}

fn main() {
    let workspace = default_workspace();
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!("cannot create the workspace at {}: {e}", workspace.display());
        std::process::exit(1);
    }

    let Some(ui) = ui_root() else {
        eprintln!("cannot find the UI directory (apps/desktop/ui). Run from a checkout.");
        std::process::exit(1);
    };

    let port: u16 = std::env::var("KAVACH_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4780);

    // Loopback only: there is no auth, so anything else would be unsafe.
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("cannot listen on 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        }
    };

    println!("Kavach on http://127.0.0.1:{port}");
    println!("workspace: {}", workspace.display());
    println!("ui:        {}", ui.display());
    println!("loopback only — this server has no authentication");

    let ui = std::sync::Arc::new(ui);
    let workspace = std::sync::Arc::new(workspace.to_string_lossy().to_string());

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let ui = std::sync::Arc::clone(&ui);
        let workspace = std::sync::Arc::clone(&workspace);
        // One thread per connection. A panic in a handler must not take the server down, so
        // the work is isolated and a failed join is simply dropped.
        std::thread::spawn(move || {
            let _ = httpd::handle(&mut stream, &ui, &workspace);
        });
    }
}
