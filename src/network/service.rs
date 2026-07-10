use std::collections::{HashMap, HashSet};
use std::time::Instant;
use std::sync::Arc;
use libp2p::{kad::QueryId, Swarm, Multiaddr, PeerId, identity::Keypair};
use libp2p::core::transport::ListenerId;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use webrtc::peer_connection::RTCPeerConnection;
use x25519_dalek::{StaticSecret, PublicKey};

use super::types::*;
use super::{registry, noise_session::NoiseSession, IntrovertBehaviour};

pub struct NetworkService {
    pub(crate) swarm: Swarm<IntrovertBehaviour>,
    pub(crate) command_rx: mpsc::Receiver<NetworkCommand>,
    pub(crate) command_tx: mpsc::Sender<NetworkCommand>,
    pub(crate) storage: Arc<crate::storage::StorageService>,
    pub(crate) peer_connections: Arc<RwLock<HashMap<PeerId, Arc<RTCPeerConnection>>>>,
    pub(crate) reward_tracker: Arc<crate::economy::RewardTracker>,
    pub(crate) solana_client: Arc<crate::economy::solana::SolanaIncentiveEngine>,
    pub(crate) daily_reward_engine: Option<Arc<crate::economy::daily_rewards::DailyRewardEngine>>,
    pub(crate) local_static_secret: StaticSecret,
    pub(crate) local_static_public: PublicKey,
    pub(crate) session_encryption_key: [u8; 32],
    pub(crate) noise_sessions: HashMap<PeerId, NoiseSession>,
    pub(crate) pending_handshakes: HashMap<QueryId, PeerId>,
    pub(crate) pending_messages: HashMap<PeerId, Vec<SignalingPayload>>,
    pub(crate) data_channels: Arc<RwLock<HashMap<PeerId, Arc<webrtc::data_channel::RTCDataChannel>>>>,
    pub(crate) incoming_transfers: HashMap<String, IncomingTransfer>,
    pub(crate) active_seeders: HashMap<String, ActiveSeeder>,
    pub(crate) active_providers: indexmap::IndexMap<String, Vec<PeerId>>,
    pub(crate) discovered_anchors: Vec<PeerId>,
    pub(crate) mesh_active_peers: HashSet<PeerId>,
    pub(crate) is_relayed_map: Arc<RwLock<HashMap<PeerId, bool>>>,
    pub(crate) connectivity_type: u8,
    pub(crate) direct_conn_count: HashMap<PeerId, usize>,
    pub(crate) relay_reservations: HashSet<PeerId>,
    pub(crate) relay_listeners: HashMap<ListenerId, PeerId>,
    pub(crate) relay_dial_limiter: HashMap<PeerId, (Instant, u32)>, // (last_attempt, failure_count)
    pub(crate) outbound_tracker: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, SignalingPayload)>,
    pub(crate) peer_supports_v2: HashSet<PeerId>,
    pub(crate) inflight_requests: HashMap<PeerId, u32>,
    pub(crate) liveness_interval_secs: u64,
    pub(crate) downloads_dir: String,
    pub(crate) local_keypair: Keypair,
    pub(crate) resolved_group_codes: indexmap::IndexMap<String, String>,
    pub(crate) anchor_mappings: HashMap<PeerId, Multiaddr>,
    pub(crate) bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
    pub(crate) _tunnel_handle: Option<tokio::task::JoinHandle<Result<(), anyhow::Error>>>,
    pub(crate) tunnel_active: bool,
    pub(crate) tunnel_started_at: Option<Instant>,
    pub(crate) pending_diagnostics: HashMap<PeerId, PendingDiagnostic>,
    pub(crate) registry: registry::RegistryManager,
    pub(crate) pending_claims: HashMap<String, HashSet<String>>,
    #[allow(dead_code)]
    pub(crate) diagnostic_requests: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, Instant)>,
    pub(crate) is_stress_test: bool,
    pub(crate) pending_offers: HashMap<PeerId, String>,
    pub(crate) early_chunks: indexmap::IndexMap<String, Vec<(u32, u32, String)>>,
    pub(crate) intro_claw: crate::intro_claw::IntroClawService,
    pub(crate) heal_rate_limiter: HashMap<PeerId, Instant>,
    pub(crate) pending_requester_static_keys: HashMap<String, Vec<u8>>,
    /// Peers discovered via mDNS (local network)
    pub(crate) mdns_peers: HashSet<PeerId>,
    /// Deduplication: recently seen group message IDs to prevent duplicate event dispatch
    pub(crate) seen_group_messages: HashSet<String>,
    pub(crate) rbn_latencies: Arc<RwLock<HashMap<PeerId, u128>>>,
    pub(crate) pending_manual_rbns: Arc<RwLock<HashMap<Multiaddr, String>>>,
    /// Verified RBNs trusted for relay routing.
    /// Populated from bootstrap_nodes (hardcoded) and future Solana registry.
    pub(crate) verified_rbns: HashSet<PeerId>,
    /// Chat syncs currently in progress (chat_id -> timestamp when sync started)
    pub(crate) sync_in_progress: HashMap<String, Instant>,
    /// Relay hints from FileChunkRequest: peer_id -> RBN peer_id they're behind
    /// Used to prioritize which RBN to dial when sending file chunks
    pub(crate) relay_hints: HashMap<PeerId, PeerId>,
    /// Last time telemetry was sent to RBN (for cooldown tracking)
    pub(crate) last_telemetry_sent: Instant,
    /// Consecutive status-check ticks with zero connected peers (for resilience ladder)
    pub(crate) consecutive_zero_peers_ticks: u32,
    /// Last time a relay reservation was attempted (rate-limit to prevent flooding)
    pub(crate) last_relay_reservation_attempt: Instant,
    /// Per-RBN push token registration timestamps (rate-limit to prevent flooding on Identify)
    pub(crate) last_token_registration: HashMap<PeerId, Instant>,
    /// Tracks the duration since pending_messages became non-empty
    pub(crate) pending_since: Option<Instant>,
    /// Tracks recently flushed message IDs per peer to prevent duplicate sends on circuit reconnect.
    /// Entries older than 60s are pruned on each flush cycle.
    pub(crate) recently_flushed: HashMap<PeerId, (Instant, HashSet<String>)>,
}

