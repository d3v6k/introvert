use libp2p::{Multiaddr, PeerId};

/// Returns a static list of global Root Bootstrap Nodes (RBNs).
/// These nodes provide initial entry points into the Sovereign P2P network.
pub fn get_bootstrap_nodes() -> Vec<(PeerId, Multiaddr)> {
    let nodes = [
        // Introvert Global Root Bootstrap Node (RBN)
        ("12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a", "/ip4/47.89.252.80/tcp/4001"),
        // official libp2p bootstrap nodes
        ("QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN", "/dnsaddr/bootstrap.libp2p.io"),
        ("QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa", "/dnsaddr/bootstrap.libp2p.io"),
        ("QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb", "/dnsaddr/bootstrap.libp2p.io"),
        ("QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt", "/dnsaddr/bootstrap.libp2p.io"),
    ];

    nodes.iter().filter_map(|(pid_str, addr_str)| {
        let pid = pid_str.parse::<PeerId>().ok()?;
        let addr = addr_str.parse::<Multiaddr>().ok()?;
        Some((pid, addr))
    }).collect()
}
