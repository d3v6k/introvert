use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use libp2p::{kad::QueryId, Swarm, Multiaddr, PeerId, identity::Keypair};
use libp2p::core::transport::ListenerId;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use webrtc::peer_connection::RTCPeerConnection;
use x25519_dalek::{StaticSecret, PublicKey};


/// Centralized connection budget to prevent connection storms.
/// All dial and reservation requests must consult this budget before proceeding.
/// This ensures scalability — at 1,000 devices, each device creates ≤6 connections per minute.
pub struct ConnectionBudget {
    /// Per-RBN last dial time
    pub(crate) last_dial: HashMap<PeerId, Instant>,
    /// Per-RBN last reservation request time
    pub(crate) last_reservation: HashMap<PeerId, Instant>,
    /// Rolling window of dial attempts (for global rate limiting)
    pub(crate) dial_window: VecDeque<Instant>,
    /// Rolling window of reservation attempts (for global rate limiting)
    pub(crate) reservation_window: VecDeque<Instant>,
}

impl ConnectionBudget {
    /// Minimum interval between dials to the same RBN
    const MIN_DIAL_INTERVAL: Duration = Duration::from_secs(30);
    /// Minimum interval between reservation requests to the same RBN
    const MIN_RESERVATION_INTERVAL: Duration = Duration::from_secs(15);
    /// Maximum dials per minute (global across all RBNs)
    const MAX_DIALS_PER_MINUTE: usize = 6;
    /// Maximum reservation requests per minute (global across all RBNs)
    const MAX_RESERVATIONS_PER_MINUTE: usize = 6;
    /// Window duration for rate limiting
    const WINDOW_DURATION: Duration = Duration::from_secs(60);

    pub fn new() -> Self {
        Self {
            last_dial: HashMap::new(),
            last_reservation: HashMap::new(),
            dial_window: VecDeque::new(),
            reservation_window: VecDeque::new(),
        }
    }

    /// Check if we can dial a specific RBN (per-RBN + global budget)
    pub fn can_dial(&mut self, peer_id: &PeerId) -> bool {
        let now = Instant::now();

        // Per-RBN throttle
        if let Some(last) = self.last_dial.get(peer_id) {
            if now.duration_since(*last) < Self::MIN_DIAL_INTERVAL {
                return false;
            }
        }

        // Global budget: prune old entries and check count
        self.dial_window.retain(|t| now.duration_since(*t) < Self::WINDOW_DURATION);
        if self.dial_window.len() >= Self::MAX_DIALS_PER_MINUTE {
            return false;
        }

        true
    }

    /// Check if we can request a reservation from a specific RBN
    pub fn can_request_reservation(&mut self, peer_id: &PeerId) -> bool {
        let now = Instant::now();

        // Per-RBN throttle
        if let Some(last) = self.last_reservation.get(peer_id) {
            if now.duration_since(*last) < Self::MIN_RESERVATION_INTERVAL {
                return false;
            }
        }

        // Global budget: prune old entries and check count
        self.reservation_window.retain(|t| now.duration_since(*t) < Self::WINDOW_DURATION);
        if self.reservation_window.len() >= Self::MAX_RESERVATIONS_PER_MINUTE {
            return false;
        }

        true
    }

    /// Record a dial attempt
    pub fn record_dial(&mut self, peer_id: PeerId) {
        let now = Instant::now();
        self.last_dial.insert(peer_id, now);
        self.dial_window.push_back(now);
    }

    /// Record a reservation request
    pub fn record_reservation(&mut self, peer_id: PeerId) {
        let now = Instant::now();
        self.last_reservation.insert(peer_id, now);
        self.reservation_window.push_back(now);
    }

    /// Urgent dial check — only per-RBN cooldown, no global budget.
    /// Used for startup, reconnect, and ping-failure recovery where
    /// the global budget would cause unacceptable delays.
    pub fn can_dial_urgent(&mut self, peer_id: &PeerId) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last_dial.get(peer_id) {
            if now.duration_since(*last) < Self::MIN_DIAL_INTERVAL {
                return false;
            }
        }
        true
    }

    /// Urgent reservation check — only per-RBN cooldown, no global budget.
    /// Used for startup, reconnect, and ping-failure recovery.
    pub fn can_request_reservation_urgent(&mut self, peer_id: &PeerId) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last_reservation.get(peer_id) {
            if now.duration_since(*last) < Self::MIN_RESERVATION_INTERVAL {
                return false;
            }
        }
        true
    }
}

