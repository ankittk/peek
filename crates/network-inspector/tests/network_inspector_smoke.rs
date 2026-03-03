use network_inspector::{resolver, tcp, unix};

#[test]
fn inodes_using_port_does_not_panic() {
    let _ = tcp::inodes_using_port(0);
}

#[test]
fn collect_network_for_current_process_is_resilient() {
    let pid = std::process::id() as i32;
    let _ = tcp::collect_network(pid);
}

#[test]
fn unix_sockets_list_is_resilient() {
    let pid = std::process::id() as i32;
    let _ = unix::list_unix_sockets(pid);
}

#[test]
fn resolver_is_resilient() {
    let _ = resolver::resolve("127.0.0.1:80");
}

