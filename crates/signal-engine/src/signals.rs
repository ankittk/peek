//! Signal menu and descriptions for kill panel and other UIs.
//!
//! Central source of truth for which signals we surface to users and how we
//! describe them in plain English.

/// `(name, number, description)` entries for commonly used signals.
///
/// The order here controls how menus are rendered.
pub const SIGNAL_MENU: &[(&str, i32, &str)] = &[
    (
        "SIGTERM",
        15,
        "asks the process to exit cleanly (can be caught/handled)",
    ),
    (
        "SIGKILL",
        9,
        "forces immediate termination; cannot be caught or ignored",
    ),
    ("SIGSTOP", 19, "pauses (stops) the process"),
    ("SIGCONT", 18, "resumes a stopped process"),
    ("SIGHUP", 1, "reloads configuration or reopens logs (convention)"),
    ("SIGUSR1", 10, "user-defined signal 1 (semantics are application-specific)"),
    ("SIGUSR2", 12, "user-defined signal 2 (semantics are application-specific)"),
];

/// Best-effort name lookup for a numeric signal.
pub fn signal_name(num: i32) -> &'static str {
    for (name, n, _) in SIGNAL_MENU {
        if *n == num {
            return name;
        }
    }
    "UNKNOWN"
}

