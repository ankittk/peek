// Linux: full /proc-based implementation. Non-Linux: sysinfo-based fallback.

#[cfg(target_os = "linux")]
pub mod env;
#[cfg(target_os = "linux")]
pub mod files;
#[cfg(target_os = "linux")]
pub mod gpu;
#[cfg(target_os = "linux")]
pub mod kernel;
#[cfg(target_os = "linux")]
pub mod network;
#[cfg(target_os = "linux")]
pub mod resources;
#[cfg(target_os = "linux")]
pub mod tree;

use crate::{ProcessInfo, Result};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::collect_process_impl;

#[cfg(not(target_os = "linux"))]
mod sysinfo_backend;
#[cfg(not(target_os = "linux"))]
use sysinfo_backend::collect_process_impl;

/// Build a ProcessInfo for the given PID. On Linux uses /proc; elsewhere uses sysinfo.
pub fn collect_process(pid: i32, sample_cpu: bool) -> Result<ProcessInfo> {
    collect_process_impl(pid, sample_cpu)
}
