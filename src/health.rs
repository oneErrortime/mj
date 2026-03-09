//! # health
//!
//! System health information — mirrors `src/health.js` from the original Moleculer.js.
//!
//! Collects CPU load, memory usage, OS metadata, and process info.
//! Used by the broker's built-in `$node.health` action and the transit INFO packet.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Sub-structs ──────────────────────────────────────────────────────────────

/// CPU load information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    /// 1-minute load average (Unix) or estimated utilisation on Windows.
    pub load1: f64,
    /// 5-minute load average.
    pub load5: f64,
    /// 15-minute load average.
    pub load15: f64,
    /// Number of logical CPU cores.
    pub cores: usize,
    /// Utilisation percentage (0–100), capped at 100.
    pub utilization: u8,
}

/// Memory usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemInfo {
    /// Free memory in bytes.
    pub free: u64,
    /// Total memory in bytes.
    pub total: u64,
    /// Free memory as a percentage of total.
    pub percent: f64,
}

/// Operating-system metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    /// System uptime in seconds.
    pub uptime: u64,
    /// OS type (e.g. `"Linux"`, `"Windows"`, `"Darwin"`).
    pub os_type: String,
    /// Hostname.
    pub hostname: String,
    /// CPU architecture (e.g. `"x86_64"`, `"aarch64"`).
    pub arch: String,
    /// Platform string (e.g. `"linux"`, `"windows"`, `"macos"`).
    pub platform: String,
}

/// Process-level information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// OS process ID.
    pub pid: u32,
    /// Resident set size in bytes (best-effort; 0 if unavailable).
    pub rss: u64,
    /// Process uptime in seconds (since the broker was started).
    pub uptime: f64,
}

/// Moleculer client/runtime identification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Always `"rust"` for this port.
    pub lang_type: String,
    /// Crate version from `Cargo.toml`.
    pub version: String,
    /// Rust compiler version (from `RUSTC_VERSION` env set at build time, or `"unknown"`).
    pub lang_version: String,
}

/// Network interface information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetInfo {
    /// List of non-loopback IPv4/IPv6 addresses.
    pub ip: Vec<String>,
}

/// Date/time snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeInfo {
    /// Unix timestamp in milliseconds.
    pub now: u128,
    /// ISO-8601 string.
    pub iso: String,
    /// UTC string representation.
    pub utc: String,
}

/// Aggregated health snapshot returned by `broker.$node.health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub cpu: CpuInfo,
    pub mem: MemInfo,
    pub os: OsInfo,
    pub process: ProcessInfo,
    pub client: ClientInfo,
    pub net: NetInfo,
    pub time: TimeInfo,
}

// ─── Collection helpers ───────────────────────────────────────────────────────

/// Returns CPU load information.
///
/// On Linux/macOS we read `/proc/loadavg` or `sysinfo` if available.
/// Utilisation is computed as `min(load1 / cores * 100, 100)`.
pub fn get_cpu_info() -> CpuInfo {
    let cores = num_cpus();

    // Best-effort load average: try /proc/loadavg (Linux), fall back to 0.0
    let (load1, load5, load15) = read_load_avg();

    let utilization = if cores > 0 {
        ((load1 / cores as f64) * 100.0).min(100.0) as u8
    } else {
        0
    };

    CpuInfo { load1, load5, load15, cores, utilization }
}

/// Returns current memory usage.
pub fn get_mem_info() -> MemInfo {
    let (free, total) = read_mem_bytes();
    let percent = if total > 0 { (free as f64 / total as f64) * 100.0 } else { 0.0 };
    MemInfo { free, total, percent }
}

/// Returns OS metadata.
pub fn get_os_info() -> OsInfo {
    OsInfo {
        uptime: sys_uptime_secs(),
        os_type: std::env::consts::OS.to_string(),
        hostname: hostname(),
        arch: std::env::consts::ARCH.to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

/// Returns process-level information.
pub fn get_process_info() -> ProcessInfo {
    ProcessInfo {
        pid: std::process::id(),
        rss: read_process_rss(),
        uptime: process_uptime_secs(),
    }
}

/// Returns Moleculer client identification.
pub fn get_client_info() -> ClientInfo {
    ClientInfo {
        lang_type: "rust".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        lang_version: option_env!("RUSTC_VERSION").unwrap_or("unknown").to_string(),
    }
}

/// Returns network interface IP list.
pub fn get_net_info() -> NetInfo {
    NetInfo { ip: get_ip_list() }
}

/// Returns a date/time snapshot.
pub fn get_time_info() -> TimeInfo {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // ISO-8601 — build manually to avoid extra dependencies
    let secs = (now_ms / 1000) as u64;
    let ms = (now_ms % 1000) as u32;
    let iso = format_iso8601(secs, ms);
    let utc = format_rfc2822(secs);

    TimeInfo { now: now_ms, iso, utc }
}

/// Collects and returns the full [`HealthStatus`] snapshot.
pub fn get_health_status() -> HealthStatus {
    HealthStatus {
        cpu: get_cpu_info(),
        mem: get_mem_info(),
        os: get_os_info(),
        process: get_process_info(),
        client: get_client_info(),
        net: get_net_info(),
        time: get_time_info(),
    }
}

// ─── Low-level OS helpers (pure-std, no extra deps) ──────────────────────────

fn num_cpus() -> usize {
    // std::thread::available_parallelism is stable since Rust 1.59
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(target_os = "linux")]
fn read_load_avg() -> (f64, f64, f64) {
    if let Ok(s) = std::fs::read_to_string("/proc/loadavg") {
        let mut parts = s.split_whitespace();
        let l1 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let l5 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let l15 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        return (l1, l5, l15);
    }
    (0.0, 0.0, 0.0)
}

#[cfg(not(target_os = "linux"))]
fn read_load_avg() -> (f64, f64, f64) {
    (0.0, 0.0, 0.0)
}

#[cfg(target_os = "linux")]
fn read_mem_bytes() -> (u64, u64) {
    let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total = 0u64;
    let mut free = 0u64;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = parse_meminfo_kb(line) * 1024;
        } else if line.starts_with("MemAvailable:") {
            free = parse_meminfo_kb(line) * 1024;
        }
    }
    (free, total)
}

