use libp2p::{
    kad::{self, store::MemoryStore},
    gossipsub,
    request_response,
    mdns,
    dcutr,
    relay,
    autonat,
    identify,
    ping,
    connection_limits,
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle},
    StreamProtocol,
    PeerId,
};
use tracing::debug;
use crate::network::{SignalingRequest, SignalingResponse};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

#[derive(NetworkBehaviour)]
pub struct IntrovertBehaviour {
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub request_response: request_response::json::Behaviour<SignalingRequest, SignalingResponse>,
    pub request_response_v2: request_response::Behaviour<crate::network::codec::IntrovertCodec>,
    pub mdns: Toggle<mdns::tokio::Behaviour>,
    pub dcutr: dcutr::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub relay_server: Toggle<relay::Behaviour>,
    pub autonat: autonat::Behaviour,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub connection_limits: connection_limits::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
}

impl IntrovertBehaviour {
    pub fn new(
        peer_id: PeerId,
        local_keypair: libp2p::identity::Keypair,
        relay_client: relay::client::Behaviour,
        enable_mdns: bool,
        enable_relay_server: bool,
        max_connections: u32,
    ) -> Self {
        let mut kad_config = kad::Config::new(StreamProtocol::new("/introvert/kad/1.0.0"));
        
        kad_config.set_record_ttl(Some(std::time::Duration::from_secs(24 * 60 * 60)));
        kad_config.set_publication_interval(Some(std::time::Duration::from_secs(60 * 60)));
        kad_config.set_replication_factor(std::num::NonZeroUsize::new(5).unwrap()); // Reduced from 20 to lower overhead
        
        let mut kademlia = kad::Behaviour::with_config(peer_id, MemoryStore::new(peer_id), kad_config);
        // DEFAULT TO CLIENT MODE: Only RBNs/Anchors should be DHT servers.
        // This prevents mobile devices from being hammered by global DHT queries.
        kademlia.set_mode(Some(kad::Mode::Client));
        
        let rr_config = request_response::Config::default()
            .with_request_timeout(std::time::Duration::from_secs(20)); // 20s is enough for relay latency without causing long stalls
        
        let codec = request_response::json::codec::Codec::<SignalingRequest, SignalingResponse>::default()
            .set_request_size_maximum(10 * 1024 * 1024) // 10MB limit for requests
            .set_response_size_maximum(10 * 1024 * 1024); // 10MB limit for responses
 
        let request_response = request_response::json::Behaviour::with_codec(
            codec,
            [(StreamProtocol::new("/introvert/signaling/1.0.0"), request_response::ProtocolSupport::Full)],
            rr_config.clone(),
        );

        let binary_codec = crate::network::codec::IntrovertCodec::default();
        let request_response_v2 = request_response::Behaviour::with_codec(
            binary_codec,
            [(StreamProtocol::new("/introvert/signaling/2.0.0"), request_response::ProtocolSupport::Full)],
            rr_config,
        );

        let mdns = if enable_mdns {
            match mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id) {
                Ok(b) => {
                    crate::dispatch_debug_log(&format!("[mDNS] Behaviour initialized for peer {}", peer_id));
                    Some(b)
                },
                Err(e) => {
                    crate::dispatch_debug_log(&format!("[mDNS] Initialization FAILED: {}", e));
                    None
                }
            }
        } else {
            crate::dispatch_debug_log("[mDNS] Disabled by config");
            None
        };
        
        let autonat = autonat::Behaviour::new(peer_id, autonat::Config::default());
        
        let identify = identify::Behaviour::new(identify::Config::new(
            "/introvert/id/1.0.0".to_string(),
            local_keypair.public(),
        ).with_agent_version("introvert/1.0.0".to_string()));


        let relay_server = if enable_relay_server {
            let relay_config = relay::Config {
                max_circuit_bytes: 1024 * 1024 * 1024, // 1GB for high-volume file relaying
                max_circuit_duration: std::time::Duration::from_secs(60 * 60), // 1 hour per circuit
                max_reservations: 8192,
                max_circuits: 4096,
                ..Default::default()
            };
            Some(relay::Behaviour::new(peer_id, relay_config))
        } else {
            None
        };

        let connection_limits = connection_limits::Behaviour::new(
            connection_limits::ConnectionLimits::default()
                .with_max_established_incoming(Some(max_connections))
                .with_max_established_outgoing(Some(max_connections))
                .with_max_established(Some(max_connections))
                .with_max_pending_incoming(Some(max_connections / 5))
                .with_max_pending_outgoing(Some(max_connections / 5))
        );

        let message_id_fn = |message: &gossipsub::Message| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            gossipsub::MessageId::from(s.finish().to_string())
        };
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(std::time::Duration::from_secs(10)) // 10s — v34/v37 baseline (DO NOT CHANGE)
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            // NOTE: No max_transmit_size — unlimited is the v34/v37 baseline.
            // A 1MB cap silently drops group messages and large profile avatars.
            .build()
            .expect("Valid gossipsub config");

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_keypair.clone()), 
            gossipsub_config
        ).expect("Valid gossipsub");

        Self {
            kademlia,
            request_response,
            request_response_v2,
            mdns: mdns.into(),
            dcutr: dcutr::Behaviour::new(peer_id),
            relay_client,
            relay_server: relay_server.into(),
            autonat,
            identify,
            ping: ping::Behaviour::default(),
            connection_limits,
            gossipsub,
        }
    }

    pub fn prune_stale_peers(&mut self) {
        let _ = self.kademlia.bootstrap();
        debug!("Swarm Maintenance: K-Bucket liveness check performed.");
    }
}
