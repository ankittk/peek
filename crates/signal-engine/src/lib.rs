//! Signal impact analysis and helpers for presenting safe signal choices.
//!
//! - `impact`: pre-flight analysis of TCP connections, children, file locks, and
//!   systemd units to help choose between graceful vs forceful signals.
//! - `signals`: canonical menu of user-facing signals and descriptions.
//! - `systemd`: unit detection from `/proc/<pid>/cgroup`.

pub mod impact;
pub mod signals;
pub mod systemd;

