//! Hardware probe.
//!
//! Reads what the operating system exposes and reports absence as absence: a field that
//! could not be read is `None`, and the reason is added to `unmeasured`, so a model-fit
//! verdict can tell "0" from "unknown".

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::exec::{run, ExecOptions};

#[derive(Debug, Default, Clone)]
pub struct Gpu {
    pub vendor: String,
    pub name: String,
    pub vram_bytes: Option<u64>,
    pub acceleration: Vec<String>,
    pub unified_memory: bool,
}

#[derive(Debug, Default, Clone)]
pub struct DiskUsage {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Probe {
    pub platform: String,
    pub arch: String,
    pub cpu_model: String,
    pub logical_cores: usize,
    pub instruction_sets: Vec<String>,
    pub ram_total_bytes: u64,
    pub ram_available_bytes: u64,
    pub gpus: Vec<Gpu>,
    pub disk: DiskUsage,
    pub unmeasured: Vec<String>,
    pub sources: BTreeMap<String, String>,
    pub probed_at: String,
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

#[cfg(target_os = "linux")]
fn meminfo() -> Option<(u64, u64)> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let field = |key: &str| -> Option<u64> {
        text.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    Some((field("MemTotal:")?, field("MemAvailable:")?))
}

#[cfg(not(target_os = "linux"))]
fn meminfo() -> Option<(u64, u64)> {
    None
}

#[cfg(target_os = "linux")]
fn cpu_model_and_flags() -> (Option<String>, Vec<String>) {
    let text = match fs::read_to_string("/proc/cpuinfo") {
        Ok(t) => t,
        Err(_) => return (None, Vec::new()),
    };
    let model = text
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let wanted = ["avx2", "avx512f", "fma", "sse4_2", "neon", "asimd"];
    let flags = text
        .lines()
        .find(|l| l.starts_with("flags"))
        .map(|l| {
            l.split(|c| c == ':' || c == ' ')
                .filter(|f| wanted.contains(f))
                .map(|f| f.to_string())
                .take(8)
                .collect()
        })
        .unwrap_or_default();
    (model, flags)
}

#[cfg(not(target_os = "linux"))]
fn cpu_model_and_flags() -> (Option<String>, Vec<String>) {
    (std::env::var("PROCESSOR_IDENTIFIER").ok(), Vec::new())
}

/// Disk usage for a path.
///
/// Linux uses `df -Pk`. Windows uses `wmic`-free PowerShell, which is present on every
/// supported version. Both are subprocess calls, so both can fail — and failure is
/// reported, not guessed.
pub fn disk_usage(path: &str) -> Option<DiskUsage> {
    #[cfg(target_os = "linux")]
    let out = run("df", &["-Pk".to_string(), path.to_string()], ExecOptions { timeout_ms: 5_000, ..Default::default() }).ok()?;

    #[cfg(target_os = "windows")]
    let out = run(
        "powershell",
        &[
            "-NoProfile".to_string(),
            "-Command".to_string(),
            format!(
                "$d = Get-PSDrive -PSProvider FileSystem | Where-Object {{ $_.Name -eq (Split-Path -Qualifier '{}').TrimEnd(':') }}; \
                 if ($d) {{ \"{0} {1}\" -f ($d.Used + $d.Free), $d.Free }}",
                path.replace('\'', "''")
            ),
        ],
        ExecOptions { timeout_ms: 10_000, ..Default::default() },
    )
    .ok()?;

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let out = {
        let _ = path;
        return None;
    };

    if out.exit_code != 0 {
        return None;
    }

    #[cfg(target_os = "linux")]
    {
        let line = out.stdout.lines().nth(1)?;
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            return None;
        }
        let total = fields[1].parse::<u64>().ok()? * 1024;
        let avail = fields[3].parse::<u64>().ok()? * 1024;
        Some(DiskUsage { total_bytes: total, available_bytes: avail })
    }

    #[cfg(target_os = "windows")]
    {
        let fields: Vec<&str> = out.stdout.split_whitespace().collect();
        if fields.len() < 2 {
            return None;
        }
        Some(DiskUsage {
            total_bytes: fields[0].parse::<u64>().ok()?,
            available_bytes: fields[1].parse::<u64>().ok()?,
        })
    }
}

fn probe_gpus(probe: &mut Probe) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = run(
            "nvidia-smi",
            &["--query-gpu=name,memory.total".to_string(), "--format=csv,noheader,nounits".to_string()],
            ExecOptions { timeout_ms: 5_000, ..Default::default() },
        ) {
            if out.exit_code == 0 {
                for line in out.stdout.lines() {
                    let mut parts = line.split(',').map(|s| s.trim());
                    let name = parts.next().unwrap_or("");
                    if name.is_empty() {
                        continue;
                    }
                    let vram = parts.next().and_then(|v| v.parse::<u64>().ok()).map(|mb| mb * 1024 * 1024);
                    probe.gpus.push(Gpu {
                        vendor: "nvidia".into(),
                        name: name.to_string(),
                        vram_bytes: vram,
                        acceleration: vec!["cuda".into(), "vulkan".into()],
                        unified_memory: false,
                    });
                }
                if !probe.gpus.is_empty() {
                    probe.sources.insert("gpu".into(), "nvidia-smi --query-gpu".into());
                }
            }
        }