/// Resolves the optimal transfer path for file payloads.
/// Priority: P1 Direct > P2/P3 LocalSeeder > P4 Relay
pub enum TransferPath {
    /// Dial recipient directly — same LAN or already-connected peer
    Direct(PeerId),
    /// Dial a different peer on our LAN who already holds the file
    LocalSeeder(PeerId),
    /// Recipient behind RBN — use gossipsub/relay circuit
    Relay(PeerId),
}

pub struct TransferRouter {
    /// Seeders that recently failed to serve — (transfer_id, seeder) -> last failure time
    pub(crate) failed_seeders: HashMap<(String, PeerId), Instant>,
    /// Cooldown before retrying a failed seeder
    pub(crate) seeder_cooldown: Duration,
}

impl TransferRouter {
    pub fn new() -> Self {
        Self {
            failed_seeders: HashMap::new(),
            seeder_cooldown: Duration::from_secs(30),
        }
    }

    /// Resolve the best transfer path for a file payload.
    pub fn resolve(
        &self,
        recipient_id: &PeerId,
        transfer_id: &str,
        mdns_peers: &HashSet<PeerId>,
        swarm_connected: impl Fn(&PeerId) -> bool,
        is_relayed: impl Fn(&PeerId) -> bool,
        known_seeders: &[PeerId],
        connectivity_type: u8,
    ) -> TransferPath {
        // On mobile data, mDNS LAN peers are unreachable — always relay
        if connectivity_type == 2 {
            crate::dispatch_debug_log(&format!(
                "[TransferRouter] Mobile data — forcing Relay for {}", recipient_id
            ));
            return TransferPath::Relay(*recipient_id);
        }

        let is_mdns = mdns_peers.contains(recipient_id);
        let is_connected = swarm_connected(recipient_id);
        let relayed = is_relayed(recipient_id);
        let in_cooldown = self.is_seeder_in_cooldown(transfer_id, recipient_id);

        crate::dispatch_debug_log(&format!(
            "[TransferRouter] resolve(tid={}, recipient={}): mdns_peers={}, connected={}, relayed={}, cooldown={}, mdns_set_size={}",
            &transfer_id[..transfer_id.len().min(20)], recipient_id, is_mdns, is_connected, relayed, in_cooldown, mdns_peers.len()
        ));

        // P1 — direct dial to the actual recipient (same LAN via mDNS AND not relayed)
        // Relay connections (different network via RBN) must NOT use Direct path even if
        // mdns_peers contains stale entries from a previous LAN session.
        if is_mdns && !relayed && !in_cooldown {
            crate::dispatch_debug_log(&format!("[TransferRouter] → Direct({})", recipient_id));
            return TransferPath::Direct(*recipient_id);
        }

        // P2/P3 — is there a seeder for this transfer on our LAN?
        if let Some(local_seeder) = known_seeders.iter().find(|p| {
            mdns_peers.contains(*p) && !self.is_seeder_in_cooldown(transfer_id, p)
        }) {
            crate::dispatch_debug_log(&format!("[TransferRouter] → LocalSeeder({})", local_seeder));
            return TransferPath::LocalSeeder(*local_seeder);
        }

        // P4 — fall back to relay
        crate::dispatch_debug_log(&format!("[TransferRouter] → Relay({})", recipient_id));
        TransferPath::Relay(*recipient_id)
    }

    fn is_seeder_in_cooldown(&self, transfer_id: &str, seeder: &PeerId) -> bool {
        if let Some(last_failure) = self.failed_seeders.get(&(transfer_id.to_string(), *seeder)) {
            last_failure.elapsed() < self.seeder_cooldown
        } else {
            false
        }
    }

    /// Mark a seeder as failed for a transfer (triggers cooldown before retry)
    pub fn mark_seeder_failed(&mut self, transfer_id: &str, seeder: PeerId) {
        self.failed_seeders.insert((transfer_id.to_string(), seeder), Instant::now());
    }