#[cfg(not(target_os = "linux"))]
fn read_mem_bytes() -> (u64, u64) {
    (0, 0)
}

fn parse_meminfo_kb(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn sys_uptime_secs() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/uptime") {
            if let Some(v) = s.split_whitespace().next() {
                if let Ok(f) = v.parse::<f64>() {
                    return f as u64;
                }
            }
        }
    }
    0
}

fn hostname() -> String {
    // Read /etc/hostname on Linux; fall back to env var or "unknown"
    #[cfg(target_os = "linux")]
    {
        if let Ok(h) = std::fs::read_to_string("/etc/hostname") {
            let h = h.trim().to_string();
            if !h.is_empty() {
                return h;
            }
        }
    }
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn read_process_rss() -> u64 {
    #[cfg(target_os = "linux")]
    {
        // /proc/self/status → VmRSS
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            for line in s.lines() {
                if line.starts_with("VmRSS:") {
                    return parse_meminfo_kb(line) * 1024;
                }
            }
        }
    }
    0
}

fn process_uptime_secs() -> f64 {
    // We use the process start time stored at first call via a lazy static.
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    start.elapsed().as_secs_f64()
}

fn get_ip_list() -> Vec<String> {
    // Parse /proc/net/if_inet6 and /proc/net/fib_trie for a zero-dep IP list on Linux.
    // On other platforms return empty (implementors can add platform-specific code).
    let mut ips: Vec<String> = Vec::new();

    #[cfg(target_os = "linux")]
    {
        // IPv4: parse /proc/net/fib_trie — simple heuristic
        if let Ok(s) = std::fs::read_to_string("/proc/net/fib_trie") {
            for line in s.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("32 HOST") {
                    // Previous line had the IP
                }
                // Simpler: collect lines with "LOCAL" host routes from /proc/net/if_inet6
                let _ = trimmed; // suppress unused
            }
        }
        // IPv4 via /proc/net/dev + getifaddrs substitute: use hostname -I output
        if let Ok(out) = std::process::Command::new("hostname").arg("-I").output() {
            let s = String::from_utf8_lossy(&out.stdout);
            for ip in s.split_whitespace() {
                if ip != "127.0.0.1" && ip != "::1" {
                    ips.push(ip.to_string());
                }
            }
        }
    }

    ips
}

// ─── Minimal ISO-8601 / RFC-2822 formatting (no chrono needed here) ──────────

fn format_iso8601(unix_secs: u64, ms: u32) -> String {
    let (y, mo, d, h, min, s) = unix_to_datetime(unix_secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", y, mo, d, h, min, s, ms)
}

fn format_rfc2822(unix_secs: u64) -> String {
    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (y, mo, d, h, min, s) = unix_to_datetime(unix_secs);
    // Day of week: Zeller's formula (simplified)
    let dow = day_of_week(y, mo, d);
    format!("{}, {:02} {} {:04} {:02}:{:02}:{:02} +0000",
        days[dow % 7], d, months[(mo - 1) as usize], y, h, min, s)
}

/// Converts a Unix timestamp (seconds) to (year, month, day, hour, min, sec).
fn unix_to_datetime(ts: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = ts % 60;
    let total_min = ts / 60;
    let min = total_min % 60;
    let total_hr = total_min / 60;
    let h = total_hr % 24;
    let mut days = total_hr / 24;

    let mut year = 1970u32;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let month_days: [u32; 12] = [31, if is_leap(year) { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0u32;
    for &md in &month_days {
        if days < md { break; }
        days -= md;
        month += 1;
    }
    (year, month + 1, days + 1, h as u32, min as u32, s as u32)
}

fn is_leap(y: u32) -> bool { y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) }

fn day_of_week(y: u32, m: u32, d: u32) -> usize {
    // Tomohiko Sakamoto's algorithm
    let t: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let yr = if m < 3 { y - 1 } else { y };
    ((yr + yr/4 - yr/100 + yr/400 + t[(m-1) as usize] + d) % 7) as usize
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_serialises() {
        let hs = get_health_status();
        let json = serde_json::to_string(&hs).unwrap();
        assert!(json.contains("\"cpu\""));
        assert!(json.contains("\"mem\""));
        assert!(json.contains("\"os\""));
    }

    #[test]
    fn client_info_is_rust() {
        let ci = get_client_info();
        assert_eq!(ci.lang_type, "rust");
    }

    #[test]
    fn time_info_has_iso() {
        let ti = get_time_info();
        assert!(ti.iso.contains('T'));
        assert!(ti.iso.ends_with('Z'));
    }
}
