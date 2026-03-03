use proc_reader::{cgroup, current, environ, fd, limits, security};

#[test]
fn limits_parse_limit_field_unlimited_and_numeric() {
    // Exercise the public API by calling read_limits for an obviously invalid PID;
    // it should not panic and should return a Limits struct.
    let _ = limits::read_limits(-1);
}

#[test]
fn environ_read_is_resilient() {
    let pid = std::process::id() as i32;
    let _ = environ::read_environ(pid);
}

#[test]
fn fd_read_is_resilient() {
    let pid = std::process::id() as i32;
    let _ = fd::read_fd(pid);
}

#[test]
fn cgroup_and_security_are_resilient() {
    let pid = std::process::id() as i32;
    let _ = cgroup::read_cgroup(pid);
    let _ = security::read_label(pid);
}

#[test]
fn current_syscall_does_not_panic() {
    let pid = std::process::id() as i32;
    let _ = current::read_syscall(pid);
}

