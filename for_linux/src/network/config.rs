use libp2p::{Multiaddr, PeerId};

/// Returns a static list of global Root Bootstrap Nodes (RBNs).
/// These nodes provide initial entry points into the Sovereign P2P network.
pub fn get_bootstrap_nodes() -> Vec<(PeerId, Multiaddr)> {
    if std::env::var("INTROVERT_SKIP_BOOTSTRAP").is_ok() {
        return Vec::new();
    }
    let mut nodes = vec![
        // Introvert Global Root Bootstrap Node (RBN) - Port 443 (HTTPS Bypass)
        ("12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".to_string(), "/ip4/47.89.252.80/tcp/443".to_string()),
        ("12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".to_string(), "/ip4/47.89.252.80/udp/443/quic-v1".to_string()),
        
        // Private Introvert Network - Isolated from Global libp2p DHT
    ];

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
