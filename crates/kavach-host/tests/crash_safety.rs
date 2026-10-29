//! Crash safety: kill the writer mid-write, repeatedly, and prove the index still loads.
//!
//! The store writes `index.tsv` via a temporary file plus a rename, so a process killed
//! between those two steps must leave the previous index intact. This test proves that
//! rather than assuming it: it re-executes this test binary in "writer" mode, kills it with
//! SIGKILL at a random point, and then loads the store.
//!
//! SIGKILL cannot be caught, so there is no shutdown hook to hide behind — which is the
//! point. It is the closest thing to a power cut that a test can arrange.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use kavach_host::store::Store;

const WRITER_ENV: &str = "KAVACH_CRASH_WRITER";
const WS_ENV: &str = "KAVACH_CRASH_WS";

/// Child mode: write to the store forever, as fast as possible.
fn run_writer(workspace: PathBuf) -> ! {
    let mut round: u64 = 0;
    loop {
        round += 1;
        let mut store = Store::load(&workspace);
        store.create_project(
            &format!("Build a web app to manage invoices, round {round}"),
            "web-app",
            &format!("Project {round}"),
        );
        // Also take checkpoints, which write into the project tree as well as the index.
        if let Some(project) = store.projects.last().cloned() {
            let dir = store.project_dir(&project.id);
            std::fs::create_dir_all(&dir).ok();
            std::fs::write(dir.join("a.js"), format!("export const round = {round};\n")).ok();
            store.checkpoint(&project.id, "before build").ok();
        }
        store.save().expect("save should not fail in the writer");
    }
}

fn is_writer_mode() -> bool {
    std::env::var(WRITER_ENV).is_ok()
}

#[test]
fn index_survives_repeated_sigkill() {
    if is_writer_mode() {
        let workspace = PathBuf::from(std::env::var(WS_ENV).expect("workspace path"));
        run_writer(workspace);
    }

    let workspace = std::env::temp_dir().join(format!("kavach-crash-{}", std::process::id()));
    std::fs::create_dir_all(&workspace).unwrap();

    let exe = std::env::current_exe().expect("current test binary");
    let mut killed = 0;

    for round in 0..6 {
        let mut child = Command::new(&exe)
            .args(["--exact", "index_survives_repeated_sigkill", "--test-threads=1", "--nocapture"])
            .env(WRITER_ENV, "1")
            .env(WS_ENV, &workspace)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the writer");

        // Vary the kill point so the process is killed at different stages of the write.
        std::thread::sleep(Duration::from_millis(80 + round * 37));
        let _ = child.kill();
        let _ = child.wait();
        killed += 1;

        // The store must load, and its contents must be coherent.
        let store = Store::load(&workspace);
        let mut ids: Vec<&str> = store.projects.iter().map(|p| p.id.as_str()).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate project ids in the index after kill {round}");
        for project in &store.projects {
            assert!(!project.name.is_empty(), "a half-written row reached the index: {project:?}");
            assert!(!project.app_kind.is_empty(), "a half-written row reached the index: {project:?}");
        }
        // A temporary file may be left behind by a kill; it must never be mistaken for the index.
        assert!(workspace.join("index.tsv").exists() || total == 0);
    }

    assert_eq!(killed, 6);

    // The writer should have produced real rows across six runs; if it never got that far,
    // the test proved nothing and should say so rather than pass quietly.
    let store = Store::load(&workspace);
    assert!(
        !store.projects.is_empty(),
        "the writer never completed a save, so crash safety was not exercised"
    );

    // And a final write after all that killing must still work.
    let mut store = Store::load(&workspace);
    store.create_project("A final project after the kills", "web-app", "Final");
    store.save().expect("the store must still be writable after repeated kills");
    let reloaded = Store::load(&workspace);
    assert!(reloaded.projects.iter().any(|p| p.name == "Final"));

    std::fs::remove_dir_all(&workspace).ok();
}
