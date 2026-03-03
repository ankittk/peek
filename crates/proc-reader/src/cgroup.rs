// /proc/PID/cgroup. Logic moved from peek-core::proc::kernel.
//
// This module focuses on parsing the raw cgroup path; callers can decide how
// to interpret or normalise the string.

/// Read the "primary" cgroup path for `pid` from `/proc/<pid>/cgroup`.
#[cfg(target_os = "linux")]
pub fn read_cgroup(pid: i32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{}/cgroup", pid))
        .map(|s| {
            s.lines()
                .find(|l| l.starts_with("0:"))
                .or_else(|| s.lines().next())
                .map(|l| {
                    let parts: Vec<&str> = l.splitn(3, ':').collect();
                    parts.get(2).unwrap_or(&"unknown").to_string()
                })
                .unwrap_or_else(|| "unknown".to_string())
        })
        .ok()
}

#[cfg(not(target_os = "linux"))]
pub fn read_cgroup(_pid: i32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn non_linux_stub_returns_none() {
        #[cfg(not(target_os = "linux"))]
        {
            assert!(super::read_cgroup(1).is_none());
        }
    }
}
