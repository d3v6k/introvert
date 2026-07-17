use libp2p::{
    kad::{self, store::MemoryStore},
    request_response,
    mdns,
    dcutr,
    relay,
    autonat,
    identify,
    ping,
    connection_limits,
    gossipsub,
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle},
    StreamProtocol,
    PeerId,
};
use crate::network::{SignalingRequest, SignalingResponse};
use crate::network::codec::{IntrovertCodec, BinarySignalingRequest, BinarySignalingResponse};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use tracing::debug;

#[derive(NetworkBehaviour)]
pub struct IntrovertBehaviour {
    pub kademlia: kad::Behaviour<MemoryStore>,
    /// v1.0.0 — JSON codec (legacy, compatible with all deployed clients)
    pub request_response: request_response::json::Behaviour<SignalingRequest, SignalingResponse>,
    /// v2.0.0 — Binary codec (activated for FileChunk sends to capable peers)
    pub request_response_v2: request_response::Behaviour<IntrovertCodec>,
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: Toggle<mdns::tokio::Behaviour>,
    pub dcutr: dcutr::Behaviour,
    pub relay_client: relay::client::Behaviour,
    pub relay_server: Toggle<relay::Behaviour>,
    pub autonat: autonat::Behaviour,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub connection_limits: connection_limits::Behaviour,
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
        
        // Set Kademlia mode: RBNs/relay servers must be in Server mode to answer queries,
        // while client edge nodes default to Client mode to conserve resources.
        kademlia.set_mode(Some(if enable_relay_server { kad::Mode::Server } else { kad::Mode::Client }));
        
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

        // v2.0.0 — Binary codec for FileChunk optimisation (~25% wire savings)
        // Both v1 and v2 are registered simultaneously. The RBN will advertise both
        // protocol IDs via Identify, allowing a seamless transition for clients.
        let binary_codec = IntrovertCodec::default();
        let request_response_v2 = request_response::Behaviour::with_codec(
            binary_codec,
            [(StreamProtocol::new("/introvert/signaling/2.0.0"), request_response::ProtocolSupport::Full)],
