//! Process execution with a hard timeout.
//!
//! Rules: argv only (never a shell string), its own process group so the whole tree can
//! be killed, capped output, and the real exit code returned.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(unix)]
extern "C" {
    fn setsid() -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
}

#[cfg(unix)]
const SIGKILL: i32 = 9;

pub const EXIT_TIMEOUT: i32 = 124;
pub const EXIT_PERMISSION_DENIED: i32 = 126;

#[derive(Debug, Clone)]
pub struct ExecOutcome {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub truncated: bool,
}

#[derive(Debug)]
pub enum ExecError {
    Spawn(String),
    Invalid(String),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Spawn(m) => write!(f, "cannot run command: {m}"),
            ExecError::Invalid(m) => write!(f, "invalid command: {m}"),
        }
    }
}

impl std::error::Error for ExecError {}

pub struct ExecOptions {
    pub cwd: Option<String>,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}

impl Default for ExecOptions {
    fn default() -> Self {
        ExecOptions {
            cwd: None,
            timeout_ms: 120_000,
            max_output_bytes: 256 * 1024,
        }
    }
}

fn read_capped<R: Read + Send + 'static>(
    mut source: R,
    cap: usize,
    sink: Arc<Mutex<(String, bool)>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match source.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // Recover from a poisoned lock rather than panicking: a reader thread
                    // that died mid-write should cost us that thread, not the whole command.
                    let mut guard = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    let room = cap.saturating_sub(guard.0.len());
                    if room == 0 {
                        guard.1 = true;
                        continue;
                    }
                    let take = room.min(n);
                    guard.0.push_str(&String::from_utf8_lossy(&buf[..take]));
                    if take < n {
                        guard.1 = true;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// Kill the child and, where the platform supports it, its whole process tree.
fn kill_tree(child: &mut std::process::Child) {
    let pid = child.id();
    #[cfg(unix)]
    {
        unsafe {
            kill(-(pid as i32), SIGKILL);
            kill(pid as i32, SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        // Windows has no process groups usable this way; taskkill walks the tree.
        let _ = Command::new("taskkill")
            .args(["/pid", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// Run a command with argv semantics and a wall-clock limit.
pub fn run(command: &str, args: &[String], options: ExecOptions) -> Result<ExecOutcome, ExecError> {
    if command.contains('\0') || args.iter().any(|a| a.contains('\0')) {
        return Err(ExecError::Invalid("NUL byte in command or argument".into()));
    }

    let started = Instant::now();
    let mut cmd = Command::new(command);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            setsid();
            Ok(())
        });
    }

    let mut child = cmd.spawn().map_err(|e| ExecError::Spawn(e.to_string()))?;

    let out_sink = Arc::new(Mutex::new((String::new(), false)));
    let err_sink = Arc::new(Mutex::new((String::new(), false)));
    let out_handle = child.stdout.take().map(|s| read_capped(s, options.max_output_bytes, Arc::clone(&out_sink)));
    let err_handle = child.stderr.take().map(|s| read_capped(s, options.max_output_bytes, Arc::clone(&err_sink)));

    // Wait for exit, enforcing the deadline by polling. An earlier version used a
    // watchdog thread plus a channel, but joined the thread before signalling it, so
    // every command ran to its full timeout and was then killed. Polling is simpler and
    // has no ordering to get wrong.
    let deadline = Instant::now() + Duration::from_millis(options.timeout_ms);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    kill_tree(&mut child);
                    break child.wait();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => break Err(e),
        }
    };

    if let Some(h) = out_handle {
        let _ = h.join();
    }
    if let Some(h) = err_handle {
        let _ = h.join();
    }

    let (stdout, out_truncated) = { out_sink.lock().unwrap_or_else(|p| p.into_inner()).clone() };
    let (stderr, err_truncated) = { err_sink.lock().unwrap_or_else(|p| p.into_inner()).clone() };

    let status = status.map_err(|e| ExecError::Spawn(e.to_string()))?;
    let exit_code = if timed_out { EXIT_TIMEOUT } else { status.code().unwrap_or(1) };

    let mut stderr = stderr;
    if timed_out {
        stderr.push_str(&format!(
            "\n[Kavach] command exceeded {} ms and its process tree was killed",
            options.timeout_ms
        ));
    }

    Ok(ExecOutcome {
        command: std::iter::once(command.to_string()).chain(args.iter().cloned()).collect::<Vec<_>>().join(" "),
        exit_code,
        stdout,
        stderr,
        duration_ms: started.elapsed().as_millis() as u64,
        timed_out,
        truncated: out_truncated || err_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// A shell is used only to *set up* the test; the assertions are about `run`.
    fn sh(script: &str) -> ExecOutcome {
        run("sh", &args(&["-c", script]), ExecOptions::default()).unwrap()
    }

    #[test]
    fn captures_output_and_exit_code() {
        let out = sh("echo hello; exit 3");
        assert_eq!(out.exit_code, 3);
        assert_eq!(out.stdout.trim(), "hello");
        assert!(!out.timed_out);
    }

    #[test]
    fn arguments_are_not_shell_interpreted() {
        let marker = std::env::temp_dir().join(format!("kavach-inject-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let out = run(
            "printf",
            &args(&["%s", &format!("; touch {}", marker.display())]),
            ExecOptions::default(),
        )
        .unwrap();
        assert_eq!(out.exit_code, 0);
        assert!(!marker.exists(), "an argument must never be executed");
    }

    #[test]
    fn times_out_and_kills_the_tree() {
        let started = Instant::now();
        let out = run("sleep", &args(&["30"]), ExecOptions { timeout_ms: 600, ..Default::default() }).unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, EXIT_TIMEOUT);
        assert!(started.elapsed() < Duration::from_secs(6));
        assert!(out.stderr.contains("was killed"));
    }

    #[test]
    fn caps_runaway_output() {
        let out = run(
            "sh",
            &args(&["-c", "for i in $(seq 1 2000); do printf 'xxxxxxxxxxxxxxxxxxxx'; done"]),
            ExecOptions { max_output_bytes: 1024, ..Default::default() },
        )
        .unwrap();
        assert!(out.truncated);
        assert!(out.stdout.len() <= 1024);
    }

    #[test]
    fn missing_binary_is_an_error() {
        assert!(matches!(run("definitely-not-real-xyz", &[], ExecOptions::default()), Err(ExecError::Spawn(_))));
    }
}
