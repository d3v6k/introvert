use libp2p::{Multiaddr, PeerId};
use std::net::ToSocketAddrs;

/// Returns a list of Root Bootstrap Nodes (RBNs).
/// These nodes provide initial entry points into the Sovereign P2P network.
pub fn get_bootstrap_nodes() -> Vec<(PeerId, Multiaddr)> {
    if std::env::var("INTROVERT_SKIP_BOOTSTRAP").is_ok() {
        return Vec::new();
    }
    let mut nodes = vec![
        // Introvert Global Root Bootstrap Node (RBN) - Port 443 (HTTPS Bypass)
        ("12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".to_string(), "/ip4/47.89.252.80/tcp/443".to_string()),
        ("12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".to_string(), "/ip4/47.89.252.80/udp/443/quic-v1".to_string()),
        
        // Local RBN on thinkpad.local (relay circuit via Alibaba) — v37 baseline
        ("12D3KooWGzorWx3pLhJCSdSZPApADf7aDM1g71WwvjjzubWSkCkG".to_string(), "/ip4/192.168.1.81/tcp/8443".to_string()),
    ];

    // NAT64 Resolution: Resolve the wildcard DNS to support IPv6-only cellular networks
    let rbn_pid = "12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a";
    if let Ok(addrs) = ("47.89.252.80.sslip.io", 443).to_socket_addrs() {
        for addr in addrs {
            let ip = addr.ip();
            // Avoid adding duplicate IPv4 if it was resolved
            if ip.is_ipv4() && ip.to_string() == "47.89.252.80" {
                continue;
            }
            let ip_str = ip.to_string();
            let (tcp_addr, udp_addr) = if ip.is_ipv4() {
                (format!("/ip4/{}/tcp/443", ip_str), format!("/ip4/{}/udp/443/quic-v1", ip_str))
            } else {
                (format!("/ip6/{}/tcp/443", ip_str), format!("/ip6/{}/udp/443/quic-v1", ip_str))
            };
            nodes.push((rbn_pid.to_string(), tcp_addr));
            nodes.push((rbn_pid.to_string(), udp_addr));
        }
    }

    // Support for extra bootstrap nodes via environment variable
    // Format: "PID1:ADDR1,PID2:ADDR2"
    if let Ok(extra) = std::env::var("INTROVERT_EXTRA_BOOTSTRAP") {
        for entry in extra.split(',') {
            if let Some((pid, addr)) = entry.split_once(':') {
                nodes.push((pid.to_string(), addr.to_string()));
            }
        }
    }

    nodes.iter().filter_map(|(pid_str, addr_str)| {
        let pid = pid_str.parse::<PeerId>().ok()?;
        let addr = addr_str.parse::<Multiaddr>().ok()?;
        Some((pid, addr))
    }).collect()
}
