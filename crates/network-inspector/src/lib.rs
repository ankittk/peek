//! Per-process network view: TCP/Unix sockets, listening state, connections, reverse DNS.
//!
//! Reads `/proc/net/*` and `/proc/<PID>/fd` to list sockets and optional traffic rates.

pub mod resolver;
pub mod tcp;
pub mod unix;