        if !probe.gpus.iter().any(|g| g.vendor == "nvidia") {
            let mut found = false;
            if let Ok(entries) = fs::read_dir("/sys/class/drm") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !(name.starts_with("card") && name[4..].chars().all(|c| c.is_ascii_digit())) {
                        continue;
                    }
                    let driver = fs::read_link(Path::new("/sys/class/drm").join(&name).join("device/driver"))
                        .ok()
                        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                        .unwrap_or_default();
                    if driver.is_empty() {
                        continue;
                    }
                    let vendor = read_trimmed(&format!("/sys/class/drm/{}/device/vendor", name)).unwrap_or_default();
                    let vendor_name = match vendor.as_str() {
                        "0x8086" => "intel",
                        "0x1002" => "amd",
                        _ => "unknown",
                    };
                    let unified = ["i915", "amdgpu", "xe", "msm", "panfrost"].contains(&driver.as_str());
                    probe.gpus.push(Gpu {
                        vendor: vendor_name.into(),
                        name: format!("{} ({})", driver, name),
                        vram_bytes: None,
                        acceleration: vec!["vulkan".into()],
                        unified_memory: unified,
                    });
                    found = true;
                }
            }
            if found {
                probe.sources.insert("gpu".into(), "/sys/class/drm".into());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(out) = run(
            "powershell",
            &[
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Get-CimInstance Win32_VideoController | ForEach-Object { \"$($_.Name)|$($_.AdapterRAM)\" }".to_string(),
            ],
            ExecOptions { timeout_ms: 10_000, ..Default::default() },
        ) {
            if out.exit_code == 0 {
                for line in out.stdout.lines() {
                    let mut parts = line.split('|');
                    let name = parts.next().unwrap_or("").trim();
                    if name.is_empty() {
                        continue;
                    }
                    let vram = parts.next().and_then(|v| v.trim().parse::<u64>().ok()).filter(|v| *v > 0);
                    let lower = name.to_lowercase();
                    let unified = lower.contains("intel") || lower.contains("radeon graphics");
                    probe.gpus.push(Gpu {
                        vendor: if lower.contains("nvidia") { "nvidia" } else if lower.contains("amd") || lower.contains("radeon") { "amd" } else { "intel" }.into(),
                        name: name.to_string(),
                        vram_bytes: vram,
                        acceleration: vec!["vulkan".into()],
                        unified_memory: unified,
                    });
                }
                if !probe.gpus.is_empty() {
                    probe.sources.insert("gpu".into(), "Win32_VideoController".into());
                }
            }
        }
    }

    if probe.gpus.is_empty() {
        probe.unmeasured.push("No GPU enumerated: this machine appears to have none, or the driver exposes none.".into());
    }
    for gpu in &probe.gpus {
        if gpu.vram_bytes.is_none() {
            probe.unmeasured.push(format!("VRAM size not reported for {}", gpu.name));
        }
    }
}

/// Probe this machine. `workdir` is used for the disk measurement.
pub fn probe(workdir: &str) -> Probe {
    let mut probe = Probe {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        logical_cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        probed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default(),
        ..Default::default()
    };

    match meminfo() {
        Some((total, available)) => {
            probe.ram_total_bytes = total;
            probe.ram_available_bytes = available;
            probe.sources.insert("ram".into(), "/proc/meminfo (MemTotal, MemAvailable)".into());
        }
        None => probe.unmeasured.push("Total and available RAM (not readable on this platform by this build)".into()),
    }

    let (model, flags) = cpu_model_and_flags();
    match model {
        Some(m) => {
            probe.cpu_model = m;
            probe.sources.insert("cpuModel".into(), "platform CPU information".into());
        }
        None => probe.unmeasured.push("CPU model name (not reported by the OS)".into()),
    }
    if flags.is_empty() {
        probe.unmeasured.push("CPU instruction-set features".into());
    } else {
        probe.instruction_sets = flags;
        probe.sources.insert("cpuFlags".into(), "/proc/cpuinfo flags".into());
    }

    match disk_usage(workdir) {
        Some(disk) => {
            probe.disk = disk;
            probe.sources.insert("disk".into(), if cfg!(windows) { "PowerShell Get-PSDrive".into() } else { "df -Pk".into() });
        }
        None => probe.unmeasured.push("Disk capacity and free space".into()),
    }

    probe_gpus(&mut probe);
    probe.unmeasured.push("Sustained memory bandwidth (used only as a heuristic band, never measured)".into());
    probe.unmeasured.push("Whether a model will run at a given speed (nothing is benchmarked before download)".into());
    probe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_real_values() {
        let p = probe("/tmp");
        assert!(!p.platform.is_empty());
        assert!(p.logical_cores >= 1);
        #[cfg(target_os = "linux")]
        {
            assert!(p.ram_total_bytes > 0);
            assert!(p.ram_available_bytes > 0);
            assert!(p.ram_available_bytes <= p.ram_total_bytes);
            assert!(p.disk.total_bytes > 0);
        }
    }

    #[test]
    fn declares_what_it_could_not_measure() {
        let p = probe("/tmp");
        assert!(p.unmeasured.len() >= 2);
        assert!(p.unmeasured.iter().any(|u| u.contains("bandwidth")));
    }
}
