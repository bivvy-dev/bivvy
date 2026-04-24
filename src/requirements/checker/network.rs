//! Network reachability check.

use std::net::TcpStream;
use std::time::Duration;

/// Check whether the network is reachable.
///
/// Attempts a TCP connection to well-known hosts with a 2-second timeout.
/// Returns `true` if any connection succeeds, `false` otherwise.
pub fn check_network() -> bool {
    const TARGETS: &[(&str, u16)] = &[
        ("1.1.1.1", 443), // Cloudflare DNS
        ("8.8.8.8", 443), // Google DNS
        ("9.9.9.9", 443), // Quad9 DNS
    ];
    let timeout = Duration::from_secs(2);

    for &(host, port) in TARGETS {
        let addr = format!("{}:{}", host, port);
        if let Ok(addr) = addr.parse() {
            if TcpStream::connect_timeout(&addr, timeout).is_ok() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_network_returns_bool() {
        // This is a real network call -- it should return true in most
        // dev environments, but we only verify it doesn't panic.
        let _reachable = check_network();
    }
}
