// /proc/PID/limits — selected resource limits.
//
// For now we expose just the "Max open files" soft/hard limits, which are
// enough to drive fd-usage warnings in higher-level crates.

use crate::error::{io_to_error, Result};
use std::path::PathBuf;

/// Subset of `/proc/<pid>/limits` that we care about.
#[derive(Debug, Clone, Default)]
pub struct Limits {
    /// Soft limit for "Max open files", if set and not "unlimited".
    pub max_open_files_soft: Option<u64>,
    /// Hard limit for "Max open files", if set and not "unlimited".
    pub max_open_files_hard: Option<u64>,
}

/// Read `/proc/<pid>/limits` and extract the "Max open files" limits.
#[cfg(target_os = "linux")]
pub fn read_limits(pid: i32) -> Result<Limits> {
    let path = PathBuf::from(format!("/proc/{}/limits", pid));
    let raw = std::fs::read_to_string(&path).map_err(|e| io_to_error(path.clone(), e, pid))?;

    let mut out = Limits::default();

    for line in raw.lines() {
        if !line.starts_with("Max open files") {
            continue;
        }

        // Example line:
        // Max open files            1024                 4096                 files
        let mut parts = line.split_whitespace();

        // "Max", "open", "files", <soft>, <hard>, <units>...
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();

        let soft = parts.next();
        let hard = parts.next();

        out.max_open_files_soft = parse_limit_field(soft);
        out.max_open_files_hard = parse_limit_field(hard);

        break;
    }

    Ok(out)
}

/// On non-Linux platforms we don't have /proc; return empty limits.
#[cfg(not(target_os = "linux"))]
pub fn read_limits(_pid: i32) -> Result<Limits> {
    Ok(Limits::default())
}

fn parse_limit_field(field: Option<&str>) -> Option<u64> {
    let v = field?;
    if v == "unlimited" {
        return None;
    }
    v.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_limit_field;
    use proptest::prelude::*;

    #[test]
    fn parses_numeric_limits() {
        assert_eq!(parse_limit_field(Some("1024")), Some(1024));
        assert_eq!(parse_limit_field(Some("0")), Some(0));
    }

    #[test]
    fn treats_unlimited_as_none() {
        assert_eq!(parse_limit_field(Some("unlimited")), None);
    }

    #[test]
    fn handles_missing_field() {
        assert_eq!(parse_limit_field(None), None);
    }

    proptest! {
        #[test]
        fn parse_limit_field_never_panics_for_arbitrary_strings(s in ".*") {
            // Should not panic for any arbitrary input.
            let _ = parse_limit_field(Some(&s));
        }
    }
}
