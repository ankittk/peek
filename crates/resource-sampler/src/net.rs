// Network device stats from /proc/net/dev (global) or /proc/<pid>/net/dev (process namespace).
// Used for delta-over-time rate computation.

/// Total RX and TX bytes across all interfaces from `/proc/net/dev`.
/// Returns (rx_bytes, tx_bytes). Use with two samples and a time delta for rate.
#[cfg(target_os = "linux")]
pub fn read_net_dev() -> Option<(u64, u64)> {
    read_net_dev_from_path("/proc/net/dev")
}

/// Total RX and TX bytes for the process's network namespace from `/proc/<pid>/net/dev`.
/// Same format as global; useful for per-process (actually per-namespace) traffic rate.
#[cfg(target_os = "linux")]
pub fn read_net_dev_for_pid(pid: i32) -> Option<(u64, u64)> {
    read_net_dev_from_path(&format!("/proc/{}/net/dev", pid))
}

#[cfg(target_os = "linux")]
fn read_net_dev_from_path(path: &str) -> Option<(u64, u64)> {
    let raw = std::fs::read_to_string(path).ok()?;
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;
    for line in raw.lines().skip(2) {
        let colon = line.find(':')?;
        let rest = line[colon + 1..].trim_start();
        let nums: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.len() >= 8 {
            rx_total += nums[0];
            tx_total += nums[8];
        }
    }
    Some((rx_total, tx_total))
}

/// Sample RX/TX bytes per second for the process's network namespace.
///
/// The sampling window is controlled by the `PEEK_NET_SAMPLE_MS` environment variable:
/// - If unset, a default of 1000ms (1s) is used.
/// - If set to `0`, rate sampling is skipped and `None` is returned.
/// - Otherwise, the value is interpreted as a window in milliseconds.
///
/// Returns (rx_bytes_per_sec, tx_bytes_per_sec).
#[cfg(target_os = "linux")]
pub fn sample_network_rate(pid: i32) -> Option<(u64, u64)> {
    let window_ms = std::env::var("PEEK_NET_SAMPLE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1000);

    if window_ms == 0 {
        return None;
    }

    let window = std::time::Duration::from_millis(window_ms);

    let (r1, t1) = read_net_dev_for_pid(pid)?;
    std::thread::sleep(window);
    let (r2, t2) = read_net_dev_for_pid(pid)?;
    Some((r2.saturating_sub(r1), t2.saturating_sub(t1)))
}

#[cfg(not(target_os = "linux"))]
pub fn read_net_dev() -> Option<(u64, u64)> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn read_net_dev_for_pid(_pid: i32) -> Option<(u64, u64)> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn sample_network_rate(_pid: i32) -> Option<(u64, u64)> {
    None
}
