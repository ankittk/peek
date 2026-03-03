// Namespace type string → short description.

/// Returns a plain-English description of a Linux namespace type (e.g. "pid" → "Process ID isolation").
pub fn namespace_description(ns_type: &str) -> String {
    match ns_type {
        "pid" => "Process ID isolation: processes see only PIDs in this namespace".to_string(),
        "net" => "Network: separate network stack, interfaces, and ports".to_string(),
        "ipc" => "IPC: isolated System V IPC and POSIX message queues".to_string(),
        "mnt" => "Mount: separate mount hierarchy and filesystem view".to_string(),
        "uts" => "UTS: isolated hostname and NIS domain".to_string(),
        "user" => "User: separate UID/GID mapping (e.g. root in namespace ≠ root on host)".to_string(),
        "cgroup" => "Cgroup: separate cgroup hierarchy for resource limits".to_string(),
        "time" => "Time: separate system clock (e.g. for containers)".to_string(),
        _ => format!("Namespace type: {}", ns_type),
    }
}