#[derive(Debug, Clone)]
pub struct PendingDiagnostic {
    pub(crate) start_time: Instant,
    pub(crate) transport: Option<String>,
}

pub struct IncomingTransfer {
    pub(crate) filename: String,
    pub(crate) mime_type: String,
    pub(crate) file_hash: String,
    pub(crate) total_size: usize,
    pub(crate) total_chunks: u32,
    pub(crate) received_chunks: HashMap<u32, Vec<u8>>,
    pub(crate) peer_id: PeerId,
    pub(crate) providers: Vec<PeerId>,
    pub(crate) start_time: Instant,
    pub(crate) last_update: Instant,
    pub(crate) last_retry: Instant,
    pub(crate) is_relayed: bool,
    pub(crate) group_id: Option<String>,
    pub(crate) next_pull_idx: u32,
    pub(crate) chunk_size: u32,
    pub(crate) stall_chunk_count: usize,
}

pub struct ActiveSeeder {
    pub(crate) peer_id: PeerId,
    pub(crate) file_path: String,
    pub(crate) file_hash: String,
    pub(crate) chunk_size: u32,
    pub(crate) total_chunks: u32,
    pub(crate) _bytes_sent: usize,
    pub(crate) _start_time: Instant,
    pub(crate) group_id: Option<String>,
    pub(crate) completions: HashSet<PeerId>,
}

pub struct NetworkConfig {
    pub keypair: Keypair,
    pub command_rx: mpsc::Receiver<NetworkCommand>,
    pub command_tx: mpsc::Sender<NetworkCommand>,
    pub storage: Arc<crate::storage::StorageService>,
    pub reward_tracker: Arc<crate::economy::RewardTracker>,
    pub solana_client: Arc<crate::economy::solana::SolanaIncentiveEngine>,
    pub daily_reward_engine: Option<Arc<crate::economy::daily_rewards::DailyRewardEngine>>,
    pub local_static_secret: StaticSecret,
    pub session_encryption_key: [u8; 32],
    pub enable_mdns: bool,
    pub enable_listeners: bool,
    pub tcp_port: u16,
    pub enable_relay_server: bool,
    pub max_connections: u32,
    pub liveness_interval_secs: u64,
    pub downloads_dir: String,
    pub is_stress_test: bool,
}

// Constructor is in mod.rs to avoid duplication
