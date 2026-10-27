//! Generate a scaffold into a directory, for inspecting what the builder produces.
//!
//!   cargo run --example scaffold -- /tmp/out "Build a web app to manage invoices"
//!
//! Useful when working on the generator: it is the same code path `build_project` uses,
//! without the verification stage, so you can read the output and run it.

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| "scaffold-out".to_string());
    let prompt = args.next().unwrap_or_else(|| "Build a web app to manage invoices and clients".to_string());

    let reqs = kavach_host::builder::extract_requirements(&prompt);
    let files = kavach_host::builder::scaffold(&reqs);
    let root = PathBuf::from(&dir);

    for file in &files {
        match kavach_host::path::write_project_file(&root, &file.path, &file.content) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("failed to write {}: {e}", file.path);
                std::process::exit(1);
            }
        }
    }
    println!("wrote {} files to {}", files.len(), root.display());
}
