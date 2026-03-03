// /proc/PID/attr/current — security label (AppArmor/SELinux/LSM).

use std::fs;
use std::path::Path;

/// Read the security label for a process from `/proc/<pid>/attr/current`.
///
/// Returns `None` if the file is missing, unreadable, or empty. The raw label
/// string is returned without interpretation.
#[cfg(target_os = "linux")]
pub fn read_label(pid: i32) -> Option<String> {
    let path = Path::new("/proc")
        .join(pid.to_string())
        .join("attr")
        .join("current");
    let raw = fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// On non-Linux platforms we don't have /proc; return `None`.
#[cfg(not(target_os = "linux"))]
pub fn read_label(_pid: i32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn non_linux_stub_returns_none() {
        #[cfg(not(target_os = "linux"))]
        {
            assert!(super::read_label(1).is_none());
        }
    }
}
