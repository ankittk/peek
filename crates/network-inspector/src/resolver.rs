// Best-effort reverse DNS with a short timeout. For CLI/TUI display only.

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc;
use std::time::Duration;

const RESOLVE_TIMEOUT_MS: u64 = 500;

/// Resolve an address like "192.168.1.1:443" or "fe80::1%eth0:80" to a hostname.
/// Returns the reverse DNS name if found within the timeout; otherwise returns `None`.
/// Use only for display (CLI/TUI); does not block core types.
pub fn resolve(addr: &str) -> Option<String> {
    let socket_addr: SocketAddr = addr.to_socket_addrs().ok()?.next()?;
    resolve_socket_addr(&socket_addr)
}

/// Reverse lookup for a `SocketAddr`. Time-bounded via a background thread.
pub fn resolve_socket_addr(addr: &SocketAddr) -> Option<String> {
    let addr = *addr;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let name = dns_lookup::lookup_addr(&addr.ip()).ok();
        let _ = tx.send(name);
    });
    rx.recv_timeout(Duration::from_millis(RESOLVE_TIMEOUT_MS))
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn resolve_handles_invalid_input() {
        // This should not panic and will likely return None.
        let res = resolve("not-a-valid-socket-address");
        assert!(res.is_none());
    }
}
