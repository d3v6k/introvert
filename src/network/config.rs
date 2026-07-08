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
        // Port 80 fallback (corporate firewalls, captive portals)
        ("12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".to_string(), "/ip4/47.89.252.80/tcp/80".to_string()),
        // Additional RBNs can be added via INTROVERT_EXTRA_BOOTSTRAP env-var.
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
        if is_private_address(&addr) {
            return None;
        }
        Some((pid, addr))
    }).collect()
}

/// Persistent RBN history list for local-first client bootstrap.
///
/// Initialized with the master seed root (47.89.252.80) and 5-6 backup
/// developer node addresses. The IntroClaw engine loads this list *first*
/// on application startup, attempting immediate handshakes so the UI can
/// transition from "Connecting to the mesh swarm..." to fully online without
/// waiting for a Solana registry query.
///
/// The list is checked from top to bottom; the first successful connection
/// wins. After the client is online, a background task queries the Solana
/// Mainnet Registry to update this cache with verified on-chain addresses.
pub fn get_persistent_rbn_history_list() -> Vec<(PeerId, Multiaddr)> {
    if std::env::var("INTROVERT_SKIP_BOOTSTRAP").is_ok() {
        return Vec::new();
    }

    // Master seed root — the unbudgeable anchor
    let master_rbn = (
        "12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".to_string(),
        vec![
            "/ip4/47.89.252.80/tcp/443".to_string(),
            "/ip4/47.89.252.80/udp/443/quic-v1".to_string(),
            "/ip4/47.89.252.80/tcp/80".to_string(),
        ],
    );

    // Backup developer RBN nodes — geographically distributed
    let backup_rbns: Vec<(String, Vec<String>)> = vec![
        // EU-West backup
        (
            "12D3KooWBackupEU1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            vec![
                "/ip4/185.234.72.18/tcp/443".to_string(),
                "/ip4/185.234.72.18/udp/443/quic-v1".to_string(),
            ],
        ),
        // US-East backup
        (
            "12D3KooWBackupUS1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            vec![
                "/ip4/45.79.112.67/tcp/443".to_string(),
                "/ip4/45.79.112.67/udp/443/quic-v1".to_string(),
            ],
        ),
        // APAC backup
        (
            "12D3KooWBackupAP1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            vec![
                "/ip4/139.162.45.89/tcp/443".to_string(),
                "/ip4/139.162.45.89/udp/443/quic-v1".to_string(),
            ],
        ),
        // US-West backup
        (
            "12D3KooWBackupUW1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            vec![
                "/ip4/104.237.137.44/tcp/443".to_string(),
                "/ip4/104.237.137.44/udp/443/quic-v1".to_string(),
            ],
        ),
        // SA-East backup
        (
            "12D3KooWBackupSA1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            vec![
                "/ip4/177.71.128.5/tcp/443".to_string(),
                "/ip4/177.71.128.5/udp/443/quic-v1".to_string(),
            ],
        ),
        // AU-SE backup
        (
            "12D3KooWBackupAU1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            vec![
                "/ip4/45.76.120.23/tcp/443".to_string(),
                "/ip4/45.76.120.23/udp/443/quic-v1".to_string(),
            ],
        ),
    ];

    let mut list: Vec<(String, Vec<String>)> = Vec::with_capacity(1 + backup_rbns.len());
    list.push(master_rbn);
    list.extend(backup_rbns);

    // Flatten: one (PeerId, Multiaddr) per address, filtering private addresses
    let mut result = Vec::new();
    for (pid_str, addrs) in list {
        let pid: PeerId = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        for addr_str in addrs {
            if let Ok(addr) = addr_str.parse::<Multiaddr>() {
                if !is_private_address(&addr) {
                    result.push((pid, addr));
                }
            }
        }
    }
    result
}

/// Checks if a Multiaddr belongs to a private/local IP address range.
pub fn is_private_address(addr: &Multiaddr) -> bool {
    let s = addr.to_string();
    if s.contains("192.168.") || s.contains("10.") {
        return true;
    }
    if s.contains("172.") {
        for octet in 16..=31 {
            if s.contains(&format!("172.{}.", octet)) {
                return true;
            }
        }
    }
    false
}
