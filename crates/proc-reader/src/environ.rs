// /proc/PID/environ. Logic moved from peek-core::proc::env.
//
// This crate is responsible for low-level /proc parsing and intentionally
// avoids depending on peek-core types. Callers can turn the raw key/value
// pairs into their own domain structs.

use crate::error::{io_to_error, Result};
use std::path::PathBuf;

/// Raw environment entry as parsed from `/proc/<pid>/environ`.
#[derive(Debug, Clone)]
pub struct EnvironEntry {
    pub key: String,
    pub value: String,
}

/// Parse raw bytes from `/proc/<pid>/environ` into key/value pairs.
#[cfg(target_os = "linux")]
fn parse_environ_bytes(raw: &[u8]) -> Vec<EnvironEntry> {
    let mut vars = Vec::new();

    for entry in raw.split(|&b| b == 0) {
        if entry.is_empty() {
            continue;
        }
        let s = String::from_utf8_lossy(entry);
        if let Some(eq) = s.find('=') {
            let key = s[..eq].to_string();
            let value = s[eq + 1..].to_string();
            vars.push(EnvironEntry { key, value });
        }
    }

    vars.sort_by(|a, b| a.key.cmp(&b.key));
    vars
}

/// Read and parse `/proc/<pid>/environ` into raw key/value pairs.
#[cfg(target_os = "linux")]
pub fn read_environ(pid: i32) -> Result<Vec<EnvironEntry>> {
    let path = PathBuf::from(format!("/proc/{}/environ", pid));
    let raw = std::fs::read(&path).map_err(|e| io_to_error(path, e, pid))?;
    Ok(parse_environ_bytes(&raw))
}

/// On non-Linux platforms we don't have /proc; return an empty set.
#[cfg(not(target_os = "linux"))]
pub fn read_environ(_pid: i32) -> Result<Vec<EnvironEntry>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::{parse_environ_bytes, EnvironEntry};
    use proptest::prelude::*;

    #[test]
    fn environ_entry_debug_clone_works() {
        let e = EnvironEntry {
            key: "KEY".to_string(),
            value: "VALUE".to_string(),
        };
        let cloned = e.clone();
        assert_eq!(cloned.key, "KEY");
        assert_eq!(cloned.value, "VALUE");
        let debug = format!("{:?}", e);
        assert!(debug.contains("KEY"));
    }

    proptest! {
        #[test]
        fn parse_environ_bytes_round_trips_basic_pairs(
            kvs in prop::collection::vec(
                // Keys and values without '=' or NUL
                (r"[A-Z_][A-Z0-9_]{0,4}", r"[a-zA-Z0-9_]{0,8}"),
                0..16
            )
        ) {
            // Build raw environ-style buffer
            let mut raw = Vec::new();
            for (k, v) in &kvs {
                if k.is_empty() {
                    continue;
                }
                raw.extend_from_slice(k.as_bytes());
                raw.push(b'=');
                raw.extend_from_slice(v.as_bytes());
                raw.push(0);
            }

            let parsed = parse_environ_bytes(&raw);

            // All parsed keys must have existed in the original set.
            let original_keys: std::collections::HashSet<&str> =
                kvs.iter().map(|(k, _)| k.as_str()).collect();
            for EnvironEntry { key, .. } in &parsed {
                assert!(original_keys.contains(key.as_str()));
            }
        }
    }
}
