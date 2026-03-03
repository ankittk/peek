// TCP/UDP /proc/net parsing for a single process.
//
// Logic moved from `peek-core::proc::network` so that low-level parsing lives
// in this crate. `peek-core` adapts these raw structs into its own
// `NetworkInfo`, `SocketEntry`, and `ConnectionEntry` types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};

/// One-pass: read /proc/net/{tcp,udp,tcp6,udp6} once and return inode -> (kind, local, remote) for the given port.
pub fn inodes_using_port(port: u16) -> HashMap<u64, (String, String, String)> {
    let mut map = HashMap::new();
    for &is_v6 in &[false, true] {
        for &udp in &[false, true] {
            let proto = match (is_v6, udp) {
                (false, false) => "TCP",
                (false, true) => "UDP",
                (true, false) => "TCP6",
                (true, true) => "UDP6",
            };
            let path = match (is_v6, udp) {
                (false, false) => "/proc/net/tcp",
                (false, true) => "/proc/net/udp",
                (true, false) => "/proc/net/tcp6",
                (true, true) => "/proc/net/udp6",
            };
            if let Ok(raw) = std::fs::read_to_string(path) {
                for line in raw.lines().skip(1) {
                    let fields: Vec<&str> = line.split_whitespace().collect();
                    if fields.len() < 10 {
                        continue;
                    }
                    let local = parse_addr(fields[1], is_v6);
                    let remote = parse_addr(fields[2], is_v6);
                    if local.1 != port && remote.1 != port {
                        continue;
                    }
                    let inode: u64 = fields[9].parse().unwrap_or(0);
                    let state_hex = u8::from_str_radix(fields[3], 16).unwrap_or(0);
                    let state = tcp_state(state_hex);
                    let kind = if state == "LISTEN" {
                        format!("LISTEN/{}", proto)
                    } else {
                        format!("CONN/{}", proto)
                    };
                    let local_s = format!("{}:{}", local.0, local.1);
                    let remote_s = if state == "LISTEN" {
                        "-".to_string()
                    } else {
                        format!("{}:{}", remote.0, remote.1)
                    };
                    map.insert(inode, (kind, local_s, remote_s));
                }
            }
        }
    }
    map
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketEntry {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionEntry {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub listening_tcp: Vec<SocketEntry>,
    pub listening_udp: Vec<SocketEntry>,
    pub connections: Vec<ConnectionEntry>,
}

/// Collect listening sockets and active connections for `pid` by inspecting
/// `/proc/<pid>/fd` and `/proc/net/{tcp,udp,tcp6,udp6}`.
pub fn collect_network(pid: i32) -> anyhow::Result<NetworkInfo> {
    // 1. Find socket inodes belonging to this process
    let socket_inodes = process_socket_inodes(pid);

    // 2. Parse kernel network tables
    let mut listening_tcp = Vec::new();
    let mut listening_udp = Vec::new();
    let mut connections = Vec::new();

    for &is_v6 in &[false, true] {
        for &udp in &[false, true] {
            let proto = match (is_v6, udp) {
                (false, false) => "TCP",
                (false, true) => "UDP",
                (true, false) => "TCP6",
                (true, true) => "UDP6",
            };
            let path = match (is_v6, udp) {
                (false, false) => "/proc/net/tcp",
                (false, true) => "/proc/net/udp",
                (true, false) => "/proc/net/tcp6",
                (true, true) => "/proc/net/udp6",
            };

            if let Ok(raw) = std::fs::read_to_string(path) {
                for line in raw.lines().skip(1) {
                    let fields: Vec<&str> = line.split_whitespace().collect();
                    if fields.len() < 10 {
                        continue;
                    }
                    let inode: u64 = fields[9].parse().unwrap_or(0);
                    if !socket_inodes.contains(&inode) {
                        continue;
                    }

                    let local = parse_addr(fields[1], is_v6);
                    let remote = parse_addr(fields[2], is_v6);
                    let state_hex = u8::from_str_radix(fields[3], 16).unwrap_or(0);
                    let state = tcp_state(state_hex);

                    if state == "LISTEN" {
                        let entry = SocketEntry {
                            protocol: proto.to_string(),
                            local_addr: local.0,
                            local_port: local.1,
                        };
                        if udp {
                            listening_udp.push(entry);
                        } else {
                            listening_tcp.push(entry);
                        }
                    } else if !udp || remote.1 != 0 {
                        connections.push(ConnectionEntry {
                            protocol: proto.to_string(),
                            local_addr: local.0,
                            local_port: local.1,
                            remote_addr: remote.0,
                            remote_port: remote.1,
                            state: state.to_string(),
                        });
                    }
                }
            }
        }
    }

    Ok(NetworkInfo {
        listening_tcp,
        listening_udp,
        connections,
    })
}

/// Socket inodes for a process (from /proc/<pid>/fd). Used by port search.
pub fn process_socket_inodes(pid: i32) -> HashSet<u64> {
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

/// Parse hex address:port like "0100007F:1F40" into ("127.0.0.1", 8000).
fn parse_addr(field: &str, is_v6: bool) -> (String, u16) {
    let parts: Vec<&str> = field.splitn(2, ':').collect();
    if parts.len() != 2 {
        return ("?".to_string(), 0);
    }
    let port = u16::from_str_radix(parts[1], 16).unwrap_or(0);
    let addr_hex = parts[0];

    let addr = if is_v6 {
        // Four 32-bit little-endian words
        let bytes: Vec<u32> = addr_hex
            .as_bytes()
            .chunks(8)
            .filter_map(|c| {
                let s = std::str::from_utf8(c).ok()?;
                u32::from_str_radix(s, 16).ok()
            })
            .collect();
        if bytes.len() == 4 {
            let b: Vec<u8> = bytes.iter().flat_map(|w| w.to_le_bytes()).collect();
            let arr: [u8; 16] = b.try_into().unwrap_or([0; 16]);
            Ipv6Addr::from(arr).to_string()
        } else {
            addr_hex.to_string()
        }
    } else if let Ok(n) = u32::from_str_radix(addr_hex, 16) {
        let ip = Ipv4Addr::from(n.to_le_bytes());
        ip.to_string()
    } else {
        addr_hex.to_string()
    };

    (addr, port)
}

fn tcp_state(state: u8) -> &'static str {
    match state {
        0x01 => "ESTABLISHED",
        0x02 => "SYN_SENT",
        0x03 => "SYN_RECV",
        0x04 => "FIN_WAIT1",
        0x05 => "FIN_WAIT2",
        0x06 => "TIME_WAIT",
        0x07 => "CLOSE",
        0x08 => "CLOSE_WAIT",
        0x09 => "LAST_ACK",
        0x0A => "LISTEN",
        0x0B => "CLOSING",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_addr, tcp_state};

    #[test]
    fn parses_ipv4_addr_and_port() {
        // 127.0.0.1:8000 encoded as hex
        let (addr, port) = parse_addr("0100007F:1F40", false);
        assert_eq!(addr, "127.0.0.1");
        assert_eq!(port, 8000);
    }

    #[test]
    fn parses_tcp_states() {
        assert_eq!(tcp_state(0x01), "ESTABLISHED");
        assert_eq!(tcp_state(0x0A), "LISTEN");
        assert_eq!(tcp_state(0xFF), "UNKNOWN");
    }
}
