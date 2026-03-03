// Unix domain sockets from /proc/net/unix, correlated with process fd inodes.

use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct UnixSocketEntry {
    pub path: String,
}

/// Collect socket inodes for this process from /proc/<pid>/fd.
fn socket_inodes(pid: i32) -> HashSet<u64> {
    let mut inodes = HashSet::new();
    let fd_dir = format!("/proc/{}/fd", pid);
    if let Ok(entries) = std::fs::read_dir(&fd_dir) {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                let s = target.to_string_lossy();
                if let Some(inode_str) =
                    s.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']'))
                {
                    if let Ok(inode) = inode_str.parse::<u64>() {
                        inodes.insert(inode);
                    }
                }
            }
        }
    }
    inodes
}

/// Parse /proc/net/unix and return path names for sockets belonging to `pid`.
/// Path may be empty for anonymous sockets.
#[cfg(target_os = "linux")]
pub fn list_unix_sockets(pid: i32) -> Vec<UnixSocketEntry> {
    let inodes = socket_inodes(pid);
    let raw = match std::fs::read_to_string("/proc/net/unix") {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut out = Vec::new();
    for line in raw.lines().skip(1) {
        // Format: "address refcount protocol flags type st inode path"
        // Path can contain spaces; inode is 7th field (index 6).
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 7 {
            continue;
        }
        let inode: u64 = match parts[6].parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if !inodes.contains(&inode) {
            continue;
        }
        let path = if parts.len() > 7 {
            parts[7..].join(" ").trim().to_string()
        } else {
            String::new()
        };
        out.push(UnixSocketEntry { path });
    }
    out
}

#[cfg(not(target_os = "linux"))]
pub fn list_unix_sockets(_pid: i32) -> Vec<UnixSocketEntry> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::UnixSocketEntry;

    #[test]
    fn unix_socket_entry_debug() {
        let e = UnixSocketEntry {
            path: "/tmp/test.sock".to_string(),
        };
        let debug = format!("{:?}", e);
        assert!(debug.contains("test.sock"));
    }
}
