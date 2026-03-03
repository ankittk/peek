// Live kernel state: current syscall. Plan: small helpers returning Option.

use std::fs;
use std::path::Path;

/// Reads the current syscall from `/proc/<pid>/syscall`.
/// Format: "syscall_num arg1 arg2 arg3 arg4 arg5 arg6" (hex args on some kernels).
/// Returns (syscall_number, args). Returns `None` if unreadable or not in a syscall.
#[cfg(target_os = "linux")]
pub fn read_syscall(pid: i32) -> Option<(u64, [u64; 6])> {
    let path = Path::new("/proc").join(pid.to_string()).join("syscall");
    let s = fs::read_to_string(&path).ok()?;
    let s = s.trim_end();
    // First field is syscall number, then up to 6 args (hex or decimal).
    let mut parts = s.split_whitespace();
    let num: u64 = parts.next()?.parse().ok()?;
    let mut args = [0u64; 6];
    for (i, a) in parts.take(6).enumerate() {
        let v = a
            .strip_prefix("0x")
            .and_then(|h| u64::from_str_radix(h, 16).ok())
            .or_else(|| a.parse().ok())?;
        args[i] = v;
    }
    Some((num, args))
}

#[cfg(not(target_os = "linux"))]
pub fn read_syscall(_pid: i32) -> Option<(u64, [u64; 6])> {
    None
}
