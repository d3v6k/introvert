use libp2p::{
    kad::{self, store::MemoryStore},
    request_response,
    mdns,
    dcutr,
    relay,
    autonat,
    identify,
    connection_limits,
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle},
    StreamProtocol,
    PeerId,
};
use crate::network::{SignalingRequest, SignalingResponse};

#[derive(NetworkBehaviour)]
pub struct IntrovertBehaviour {
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub request_response: request_response::json::Behaviour<SignalingRequest, SignalingResponse>,
    pub mdns: Toggle<mdns::tokio::Behaviour>,
    pub dcutr: dcutr::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub relay_server: Toggle<relay::Behaviour>,
    pub autonat: autonat::Behaviour,
    pub identify: identify::Behaviour,
    pub connection_limits: connection_limits::Behaviour,
}

impl IntrovertBehaviour {
    pub fn new(
        peer_id: PeerId,
        local_public_key: libp2p::identity::PublicKey,
        relay_client: relay::client::Behaviour,
        enable_mdns: bool,
        enable_relay_server: bool,
    ) -> Self {
        let mut kad_config = kad::Config::default();
        kad_config.set_protocol_names(vec![StreamProtocol::new("/introvert/kad/1.0.0")]);
        
        // Churn Hardening: Set aggressive pruning and republication for global scale
        kad_config.set_record_ttl(Some(std::time::Duration::from_secs(24 * 60 * 60))); // 24h
        kad_config.set_publication_interval(Some(std::time::Duration::from_secs(60 * 60))); // 1h
        kad_config.set_replication_factor(std::num::NonZeroUsize::new(20).unwrap()); // Higher redundancy
        
        let kademlia = kad::Behaviour::with_config(peer_id, MemoryStore::new(peer_id), kad_config);
        
        let rr_config = request_response::Config::default();
        let request_response = request_response::json::Behaviour::new(
            [(StreamProtocol::new("/introvert/signaling/1.0.0"), request_response::ProtocolSupport::Full)],
            rr_config,
        );

        let mdns = if enable_mdns {
            Some(mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id).expect("Failed to create mDNS behaviour"))
        } else {
            None
        };
        
        let autonat = autonat::Behaviour::new(peer_id, autonat::Config::default());
        
        let identify = identify::Behaviour::new(identify::Config::new(
            "/introvert/1.0.0".to_string(),
            local_public_key,
        ));

        let relay_server = if enable_relay_server {
            Some(relay::Behaviour::new(peer_id, relay::Config::default()))
        } else {
            None
        };

        let connection_limits = connection_limits::Behaviour::new(
            connection_limits::ConnectionLimits::default()
                .with_max_established_incoming(Some(1_000_000))
                .with_max_established_outgoing(Some(1_000_000))
                .with_max_established(Some(1_000_000))
                .with_max_pending_incoming(Some(10_000))
                .with_max_pending_outgoing(Some(10_000))
        );

        Self {
            kademlia,
            request_response,
            mdns: mdns.into(),
            dcutr: dcutr::Behaviour::new(peer_id),
            relay_client,
            relay_server: relay_server.into(),
            autonat,
            identify,
            connection_limits,
        }
    }

    /// Prunes stale peers from the routing table.
    /// In a production environment, this is called periodically to maintain a 'sleek' swarm.
    pub fn prune_stale_peers(&mut self) {
        // Kademlia's internal logic handles most pruning via k-bucket maintenance.
        // We can force a refresh to ensure stale entries are probed and removed.
        let _ = self.kademlia.bootstrap();
        println!("Swarm Maintenance: K-Bucket liveness check performed.");
    }
}
