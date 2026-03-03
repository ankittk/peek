// /proc/PID/fd/* and fdinfo. Logic moved from peek-core::proc::files.
//
// This crate is responsible for low-level /proc parsing and intentionally
// avoids depending on peek-core types. Callers can adapt `FdEntry` into
// whatever domain struct they need.

use crate::error::{io_to_error, Result};
use std::path::PathBuf;

/// One entry from `/proc/<pid>/fd`.
#[derive(Debug, Clone)]
pub struct FdEntry {
    pub fd: u32,
    pub fd_type: String,
    pub description: String,
}

/// Collect detailed information about open file descriptors for `pid`.
#[cfg(target_os = "linux")]
pub fn read_fd(pid: i32) -> Result<Vec<FdEntry>> {
    let fd_dir = format!("/proc/{}/fd", pid);
    let path = PathBuf::from(&fd_dir);
    let mut files = Vec::new();

    let entries = std::fs::read_dir(&path).map_err(|e| io_to_error(path.clone(), e, pid))?;
    let mut pairs: Vec<(u32, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let fd: u32 = e.file_name().to_string_lossy().parse().ok()?;
            Some((fd, e.path()))
        })
        .collect();
    pairs.sort_by_key(|(fd, _)| *fd);

    for (fd, path) in pairs {
        let (fd_type, description) = resolve_fd(pid, fd, &path);
        files.push(FdEntry {
            fd,
            fd_type,
            description,
        });
    }

    Ok(files)
}

/// Count open file descriptors for `pid`.
#[cfg(target_os = "linux")]
pub fn count_fds(pid: i32) -> Result<usize> {
    let fd_dir = format!("/proc/{}/fd", pid);
    let path = PathBuf::from(&fd_dir);
    let count = std::fs::read_dir(&path).map_err(|e| io_to_error(path, e, pid))?.count();
    Ok(count)
}

/// Non-Linux stub implementations: /proc is not available.
#[cfg(not(target_os = "linux"))]
pub fn read_fd(_pid: i32) -> Result<Vec<FdEntry>> {
    Ok(Vec::new())
}

#[cfg(not(target_os = "linux"))]
pub fn count_fds(_pid: i32) -> Result<usize> {
    Ok(0)
}

#[cfg(target_os = "linux")]
fn resolve_fd(pid: i32, fd: u32, fd_path: &std::path::Path) -> (String, String) {
    let target = match std::fs::read_link(fd_path) {
        Ok(t) => t.to_string_lossy().into_owned(),
        Err(_) => return ("unknown".to_string(), "?".to_string()),
    };

    if target.starts_with('/') {
        // Regular file or device
        let fd_type = if target.starts_with("/dev/") {
            "device"
        } else {
            "file"
        };
        // Try to get access mode from fdinfo
        let mode = read_fdinfo_mode(pid, fd);
        return (fd_type.to_string(), format!("{}{}", target, mode));
    }

    if let Some(inode) = target
        .strip_prefix("socket:[")
        .and_then(|s| s.strip_suffix(']'))
    {
        return ("socket".to_string(), format!("socket inode {}", inode));
    }

    if let Some(inode) = target
        .strip_prefix("pipe:[")
        .and_then(|s| s.strip_suffix(']'))
    {
        return ("pipe".to_string(), format!("pipe:[{}]", inode));
    }

    if target.starts_with("anon_inode:") {
        let kind = target.trim_start_matches("anon_inode:");
        let desc = match kind {
            "eventfd" => "eventfd (async event notification)".to_string(),
            "eventpoll" => "epoll instance".to_string(),
            "timerfd" => "timerfd (timer)".to_string(),
            "signalfd" => "signalfd (signal handler)".to_string(),
            _ => format!("anon_inode:{}", kind),
        };
        return ("anon_inode".to_string(), desc);
    }

    ("other".to_string(), target)
}

#[cfg(target_os = "linux")]
fn read_fdinfo_mode(pid: i32, fd: u32) -> String {
    let path = format!("/proc/{}/fdinfo/{}", pid, fd);
    if let Ok(raw) = std::fs::read_to_string(path) {
        for line in raw.lines() {
            if let Some(val) = line.strip_prefix("flags:\t") {
                let flags = u32::from_str_radix(val.trim(), 8).unwrap_or(0);
                let mode = flags & 0b11;
                return match mode {
                    0 => " (read-only)".to_string(),
                    1 => " (write-only)".to_string(),
                    2 => " (read-write)".to_string(),
                    _ => "".to_string(),
                };
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::FdEntry;

    #[test]
    fn fd_entry_clone_and_debug() {
        let e = FdEntry {
            fd: 3,
            fd_type: "file".to_string(),
            description: "/tmp/test".to_string(),
        };
        let cloned = e.clone();
        assert_eq!(cloned.fd, 3);
        assert_eq!(cloned.fd_type, "file");
        assert_eq!(cloned.description, "/tmp/test");
        let debug = format!("{:?}", e);
        assert!(debug.contains("FdEntry"));
    }
}
