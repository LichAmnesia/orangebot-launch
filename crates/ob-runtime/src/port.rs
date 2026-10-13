//! TCP port probing for sidecar services.

use std::net::TcpListener;

/// Whether `host:port` can currently be bound.
pub fn is_port_free(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port)).is_ok()
}

/// First free port in `[start, end_inclusive]`, if any.
pub fn find_free_port(host: &str, start: u16, end_inclusive: u16) -> Option<u16> {
    (start..=end_inclusive).find(|&port| is_port_free(host, port))
}