    /// Mark a direct-recipient as failed for a transfer (triggers cooldown before retry).
    /// When a Direct-path send fails, this prevents resolve() from returning Direct
    /// for the same (transfer_id, recipient) pair for seeder_cooldown seconds,
    /// allowing the next chunk to fall through to Relay.
    pub fn mark_direct_failed(&mut self, transfer_id: &str, recipient: PeerId) {
        self.failed_seeders.insert((transfer_id.to_string(), recipient), Instant::now());
    }

    /// Cleanup all state for a completed/evicted transfer
    pub fn clear_transfer(&mut self, transfer_id: &str) {
        self.failed_seeders.retain(|(tid, _), _| tid != transfer_id);
    }
}

use super::types::*;
use super::{registry, noise_session::NoiseSession, IntrovertBehaviour};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AppState {
    Foreground,
    Backgrounded,
    BackgroundedPendingWake,
}

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
    pub(crate) last_file_chunk_dial: HashMap<PeerId, Instant>,     // Phase 3.4: file chunk dial cooldown
    pub(crate) outbound_tracker: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, SignalingPayload)>,
    pub(crate) peer_supports_v2: HashSet<PeerId>,
    pub(crate) outbound_tracker_v2: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, SignalingPayload)>,
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
    pub(crate) tunnel_retry_count: u32,
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
    pub(crate) introclaw_command_log: Vec<(Instant, String)>,
    /// Pending ACKs to be batched: peer_id -> Vec<(msg_id, status)>
    pub(crate) pending_acks: HashMap<PeerId, Vec<(String, u8)>>,
    /// Peers discovered via mDNS (local network)
    pub(crate) mdns_peers: HashSet<PeerId>,
    /// Deduplication: recently seen group message IDs to prevent duplicate event dispatch
    pub(crate) seen_group_messages: HashSet<String>,
    /// Last time ACKs were flushed
    pub(crate) last_ack_flush: Instant,
    pub(crate) rbn_latencies: Arc<RwLock<HashMap<PeerId, u128>>>,
    pub(crate) pending_manual_rbns: Arc<RwLock<HashMap<Multiaddr, String>>>,
    /// Verified RBNs trusted for persistent mailbox storage.
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
    /// App foreground/background state — suppresses proactive dials when not foreground
    pub(crate) app_state: AppState,
    /// Last time app_state changed (for wake-on-push debounce)
    pub(crate) last_state_change: Instant,
    /// Cached connected peer count (avoids O(n) swarm.connected_peers().count())
    pub(crate) connected_peer_count: Arc<AtomicUsize>,
    /// Last time an idle-mode suppression log was emitted (rate-limit to 5min)
    pub(crate) last_idle_log: Instant,
    /// Last time a mailbox drain was performed (rate-limit to prevent spam on reservation flap)
    pub(crate) last_mailbox_drain: Instant,
    /// Last time the "skipping drain" log was printed (throttle to 1/min)
    pub(crate) last_mailbox_skip_log: Instant,
    /// Anchors with an active drain request — prevents concurrent drains to same RBN
    pub(crate) drain_in_progress: HashSet<PeerId>,
    /// Per-anchor last empty drain response (for empty-drain backoff)
    pub(crate) last_empty_drain: HashMap<PeerId, Instant>,
    /// Last time a file chunk drain was performed (separate from mail drain for faster chunk delivery)
    pub(crate) last_chunk_drain: Instant,
    /// Peers with an active flush task — prevents duplicate flush spawns on circuit flap
    /// Value is the Instant when the lock was acquired (for timeout fallback)
    pub(crate) flush_in_progress: HashMap<PeerId, Instant>,
    /// Intelligent file transfer router — resolves Direct vs LocalSeeder vs Relay path
    pub(crate) transfer_router: TransferRouter,
    /// Centralized connection budget — prevents connection storms at scale
    pub(crate) connection_budget: ConnectionBudget,
    /// Last Event 10 (node status) value dispatched — suppress duplicate status events
    pub(crate) last_dispatched_status: u8,
    /// Per-peer last Event 8 (relay status) dispatched — suppress duplicate peer status events
    pub(crate) last_peer_status: HashMap<PeerId, u8>,
    /// Peers that have received Event 1 (peer connected) — suppress re-dispatch on same connection
    pub(crate) connected_event_dispatched: HashSet<PeerId>,
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
