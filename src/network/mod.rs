use libp2p::{
    kad::{self, Record, RecordKey, QueryId},
    request_response,
    swarm::SwarmEvent,
    core::transport::ListenerId,
    identity::Keypair,
    PeerId, Swarm, Multiaddr,
    futures::StreamExt,
};
use base64::{Engine as _, engine::general_purpose};
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};
use std::io::Read;
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use chrono::Utc;
use parking_lot::RwLock;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;
use libp2p::{autonat, identify};
use x25519_dalek::{StaticSecret, PublicKey};

pub mod noise_session;
pub mod wormhole;
pub mod behaviour;
pub mod config;
pub mod group;
pub mod registry;
pub mod tunnel;

use crate::media::{MediaManager, WebRtcSignal};
use crate::identity::SovereignIdentity;
use noise_session::NoiseSession;
pub use behaviour::{IntrovertBehaviour, IntrovertBehaviourEvent};

pub const ANCHOR_PROVIDER_KEY: &[u8] = b"/introvert/anchor_nodes";

// --- Group Mesh Types ---
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupRole {
    Creator,
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMemberMetadata {
    pub peer_id: String,
    pub pubkey: Vec<u8>,
    pub role: GroupRole,
    pub alias: Option<String>,
    pub avatar_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupAction {
    Message { 
        content_encrypted: Vec<u8>, 
        msg_id: String,
        #[serde(default)]
        reply_to: Option<String>,
    },
    AddMember { metadata: GroupMemberMetadata },
    RemoveMember { peer_id: String },
    UpdateRole { peer_id: String, new_role: GroupRole },
    DeleteGroup,
    Reaction { msg_id: String, emoji: String },
    SetRetention { seconds: u32 },
    DeleteMessage { msg_id: String },
    EditMessage { msg_id: String, new_content_encrypted: Vec<u8> },
    MuteMember { peer_id: String },
    UnmuteMember { peer_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedGroupAction {
    pub group_id: String,
    pub action: GroupAction,
    pub signer_peer_id: String,
    pub signature: Vec<u8>,
    pub timestamp: u64,
}

// --- Signaling Types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecureMessage {
    Handshake(Vec<u8>),
    Transport(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub sender_id: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum SignalingPayload {
    Standard(String),
    WebRtc(WebRtcSignal),
    Secure(SecureMessage),
    MailboxStore { recipient_id: String, payload: Vec<u8> },
    MailboxDrain,
    MailboxDrained(Vec<MailboxMessage>),
    Acknowledgement { msg_id: String, status: u8 }, // 1=Delivered, 2=Read
    Handshake(SovereignIdentity),
    Offer(WebRtcSignal),
    Answer(WebRtcSignal),
    Candidate(String),
    /// Raw flutter_webrtc SDP/ICE signal forwarded over the mesh
    WebRtcNative(String),
    ChatMessage { 
        content: String, 
        msg_id: String, 
        #[serde(default)]
        timestamp: i64,
        #[serde(default)]
        reply_to: Option<String>,
    },
    DirectInviteRequest(SovereignIdentity),
    DirectInviteAccept(SovereignIdentity),
    HandleClaimRequest {
        handle: String,
        peer_id: String,
        timestamp: i64,
        pow_nonce: u64,
    },
    HandleClaimWitnessed {
        handle: String,
        peer_id: String,
        timestamp: i64,
        rbn_peer_id: String,
        rbn_pubkey: Vec<u8>, // Ed25519 public key (Protobuf)
        rbn_signature: Vec<u8>,
    },
    IdentifySleepState {
        device_type: String, // "ios" or "android"
        push_token: String,
    },
    TypingStart {
        chat_id: String,
    },
    TypingStop {
        chat_id: String,
    },
    Heartbeat {
        timestamp: i64,
    },
    FileTransfer { 
        transfer_id: String, 
        filename: String, 
        mime_type: String, 
        file_hash: String, 
        total_size: usize, 
        #[serde(default)]
        is_relayed: bool,
        #[serde(default)]
        sender_peer_id: Option<String>,
        #[serde(default)]
        group_id: Option<String>,
    },
    FileChunkRequest { transfer_id: String, chunk_index: u32, #[serde(default)] chunk_size: Option<u32> },
    FileChunk { transfer_id: String, chunk_index: u32, total_chunks: u32, data_base64: String },
    FileTransferComplete { transfer_id: String },
    FileTransferError { transfer_id: String, reason: String },
    DeleteMessage { msg_id: String },
    EditMessage { msg_id: String, new_content: String },
    SetRetention { seconds: u32 },
    MessageReaction { msg_id: String, emoji: String },
    // Group Mesh
    GroupManifestRequest { group_id: String, alias: Option<String>, avatar: Option<String>, #[serde(default)] handle: Option<String> },
    GroupInvite { group_id: String, name: String, description: String, inviter_peer_id: String, group_secret_wrapped: Vec<u8>, members: Vec<GroupMemberMetadata> },
    GroupAction(SignedGroupAction),
    GroupManifest { group_id: String, name: String, description: String, members: Vec<GroupMemberMetadata>, secret: [u8; 32] },
    GroupJoinRejected { group_id: String, group_name: String, reason: String },
    RequestHandshake,
    ProfileRequest,
    ProfileResponse {
        name: String,
        handle: String,
        avatar_base64: Option<String>,
    },
    /// Request message sync from a peer (1:1 or group)
    ChatSyncRequest {
        chat_id: String,
        is_group: bool,
        last_msg_id: Option<String>,
        last_timestamp: i64,
        limit: u32,
    },
    /// Response with messages the requester is missing
    ChatSyncResponse {
        chat_id: String,
        is_group: bool,
        messages: Vec<SyncMessage>,
        has_more: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    pub msg_id: String,
    pub sender_id: String,
    pub content: String,
    pub timestamp: String,
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingRequest(pub SignalingPayload);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingResponse(pub String);

// --- Network Commands ---

pub enum NetworkCommand {
    Dial { peer_id: PeerId, address: Option<Multiaddr> },
    ListenOn { address: Multiaddr },
    SendSignaling { peer_id: PeerId, msg_id: String, message: String, reply_to: Option<String> },
    InitiateWebRtc { peer_id: PeerId, media_type: u8 },
    StartMediaStream { peer_id: PeerId, media_type: u8 },
    CloseWebRtc { peer_id: PeerId },
    WebRtcFailed { peer_id: PeerId },
    RenegotiateWebRtc { peer_id: PeerId },
    AcceptWebRtc { peer_id: PeerId, media_type: u8 },
    RejectWebRtc { peer_id: PeerId },
    AddAddress { peer_id: PeerId, address: Multiaddr },
    EstablishSecureSession { peer_id: PeerId },
    FetchMailbox,
    UpdateAnchorStatus { enabled: bool },
    SendFile { peer_id: PeerId, file_path: String, group_id: Option<String>, transfer_id: Option<String> },
    SendFileFinalize { peer_id: PeerId, file_path: String, has_dc_already: bool, group_id: Option<String>, transfer_id: Option<String> },
    SendFileChunk { peer_id: PeerId, payload: SignalingPayload, progress: FileTransferProgress },
    SendAcknowledgement { peer_id: PeerId, msg_id: String, status: u8 },
    ForwardMeshSignaling { peer_id: PeerId, payload: SignalingPayload },
    /// Forward a raw flutter_webrtc JSON signal over the mesh (no webrtc-rs involvement)
    ForwardWebRtcNative { peer_id: PeerId, json: String },
    HandleIncomingPayload { peer_id: PeerId, payload: SignalingPayload },
    HandleIncomingWebRtcPayload { peer_id: PeerId, payload: SignalingPayload },
    ResolveHandle { handle: String },
    SendDirectInvite { peer_id: PeerId, identity: SovereignIdentity, is_accept: bool },
    ClaimHandle { handle: String },
    BroadcastWitness { handle: String, peer_id: String, timestamp: i64, pubkey: Vec<u8>, signature: Vec<u8> },
    AddGroupMember { group_id: String, peer_id: String },
    RemoveGroupMember { group_id: String, peer_id: String, members_json: Option<String> },
    UpdateGroupRole { group_id: String, peer_id: String, role: GroupRole },
    PublishGroupManifest { group_id: String, code: String },
    JoinMeshByCode { code: String },
    AcceptGroupInvite { group_id: String },
    DeclineGroupInvite { group_id: String },
    ApproveGroupJoin { group_id: String, requester_peer_id: String, alias: Option<String>, avatar: Option<String>, handle: Option<String> },
    RejectGroupJoin { group_id: String, requester_peer_id: String, reason: String },
    BroadcastGroupMessage { group_id: String, message: String, reply_to: Option<String> },
    PublishGossipsub { topic: String, data: Vec<u8> },
    ForceMeshRefresh,
    RegisterSeeder { peer_id: PeerId, transfer_id: String, file_path: String, file_hash: String, chunk_size: u32, total_chunks: u32, group_id: Option<String> },
    UnregisterSeeder { transfer_id: String },
    FindProviders { file_hash: String },
    /// Force-store a payload in the anchor mailbox for a peer, bypassing all direct delivery.
    /// Used when direct relay sends fail — avoids the direct-retry loop.
    StoreInMailbox { peer_id: PeerId, payload: SignalingPayload },
    CancelFileTransfer { transfer_id: String },
    RecheckConnection { peer_id: PeerId },
    HandleDiagnosticTimeout { peer_id: PeerId },
    RequestSwarmStats,
    PollPeerProfile { peer_id: PeerId },
    /// Trigger message sync with a peer (1:1) or all group members
    SyncChatMessages { peer_id: PeerId, chat_id: String, is_group: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferProgress {
    pub transfer_id: String,
    pub peer_id: String,
    pub filename: String,
    pub mime_type: String,
    pub file_hash: String,
    pub progress: f32, // 0.0 to 1.0
    pub is_complete: bool,
    pub is_verified: bool,
    pub is_outgoing: bool,
    pub local_path: Option<String>,
    pub start_time_ms: u64,
    pub speed_bps: f64,
    pub group_id: Option<String>,
}

#[allow(dead_code)]
struct IncomingTransfer {
    filename: String,
    mime_type: String,
    file_hash: String,
    total_size: usize,
    total_chunks: u32,
    received_chunks: HashMap<u32, Vec<u8>>,
    peer_id: PeerId,
    providers: Vec<PeerId>,
    start_time: Instant,
    last_update: Instant,
    is_relayed: bool,
    group_id: Option<String>,
    next_pull_idx: u32,
    chunk_size: u32,
    /// Chunk count at the last watchdog tick — only re-request if this hasn't grown.
    /// Prevents the watchdog from looping when a slow seeder (serving multiple peers)
    /// takes longer than the stall threshold to deliver the next chunk.
    stall_chunk_count: usize,
}

struct ActiveSeeder {
    peer_id: PeerId,
    file_path: String,
    file_hash: String,
    chunk_size: u32,
    total_chunks: u32,
    _bytes_sent: usize,
    _start_time: Instant,
    group_id: Option<String>,
    pub completions: HashSet<PeerId>,
}

// --- FFI Network Callback ---
pub type FfiNetworkCallback = extern "C" fn(event_type: i32, data_ptr: *const u8, data_len: usize);

pub struct NetworkService {
    swarm: Swarm<IntrovertBehaviour>,
    command_rx: mpsc::Receiver<NetworkCommand>,
    command_tx: mpsc::Sender<NetworkCommand>,
    storage: Arc<crate::storage::StorageService>,
    peer_connections: Arc<RwLock<HashMap<PeerId, Arc<RTCPeerConnection>>>>,
    reward_tracker: Arc<crate::economy::RewardTracker>,
    solana_client: Arc<crate::economy::solana::SolanaIncentiveEngine>,
    local_static_secret: StaticSecret,
    local_static_public: PublicKey,
    session_encryption_key: [u8; 32],
    noise_sessions: HashMap<PeerId, NoiseSession>,
    pending_handshakes: HashMap<QueryId, PeerId>,
    pending_messages: HashMap<PeerId, Vec<SignalingPayload>>,
    data_channels: Arc<RwLock<HashMap<PeerId, Arc<webrtc::data_channel::RTCDataChannel>>>>,
    incoming_transfers: HashMap<String, IncomingTransfer>,
    active_seeders: HashMap<String, ActiveSeeder>,
    active_providers: HashMap<String, Vec<PeerId>>,
    discovered_anchors: Vec<PeerId>,
    mesh_active_peers: HashSet<PeerId>,
    is_relayed_map: Arc<RwLock<HashMap<PeerId, bool>>>,
    direct_conn_count: HashMap<PeerId, usize>,
    /// Peers we have already requested a relay reservation from in this session
    relay_reservations: HashSet<PeerId>,
    /// Map of ListenerId to the relay PeerId it belongs to
    relay_listeners: HashMap<ListenerId, PeerId>,
    relay_dial_limiter: HashMap<PeerId, Instant>,
    outbound_tracker: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, SignalingPayload)>,
    /// Per-peer count of in-flight request_response sends (to prevent relay flooding)
    inflight_requests: HashMap<PeerId, u32>,
    liveness_interval_secs: u64,
    downloads_dir: String,
    local_keypair: Keypair,
    resolved_group_codes: HashMap<String, String>,
    /// Track the physical addresses of connected anchor nodes for reliable relaying
    anchor_mappings: HashMap<PeerId, Multiaddr>,
    bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
    _tunnel_handle: Option<tokio::task::JoinHandle<Result<(), anyhow::Error>>>,
    pending_diagnostics: HashMap<PeerId, PendingDiagnostic>,
    registry: registry::RegistryManager,
    pending_claims: HashMap<String, HashSet<String>>, // Handle -> RBN Witnesses
    #[allow(dead_code)]
    diagnostic_requests: HashMap<libp2p::request_response::OutboundRequestId, (PeerId, Instant)>,
    is_stress_test: bool,
    pending_offers: HashMap<PeerId, String>,
}

#[derive(Debug, Clone)]
struct PendingDiagnostic {
    start_time: Instant,
    transport: Option<String>,
}

impl NetworkService {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        keypair: Keypair, 
        _callback: FfiNetworkCallback,
        command_rx: mpsc::Receiver<NetworkCommand>,
        command_tx: mpsc::Sender<NetworkCommand>,
        storage: Arc<crate::storage::StorageService>,
        reward_tracker: Arc<crate::economy::RewardTracker>,
        solana_client: Arc<crate::economy::solana::SolanaIncentiveEngine>,
        local_static_secret: StaticSecret,
        session_encryption_key: [u8; 32],
        enable_mdns: bool,
        enable_listeners: bool,
        tcp_port: u16,
        enable_relay_server: bool,
        max_connections: u32,
        liveness_interval_secs: u64,
        downloads_dir: String,
        is_stress_test: bool,
    ) -> anyhow::Result<Self> {
        let local_static_public = PublicKey::from(&local_static_secret);
        let local_peer_id = PeerId::from(keypair.public());

        // Resolve Bootstrap Nodes (taking into account Tunnel Mode)
        let is_tunnel_enabled = storage.is_tunnel_mode_enabled();
        let mut tunnel_handle = None;
        let mut bootstrap_nodes = config::get_bootstrap_nodes();

        if is_tunnel_enabled {
            println!("[Tunnel] Secure Tunnel Mode is active. Launching loopback client...");
            // Start local tunnel listener on a dynamic port (0 means dynamic)
            let rbn_ws_url = "ws://47.89.252.80:80/tunnel".to_string();
            match tunnel::start_tunnel_client(0, rbn_ws_url).await {
                Ok((assigned_port, handle)) => {
                    tunnel_handle = Some(handle);
                    // Map RBN PeerID to localhost TCP port
                    let rbn_peer_id: PeerId = "12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a".parse().unwrap();
                    let local_tunnel_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", assigned_port).parse().unwrap();
                    bootstrap_nodes = vec![(rbn_peer_id, local_tunnel_addr)];
                    println!("[Tunnel] WebSocket Tunnel active on local port {}. Bootstrapping via localhost.", assigned_port);
                }
                Err(e) => {
                    eprintln!("[Tunnel] Failed to start WebSocket tunnel: {}", e);
                }
            }
        }

        macro_rules! build_swarm {
            ($builder:expr) => {
                {
                    let mut yamux_config = libp2p::yamux::Config::default();
                    yamux_config.set_max_num_streams(1024);
                    #[allow(deprecated)]
                    yamux_config.set_receive_window_size(16 * 1024 * 1024); // 16 MiB window size for high-speed transfers
                    $builder
                        .with_relay_client(libp2p::noise::Config::new, move || yamux_config.clone())?
                        .with_behaviour(|keypair: &libp2p::identity::Keypair, relay_client| {
                            IntrovertBehaviour::new(local_peer_id, keypair.clone(), relay_client, enable_mdns, enable_relay_server, max_connections)
                        })?
                        .with_swarm_config(|c: libp2p::swarm::Config| {
                            c.with_idle_connection_timeout(Duration::from_secs(600)) // Keep idle connections alive for 10 mins
                        })
                        .build()
                }
            };
        }

        let builder = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default().nodelay(true),
                libp2p::noise::Config::new,
                || {
                    let mut yamux_config = libp2p::yamux::Config::default();
                    yamux_config.set_max_num_streams(1024);
                    #[allow(deprecated)]
                    yamux_config.set_receive_window_size(16 * 1024 * 1024); // 16 MiB window size for high-speed transfers
                    yamux_config
                },
            )?
            .with_quic_config(|mut c| {
                c.keep_alive_interval = Duration::from_secs(30);
                c
            });

        let mut swarm = if cfg!(target_os = "android") {
            build_swarm!(builder)
        } else {
            build_swarm!(builder.with_dns()?)
        };

        if enable_listeners {
            swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{}", tcp_port).parse()?)?;
            swarm.listen_on(format!("/ip4/0.0.0.0/udp/{}/quic-v1", tcp_port).parse()?)?;

            // Event 10: Local Node Status (1 = Online/Listening)
            crate::dispatch_global_event(10, &[1]);
        }

        // Subscribe to gossipsub topics for all existing groups
        if let Ok(groups) = storage.get_all_groups() {
            for (group_id, _, _, _, _) in groups {
                let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                if let Err(e) = swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    eprintln!("[Mesh] Failed to subscribe to gossipsub topic {}: {}", group_id, e);
                } else {
                    println!("[Mesh] Subscribed to gossipsub topic {}", group_id);
                }
            }
        }

        Ok(Self {
            swarm, 
            command_rx,
            command_tx,
            storage: storage.clone(),
            peer_connections: Arc::new(RwLock::new(HashMap::new())),
            reward_tracker,
            solana_client,
            local_static_secret,
            local_static_public,
            session_encryption_key,
            noise_sessions: HashMap::new(),
            pending_handshakes: HashMap::new(),
            pending_messages: HashMap::new(),
            data_channels: Arc::new(RwLock::new(HashMap::new())),
            incoming_transfers: HashMap::new(),
            active_seeders: HashMap::new(),
            active_providers: HashMap::new(),
            discovered_anchors: Vec::new(),
            mesh_active_peers: HashSet::new(),
            is_relayed_map: Arc::new(RwLock::new(HashMap::new())),
            direct_conn_count: HashMap::new(),
            relay_reservations: HashSet::new(),
            relay_listeners: HashMap::new(),
            relay_dial_limiter: HashMap::new(),
            outbound_tracker: HashMap::new(),
            inflight_requests: HashMap::new(),
            liveness_interval_secs,
            downloads_dir,
            local_keypair: keypair,
            resolved_group_codes: HashMap::new(),
            anchor_mappings: HashMap::new(),
            bootstrap_nodes,
            _tunnel_handle: tunnel_handle,
            pending_diagnostics: HashMap::new(),
            registry: registry::RegistryManager::new(storage.clone()),
            pending_claims: HashMap::new(),
            diagnostic_requests: HashMap::new(),
            is_stress_test,
            pending_offers: HashMap::new(),
        })
    }

    pub async fn run(mut self) {
        let peer_connections_reaper = Arc::clone(&self.peer_connections);
        tokio::spawn(async move {
            Self::start_peer_reaper(peer_connections_reaper).await;
        });

        let storage_cleaner = Arc::clone(&self.storage);
        tokio::spawn(async move {
            Self::start_mailbox_cleanup(storage_cleaner).await;
        });

        let pruner_storage = Arc::clone(&self.storage);
        tokio::spawn(async move {
            Self::start_message_pruning(pruner_storage).await;
        });

        let local_peer_id = *self.swarm.local_peer_id();
        let pubkey_record = Record {
            key: RecordKey::new(&local_peer_id.to_bytes()),
            value: self.local_static_public.to_bytes().to_vec(),
            publisher: Some(local_peer_id),
            expires: None,
        };
        let _ = self.swarm.behaviour_mut().kademlia.put_record(pubkey_record, kad::Quorum::One);

        // Pre-populate anchors with known RBN nodes
        for (peer_id, addr) in self.bootstrap_nodes.clone() {
            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
            if !self.discovered_anchors.contains(&peer_id) {
                self.discovered_anchors.push(peer_id);
            }
            let _ = self.swarm.dial(addr);
        }
        
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
        
        // Check Kademlia DHT for restored handle claim: ph_<peer_id>
        let has_handle = self.storage.get_profile().ok().flatten().and_then(|(_, h, _, _)| h).is_some();
        if !has_handle {
            let my_pid = local_peer_id.to_string();
            println!("[Mesh] No local handle set. Querying Kademlia DHT for restored handle claim ph_{}...", my_pid);
            let ph_key = RecordKey::new(&format!("ph_{}", my_pid).as_bytes());
            let _ = self.swarm.behaviour_mut().kademlia.get_record(ph_key);
        }

        self.perform_mailbox_fetch().await;

        // RBN HARDENING: Always provide Anchor Node service if we are a relay server
        if self.storage.is_anchor_mode_enabled() || self.swarm.behaviour().relay_server.as_ref().is_some() {
            println!("[Network] Sovereign Anchor Mode: Providing Anchor Node service.");
            println!("[Network] 🛡️  ISOLATION ACTIVE: Protocol set to /introvert/kad/1.0.0");
            self.swarm.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server)); // Act as full DHT server
            let key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
            let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
        }

        let mut republication_interval = tokio::time::interval(Duration::from_secs(60)); // 1 min (Aggressive for Background Reachability)
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(30)); // 30s last-seen broadcast

        let mut liveness_interval = tokio::time::interval(Duration::from_secs(self.liveness_interval_secs));
        let mut contact_refresh_interval = tokio::time::interval(Duration::from_secs(120)); // 2 min (was 30s)
        let mut anchor_discovery_interval = tokio::time::interval(Duration::from_secs(2 * 60));
        let mut mailbox_fetch_interval = tokio::time::interval(Duration::from_secs(120)); // 2 min (was 30s)
        let mut fast_poll_interval = tokio::time::interval(Duration::from_secs(1)); // Fast poll when transfers are active
        let mut status_check_interval = tokio::time::interval(Duration::from_secs(60)); // 60s (was 15s)
        let mut pull_retry_interval = tokio::time::interval(Duration::from_secs(1));
        let mut lease_interval = tokio::time::interval(Duration::from_secs(60 * 60));
        // heartbeat_interval REMOVED — not needed with push notifications


        let mut last_status = 0u8;
        let mut last_fast_mailbox_fetch = Instant::now() - Duration::from_secs(60);

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    // Broadcast last-seen heartbeat to connected peers
                    let timestamp = chrono::Utc::now().timestamp();
                    let connected: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                    for peer_id in connected {
                        let payload = crate::network::SignalingPayload::Heartbeat { timestamp };
                        let _ = self.forward_to_mesh(peer_id, payload, false).await;
                    }
                }
                _ = fast_poll_interval.tick() => {
                    let has_active_incoming = self.incoming_transfers.values().any(|t| t.is_relayed);
                    let has_active_seeding = !self.active_seeders.is_empty();
                    let has_relay_peers = self.is_relayed_map.read().values().any(|&r| r);
                    if has_active_incoming || has_active_seeding || has_relay_peers {
                        // FLUSH NON-CHUNK PENDING MESSAGES ONLY.
                        // File chunks/requests are handled by the pull model (pull_retry_interval below).
                        // Flushing file chunks here would cause relay flooding.
                        let all_pending_targets: Vec<PeerId> = self.pending_messages.keys().cloned().collect();
                        for recipient in all_pending_targets {
                            if let Some(payloads) = self.pending_messages.get_mut(&recipient) {
                                // Stable extract: split into chunks (keep) and non-chunks (send now)
                                let mut non_chunks = Vec::new();
                                payloads.retain(|p| {
                                    if matches!(p, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
                                        true // keep in pending
                                    } else {
                                        non_chunks.push(p.clone());
                                        false // remove from pending
                                    }
                                });
                                if payloads.is_empty() { self.pending_messages.remove(&recipient); }
                                for payload in non_chunks {
                                    let _ = self.forward_to_mesh(recipient, payload, false).await;
                                }
                            }
                        }
                        // FAST MAILBOX FETCH: Poll mailbox every 5s when relay peers are connected.
                        // This ensures ACKs and receipts arrive promptly — not just during file transfers.
                        // Throttled to 5s to avoid flooding the RBN anchor with MailboxDrain requests.
                        if last_fast_mailbox_fetch.elapsed() > Duration::from_secs(5) {
                            last_fast_mailbox_fetch = Instant::now();
                            self.perform_mailbox_fetch().await;
                        }
                    }
                }
                _ = pull_retry_interval.tick() => {
                    // Check if we are online (have at least one active connection to the swarm/mesh)
                    let is_online = self.swarm.connected_peers().count() > 0;
                    
                    if is_online {
                        // Check for stalled relay transfers: if no new chunk in 8s, re-request missing chunks
                        let mut stalled = Vec::new();
                        for (tid, t) in self.incoming_transfers.iter_mut() {
                            let is_connected = self.swarm.is_connected(&t.peer_id);
                            let is_relayed_conn = self.is_relayed_map.read().get(&t.peer_id).cloned().unwrap_or(false);
                            let has_webrtc = {
                                let dc_store_read = self.data_channels.read();
                                if let Some(dc) = dc_store_read.get(&t.peer_id) {
                                    dc.ready_state() == RTCDataChannelState::Open
                                } else {
                                    false
                                }
                            };
                            let is_direct_p2p = (is_connected && !is_relayed_conn) || has_webrtc;
                            
                            // SOVEREIGN SWARM: Periodically query Kademlia DHT for new seeders/providers
                            // during active group downloads to dynamically discover completed peers (every 5 seconds).
                            if t.group_id.is_some() && t.last_update.elapsed() > Duration::from_secs(5) {
                                let tx = self.command_tx.clone();
                                let hash = t.file_hash.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::FindProviders { file_hash: hash }).await;
                                });
                            }

                            // SMART STALL DETECTION: Only retry if the received chunk count has NOT
                            // grown since the last watchdog tick. This prevents re-requesting chunks
                            // that are actively arriving from a slow seeder serving multiple peers.
                            let current_chunk_count = t.received_chunks.len();
                            let watchdog_timeout = if is_direct_p2p { 10 } else { 8 };
                            let truly_stalled = current_chunk_count == t.stall_chunk_count
                                && t.last_update.elapsed() > Duration::from_secs(watchdog_timeout);
                            // Always update the stall_chunk_count so next tick reflects new arrivals.
                            t.stall_chunk_count = current_chunk_count;

                            // Direct P2P switches to pull recovery if completely stalled for watchdog_timeout (10s)
                            // to handle reconnects or lost chunks without permanently downgrading healthy high-speed transfers.
                            let should_retry = truly_stalled;

                            if should_retry {
                                // Find the first missing chunk index
                                let mut next = 0u32;
                                while t.received_chunks.contains_key(&next) { next += 1; }
                                let window = if is_direct_p2p { 8 } else { 4 };
                                let limit = if t.total_chunks > 0 {
                                    std::cmp::min(next + window, t.total_chunks)
                                } else {
                                    next + window
                                };
                                if next < limit {
                                    // Align next_pull_idx so it starts pulling sequentially from the new limit
                                    t.next_pull_idx = limit;
                                    t.last_update = Instant::now();
                                    t.is_relayed = true; // Auto-transition to pull model on stall
                                    stalled.push((tid.clone(), t.peer_id, t.providers.clone(), next, limit, t.chunk_size));
                                }
                            }
                        }
                        
                        for (tid, peer, providers, first_missing_idx, limit, csize) in stalled {
                            println!("[Mesh] Transfer {} stalled. Retrying PULL for chunks {}..{} from {} providers", 
                                     tid, first_missing_idx, limit - 1, providers.len());
                            
                            // REDUNDANCY FILTER: Remove old requests for this transfer from RAM buffer
                            if let Some(pending) = self.pending_messages.get_mut(&peer) {
                                pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: ref id, .. } if id == &tid));
                            }
                            
                            let tx = self.command_tx.clone();
                            let tid_clone = tid.clone();

                            // Intelligent Provider Selection: Prioritize connected direct peers over relayed/offline ones
                            let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &providers);

                            tokio::spawn(async move {
                                for idx in first_missing_idx..limit {
                                    let target_peer = if !selected_providers.is_empty() {
                                        selected_providers[(idx as usize) % selected_providers.len()]
                                    } else {
                                        peer
                                    };
                                    let req = SignalingPayload::FileChunkRequest { 
                                        transfer_id: tid_clone.clone(), 
                                        chunk_index: idx,
                                        chunk_size: Some(csize)
                                    };
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                                        peer_id: target_peer, 
                                        payload: req 
                                    }).await;
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                }
                            });
                        }
                    }
                }
                _ = status_check_interval.tick() => {
                    let connected_count = self.swarm.connected_peers().count();
                    let current_status = if connected_count == 0 {
                        // Check if we have anchors but no peers
                        if self.discovered_anchors.is_empty() { 0u8 } else { 3u8 } // 0=Offline, 3=Syncing
                    } else if self.swarm.listeners().any(|l| l.to_string().contains("p2p-circuit")) {
                        2u8 // Relay Ready
                    } else {
                        1u8 // Connected
                    };

                    if current_status != last_status {
                        last_status = current_status;
                        crate::dispatch_global_event(10, &[current_status]);
                    }

                    // --- RELIABILITY FIX: Proactive Reservation Check ---
                    // If we are NOT an anchor and have no active relay listeners,
                    // we might have lost our reachability. Re-listen on all RBNs.
                    let has_relay_listener = self.swarm.listeners().any(|l| l.to_string().contains("p2p-circuit"));
                    let we_are_anchor = self.swarm.behaviour().relay_server.as_ref().is_some() || self.storage.is_anchor_mode_enabled();
                    if !has_relay_listener && !we_are_anchor {
                        println!("[Mesh] Relay reachability lost. Re-requesting reservations on bootstrap nodes...");
                        for (rbn_id, _) in self.bootstrap_nodes.clone() {
                            if let Ok(addr) = format!("/p2p/{}/p2p-circuit", rbn_id).parse() {
                                let _ = self.swarm.listen_on(addr);
                            }
                        }
                    }

                    // Periodically broadcast connection status of all currently connected peers.
                    let connected_peers: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                    for peer_id in connected_peers {
                        let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                        let status: u8 = if is_relayed { 1 } else { 0 }; // 0 = Direct P2P, 1 = Relay Active
                        let mut data = peer_id.to_string().into_bytes();
                        data.push(b':');
                        data.push(status);
                        crate::dispatch_global_event(8, &data);
                    }
                }
                _ = mailbox_fetch_interval.tick() => {
                    self.perform_mailbox_fetch().await;
                    for (_, addr) in self.bootstrap_nodes.clone() {
                        let _ = self.swarm.dial(addr);
                    }

                    // Flush pending messages periodically (every 30 seconds)
                    let all_pending: Vec<(PeerId, Vec<SignalingPayload>)> = self.pending_messages.drain().collect();
                    for (recipient, payloads) in all_pending {
                        for payload in payloads {
                            let _ = self.forward_to_mesh(recipient, payload, false).await;
                        }
                    }
                }
                _ = lease_interval.tick() => {
                    let solana_client = Arc::clone(&self.solana_client);
                    let local_pubkey = solana_client.get_treasury_pubkey();
                    if let Ok(balance) = solana_client.fetch_balance(&local_pubkey).await {
                        if !self.reward_tracker.is_lease_valid(balance) {
                            println!("[Economy] Identity Lease EXPIRED. Pruning node from mesh.");
                            let local_peer_id = *self.swarm.local_peer_id();
                            self.swarm.behaviour_mut().kademlia.remove_peer(&local_peer_id);
                            let anchor_key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
                            self.swarm.behaviour_mut().kademlia.stop_providing(&anchor_key);
                            let _ = self.swarm.disconnect_peer_id(local_peer_id); 
                            crate::dispatch_global_event(10, &[0]);
                        }
                    }
                }
                _ = republication_interval.tick() => {
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                    
                    // Periodically republish local public key for E2EE discovery
                    let local_peer_id = *self.swarm.local_peer_id();
                    let pubkey_record = kad::Record {
                        key: RecordKey::new(&local_peer_id.to_bytes()),
                        value: self.local_static_public.to_bytes().to_vec(),
                        publisher: Some(local_peer_id),
                        expires: None,
                    };
                    let _ = self.swarm.behaviour_mut().kademlia.put_record(pubkey_record, kad::Quorum::One);

                    // Periodically republish HANDLE if set
                    if let Ok(Some((_, Some(handle), _, _))) = self.storage.get_profile() {
                        if handle.starts_with("i@") {
                            let h_key = RecordKey::new(&handle.as_bytes());
                            let mut h_value = local_peer_id.to_string().into_bytes();
                            if let Ok(Some((_, timestamp, sigs_json, verified))) = self.storage.get_handle_claim(&handle) {
                                if verified {
                                    if let Ok(sigs) = serde_json::from_str::<Vec<String>>(&sigs_json) {
                                        let claim = registry::HandleClaim {
                                            handle: handle.clone(),
                                            peer_id: local_peer_id.to_string(),
                                            timestamp,
                                            pow_nonce: 0,
                                            signatures: sigs,
                                        };
                                        if let Ok(json) = serde_json::to_string(&claim) {
                                            h_value = json.into_bytes();
                                        }
                                    }
                                }
                            }
                            let h_record = kad::Record {
                                key: h_key.clone(),
                                value: h_value,
                                publisher: Some(local_peer_id),
                                expires: None,
                            };
                            let _ = self.swarm.behaviour_mut().kademlia.put_record(h_record, kad::Quorum::One);
                            let _ = self.swarm.behaviour_mut().kademlia.start_providing(h_key);

                            // Also publish reverse mapping peer_id -> handle for device restoration
                            let ph_key = RecordKey::new(&format!("ph_{}", local_peer_id).as_bytes());
                            let ph_record = kad::Record {
                                key: ph_key,
                                value: handle.into_bytes(),
                                publisher: Some(local_peer_id),
                                expires: None,
                            };
                            let _ = self.swarm.behaviour_mut().kademlia.put_record(ph_record, kad::Quorum::One);
                        }
                    }
                }
                _ = anchor_discovery_interval.tick() => {
                    let key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
                    let _ = self.swarm.behaviour_mut().kademlia.get_providers(key);
                }
                _ = liveness_interval.tick() => {
                    self.swarm.behaviour_mut().prune_stale_peers();
                }
                _ = contact_refresh_interval.tick() => {
                    if let Ok(contacts) = self.storage.get_all_contacts() {
                        for contact in contacts {
                            if let Ok(pid) = contact.peer_id.parse::<PeerId>() {
                                if !self.swarm.is_connected(&pid) {
                                    let _ = self.swarm.dial(pid);
                                }
                            }
                        }
                    }
                }
                event = self.swarm.select_next_some() => {
                    if let Err(e) = self.handle_swarm_event(event).await {
                        eprintln!("Error handling swarm event: {:?}", e);
                    }
                }
                command = self.command_rx.recv() => {
                    if let Some(cmd) = command {
                        if let Err(e) = self.handle_command(cmd).await {
                            eprintln!("Command error: {:?}", e);
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_file_chunk(&mut self, peer: PeerId, transfer_id: String, chunk_index: u32, total_chunks: u32, data_base64: String) {
        println!("[DEBUG] handle_file_chunk: transfer_id={}, chunk_index={}/{}", transfer_id, chunk_index, total_chunks);
        if let Some(transfer) = self.incoming_transfers.get_mut(&transfer_id) {
            transfer.total_chunks = total_chunks;
            transfer.last_update = Instant::now();
            println!("[DEBUG] Found transfer in incoming_transfers. Decoded chunks so far: {}", transfer.received_chunks.len());
            if let Ok(chunk_data) = general_purpose::STANDARD.decode(&data_base64) {
                println!("[DEBUG] Successfully decoded chunk {} (size: {} bytes)", chunk_index, chunk_data.len());
                // RELIABILITY: Only trigger N+4 if this is a NEW chunk.
                // This prevents duplicate requests from retries or re-deliveries.
                let is_new_chunk = transfer.received_chunks.insert(chunk_index, chunk_data).is_none();
                println!("[DEBUG] Chunk {} is new: {}. Total unique chunks: {}", chunk_index, is_new_chunk, transfer.received_chunks.len());
                
                let progress_val = transfer.received_chunks.len() as f32 / total_chunks as f32;
                let is_complete = transfer.received_chunks.len() as u32 == total_chunks;
                println!("[DEBUG] progress_val={}, is_complete={}", progress_val, is_complete);
                
                let received_bytes: usize = transfer.received_chunks.values().map(|v| v.len()).sum();
                let elapsed = transfer.start_time.elapsed().as_secs_f64();
                let speed_bps = if elapsed > 0.0 { (received_bytes as f64 * 8.0) / elapsed } else { 0.0 };

                let mut local_path = None;
                let mut is_verified = false;

                if is_complete {
                    let mut full_data = Vec::new();
                    for i in 0..total_chunks { if let Some(chunk) = transfer.received_chunks.get(&i) { full_data.extend_from_slice(chunk); } }
                    
                    // Verify SHA-256 integrity
                    use sha2::{Sha256, Digest};
                    let mut hasher = Sha256::new();
                    hasher.update(&full_data);
                    let actual_hash = format!("{:x}", hasher.finalize());

                    if actual_hash == transfer.file_hash {
                        println!("✅ File integrity VERIFIED for transfer {}", transfer_id);
                        is_verified = true;
                        
                        let subfolder = if let Some(ref gid) = transfer.group_id {
                            println!("[Mesh] Identifying group for organization: {}", gid);
                            if let Ok(Some(group)) = self.storage.get_group(gid) {
                                let g_name = group.name.replace(" ", "_");
                                println!("[Mesh] Organized into group folder: {}_Media", g_name);
                                format!("{}_Media", g_name)
                            } else {
                                println!("[Mesh] ⚠️ Group not found in storage for folder organization: {}", gid);
                                "Group_Media".to_string()
                            }
                        } else {
                            let peer_str = peer.to_string();
                            println!("[Mesh] Identifying contact for organization: {}", peer_str);
                            if let Ok(Some(contact)) = self.storage.get_contact(&peer_str) {
                                let alias = contact.local_alias.as_deref().or(contact.global_name.as_deref()).unwrap_or("Direct");
                                let s_name = alias.replace(" ", "_");
                                println!("[Mesh] Organized into contact folder: {}_Media", s_name);
                                format!("{}_Media", s_name)
                            } else {
                                println!("[Mesh] ⚠️ Contact not found in storage for folder organization: {}", peer_str);
                                "Direct_Media".to_string()
                            }
                        };

                        let safe_subfolder = Self::sanitize_filename(&subfolder);
                        let dir_path = format!("{}/{}", self.downloads_dir, safe_subfolder);
                        println!("[Mesh] Creating Drive directory: {}", dir_path);
                        if let Err(e) = std::fs::create_dir_all(&dir_path) {
                            eprintln!("[Mesh] ❌ Failed to create Drive subfolder {}: {:?}", dir_path, e);
                        }

                        let safe_filename = Self::sanitize_filename(&transfer.filename);
                        let path = format!("{}/introvert_{}", dir_path, safe_filename);
                        println!("[Mesh] Automatic Drive Organization: Saving to {}", path);

                        // SOVEREIGN SWARM: Seeding logic depends on group context
                        if let Some(ref gid) = transfer.group_id {
                            println!("[Mesh] Group transfer complete. Joining swarm as seeder for group: {}", gid);
                            let key = RecordKey::new(&transfer.file_hash.as_bytes());
                            let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
                            
                            // Register as active seeder to serve chunk requests for this group
                            let _ = self.command_tx.send(NetworkCommand::RegisterSeeder {
                                peer_id: *self.swarm.local_peer_id(),
                                transfer_id: transfer_id.clone(),
                                file_path: path.clone(),
                                file_hash: transfer.file_hash.clone(),
                                chunk_size: transfer.chunk_size,
                                total_chunks,
                                group_id: Some(gid.clone()),
                            }).await;
                        } else {
                            println!("[Mesh] 1-to-1 transfer complete. Skipping mesh seeding to preserve privacy.");
                        }

                        if std::fs::write(&path, full_data).is_ok() { 
                            local_path = Some(path.clone()); 

                            // DISPATCH LOCAL EVENT: Update UI with the organized path
                            let progress = FileTransferProgress {
                                transfer_id: transfer_id.clone(),
                                peer_id: peer.to_string(),
                                filename: transfer.filename.clone(),
                                mime_type: transfer.mime_type.clone(),
                                file_hash: transfer.file_hash.clone(),
                                progress: 1.0,
                                is_complete: true,
                                is_verified: true,
                                is_outgoing: false,
                                local_path: Some(path.clone()),
                                start_time_ms: transfer.start_time.elapsed().as_millis() as u64,
                                speed_bps: 0.0,
                                group_id: transfer.group_id.clone(),
                            };
                            crate::dispatch_global_event(12, &serde_json::to_vec(&progress).unwrap_or_default());

                            // SOVEREIGN DRIVE: Persist metadata so this node can serve as a mesh seeder indefinitely
                            let storage_d = self.storage.clone();
                            let filename_d = transfer.filename.clone();
                            let hash_d = transfer.file_hash.clone();
                            let mime_d = transfer.mime_type.clone();
                            let size_d = transfer.total_size;
                            let path_d = path.clone();
                            let peer_id_str = peer.to_string();
                            let msg_id = transfer_id.clone();
                            let progress_d = progress.clone();
                            tokio::task::spawn_blocking(move || {
                                let _ = storage_d.upsert_drive_file(&filename_d, &hash_d, &mime_d, size_d as i64, &path_d);
                                if let Ok(json_str) = serde_json::to_string(&progress_d) {
                                    let c = format!("[FILE]:{}", json_str);
                                    if let Some(ref gid) = progress_d.group_id {
                                        let _ = storage_d.store_group_message(gid, &peer_id_str, &msg_id, &c, false, None);
                                    } else {
                                        let _ = storage_d.store_message_with_id(&peer_id_str, &msg_id, &c, false, None);
                                    }
                                }
                            });

                            // Send Completion ACK back to sender via command queue to ensure Mailbox routing
                            let ack = SignalingPayload::FileTransferComplete { transfer_id: transfer_id.clone() };
                            let tx = self.command_tx.clone();
                            tokio::spawn(async move { let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload: ack }).await; });
                        }
                    } else {
                        eprintln!("❌ File integrity FAILED for transfer {}. Expected {}, got {}", transfer_id, transfer.file_hash, actual_hash);
                        let error = SignalingPayload::FileTransferError { transfer_id: transfer_id.clone(), reason: "Integrity verification failed".to_string() };
                        let tx = self.command_tx.clone();
                        tokio::spawn(async move { let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload: error }).await; });
                        transfer.filename = format!("ERROR: {}", transfer.filename);
                    }
                } else if !transfer.filename.starts_with("ERROR:") {
                    let is_relayed_conn = self.is_relayed_map.read().get(&peer).cloned().unwrap_or(false);
                    if transfer.is_relayed || is_relayed_conn {
                        // SOVEREIGN SWARM: Stable windowed pull using next_pull_idx, distributed across providers.
                        // This maintains the concurrency pipeline even if duplicate chunks are received.
                        let next_idx = transfer.next_pull_idx;
                        if next_idx < total_chunks {
                            transfer.next_pull_idx += 1;
                            let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &transfer.providers);
                            let target_peer = if !selected_providers.is_empty() {
                                selected_providers[(next_idx as usize) % selected_providers.len()]
                            } else {
                                peer
                            };

                            let tx = self.command_tx.clone();
                            let tid = transfer_id.clone();
                            let csize = transfer.chunk_size;
                            tokio::spawn(async move {
                                let req = SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: next_idx, chunk_size: Some(csize) };
                                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: target_peer, payload: req }).await;
                            });
                        }
                    }
                }

                let progress = FileTransferProgress { 
                    transfer_id: transfer_id.clone(), 
                    peer_id: peer.to_string(), 
                    filename: transfer.filename.clone(), 
                    mime_type: transfer.mime_type.clone(),
                    file_hash: transfer.file_hash.clone(),
                    progress: progress_val, 
                    is_complete, 
                    is_verified,
                    is_outgoing: false, 
                    local_path: local_path.clone(),
                    start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                    speed_bps,
                    group_id: transfer.group_id.clone(),
                };

                let data = serde_json::to_vec(&progress).unwrap_or_default();
                crate::dispatch_global_event(12, &data);

                if let Some(ref gid) = transfer.group_id {
                    if let Ok(json_str) = serde_json::to_string(&progress) {
                        let content = format!("[FILE]:{}", json_str);
                        let storage = Arc::clone(&self.storage);
                        let gid_clone = gid.clone();
                        let peer_str = peer.to_string();
                        let tid_clone = transfer_id.clone();
                        if !self.is_stress_test {
                            tokio::task::spawn_blocking(move || {
                                let _ = storage.store_group_message(&gid_clone, &peer_str, &tid_clone, &content, false, None);
                            });
                        }
                    }
                } else {
                    if let Ok(json_str) = serde_json::to_string(&progress) {
                        let content = format!("[FILE]:{}", json_str);
                        let storage = Arc::clone(&self.storage);
                        let peer_str = peer.to_string();
                        let tid_clone = transfer_id.clone();
                        if !self.is_stress_test {
                            tokio::task::spawn_blocking(move || {
                                let _ = storage.store_message_with_id(&peer_str, &tid_clone, &content, false, None);
                            });
                        }
                    }
                }
                // CRITICAL FIX: Always remove from incoming transfers when complete (success or fail) to prevent memory leak
                if is_complete { self.incoming_transfers.remove(&transfer_id); }
            } else {
                println!("[DEBUG] Failed to decode base64 for chunk {}", chunk_index);
            }
        } else {
            println!("[DEBUG] transfer_id {} not found in incoming_transfers. Available: {:?}", 
                     transfer_id, self.incoming_transfers.keys().collect::<Vec<_>>());
        }
    }

    fn sanitize_filename(filename: &str) -> String {
        let path = std::path::Path::new(filename);
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_"))
            .unwrap_or_else(|| "unknown_file".to_string())
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<IntrovertBehaviourEvent>) -> anyhow::Result<()> {
        match event {
            SwarmEvent::Behaviour(b_event) => {
                match b_event {
                    IntrovertBehaviourEvent::Mdns(libp2p::mdns::Event::Discovered(list)) => {
                        let mut grouped: std::collections::HashMap<PeerId, Vec<libp2p::Multiaddr>> = std::collections::HashMap::new();
                        for (peer_id, addr) in list {
                            grouped.entry(peer_id).or_default().push(addr);
                        }
                        for (peer_id, addrs) in grouped {
                            println!("mDNS discovered peer: {} with {} addresses", peer_id, addrs.len());
                            
                            // Check if this peer is a static bootstrap node to prevent clearing its bootstrap configuration
                            let is_bootstrap = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id);
                            if !is_bootstrap {
                                println!("[Mesh] Clearing stale addresses for peer {} prior to applying new mDNS discoveries.", peer_id);
                                self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
                            }

                            for addr in addrs {
                                println!("  address: {}", addr);
                                self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                                // Dial the specific active address directly to bypass PeerId dial backoff
                                let _ = self.swarm.dial(addr);
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Autonat(autonat::Event::StatusChanged { old, new }) => {
                        println!("[AutoNAT] Reachability changed: {:?} -> {:?}", old, new);
                        
                        // Clear all WebRTC connections since our network interface changed
                        // CRITICAL: Avoid clearing WebRTC connections during initial boot transition from Unknown.
                        // Initial Nat status resolves to Private/Public after ~5s, which was clearing perfectly good local WebRTC links mid-transfer.
                        let is_initial_boot = matches!(old, autonat::NatStatus::Unknown);
                        if !is_initial_boot {
                            self.data_channels.write().clear();
                            let pcs: Vec<Arc<RTCPeerConnection>> = self.peer_connections.write().drain().map(|(_, pc)| pc).collect();
                            for pc in pcs {
                                let _ = pc.close().await;
                            }
                        }

                        // PROACTIVE MESH REBUILD: If we just moved networks, re-dial bootstrap nodes
                        for (_, addr) in self.bootstrap_nodes.clone() {
                            let _ = self.swarm.dial(addr);
                        }
                        // Also re-dial known contacts to restore direct paths if possible
                        if let Ok(contacts) = self.storage.get_all_contacts() {
                            for contact in contacts {
                                if let Ok(pid) = contact.peer_id.parse::<PeerId>() {
                                    let _ = self.swarm.dial(pid);
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                        println!("Identify received from {}: Protocols={:?}", peer_id, info.protocols);
                        self.mesh_active_peers.insert(peer_id);
                        
                        // Add addresses to both Kademlia AND the swarm's direct address book
                        // This is critical for the Relay Client to find the relay server.
                        let currently_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                        
                        // Clear old Kademlia addresses first to avoid dialing stale dynamic ports (Connection Refused errors)
                        let is_bootstrap = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id);
                        if !is_bootstrap {
                            println!("[Mesh] Clearing stale addresses for peer {} prior to applying new Identify listen addresses.", peer_id);
                            self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
                        }

                        for addr in &info.listen_addrs {
                            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                            
                            // Upgrading path: if we are connected via relay, try to dial direct listen addresses
                            if currently_relayed {
                                let is_circuit = addr.iter().any(|proto| matches!(proto, libp2p::multiaddr::Protocol::P2pCircuit));
                                if !is_circuit {
                                    println!("[Mesh] Attempting direct dial to upgrade relayed connection to {}: {}", peer_id, addr);
                                    let _ = self.swarm.dial(addr.clone());
                                }
                            }
                        }
                        
                        // Discovery: If peer supports our protocol AND HOP relay protocol, they can be an anchor/relay
                        let supports_signaling = info.protocols.iter().any(|p| p.to_string().contains("/introvert/signaling/1.0.0"));
                        let supports_hop = info.protocols.iter().any(|p| p.to_string().contains("/libp2p/circuit/relay/0.2.0/hop"));
                        if supports_signaling && supports_hop {
                            println!("✨ Peer {} supports Introvert Signaling and HOP. Discovered as Anchor.", peer_id);
                            if !self.discovered_anchors.contains(&peer_id) {
                                self.discovered_anchors.push(peer_id);
                            }
                        }

                        // Refresh view of the network
                        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

                        if info.protocols.iter().any(|p| p.to_string().contains("/libp2p/circuit/relay/0.2.0/hop")) {
                            if !self.relay_reservations.contains(&peer_id) {
                                println!("Relay node {} supports HOP. Requesting reservation...", peer_id);

                                // BUG FIX: Construct the FULL multiaddr for the relay reservation.
                                // We prioritize the first address that looks like a public IP.
                                let base_addr = info.listen_addrs.iter()
                                    .find(|a| !a.to_string().contains("127.0.0.1") && !a.to_string().contains("192.168"))
                                    .or_else(|| info.listen_addrs.first())
                                    .cloned();

                                let relay_addr = if let Some(mut addr) = base_addr {
                                    // If the address doesn't already contain the peer ID, add it.
                                    if !addr.to_string().contains(&peer_id.to_string()) {
                                        addr = addr.with(libp2p::multiaddr::Protocol::P2p(peer_id));
                                    }
                                    addr.with(libp2p::multiaddr::Protocol::P2pCircuit)
                                } else {
                                    // Fallback to relative circuit address
                                    libp2p::multiaddr::Multiaddr::empty()
                                        .with(libp2p::multiaddr::Protocol::P2p(peer_id))
                                        .with(libp2p::multiaddr::Protocol::P2pCircuit)
                                };

                                match self.swarm.listen_on(relay_addr.clone()) {
                                    Ok(id) => {
                                        println!("[Mesh] Relay listen request SUCCESS. Address: {}, Listener ID: {:?}", relay_addr, id);
                                        self.relay_reservations.insert(peer_id);
                                        self.relay_listeners.insert(id, peer_id);
                                    },
                                    Err(e) => println!("[Mesh] Relay listen request FAILED on {}: {:?}", relay_addr, e),
                                }
                            }
                        }
                        // --- RELIABILITY FIX: Flush pending messages only AFTER Identify succeeds ---
                        if supports_signaling {
                            if let Some(payloads) = self.pending_messages.remove(&peer_id) {
                                println!("[Mesh] Peer {} identified. Flushing {} pending messages.", peer_id, payloads.len());
                                for payload in payloads {
                                    let _ = self.handle_command(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                                }
                            }

                            // If this is an anchor node, drain our mailbox and flush non-file pending messages
                            let is_anchor = self.discovered_anchors.contains(&peer_id) || 
                                           self.storage.fetch_all_anchor_nodes().map(|nodes| nodes.iter().any(|n| n.peer_id == peer_id.to_string())).unwrap_or(false);
                            if is_anchor {
                                println!("[Mesh] Anchor {} identified. Draining mailbox...", peer_id);
                                self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::MailboxDrain));

                                // Flush ONLY non-file-chunk payloads for other blocked peers via mailbox.
                                // File chunks are handled by the pull model (receiver re-requests).
                                // Rate-limit sends to avoid relay flooding.
                                let all_pending_targets: Vec<(PeerId, bool)> = self.pending_messages.keys()
                                    .map(|p| (*p, self.is_relayed_map.read().get(p).cloned().unwrap_or(false)))
                                    .collect();
                                for (recipient, is_relay_recipient) in all_pending_targets {
                                    if let Some(payloads) = self.pending_messages.get_mut(&recipient) {
                                        // Stable extract: keep chunks in pending, flush non-chunks
                                        let mut non_chunk_payloads = Vec::new();
                                        payloads.retain(|p| {
                                            if matches!(p, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
                                                true
                                            } else {
                                                non_chunk_payloads.push(p.clone());
                                                false
                                            }
                                        });
                                        if payloads.is_empty() { self.pending_messages.remove(&recipient); }
                                        
                                        let tx = self.command_tx.clone();
                                        tokio::spawn(async move {
                                            for payload in non_chunk_payloads {
                                                // For relay peers: force mailbox to avoid direct-retry loop
                                                let cmd = if is_relay_recipient {
                                                    NetworkCommand::StoreInMailbox { peer_id: recipient, payload }
                                                } else {
                                                    NetworkCommand::ForwardMeshSignaling { peer_id: recipient, payload }
                                                };
                                                let _ = tx.send(cmd).await;
                                                tokio::time::sleep(Duration::from_millis(100)).await;
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::RelayClient(event) => {
                        match event {
                            libp2p::relay::client::Event::ReservationReqAccepted { relay_peer_id, renewal, .. } => {
                                println!("✅ Relay reservation ACCEPTED by {}. Renewal: {}", relay_peer_id, renewal);
                                let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                                let mut data = relay_peer_id.to_string().into_bytes();
                                data.push(b':');
                                data.push(1); // 1 = Relay Active
                                crate::dispatch_global_event(8, &data);
                                crate::dispatch_global_event(10, &[2]);
                            }
                            libp2p::relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
                                println!("🔌 Outbound relay circuit established via {}", relay_peer_id);
                            }
                            libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
                                println!("🔌 Inbound relay circuit established from {}", src_peer_id);
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { key, providers, .. })), .. }) => {
                        let key_str = String::from_utf8_lossy(key.as_ref()).into_owned();
                        let local_id = *self.swarm.local_peer_id();
                        
                        // Filter out local peer ID from providers to prevent self-dials and self-requests
                        let filtered_providers: Vec<PeerId> = providers.into_iter().filter(|p| p != &local_id).collect();
                        
                        // SOVEREIGN SWARM: Link providers to active file transfers
                        self.active_providers.insert(key_str.clone(), filtered_providers.iter().cloned().collect());
                        
                        // Update specific incoming transfers that match this hash
                        for (transfer_id, transfer) in self.incoming_transfers.iter_mut() {
                            if transfer.file_hash == key_str {
                                for pid in &filtered_providers {
                                     if !transfer.providers.contains(pid) { transfer.providers.push(*pid); }
                                }

                                // If transfer is stalled (no chunks yet) and we just found providers, kickstart it
                                if transfer.received_chunks.is_empty() && !transfer.providers.is_empty() {
                                    let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &transfer.providers);
                                    let target_peer = if !selected_providers.is_empty() {
                                        selected_providers[0]
                                    } else {
                                        transfer.providers[0]
                                    };
                                    let tid = transfer_id.clone();
                                    let tx = self.command_tx.clone();
                                    let csize = transfer.chunk_size;
                                    tokio::spawn(async move {
                                        let req = SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: 0, chunk_size: Some(csize) };
                                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: target_peer, payload: req }).await;
                                    });
                                }
                            }
                        }
                        
                        let is_anchor_key = key.as_ref() == ANCHOR_PROVIDER_KEY;
                        let mut dial_count = 0;
                        for peer_id in filtered_providers {
                            if is_anchor_key {
                                if !self.discovered_anchors.contains(&peer_id) { self.discovered_anchors.push(peer_id); }
                            }
                            if !self.swarm.is_connected(&peer_id) {
                                if dial_count < 3 {
                                    println!("[Mesh] Provider {} found via DHT. Constructing relay path dial...", peer_id);
                                    self.dial_relay_path(peer_id);
                                    dial_count += 1;
                                } else {
                                    println!("[Mesh] Provider {} found via DHT, but dial limit (3) reached. Skipping dial.", peer_id);
                                }
                            }
                            
                            // SECURITY HARDENING: Group discovery link (Only if not a file hash)
                            if key_str.len() < 32 { // Simple heuristic: hashes are long hex strings
                                let tx = self.command_tx.clone();
                                if let Some(gid) = self.resolved_group_codes.get(&key_str).cloned() {
                                    let local_profile = self.storage.get_profile().ok().flatten();
                                    let alias = local_profile.as_ref().and_then(|(n, _, _, _)| n.clone());
                                    let handle = local_profile.as_ref().and_then(|(_, h, _, _)| h.clone());
                                    let avatar = local_profile.and_then(|(_, _, a, _)| a);
                                    tokio::spawn(async move {
                                        let req = SignalingPayload::GroupManifestRequest { group_id: gid, alias, avatar, handle };
                                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload: req }).await;
                                    });
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::RoutingUpdated { peer, .. }) => {
                        let data = peer.to_bytes();
                        crate::dispatch_global_event(1, &data); 
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetClosestPeers(Ok(kad::GetClosestPeersOk { key, peers })), .. }) => {
                        if let Ok(target_peer) = PeerId::from_bytes(&key) {
                            let peer_ids: Vec<PeerId> = peers.into_iter().map(|p| p.peer_id).collect();
                            if peer_ids.contains(&target_peer) { let _ = self.swarm.dial(target_peer); }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { id, result: kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(record))), .. }) => {
                        let key_str = String::from_utf8_lossy(record.record.key.as_ref()).into_owned();
                        let value_str = String::from_utf8_lossy(&record.record.value).into_owned();
                        println!("[Mesh] Kademlia resolved record key: {}, value: {}", key_str, value_str);
                        
                        // Handle resolution logic
                        if key_str.starts_with("i@") {
                            let (resolved_peer_id, _) = if let Ok(claim) = serde_json::from_str::<registry::HandleClaim>(&value_str) {
                                let mut valid_witnesses = 0;
                                for rbn_peer_id in &claim.signatures {
                                    if let Ok(pid) = rbn_peer_id.parse::<PeerId>() {
                                        if self.bootstrap_nodes.iter().any(|(id, _)| id == &pid) {
                                            valid_witnesses += 1;
                                        }
                                    }
                                }
                                let verified = valid_witnesses >= 1;
                                if verified {
                                    let _ = self.registry.verify_claim(&claim);
                                }
                                (claim.peer_id, verified)
                            } else {
                                (value_str.clone(), false)
                            };

                            let mut data = key_str.clone().into_bytes();
                            data.push(0);
                            data.extend(resolved_peer_id.as_bytes());
                            crate::dispatch_global_event(33, &data); // Event Type 33: Handle Resolved [handle, 0, peer_id]
                        }

                        if key_str.starts_with("ph_") {
                            let target_peer_id = key_str.trim_start_matches("ph_").to_string();
                            let handle_resolved = value_str.clone();
                            let my_peer_id = self.swarm.local_peer_id().to_string();
                            if target_peer_id == my_peer_id {
                                let name = self.storage.get_profile().ok().flatten().and_then(|(n, _, _, _)| n);
                                let avatar = self.storage.get_profile().ok().flatten().and_then(|(_, _, a, _)| a);
                                let privacy = self.storage.get_profile().ok().flatten().map(|(_, _, _, p)| p).unwrap_or(1);
                                let _ = self.storage.set_profile(name.as_deref(), Some(&handle_resolved), avatar.as_deref(), privacy);
                                let _ = self.storage.upsert_handle_claim(&handle_resolved, &my_peer_id, chrono::Utc::now().timestamp(), "[]", true);
                                println!("[Mesh] Restored handle {} for local profile!", handle_resolved);
                            }
                            
                            // Send Event 37: Peer Handle Restored/Resolved
                            let mut data = target_peer_id.into_bytes();
                            data.push(0);
                            data.extend(handle_resolved.as_bytes());
                            crate::dispatch_global_event(37, &data);
                        }

                        // Store the resolved mapping
                        self.resolved_group_codes.insert(key_str.clone(), value_str.clone());

                        // If we have providers already discovered for this key, query them immediately
                        if let Some(providers) = self.active_providers.get(&key_str).cloned() {
                            let local_profile = self.storage.get_profile().ok().flatten();
                            let alias = local_profile.as_ref().and_then(|(n, _, _, _)| n.clone());
                            let handle = local_profile.as_ref().and_then(|(_, h, _, _)| h.clone());
                            let avatar = local_profile.and_then(|(_, _, a, _)| a);
                            for peer_id in providers {
                                let tx = self.command_tx.clone();
                                let gid = value_str.clone();
                                let alias_clone = alias.clone();
                                let handle_clone = handle.clone();
                                let avatar_clone = avatar.clone();
                                tokio::spawn(async move {
                                    let req = SignalingPayload::GroupManifestRequest { group_id: gid, alias: alias_clone, avatar: avatar_clone, handle: handle_clone };
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload: req }).await;
                                });
                            }
                        }

                        if let Some(peer_id) = self.pending_handshakes.remove(&id) {
                            if let Ok(remote_static_pub) = <[u8; 32]>::try_from(record.record.value.as_slice()) {
                                if let Ok(mut session) = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), &remote_static_pub) {
                                    if let Ok(handshake_msg) = session.send_message(&[]) {
                                        let storage = Arc::clone(&self.storage);
                                        let enc_key = self.session_encryption_key;
                                        let session_state = session.get_state();
                                        tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage, enc_key, peer_id, session_state).await; });
                                        self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                                        self.noise_sessions.insert(peer_id, session);
                                    }
                                }
                            }
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer, message: request_response::Message::Request { request, channel, .. }, .. }) => {
                        let _ = self.swarm.behaviour_mut().request_response.send_response(channel, SignalingResponse("ACK".to_string()));
                        let tx = self.command_tx.clone();
                        let payload = request.0;
                        tokio::spawn(async move {
                            let _ = tx.send(NetworkCommand::HandleIncomingPayload { peer_id: peer, payload }).await;
                        });
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer, message: request_response::Message::Response { request_id, .. }, .. }) => {
                        // SUCCESS: Remove from tracker and decrement in-flight counter
                        self.outbound_tracker.remove(&request_id);
                        if let Some(count) = self.inflight_requests.get_mut(&peer) {
                            *count = count.saturating_sub(1);
                            if *count == 0 { self.inflight_requests.remove(&peer); }
                        }
                        // Sliding window drain: flush one back-pressured chunk now that a slot opened
                        if let Some(pending) = self.pending_messages.get_mut(&peer) {
                            if let Some(next_chunk) = pending.iter().position(|p| matches!(p, SignalingPayload::FileChunk { .. })) {
                                let payload = pending.remove(next_chunk);
                                if pending.is_empty() { self.pending_messages.remove(&peer); }
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload }).await;
                                });
                            }
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::OutboundFailure { request_id, peer, error, .. }) => {
                        println!("[Mesh] Outbound Request-Response FAILURE to {}: {:?}", peer, error);
                        
                        // Decrement in-flight counter for this peer
                        if let Some(count) = self.inflight_requests.get_mut(&peer) {
                            *count = count.saturating_sub(1);
                            if *count == 0 { self.inflight_requests.remove(&peer); }
                        }

                        // Sliding window drain: flush one back-pressured chunk now that a slot opened
                        if let Some(pending) = self.pending_messages.get_mut(&peer) {
                            if let Some(next_chunk) = pending.iter().position(|p| matches!(p, SignalingPayload::FileChunk { .. })) {
                                let payload = pending.remove(next_chunk);
                                if pending.is_empty() { self.pending_messages.remove(&peer); }
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: peer, payload }).await;
                                });
                            }
                        }

                        let is_network_failure = matches!(error, libp2p::request_response::OutboundFailure::ConnectionClosed | libp2p::request_response::OutboundFailure::Timeout);
                        let is_unexpected_eof = format!("{:?}", error).contains("UnexpectedEof") || format!("{:?}", error).contains("EOF while parsing");

                        // If the direct push failed, handle based on payload type:
                        // - FileChunk: DO NOT re-queue — the pull model means the receiver will re-request missing chunks.
                        //   Re-queuing causes a thundering herd on reconnect that floods relay circuits.
                        // - Other payloads: re-queue for mailbox routing.
                        if let Some((target_peer, payload)) = self.outbound_tracker.remove(&request_id) {
                            let is_file_chunk = matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. });
                            let is_sent_to_anchor = target_peer != peer;
                            if is_file_chunk {
                                // Pull model: receiver will retry via FileChunkRequest — don't re-queue
                                println!("[Mesh] FileChunk/Request send failed for {}. Receiver will re-request via pull model.", peer);
                            } else if is_unexpected_eof && is_sent_to_anchor {
                                println!("[Mesh] Outbound failure to anchor {} was UnexpectedEof. Bypassing re-queue as anchor likely processed it.", peer);
                            } else {
                                // For relay peers: force-store in mailbox (bypasses direct delivery entirely).
                                // Using StoreInMailbox avoids the ForwardMeshSignaling → direct retry → EOF loop.
                                let is_relay_target = self.is_relayed_map.read().get(&target_peer).cloned().unwrap_or(false);
                                if is_relay_target {
                                    println!("[Mesh] Direct relay send failed for {}. Force-storing in mailbox.", peer);
                                    let tx = self.command_tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx.send(NetworkCommand::StoreInMailbox { peer_id: target_peer, payload }).await;
                                    });
                                } else {
                                    println!("[Mesh] Re-queuing failed payload for Mailbox routing...");
                                    self.pending_messages.entry(target_peer).or_default().push(payload);
                                }
                            }
                        }

                        if is_network_failure {
                            println!("[Mesh] Network failure (Ghost Connection) detected for {}. Forcing disconnect to trigger clean reconnect.", peer);
                            let _ = self.swarm.disconnect_peer_id(peer);
                        } else if !self.swarm.is_connected(&peer) {
                            self.is_relayed_map.write().remove(&peer);
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::ResponseSent { .. }) => {}
                    IntrovertBehaviourEvent::Ping(ping_event) => {
                        // Check for pending diagnostic RTT measurement
                        if let Ok(rtt) = ping_event.result {
                            if let Some(diag) = self.pending_diagnostics.remove(&ping_event.peer) {
                                let transport = diag.transport.unwrap_or_else(|| "Unknown".to_string());
                                let payload = format!(
                                    r#"{{"peer_id":"{}","step":"settled","status":"settled","transport":"{}","rtt_ms":{}}}"#,
                                    ping_event.peer, transport, rtt.as_millis()
                                );
                                crate::dispatch_global_event(15, payload.as_bytes());
                            }
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::InboundRequest { .. }) => {}
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetRecord(Err(e)), .. }) => {
                        let key = match &e {
                            kad::GetRecordError::NotFound { key, .. } => key,
                            kad::GetRecordError::QuorumFailed { key, .. } => key,
                            kad::GetRecordError::Timeout { key } => key,
                        };
                        let key_str = String::from_utf8_lossy(key.as_ref()).into_owned();
                        if key_str.starts_with("i@") {
                            println!("[Mesh] Failed to resolve handle {}: {:?}", key_str, e);
                            let data = key_str.into_bytes();
                            crate::dispatch_global_event(35, &data); // Event 35: Handle Resolve Failed
                        }
                    }
                    IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { .. }) => {}
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::InboundFailure { .. }) => {}
                    IntrovertBehaviourEvent::Identify(identify::Event::Sent { .. }) => {}
                    IntrovertBehaviourEvent::Identify(identify::Event::Pushed { .. }) => {}
                    IntrovertBehaviourEvent::Gossipsub(libp2p::gossipsub::Event::Message { propagation_source, message_id, message }) => {
                        println!("[Mesh] Received gossipsub message from {} with id {}", propagation_source, message_id);
                        self.mesh_active_peers.insert(propagation_source);
                        if let Ok(payload) = serde_json::from_slice::<SignalingPayload>(&message.data) {
                            // Determine the peer id from the message source or payload if applicable.
                            // The actual signer is verified inside handle_single_payload via GroupManager::verify_action.
                            // We can pass propagation_source as the "peer" for now.
                            self.handle_single_payload(propagation_source, payload, false).await;
                        }
                    }
                    IntrovertBehaviourEvent::Gossipsub(libp2p::gossipsub::Event::Subscribed { peer_id, topic }) => {
                        println!("[Mesh] Peer {} subscribed to topic {}", peer_id, topic);
                        self.mesh_active_peers.insert(peer_id);
                    }
                    IntrovertBehaviourEvent::Gossipsub(_) => {}
                    IntrovertBehaviourEvent::Dcutr(_) => {}
                    IntrovertBehaviourEvent::Identify(_) => {}
                    IntrovertBehaviourEvent::Autonat(_) => {}
                    _ => {
                        // Only log truly unexpected behaviour events
                        println!("[Swarm Debug] Unhandled behaviour event: {:?}", b_event);
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[Swarm] New listen address: {}", address);
            }
            SwarmEvent::ExternalAddrConfirmed { address } => {
                println!("[Swarm] External address CONFIRMED: {}", address);
                // Proactively bootstrap and re-dial RBNs on address confirmation to update DHT/Relay
                let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                for (_, addr) in self.bootstrap_nodes.clone() {
                    let _ = self.swarm.dial(addr);
                }
            }
            SwarmEvent::ExternalAddrExpired { address } => {
                println!("[Swarm] External address EXPIRED: {}", address);
                // If our only external address expired, we might be transitioning networks
                if self.swarm.external_addresses().count() == 0 {
                    println!("[Swarm] All external addresses expired. Forcing mesh re-entry...");
                    for (peer_id, addr) in self.bootstrap_nodes.clone() {
                        self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    }
                    let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                }
            }

            SwarmEvent::ListenerError { listener_id, error, .. } => {
                println!("[Swarm] Listener error ({:?}): {:?}", listener_id, error);
            }
            SwarmEvent::ListenerClosed { listener_id, reason, .. } => {
                println!("[Swarm] Listener closed ({:?}): {:?}", listener_id, reason);
                if let Some(peer_id) = self.relay_listeners.remove(&listener_id) {
                    println!("[Mesh] Relay listener for {} closed. Clearing reservation record.", peer_id);
                    self.relay_reservations.remove(&peer_id);
                }
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                println!("[Swarm] Connection established with {}", peer_id);

                // If this is a direct (non-relayed) connection, save the address as a potential relay mapping
                if !endpoint.is_relayed() {
                    self.anchor_mappings.insert(peer_id, endpoint.get_remote_address().clone());
                }

                // Immediately transition out of 'Syncing' status
                // Status 1 = Mesh Active (at least one peer connected)
                crate::dispatch_global_event(10, &[1]);

                let endpoint_addr = endpoint.get_remote_address();
                let is_local_ip = endpoint_addr.to_string().contains("192.168.") || 
                                 endpoint_addr.to_string().contains("10.") || 
                                 endpoint_addr.to_string().contains("172.") ||
                                 endpoint_addr.to_string().contains("127.0.0.1");

                let is_relayed = endpoint.is_relayed() && !is_local_ip;
                if !is_relayed {
                    let count = self.direct_conn_count.entry(peer_id).or_insert(0);
                    *count += 1;
                }
                
                let is_now_relayed = self.direct_conn_count.get(&peer_id).cloned().unwrap_or(0) == 0;
                self.is_relayed_map.write().insert(peer_id, is_now_relayed);
                
                if is_local_ip && endpoint.is_relayed() {
                    println!("[Mesh] Peer {} connected via LOCAL RELAY. Treating as DIRECT for performance.", peer_id);
                }

                // --- RELIABILITY FIX: Relay Reservation ---
                // If we connect to a bootstrap node (RBN) or any anchor node, and we are NOT an anchor ourselves,
                // we must request a reservation to be reachable via that relay.
                let is_rbn = self.bootstrap_nodes.iter().any(|(id, _)| id == &peer_id);
                let is_anchor = self.discovered_anchors.contains(&peer_id);
                let we_are_anchor = self.storage.is_anchor_mode_enabled();

                if (is_rbn || is_anchor) && !we_are_anchor && !self.relay_reservations.contains(&peer_id) {
                    println!("[Mesh] Requesting RELAY RESERVATION from anchor: {}", peer_id);
                    if let Ok(addr) = format!("/p2p/{}/p2p-circuit", peer_id).parse() {
                        match self.swarm.listen_on(addr) {
                            Ok(id) => {
                                self.relay_reservations.insert(peer_id);
                                self.relay_listeners.insert(id, peer_id);
                            }
                            Err(e) => println!("[Mesh] Relay reservation failed: {:?}", e),
                        }
                    }
                }
                let status: u8 = if is_now_relayed { 1 } else { 0 };
                let mut data = peer_id.to_string().into_bytes();
                data.push(b':');
                data.push(status);
                crate::dispatch_global_event(8, &data);
                
                if is_now_relayed { self.reward_tracker.record_relay(&peer_id.to_string(), 1024); }
                
                let data = peer_id.to_bytes();
                crate::dispatch_global_event(1, &data); 

                // Diagnostic recheck: record transport type (settled event fires on first Ping RTT)
                if let Some(diag) = self.pending_diagnostics.get_mut(&peer_id) {
                    let addr_str = endpoint.get_remote_address().to_string();
                    let transport_type = if !is_relayed {
                        if addr_str.contains("127.0.0.1") || addr_str.contains("localhost") {
                            "WebSocket Tunnel (Port 80)"
                        } else {
                            "Direct P2P"
                        }
                    } else if addr_str.contains("quic") || addr_str.contains("udp") {
                        "Relayed UDP/QUIC (Port 443)"
                    } else {
                        "Relayed TCP (Port 443)"
                    };
                    diag.transport = Some(transport_type.to_string());
                    let diag_payload = format!(
                        r#"{{"peer_id":"{}","step":"connected","status":"connected","transport":"{}"}}"#,
                        peer_id, transport_type
                    );
                    crate::dispatch_global_event(15, diag_payload.as_bytes());
                }

                // Flush pending messages on connection — but RATE-LIMITED to prevent thundering herd
                // on relay circuits. File chunks are paced: max 4 in-flight at 50ms intervals.
                if let Some(payloads) = self.pending_messages.remove(&peer_id) {
                    let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                    let tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        for payload in payloads {
                            let is_chunk = matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. });
                            if is_chunk && is_relayed {
                                // Pace relay chunk sends at 50ms each to avoid flooding the circuit
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                            let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id, payload }).await;
                        }
                    });
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, endpoint, .. } => {
               // Clean up WebRTC resources immediately on connection loss to prevent stale ghost channels
               self.data_channels.write().remove(&peer_id);
               self.anchor_mappings.remove(&peer_id);
               self.relay_reservations.remove(&peer_id);
               self.inflight_requests.remove(&peer_id);

               let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
               if let Some(pc) = pc {
                   let _ = pc.close().await;
               }

               let endpoint_addr = endpoint.get_remote_address();
               let is_local_ip = endpoint_addr.to_string().contains("192.168.") || 
                                endpoint_addr.to_string().contains("10.") || 
                                endpoint_addr.to_string().contains("172.") ||
                                endpoint_addr.to_string().contains("127.0.0.1");

               let is_relayed = endpoint.is_relayed() && !is_local_ip;
               if !is_relayed {
                   if let Some(count) = self.direct_conn_count.get_mut(&peer_id) {
                       if *count > 0 {
                           *count -= 1;
                       }
                       if *count == 0 {
                           self.direct_conn_count.remove(&peer_id);
                       }
                   }
               }

               if !self.swarm.is_connected(&peer_id) {
                   self.noise_sessions.remove(&peer_id); // MEMORY FIX: Remove stale noise session
                   self.is_relayed_map.write().remove(&peer_id);
                   self.direct_conn_count.remove(&peer_id);
                   println!("[Swarm] Connection lost with {}. Peer is now truly offline.", peer_id);

                    let mut data = peer_id.to_string().into_bytes();
                    data.push(b':');
                    data.push(2); // 2 = Offline
                    crate::dispatch_global_event(8, &data);

                    // Re-dial contacts or anchors to ensure mesh remains alive during network transitions
                    let is_anchor = self.discovered_anchors.contains(&peer_id) ||
                                    self.storage.fetch_all_anchor_nodes().map(|nodes| nodes.iter().any(|n| n.peer_id == peer_id.to_string())).unwrap_or(false);

                    if is_anchor {
                        self.dial_relay_path(peer_id); // Use helper for consistent re-dialing
                    } else if let Ok(contacts) = self.storage.get_all_contacts() {
                        if contacts.iter().any(|c| c.peer_id == peer_id.to_string()) {
                            self.dial_relay_path(peer_id);
                        }
                    }
               } else {
                   // Peer is still connected via remaining connections, update relayed state
                   let is_now_relayed = self.direct_conn_count.get(&peer_id).cloned().unwrap_or(0) == 0;
                   self.is_relayed_map.write().insert(peer_id, is_now_relayed);
                   
                   let status: u8 = if is_now_relayed { 1 } else { 0 };
                   let mut data = peer_id.to_string().into_bytes();
                   data.push(b':');
                   data.push(status);
                   crate::dispatch_global_event(8, &data);
               }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(pid) = peer_id {
                    if pid == *self.swarm.local_peer_id() { return Ok(()); }
                    println!("[Swarm] Outgoing connection error for peer {}: {:?}", pid, error);

                    // Clean up the failed address from Kademlia to stop propagating stale routes
                    if let libp2p::swarm::DialError::Transport(errors) = &error {
                        for (addr, _) in errors {
                            println!("[Mesh] Removing failed address {} from Kademlia for peer {}", addr, pid);
                            self.swarm.behaviour_mut().kademlia.remove_address(&pid, addr);
                        }
                    }

                    // Track diagnostic failures for the recheck overlay
                    if self.pending_diagnostics.contains_key(&pid) {
                        let err_str = format!("{:?}", error).replace('"', "'");
                        if err_str.contains("ResourceLimitExceeded") {
                            println!("[Mesh] ⚠️ RELAY CONGESTION: RBN rejected circuit for {} due to ResourceLimitExceeded.", pid);
                        }
                        let diag_payload = format!(
                            r#"{{"peer_id":"{}","step":"error","status":"dial_failed","error":"{}"}}"#,
                            pid, err_str
                        );
                        crate::dispatch_global_event(15, diag_payload.as_bytes());
                    }
                }
            }

            _ => {}
        }
        Ok(())
    }

    fn dial_relay_path(&mut self, recipient_id: PeerId) {
        let recipient_str = recipient_id.to_string();

        // Rate-limit dials to 5s per peer to avoid ResourceLimitExceeded
        if let Some(last) = self.relay_dial_limiter.get(&recipient_id) {
            if last.elapsed() < Duration::from_secs(5) { return; }
        }
        self.relay_dial_limiter.insert(recipient_id, Instant::now());

        println!("[Mesh] Peer {} not connected. Constructing relay paths...", recipient_str);

        // 1. Dial ONE random RBN node from the bootstrap list (Scalability fix for Million-Node Mandate)
        // Dilation all RBNs simultaneously causes ResourceLimitExceeded on the relays.
        let mut rbn_list: Vec<_> = self.bootstrap_nodes.iter()
            .filter(|(_, addr)| addr.to_string().contains("443"))
            .collect();
        
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        rbn_list.shuffle(&mut rng);

        if let Some((rbn_id, rbn_addr)) = rbn_list.first() {
            let relay_addr = rbn_addr.clone()
                .with(libp2p::multiaddr::Protocol::P2p(*rbn_id))
                .with(libp2p::multiaddr::Protocol::P2pCircuit)
                .with(libp2p::multiaddr::Protocol::P2p(recipient_id));

            println!("[Mesh] Attempting relay path dial via RBN: {}", rbn_id);
            let _ = self.swarm.dial(relay_addr);
        }

        // 2. Also attempt direct dial as primary fallback
        let _ = self.swarm.dial(recipient_id);
    }

    async fn forward_to_mesh(&mut self, recipient_id: PeerId, payload: SignalingPayload, force_mailbox: bool) -> anyhow::Result<()> {
        let recipient_str = recipient_id.to_string();

        // LOOPBACK PROTECTION: If sending to ourselves, handle locally
        if recipient_id == *self.swarm.local_peer_id() {
             println!("[Mesh] Loopback payload detected for {}. Routing to local handler.", recipient_str);
             let tx = self.command_tx.clone();
             let p = payload.clone();
             tokio::spawn(async move {
                 let _ = tx.send(NetworkCommand::HandleIncomingPayload { peer_id: recipient_id, payload: p }).await;
             });
             return Ok(());
        }

        if !force_mailbox {
            // 1. Try WebRTC Data Channel if open
            // HYBRID ROUTING: WebRTC Data Channels are extremely fast for small signaling payloads.
            // However, the SCTP stack in webrtc-rs can severely bottleneck on large 64KB+ file chunks.
            // Therefore, we skip WebRTC ONLY for actual FileChunk data (large binary payloads),
            // but allow FileChunkRequest and FileTransfer manifest to use WebRTC for ultra-low latency.
            let is_large_data = matches!(payload, SignalingPayload::FileChunk { .. });
            let is_relayed_conn = self.is_relayed_map.read().get(&recipient_id).cloned().unwrap_or(true);
            let skip_webrtc = is_large_data && !is_relayed_conn;

            if !skip_webrtc {
                let dc_opt = { self.data_channels.read().get(&recipient_id).cloned() };
                let pc_opt = { self.peer_connections.read().get(&recipient_id).cloned() };
                if let (Some(dc), Some(pc)) = (dc_opt, pc_opt) {
                    if dc.ready_state() == RTCDataChannelState::Open && pc.connection_state() == RTCPeerConnectionState::Connected {
                        // Prevent SCTP buffer overflow: wait up to 1 second (100 * 10ms) if buffer is full
                        let mut wait_count = 0;
                        while dc.buffered_amount().await > 256 * 1024 && wait_count < 100 {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            wait_count += 1;
                        }
                        if let Ok(bytes) = serde_json::to_vec(&payload) {
                            if dc.send(&bytes.into()).await.is_ok() {
                                println!("[Mesh] Delivered payload to {} via WebRTC Data Channel", recipient_str);
                                return Ok(());
                            } else {
                                println!("[Mesh] WebRTC Data Channel send FAILED for {}. Removing and closing WebRTC resources.", recipient_str);
                                self.data_channels.write().remove(&recipient_id);
                                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&recipient_id) };
                                if let Some(pc) = pc {
                                    let _ = pc.close().await;
                                }
                            }
                        }
                    }
                }
            }
            // 2. Try direct libp2p delivery if connected
            if self.swarm.is_connected(&recipient_id) {
                // Enforce in-flight concurrency limit (backpressure flow control) for both direct and relay links
                // to prevent socket/substream multiplexer saturation.
                let is_chunk_data = matches!(payload, SignalingPayload::FileChunk { .. });
                if is_chunk_data {
                    let inflight = self.inflight_requests.get(&recipient_id).cloned().unwrap_or(0);
                    let limit = if is_relayed_conn { 4 } else { 8 };
                    if inflight >= limit {
                        println!("[Mesh] In-flight limit ({}) reached for {}. Buffering chunk.", limit, recipient_str);
                        self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
                        return Ok(());
                    }
                }

                println!("[Mesh] Peer {} is connected. Attempting direct delivery...", recipient_str);
                let mut sent = false;
                // If it's a message/ack that can be encrypted, try Noise.
                // NOTE: FileChunk is intentionally excluded from Noise on relay connections:
                // relay transport is already encrypted (libp2p Noise), and adding app-level
                // Noise causes double-JSON-base64 overhead (~83% extra wire cost per chunk).
                let noise_eligible = match &payload {
                    SignalingPayload::Standard(_) | 
                    SignalingPayload::ChatMessage { .. } | 
                    SignalingPayload::Acknowledgement { .. } |
                    SignalingPayload::GroupAction(_) |
                    SignalingPayload::GroupManifest { .. } |
                    SignalingPayload::GroupInvite { .. } |
                    SignalingPayload::MessageReaction { .. } |
                    SignalingPayload::EditMessage { .. } |
                    SignalingPayload::SetRetention { .. } => true,
                    // Only encrypt FileChunk over direct (non-relay) connections
                    SignalingPayload::FileChunk { .. } => !is_relayed_conn,
                    _ => false,
                };
                if noise_eligible {
                    if let Some(session) = self.noise_sessions.get_mut(&recipient_id) {

                        if session.is_finished() {
                            if let Ok(bytes) = serde_json::to_vec(&payload) {
                                if let Ok(encrypted) = session.send_message(&bytes) {
                                    println!("[Mesh] Sending ENCRYPTED payload to {}", recipient_str);
                                    let req_id = self.swarm.behaviour_mut().request_response.send_request(&recipient_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Transport(encrypted))));
                                    self.outbound_tracker.insert(req_id, (recipient_id, payload.clone()));
                                    sent = true;
                                } else {
                                    println!("[Mesh] Noise encryption FAILED for {}. Clearing session and starting a new handshake.", recipient_str);
                                    self.noise_sessions.remove(&recipient_id);
                                    let storage = Arc::clone(&self.storage);
                                    let pid_str = recipient_id.to_string();
                                    tokio::task::spawn_blocking(move || {
                                        let _ = storage.delete_session_state(&pid_str);
                                    });
                                    let tx = self.command_tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx.send(NetworkCommand::EstablishSecureSession { peer_id: recipient_id }).await;
                                    });
                                }
                            }
                        }
                    }
                }

                if !sent {
                    println!("[Mesh] Sending PLAIN payload to {}", recipient_str);
                    let req_id = self.swarm.behaviour_mut().request_response.send_request(&recipient_id, SignalingRequest(payload.clone()));
                    self.outbound_tracker.insert(req_id, (recipient_id, payload.clone()));
                }

                if is_chunk_data {
                    *self.inflight_requests.entry(recipient_id).or_insert(0) += 1;
                }
                return Ok(());
            }

            // 3. Active Relay Dialing (Messenger Strategy)
            // If not connected, construct and dial the relay path via RBN
            self.dial_relay_path(recipient_id);
        }
        // 4. Fallback: Persistent Mesh Storage (Mailbox)
        
        // Check for Push Token to trigger background wakeup (iOS/Android parity)
        if let Ok(Some((device_type, token))) = self.storage.get_push_token(&recipient_str) {
            println!("[Registry] 🔔 Triggering Push Wakeup for {} ({})", recipient_str, device_type);
            let client = reqwest::Client::new();
            let peer_id_clone = recipient_str.clone();
            tokio::spawn(async move {
                // Send generic wakeup trigger to the sovereign push bridge (Anonymized)
                use sha2::{Sha256, Digest};
                let anonymized_peer_id = hex::encode(Sha256::digest(peer_id_clone.as_bytes()));
                
                let payload = serde_json::json!({
                    "device_type": device_type,
                    "token": token,
                    "peer_id_hash": anonymized_peer_id
                });
                let _ = client.post("https://push.introvert.network/wakeup")
                    .json(&payload)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await;
            });
        }

        // WebRTC signaling and handle claims are transient and should never be stored in persistent mailboxes.
        if matches!(payload, SignalingPayload::WebRtc(_) | SignalingPayload::WebRtcNative(_) | SignalingPayload::Candidate(_) | SignalingPayload::Offer(_) | SignalingPayload::Answer(_) | SignalingPayload::HandleClaimRequest { .. } | SignalingPayload::HandleClaimWitnessed { .. }) {
            println!("[Mesh] Buffering real-time signaling/handle registry payload for {} in RAM...", recipient_str);
            self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
            return Ok(());
        }

        // CRITICAL: File data and requests must NEVER be stored in the persistent mailbox.
        // They are buffered in RAM (pending_messages) and flushed only upon circuit establishment.
        if matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
            println!("[Mesh] Path not ready. Buffering file chunk/request for {} in RAM...", recipient_str);
            // REDUNDANCY FILTER: If adding a Request, remove older Requests for the same transfer to prevent buffer bloat
            if let SignalingPayload::FileChunkRequest { ref transfer_id, .. } = payload {
                if let Some(pending) = self.pending_messages.get_mut(&recipient_id) {
                    pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: ref tid, .. } if tid == transfer_id));
                }
            }
            self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
            return Ok(());
        }





        let mut anchor_ids = Vec::new();
        if let Ok(verified_anchors) = self.storage.fetch_all_anchor_nodes() {
            for node in verified_anchors {
                if let Ok(pid) = node.peer_id.parse::<PeerId>() { anchor_ids.push(pid); }
            }
        }
        for pid in &self.discovered_anchors {
            if !anchor_ids.contains(pid) { anchor_ids.push(*pid); }
        }

        // Filter for connected anchors only
        let target_anchor = anchor_ids.iter().find(|pid| self.swarm.is_connected(pid)).cloned();

        if let Some(anchor_id) = target_anchor {
            let allowed_in_mailbox = matches!(payload, 
                SignalingPayload::ChatMessage { .. } | 
                SignalingPayload::Acknowledgement { .. } | 
                SignalingPayload::FileTransfer { .. } |
                SignalingPayload::FileTransferComplete { .. } |
                SignalingPayload::FileTransferError { .. } |
                SignalingPayload::DeleteMessage { .. } |
                SignalingPayload::GroupInvite { .. } |
                SignalingPayload::GroupAction(_) |
                SignalingPayload::GroupManifest { .. } |
                SignalingPayload::MessageReaction { .. } |
                SignalingPayload::EditMessage { .. } |
                SignalingPayload::SetRetention { .. } |
                SignalingPayload::HandleClaimWitnessed { .. }
            );

            if !allowed_in_mailbox {
                return Ok(());
            }

            println!("[Mesh] Storing message for {} on Anchor {}", recipient_str, anchor_id);
            
            // Ensure Mailbox payloads are only ENCRYPTED if they are noise-eligible (Messages/Standard)
            // and a session exists. Transient payloads like Acknowledgements should remain PLAIN 
            // for reliable mailbox delivery across session restarts.
            let noise_eligible = match &payload {
                SignalingPayload::Standard(_) | SignalingPayload::ChatMessage { .. } => true,
                _ => false,
            };

            let secure_payload = if noise_eligible {
                if let Some(session) = self.noise_sessions.get_mut(&recipient_id) {
                    if session.is_finished() {
                        let msg_bytes = serde_json::to_vec(&payload)?;
                        SignalingPayload::Secure(SecureMessage::Transport(session.send_message(&msg_bytes)?))
                    } else {
                        payload
                    }
                } else {
                    // Proactively initiate session for contacts
                    if let Ok(contacts) = self.storage.get_all_contacts() {
                        if contacts.iter().any(|c| c.peer_id == recipient_str) {
                            println!("[Mesh] Initiating Noise session with contact {} for Mailbox delivery", recipient_str);
                            let tx = self.command_tx.clone();
                            let rid = recipient_id;
                            tokio::spawn(async move { let _ = tx.send(NetworkCommand::EstablishSecureSession { peer_id: rid }).await; });
                        }
                    }
                    payload
                }
            } else {
                payload
            };

            let bytes = serde_json::to_vec(&secure_payload)?;
            let req_id = self.swarm.behaviour_mut().request_response.send_request(
                &anchor_id, 
                SignalingRequest(SignalingPayload::MailboxStore { 
                    recipient_id: recipient_str, 
                    payload: bytes 
                })
            );
            self.outbound_tracker.insert(req_id, (recipient_id, secure_payload));
            Ok(())
        } else {
            // No connected anchors for storage. Queue locally in RAM for when we eventually connect.
            println!("[Mesh] No connected anchors for storage. Queuing locally for {}.", recipient_str);
            let pending = self.pending_messages.entry(recipient_id).or_default();
            
            // Deduplicate Chunks/Requests to keep RAM lean
            if let SignalingPayload::FileChunk { transfer_id, chunk_index, .. } = &payload {
                pending.retain(|p| !matches!(p, SignalingPayload::FileChunk { transfer_id: tid, chunk_index: idx, .. } if tid == transfer_id && idx == chunk_index));
            } else if let SignalingPayload::FileChunkRequest { transfer_id, chunk_index, .. } = &payload {
                pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: idx, .. } if tid == transfer_id && idx == chunk_index));
            }

            pending.push(payload.clone());

            // Dial mesh to find anchors
            for pid in anchor_ids { let _ = self.swarm.dial(pid); }
            for (_, addr) in self.bootstrap_nodes.clone() { let _ = self.swarm.dial(addr); }

            let _ = self.swarm.dial(recipient_id);
            Err(anyhow::anyhow!("Mesh storage temporarily unavailable, message queued"))
        }
    }

    async fn perform_mailbox_fetch(&mut self) {
        let mut anchor_ids = Vec::new();
        if let Ok(verified_anchors) = self.storage.fetch_all_anchor_nodes() {
            for node in verified_anchors { if let Ok(pid) = node.peer_id.parse::<PeerId>() { anchor_ids.push(pid); } }
        }
        for pid in &self.discovered_anchors { if !anchor_ids.contains(pid) { anchor_ids.push(*pid); } }
        
        for peer_id in anchor_ids {
            if self.swarm.is_connected(&peer_id) { 
                println!("[Mesh] Draining verified anchor: {}", peer_id);
                self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::MailboxDrain));
            } else { 
                let _ = self.swarm.dial(peer_id); 
            }
        }

    }

    async fn handle_command(&mut self, command: NetworkCommand) -> anyhow::Result<()> {
        match command {
            NetworkCommand::Dial { peer_id, address } => {
                if let Some(addr) = address {
                    let final_addr = if addr.iter().any(|p| matches!(p, libp2p::multiaddr::Protocol::P2p(_))) { addr } else { addr.with(libp2p::multiaddr::Protocol::P2p(peer_id)) };
                    self.swarm.dial(final_addr)?;
                } else {
                    if let Err(_) = self.swarm.dial(peer_id) {
                        let _ = self.swarm.behaviour_mut().kademlia.get_closest_peers(peer_id);
                    }
                }
            }
            NetworkCommand::ListenOn { address } => { self.swarm.listen_on(address)?; }
            NetworkCommand::SendSignaling { peer_id, msg_id, message, reply_to } => {
                let peer_id_str = peer_id.to_string();
                let content_str = message.clone();
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                let c = content_str.clone();
                let rt = reply_to.clone();
                tokio::task::spawn_blocking(move || storage.store_message_with_id(&peer_id_str, &mid, &c, true, rt.as_deref()));
                self.reward_tracker.record_message_activity(&peer_id.to_string());
                
                let timestamp = chrono::Utc::now().timestamp();
                let payload = SignalingPayload::ChatMessage { content: message, msg_id, timestamp, reply_to };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::FetchMailbox => {
                self.perform_mailbox_fetch().await;
            }
            NetworkCommand::UpdateAnchorStatus { enabled } => {
                let key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
                
                if enabled {
                    println!("[Mesh] Opting in as Anchor Node. Advertising to DHT...");
                    let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
                } else {
                    println!("[Mesh] Opting out of Anchor services.");
                    let _ = self.swarm.behaviour_mut().kademlia.stop_providing(&key);
                }

                let payload = [if enabled { 1 } else { 0 }];
                crate::dispatch_global_event(11, &payload);
            }
            NetworkCommand::AddGroupMember { group_id, peer_id } => {
                println!("[Mesh] Adding member {} to group {}", peer_id, group_id);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();
                    
                    let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                    if !is_admin {
                        eprintln!("[Mesh] Permission denied: Only admins can add members");
                        return Ok(());
                    }

                    if members.iter().any(|m| m.peer_id == peer_id) {
                        println!("[Mesh] Peer {} is already a member", peer_id);
                        return Ok(());
                    }

                    if let Ok(Some(contact)) = self.storage.get_contact(&peer_id) {
                        let new_meta = GroupMemberMetadata {
                            peer_id: peer_id.clone(),
                            pubkey: contact.p2p_pubkey.clone(),
                            role: GroupRole::Member,
                            alias: contact.local_alias.or(contact.global_name),
                            avatar_base64: contact.avatar_base64,
                        };
                        members.push(new_meta.clone());
                        let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                        let _ = self.storage.update_group_members(&group_id, &updated_members_json);

                        let action = GroupAction::AddMember { metadata: new_meta };
                        if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                            let action_payload = SignalingPayload::GroupAction(signed);
                            
                            if let Ok(wrapped) = group::GroupManager::wrap_group_secret(&group_info.secret, &contact.static_key) {
                                let invite = SignalingPayload::GroupInvite {
                                    group_id: group_id.clone(),
                                    name: group_info.name.clone(),
                                    description: group_info.description.clone(),
                                    inviter_peer_id: my_peer_id.clone(),
                                    group_secret_wrapped: wrapped,
                                    members: members.clone(),
                                };
                                if let Ok(pid) = peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, invite, false).await;
                                }
                            }

                            for m in &members {
                                if m.peer_id == my_peer_id || m.peer_id == peer_id { continue; }
                                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }
                        }
                    } else {
                        eprintln!("[Mesh] Cannot add member: Peer {} is not in contacts list", peer_id);
                    }
                }
            }
            NetworkCommand::ApproveGroupJoin { group_id, requester_peer_id, alias, avatar, handle: _handle } => {
                println!("[Mesh] Admin approving group join request for {} to group {}", requester_peer_id, group_id);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();

                    if members.iter().any(|m| m.peer_id == requester_peer_id) {
                        println!("[Mesh] Peer {} is already a member", requester_peer_id);
                        return Ok(());
                    }

                    let mut p2p_pubkey = vec![];
                    if let Ok(peer) = requester_peer_id.parse::<PeerId>() {
                        let peer_bytes = peer.to_bytes();
                        if peer_bytes.len() >= 38 && peer_bytes[0] == 0x00 && peer_bytes[1] == 0x24 {
                            if let Ok(pubk) = libp2p::identity::PublicKey::try_decode_protobuf(&peer_bytes[2..]) {
                                p2p_pubkey = pubk.encode_protobuf();
                            }
                        }
                    }

                    let new_meta = GroupMemberMetadata {
                        peer_id: requester_peer_id.clone(),
                        pubkey: p2p_pubkey,
                        role: GroupRole::Member,
                        alias,
                        avatar_base64: avatar,
                    };
                    members.push(new_meta.clone());
                    let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                    let _ = self.storage.update_group_members(&group_id, &updated_members_json);

                    let action = GroupAction::AddMember { metadata: new_meta };
                    if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                        let action_payload = SignalingPayload::GroupAction(signed);
                        for m in &members {
                            if m.peer_id == my_peer_id || m.peer_id == requester_peer_id { continue; }
                            if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                            }
                        }
                    }

                    if let Ok(peer) = requester_peer_id.parse::<PeerId>() {
                        let manifest_payload = SignalingPayload::GroupManifest {
                            group_id: group_id.clone(),
                            name: group_info.name.clone(),
                            description: group_info.description.clone(),
                            members: members.clone(),
                            secret: group_info.secret,
                        };
                        let _ = self.forward_to_mesh(peer, manifest_payload, false).await;
                    }

                    crate::dispatch_global_event(23, group_id.as_bytes());
                }
            }
            NetworkCommand::RejectGroupJoin { group_id, requester_peer_id, reason } => {
                println!("[Mesh] Admin rejecting group join request for {} to group {}", requester_peer_id, group_id);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    if let Ok(peer) = requester_peer_id.parse::<PeerId>() {
                        let reject_payload = SignalingPayload::GroupJoinRejected {
                            group_id,
                            group_name: group_info.name,
                            reason,
                        };
                        let _ = self.forward_to_mesh(peer, reject_payload, false).await;
                    }
                }
            }
            NetworkCommand::RemoveGroupMember { group_id, peer_id, members_json } => {
                println!("[Mesh] Removing member {} from group {}", peer_id, group_id);
                
                let group_data = if let Some(mj) = members_json {
                    Some(mj)
                } else if let Ok(Some(gi)) = self.storage.get_group(&group_id) {
                    Some(gi.members_json)
                } else {
                    None
                };

                if let Some(mj) = group_data {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&mj).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();
                    let is_self = peer_id == my_peer_id;
                    
                    let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                    if !is_admin && !is_self {
                        eprintln!("[Mesh] Permission denied: Only admins can remove members");
                        return Ok(());
                    }

                    if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                        if members[pos].role == GroupRole::Creator {
                            eprintln!("[Mesh] Permission denied: Creator cannot leave or be removed from the group");
                            return Ok(());
                        }

                        members.remove(pos);
                        let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                        
                        let action = GroupAction::RemoveMember { peer_id: peer_id.clone() };
                        if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                            let action_payload = SignalingPayload::GroupAction(signed);
                            
                            if is_self {
                                // If we removed ourselves, delete the group locally (if not already done) and notify the mesh
                                let _ = self.storage.delete_group(&group_id);
                                crate::dispatch_global_event(22, group_id.as_bytes());
                            } else {
                                // If we removed someone else (as admin), update DB locally and notify that person
                                let _ = self.storage.update_group_members(&group_id, &updated_members_json);
                                crate::dispatch_global_event(23, group_id.as_bytes());
                                if let Ok(pid) = peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }

                            // Notify all other members
                            for m in &members {
                                if m.peer_id == my_peer_id { continue; }
                                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }
                        }
                    }
                }
            }
            NetworkCommand::UpdateGroupRole { group_id, peer_id, role } => {
                println!("[Mesh] Updating member {} role in group {} to {:?}", peer_id, group_id, role);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
                    let my_peer_id = self.swarm.local_peer_id().to_string();
                    
                    let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                    if !is_admin {
                        eprintln!("[Mesh] Permission denied: Only admins can update roles");
                        return Ok(());
                    }

                    if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                        if members[pos].role == GroupRole::Creator {
                            eprintln!("[Mesh] Permission denied: Cannot change creator's role");
                            return Ok(());
                        }

                        members[pos].role = role.clone();
                        let updated_members_json = serde_json::to_string(&members).unwrap_or_default();
                        let _ = self.storage.update_group_members(&group_id, &updated_members_json);

                        let action = GroupAction::UpdateRole { peer_id: peer_id.clone(), new_role: role };
                        if let Ok(signed) = group::GroupManager::sign_action(group_id.clone(), action, &self.local_keypair) {
                            let action_payload = SignalingPayload::GroupAction(signed);
                            
                            for m in &members {
                                if m.peer_id == my_peer_id { continue; }
                                if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                    let _ = self.forward_to_mesh(pid, action_payload.clone(), false).await;
                                }
                            }
                        }
                }
            }
        }
            NetworkCommand::PublishGroupManifest { group_id, code } => {
                println!("[Mesh] Publishing discovery record for Sovereign Group: {}", group_id);
                // SECURITY HARDENING: Never publish the group secret to the DHT.
                let key = RecordKey::new(&code.as_bytes());
                let record = libp2p::kad::Record {
                    key: key.clone(),
                    value: group_id.as_bytes().to_vec(),
                    publisher: None,
                    expires: None,
                };
                let _ = self.swarm.behaviour_mut().kademlia.put_record(record, kad::Quorum::One);
                let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
            }
            NetworkCommand::JoinMeshByCode { code } => {
                println!("[Mesh] Searching for Sovereign Group via code: {}", code);
                let key = RecordKey::new(&code.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.get_providers(key.clone());
                let _ = self.swarm.behaviour_mut().kademlia.get_record(key);
            }
            NetworkCommand::ResolveHandle { handle } => {
                println!("[Mesh] Resolving handle {} via DHT...", handle);
                let key = RecordKey::new(&handle.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.get_record(key);
            }
            NetworkCommand::SendDirectInvite { peer_id, identity, is_accept } => {
                let payload = if is_accept {
                    SignalingPayload::DirectInviteAccept(identity)
                } else {
                    SignalingPayload::DirectInviteRequest(identity)
                };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::ClaimHandle { handle } => {
                println!("[Registry] Initiating claim for handle: {}", handle);
                let my_peer_id = self.swarm.local_peer_id().to_string();
                let timestamp = Utc::now().timestamp();
                
                // Generating PoW can be heavy, but at difficulty 4 it's fast.
                let pow_nonce = self.registry.generate_pow(&handle, &my_peer_id, timestamp);
                
                let payload = SignalingPayload::HandleClaimRequest {
                    handle,
                    peer_id: my_peer_id,
                    timestamp,
                    pow_nonce,
                };
                
                // Blast to all RBNs (Bootstrap nodes are the default RBNs)
                for (rbn_id, _) in self.bootstrap_nodes.clone() {
                    let _ = self.forward_to_mesh(rbn_id, payload.clone(), false).await;
                }
                
                // Also forward to any other discovered anchors
                for anchor_id in self.discovered_anchors.clone() {
                    let _ = self.forward_to_mesh(anchor_id, payload.clone(), false).await;
                }
            }
            NetworkCommand::BroadcastWitness { handle, peer_id, timestamp, pubkey, signature } => {
                 let my_peer_id = self.swarm.local_peer_id().to_string();
                 let payload = SignalingPayload::HandleClaimWitnessed {
                     handle,
                     peer_id: peer_id.clone(),
                     timestamp,
                     rbn_peer_id: my_peer_id,
                     rbn_pubkey: pubkey,
                     rbn_signature: signature,
                 };
                 
                 // Gossip back to requester
                 if let Ok(requester_pid) = PeerId::from_str(&peer_id) {
                     let _ = self.forward_to_mesh(requester_pid, payload.clone(), false).await;
                 }
                 
                 // Gossip to other anchors/RBNs
                 for (rbn_id, _) in self.bootstrap_nodes.clone() {
                    let _ = self.forward_to_mesh(rbn_id, payload.clone(), false).await;
                 }
            }
            NetworkCommand::AcceptGroupInvite { group_id } => {
                println!("[Mesh] Accepting group invite for: {}", group_id);
                if let Ok(Some(invite)) = self.storage.get_pending_invite(&group_id) {
                    if let Ok(group_secret) = group::GroupManager::unwrap_group_secret(&invite.group_secret_wrapped, &self.local_static_secret) {
                        let _ = self.storage.save_group_secret(&group_id, &group_secret);
                        let _ = self.storage.upsert_group(&group_id, &invite.name, &invite.description, &invite.members_json);
                        let _ = self.storage.delete_pending_invite(&group_id);
                        let _ = self.storage.untombstone_group(&group_id);
                        crate::dispatch_global_event(23, group_id.as_bytes());
                        println!("[Mesh] ✅ Group invite accepted: {}", invite.name);

                        // --- RELIABILITY FIX: Proactive Member Discovery ---
                        // Immediately attempt to dial all group members to establish the mesh.
                        let members: Vec<GroupMemberMetadata> = serde_json::from_str(&invite.members_json).unwrap_or_default();
                        let my_peer_id = self.swarm.local_peer_id().to_string();
                        for m in members {
                            if m.peer_id == my_peer_id { continue; }
                            if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                                println!("[Mesh] Proactively dialing group member {} for mesh {}", pid, invite.name);
                                self.dial_relay_path(pid);
                            }
                        }
                    } else {
                        eprintln!("[Mesh] ❌ Failed to unwrap group secret for {}", group_id);
                    }
                } else {
                    eprintln!("[Mesh] No pending invite found for group: {}", group_id);
                }
            }
            NetworkCommand::DeclineGroupInvite { group_id } => {
                println!("[Mesh] Declining group invite for: {}", group_id);
                let _ = self.storage.delete_pending_invite(&group_id);
                println!("[Mesh] ✅ Group invite declined and removed.");
            }
            NetworkCommand::PublishGossipsub { topic, data } => {
                let ident_topic = libp2p::gossipsub::IdentTopic::new(topic.clone());
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(ident_topic, data) {
                    eprintln!("[Mesh] ❌ Failed to publish gossipsub message to topic {}: {:?}", topic, e);
                }
            }
            NetworkCommand::BroadcastGroupMessage { group_id, message, reply_to } => {
                println!("[Mesh] Internal Broadcast for group {}: {}", group_id, message);
                let storage = self.storage.clone();
                let gid = group_id.clone();
                let keypair = self.local_keypair.clone();
                let tx = self.command_tx.clone();
                let my_peer_id = self.swarm.local_peer_id().to_string();

                tokio::spawn(async move {
                    // Check if we are muted before broadcasting
                    if let Ok(muted) = storage.get_group_muted_members(&gid) {
                        if muted.contains(&my_peer_id) {
                            eprintln!("[Mesh] ❌ Blocked broadcast: User is MUTED in group {}", gid);
                            return;
                        }
                    }

                    if let Ok(Some(group_secret_vec)) = storage.load_group_secret(&gid) {
                        if group_secret_vec.len() == 32 {
                            let mut group_secret = [0u8; 32];
                            group_secret.copy_from_slice(&group_secret_vec);

                            use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                            use rand::RngCore;
                            let mut nonce_bytes = [0u8; 12];
                            rand::thread_rng().fill_bytes(&mut nonce_bytes);
                            let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_secret));

                            if let Ok(encrypted) = cipher.encrypt(Nonce::from_slice(&nonce_bytes), message.as_bytes()) {
                                let mut content_encrypted = nonce_bytes.to_vec();
                                content_encrypted.extend(encrypted);

                                let mut msg_id = format!("gm_int_{}_{}", gid, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
                                if message.starts_with("[FILE]:") {
                                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&message[7..]) {
                                        if let Some(tid) = meta.get("transfer_id").and_then(|v| v.as_str()) {
                                            msg_id = tid.to_string();
                                        }
                                    }
                                }

                                let action = GroupAction::Message { content_encrypted, msg_id, reply_to };
                                if let Ok(signed) = group::GroupManager::sign_action(gid.clone(), action, &keypair) {
                                    let payload = SignalingPayload::GroupAction(signed);
                                    if let Ok(data) = serde_json::to_vec(&payload) {
                                        let _ = tx.send(NetworkCommand::PublishGossipsub { topic: gid, data }).await;
                                    }
                                }
                            }
                        }
                    }
                });
            }
            NetworkCommand::RegisterSeeder { peer_id, transfer_id, file_path, file_hash, chunk_size, total_chunks, group_id } => {
                self.active_seeders.insert(transfer_id, ActiveSeeder {
                    peer_id,
                    file_path: file_path.clone(),
                    file_hash: file_hash.clone(),
                    chunk_size,
                    total_chunks,
                    _bytes_sent: 0,
                    _start_time: Instant::now(),
                    group_id,
                    completions: HashSet::new(),
                });

                // SOVEREIGN DRIVE: Persist metadata so this node can serve as a mesh seeder indefinitely
                let storage_d = self.storage.clone();
                let path_d = file_path.clone();
                let hash_d = file_hash.clone();
                tokio::task::spawn_blocking(move || {
                    let filename = std::path::Path::new(&path_d).file_name().unwrap_or_default().to_string_lossy().into_owned();
                    let size = std::fs::metadata(&path_d).map(|m| m.len()).unwrap_or(0);
                    // Use a generic mime type for seeding persistence if unknown
                    let _ = storage_d.upsert_drive_file(&filename, &hash_d, "application/octet-stream", size as i64, &path_d);
                });

                // SOVEREIGN SWARM: Announce that we are providing this file hash to the mesh
                let key = RecordKey::new(&file_hash.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
            }
            NetworkCommand::UnregisterSeeder { transfer_id } => {
                self.active_seeders.remove(&transfer_id);
            }
            NetworkCommand::CancelFileTransfer { transfer_id } => {
                println!("[Mesh] Cancelling file transfer: {}", transfer_id);
                // Remove from active seeders (outgoing transfers)
                self.active_seeders.remove(&transfer_id);
                // Remove from incoming transfers
                self.incoming_transfers.remove(&transfer_id);
                // Notify UI of cancellation
                let progress = FileTransferProgress {
                    transfer_id: transfer_id.clone(),
                    peer_id: String::new(),
                    filename: String::new(),
                    mime_type: String::new(),
                    file_hash: String::new(),
                    progress: 0.0,
                    is_complete: true,
                    is_verified: false,
                    is_outgoing: false,
                    local_path: None,
                    start_time_ms: 0,
                    speed_bps: 0.0,
                    group_id: None,
                };
                crate::dispatch_global_event(12, &serde_json::to_vec(&progress).unwrap_or_default());
            }
            NetworkCommand::FindProviders { file_hash } => {
                println!("[Mesh] Searching Sovereign Swarm for providers of file: {}", file_hash);
                let key = RecordKey::new(&file_hash.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.get_providers(key);
            }
            NetworkCommand::SendFile { peer_id, file_path, group_id, transfer_id } => {
                let local_id = *self.swarm.local_peer_id();
                
                // If peer_id == local_id, it's a group broadcast share from Drive.
                if peer_id == local_id && group_id.is_some() {
                    let gid = group_id.as_ref().unwrap().clone();
                    println!("[Mesh] Group-wide file share detected for {}. Initiating intelligent swarm logic.", gid);

                    // Compute hash once to ensure stable transfer_id for all paths
                    let path = std::path::Path::new(&file_path);
                    if !path.exists() { return Err(anyhow::anyhow!("File not found: {}", file_path)); }
                    
                    let file_hash = {
                        let mut hasher = Sha256::new();
                        let f = std::fs::File::open(path)?;
                        let mut reader = std::io::BufReader::new(f);
                        std::io::copy(&mut reader, &mut hasher)?;
                        format!("{:x}", hasher.finalize())
                    };
                    
                    let t_id = transfer_id.unwrap_or_else(|| {
                        format!("gft_{}_{}", file_hash, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                    });
                    let tx = self.command_tx.clone();
                    
                    // 1. Proactive Local Push: Attempt to reach group members via direct paths
                    if let Ok(Some(group)) = self.storage.get_group(&gid) {
                        if let Ok(members) = serde_json::from_str::<Vec<GroupMemberMetadata>>(&group.members_json) {
                            let mut push_count = 0;
                            for member in members {
                                if member.peer_id == local_id.to_string() { continue; }
                                if push_count >= 100 { break; } // Scalability cap for proactive push
                                
                                if let Ok(m_peer_id) = member.peer_id.parse::<PeerId>() {
                                    // Even if not currently connected direct, try a SendFile.
                                    // SendFile logic will auto-negotiate WebRTC/Direct paths.
                                    println!("[Mesh] 🚀 Proactively initiating path discovery for group member {}.", member.peer_id);
                                    let f_path = file_path.clone();
                                    let g_id = Some(gid.clone());
                                    let t_id_clone = Some(t_id.clone());
                                    let tx_clone = tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx_clone.send(NetworkCommand::SendFile { 
                                            peer_id: m_peer_id, 
                                            file_path: f_path, 
                                            group_id: g_id,
                                            transfer_id: t_id_clone 
                                        }).await;
                                    });
                                    push_count += 1;
                                }
                            }
                        }
                    }

                    // 2. Gossipsub Manifest Broadcast: announce to the group topic so all members
                    // (including those not yet directly reachable) know a transfer is in progress.
                    // We pass is_relayed=false so that receivers who receive this manifest via Gossipsub
                    // will wait for the direct PUSH from the per-member SendFile calls above,
                    // rather than immediately entering the slow pull pipeline.
                    // Chunk delivery is handled entirely by the individual SendFile per-member paths.
                    let storage = self.storage.clone();
                    let is_stress = self.is_stress_test;
                    let t_id_for_broadcast = t_id.clone();
                    let relayed_map = self.is_relayed_map.clone();
                    let dc_store = self.data_channels.clone();
                    tokio::spawn(async move {
                        let _ = Self::process_outgoing_file(peer_id, file_path, false, relayed_map, dc_store, tx, storage, local_id, group_id, is_stress, Some(t_id_for_broadcast)).await;
                    });
                    return Ok(());
                }

                let already_direct = self.swarm.is_connected(&peer_id)
                    && self.is_relayed_map.read().get(&peer_id).cloned() == Some(false);
                let has_dc_already = {
                    let dc_store_read = self.data_channels.read();
                    if let Some(dc) = dc_store_read.get(&peer_id) {
                        dc.ready_state() == RTCDataChannelState::Open
                    } else {
                        false
                    }
                };

                if !has_dc_already && !already_direct {
                    // Kick off WebRTC negotiation — the spawn below will wait for it
                    println!("[Mesh] File transfer to {} initiated. Auto-negotiating WebRTC Data Channel...", peer_id);
                    let tx_webrtc = self.command_tx.clone();
                    let pid_webrtc = peer_id;
                    tokio::spawn(async move {
                        let _ = tx_webrtc.send(NetworkCommand::InitiateWebRtc { peer_id: pid_webrtc, media_type: 3 }).await;
                    });
                }

                let dc_store = Arc::clone(&self.data_channels);
                let tx = self.command_tx.clone();
                let tid_pass = transfer_id;

                tokio::spawn(async move {
                    // Wait up to 4 seconds (40 × 100ms) for the WebRTC Data Channel to reach Open state
                    if !has_dc_already {
                        for _ in 0..40 {
                            let open = {
                                let dc_store_read = dc_store.read();
                                if let Some(dc) = dc_store_read.get(&peer_id) {
                                    dc.ready_state() == RTCDataChannelState::Open
                                } else {
                                    false
                                }
                            };
                            if open {
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                    let _ = tx.send(NetworkCommand::SendFileFinalize { peer_id, file_path, has_dc_already, group_id, transfer_id: tid_pass }).await;
                });
            }
            NetworkCommand::SendFileFinalize { peer_id, file_path, has_dc_already: _, group_id, transfer_id } => {
                let is_connected_now = self.swarm.is_connected(&peer_id);
                let relayed_map_snapshot = self.is_relayed_map.read().get(&peer_id).cloned();
                let tx = self.command_tx.clone();
                let storage = self.storage.clone();
                let local_peer_id = *self.swarm.local_peer_id();
                let is_stress = self.is_stress_test;
                let relayed_map = self.is_relayed_map.clone();
                let dc_store = self.data_channels.clone();

                tokio::spawn(async move {
                    let is_relayed = if is_connected_now {
                        relayed_map_snapshot.unwrap_or(false) // Default to false (direct P2P) if connected
                    } else {
                        true
                    };

                    let _ = Self::process_outgoing_file(peer_id, file_path, is_relayed, relayed_map, dc_store, tx, storage, local_peer_id, group_id, is_stress, transfer_id).await;
                });
            }
            NetworkCommand::SendFileChunk { peer_id, payload, progress } => {
                // Persistent History/Torrent model: Always forward to mesh
                match self.forward_to_mesh(peer_id, payload, false).await {
                    Ok(_) => {
                        let data = serde_json::to_vec(&progress).unwrap_or_default();
                        crate::dispatch_global_event(12, &data);
                    }
                    Err(_) => {
                        // If forward failed, the helper already dialed or queued.
                        // We still show progress as "QUEUED" essentially.
                    }
                }
            }
            NetworkCommand::SendAcknowledgement { peer_id, msg_id, status } => {
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                tokio::task::spawn_blocking(move || storage.update_message_status(&mid, status));
                
                let payload = SignalingPayload::Acknowledgement { msg_id, status };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::ForwardMeshSignaling { peer_id, payload } => {
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::ForwardWebRtcNative { peer_id, json } => {
                // Forward a raw flutter_webrtc SDP/ICE JSON signal to the remote peer via mesh
                let payload = SignalingPayload::WebRtcNative(json);
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::StoreInMailbox { peer_id, payload } => {
                // Force mailbox: bypass direct delivery entirely.
                // This breaks the relay direct-retry loop for non-FileChunk payloads.
                let _ = self.forward_to_mesh(peer_id, payload, true).await;
            }
            NetworkCommand::HandleIncomingPayload { peer_id, payload } => {
                self.handle_signaling_payload(peer_id, payload, false).await;
            }
            NetworkCommand::HandleIncomingWebRtcPayload { peer_id, payload } => {
                self.handle_signaling_payload(peer_id, payload, true).await;
            }
            NetworkCommand::ForceMeshRefresh => {
                println!("[Network] Force Mesh Refresh triggered. Performing HARD RESET of networking stack.");
                // Immediately notify UI we are connecting
                crate::dispatch_global_event(10, &[3]); 

                // 1. Actively disconnect all current peers to clear stale WiFi/VPN sockets
                let current_peers: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                for pid in current_peers {
                    let _ = self.swarm.disconnect_peer_id(pid);
                }

                // 2. Clear established Noise sessions to force re-handshake on new IP
                self.noise_sessions.clear();

                // 3. Re-inject bootstrap nodes and refresh DHT
                // Messenger strategy: Prioritize Port 443 RBN connection
                for (peer_id, addr) in self.bootstrap_nodes.clone() {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
                    // Aggressively dial Introvert RBN node first
                    if addr.to_string().contains("443") {
                        println!("[Network] Aggressively dialing hardened RBN: {}", addr);
                        let _ = self.swarm.dial(addr);
                    }
                }
                
                // Speed up discovery during sync
                let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
                self.perform_mailbox_fetch().await;
            }
            NetworkCommand::InitiateWebRtc { peer_id, media_type } => {
                let (pc, mut dc_rx) = MediaManager::create_peer_connection(true, Arc::clone(&self.reward_tracker), peer_id, self.command_tx.clone()).await?;
                let dc_store = Arc::clone(&self.data_channels);
                tokio::spawn(async move {
                    if let Some(dc) = dc_rx.recv().await {
                        dc_store.write().insert(peer_id, dc);
                    }
                });

                if let Err(e) = MediaManager::add_media_tracks(Arc::clone(&pc), media_type).await {
                    eprintln!("❌ Failed to add media tracks: {:?}", e);
                }

                let offer_sdp = MediaManager::create_offer(Arc::clone(&pc)).await?;
                self.peer_connections.write().insert(peer_id, pc);
                let signal = WebRtcSignal { signal_type: "offer".to_owned(), sdp: offer_sdp };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&signal).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure { 
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(signal), false).await;
                }
            }
            NetworkCommand::StartMediaStream { peer_id, media_type } => {
                let pc_clone = { let pcs = self.peer_connections.read(); pcs.get(&peer_id).cloned() };
                if let Some(pc) = pc_clone { MediaManager::add_media_tracks(pc, media_type).await?; }
            }
            NetworkCommand::AcceptWebRtc { peer_id, media_type } => {
                if let Some(offer_sdp) = self.pending_offers.remove(&peer_id) {
                    if let Ok((pc, mut dc_rx)) = MediaManager::create_peer_connection(false, Arc::clone(&self.reward_tracker), peer_id, self.command_tx.clone()).await {
                        let dc_store = Arc::clone(&self.data_channels);
                        tokio::spawn(async move {
                            if let Some(dc) = dc_rx.recv().await {
                                dc_store.write().insert(peer_id, dc);
                            }
                        });

                        if let Err(e) = MediaManager::add_media_tracks(Arc::clone(&pc), media_type).await {
                            eprintln!("❌ Failed to add media tracks: {:?}", e);
                        }

                        if let Ok(answer_sdp) = MediaManager::handle_offer(offer_sdp, Arc::clone(&pc)).await {
                            self.peer_connections.write().insert(peer_id, pc);
                            let response = WebRtcSignal { signal_type: "answer".to_owned(), sdp: answer_sdp };
                            
                            let mut is_secure = false;
                            if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                                if session.is_finished() {
                                    let mut payload = b"WEBRTC:".to_vec();
                                    payload.extend_from_slice(&serde_json::to_vec(&response).unwrap());
                                    if let Ok(encrypted) = session.send_message(&payload) {
                                        let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                                        is_secure = true;
                                    }
                                }
                            }
                            if !is_secure {
                                let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(response), false).await;
                            }
                        }
                    }
                }
            }
            NetworkCommand::RejectWebRtc { peer_id } => {
                self.pending_offers.remove(&peer_id);
                let response = WebRtcSignal { signal_type: "reject".to_owned(), sdp: "".to_owned() };
                
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&response).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure {
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(response), false).await;
                }
            }
            NetworkCommand::CloseWebRtc { peer_id } => { 
                let response = WebRtcSignal { signal_type: "reject".to_owned(), sdp: "".to_owned() };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&response).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure {
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(response), false).await;
                }

                self.data_channels.write().remove(&peer_id);
                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
                if let Some(pc) = pc { let _ = pc.close().await; } 

                let data = peer_id.to_string().into_bytes();
                crate::dispatch_global_event(16, &data);
            }
            NetworkCommand::WebRtcFailed { peer_id } => {
                println!("Peer Connection State has changed: failed");
                self.data_channels.write().remove(&peer_id);
                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
                if let Some(pc) = pc { let _ = pc.close().await; }

                let data = peer_id.to_string().into_bytes();
                crate::dispatch_global_event(16, &data);

                // RESTORE/UPDATE STATUS: WebRTC channel failed, fall back to current libp2p link state
                let is_connected = self.swarm.is_connected(&peer_id);
                let status: u8 = if is_connected {
                    if self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false) {
                        1 // Relayed
                    } else {
                        0 // Direct P2P
                    }
                } else {
                    2 // Offline
                };
                let mut data = peer_id.to_string().into_bytes();
                data.push(b':');
                data.push(status);
                crate::dispatch_global_event(8, &data);
                
                // RECOVERY: For any active outgoing transfers to this peer, re-broadcast
                // the manifest with is_relayed=true so the receiver switches to pull mode
                // and requests the chunks it's missing via the mailbox path.
                let active_transfers: Vec<(String, String, String, usize, u32, u32)> = self.active_seeders
                    .iter()
                    .filter(|(_, s)| s.peer_id == peer_id)
                    .map(|(tid, s)| (tid.clone(), s.file_path.clone(), s.file_hash.clone(), 
                                     (s.total_chunks * s.chunk_size) as usize, s.chunk_size, s.total_chunks))
                    .collect();
                
                for (transfer_id, _, file_hash, total_size, _, _) in active_transfers {
                    let local_peer_id = *self.swarm.local_peer_id();
                    let recovery_manifest = SignalingPayload::FileTransfer {
                        transfer_id,
                        filename: "".to_string(), // receiver already has this
                        mime_type: "".to_string(),
                        file_hash,
                        total_size,
                        is_relayed: true,  // Switch receiver to pull mode
                        sender_peer_id: Some(local_peer_id.to_string()),
                        group_id: None,
                    };
                    let tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                            peer_id, 
                            payload: recovery_manifest 
                        }).await;
                    });
                }
            }
            NetworkCommand::RenegotiateWebRtc { peer_id } => { 
                let (pc, mut dc_rx) = MediaManager::create_peer_connection(true, Arc::clone(&self.reward_tracker), peer_id, self.command_tx.clone()).await?;
                let dc_store = Arc::clone(&self.data_channels);
                tokio::spawn(async move {
                    if let Some(dc) = dc_rx.recv().await {
                        dc_store.write().insert(peer_id, dc);
                    }
                });

                let offer_sdp = MediaManager::create_offer(Arc::clone(&pc)).await?;
                self.peer_connections.write().insert(peer_id, pc);
                let signal = WebRtcSignal { signal_type: "offer".to_owned(), sdp: offer_sdp };
                let mut is_secure = false;
                if let Some(session) = self.noise_sessions.get_mut(&peer_id) {
                    if session.is_finished() {
                        let mut payload = b"WEBRTC:".to_vec();
                        payload.extend_from_slice(&serde_json::to_vec(&signal).unwrap());
                        if let Ok(encrypted) = session.send_message(&payload) {
                            let _ = self.forward_to_mesh(peer_id, SignalingPayload::Secure(SecureMessage::Transport(encrypted)), false).await;
                            is_secure = true;
                        }
                    }
                }
                if !is_secure { 
                    let _ = self.forward_to_mesh(peer_id, SignalingPayload::WebRtc(signal), false).await;
                }
            }
            NetworkCommand::AddAddress { peer_id, address } => { self.swarm.behaviour_mut().kademlia.add_address(&peer_id, address); }
            NetworkCommand::EstablishSecureSession { peer_id } => {
                if self.noise_sessions.contains_key(&peer_id) { return Ok(()); }
                let peer_id_str = peer_id.to_string();
                let storage = Arc::clone(&self.storage);

                // Tie-breaker: lexicographical comparison of PeerId strings
                let local_peer_str = self.swarm.local_peer_id().to_string();
                let is_initiator = local_peer_str < peer_id_str;

                if is_initiator {
                    let storage_contact = storage.clone();
                    let pid_str = peer_id_str.clone();
                    let contact = tokio::task::spawn_blocking(move || storage_contact.get_contact(&pid_str)).await??;
                    if let Some(identity) = contact {
                        println!("[Mesh] Establishing secure session: Initiator role for peer {}", peer_id_str);
                        let mut session = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), &identity.static_key)?;
                        let handshake_msg = session.send_message(&[])?;
                        let storage_save = Arc::clone(&self.storage);
                        let enc_key = self.session_encryption_key;
                        let session_state = session.get_state();
                        tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage_save, enc_key, peer_id, session_state).await; });
                        self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                        self.noise_sessions.insert(peer_id, session);
                    } else {
                        // Fallback: try loading public key from session cache if contact database isn't fully synced yet
                        let storage_load = storage.clone();
                        let session_blob = tokio::task::spawn_blocking({ let peer_id_str = peer_id_str.clone(); move || storage_load.load_session_state(&peer_id_str) }).await??;
                        if let Some(encrypted_blob) = session_blob {
                            use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                            let key = Key::<Aes256Gcm>::from_slice(&self.session_encryption_key);
                            let cipher = Aes256Gcm::new(key);
                            if encrypted_blob.len() > 12 {
                                let nonce = Nonce::from_slice(&encrypted_blob[0..12]);
                                if let Ok(decrypted) = cipher.decrypt(nonce, &encrypted_blob[12..]) {
                                    if let Ok(state) = bincode::deserialize::<crate::network::noise_session::NoiseSessionState>(&decrypted) {
                                        if let Some(remote_pk) = &state.remote_public {
                                            println!("[Mesh] Establishing secure session: Initiator role (loaded from cache) for peer {}", peer_id_str);
                                            let mut session = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), remote_pk)?;
                                            let handshake_msg = session.send_message(&[])?;
                                            let storage_save = Arc::clone(&self.storage);
                                            let enc_key = self.session_encryption_key;
                                            let session_state = session.get_state();
                                            tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage_save, enc_key, peer_id, session_state).await; });
                                            self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
                                            self.noise_sessions.insert(peer_id, session);
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }

                        println!("[Mesh] Peer {} not in contacts. Querying Kademlia for identity...", peer_id_str);
                        let key = RecordKey::new(&peer_id.to_bytes());
                        let query_id = self.swarm.behaviour_mut().kademlia.get_record(key);
                        self.pending_handshakes.insert(query_id, peer_id);
                    }
                } else {
                    println!("[Mesh] Establishing secure session: Responder role for peer {}", peer_id_str);
                    if let Ok(session) = NoiseSession::responder(self.local_static_secret.to_bytes().as_slice()) {
                        self.noise_sessions.insert(peer_id, session);
                    }
                }
            }
            NetworkCommand::RecheckConnection { peer_id } => {
                println!("[Diagnostics] Starting connection recheck for peer {}", peer_id);

                // Register diagnostic state
                self.pending_diagnostics.insert(peer_id, PendingDiagnostic {
                    start_time: Instant::now(),
                    transport: None,
                });

                // Dispatch scanning started event (Event Type 15: Connection Diagnostics)
                let diag_payload = format!(
                    r#"{{"peer_id":"{}","step":"start","status":"scanning"}}"#,
                    peer_id
                );
                crate::dispatch_global_event(15, diag_payload.as_bytes());

                // If ALREADY connected, report current status immediately so UI doesn't look stuck
                if self.swarm.is_connected(&peer_id) {
                    let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
                    let transport = if !is_relayed { "Direct P2P" } else { "Relayed Connection" };
                    let diag_payload = format!(
                        r#"{{"peer_id":"{}","step":"settled","transport":"{}"}}"#,
                        peer_id, transport
                    );
                    crate::dispatch_global_event(15, diag_payload.as_bytes());
                }

                // SOFT RECHECK: Don't disconnect if we already have a connection.
                // Just force a re-dial of known addresses (including direct ones).
                // This allows libp2p to upgrade or add a direct path without dropping the relay path.
                if !self.swarm.is_connected(&peer_id) {
                    self.is_relayed_map.write().remove(&peer_id);
                }

                // Clear rate limiter to allow immediate redial
                self.relay_dial_limiter.remove(&peer_id);

                // 1. Try direct dial (Direct P2P)
                let _ = self.swarm.dial(peer_id);

                // 2. Try relay paths through all bootstrap nodes (Relayed QUIC + TCP)
                for (rbn_id, rbn_addr) in self.bootstrap_nodes.clone() {
                    if rbn_addr.to_string().contains("443") {
                        let relay_addr = rbn_addr.clone()
                            .with(libp2p::multiaddr::Protocol::P2p(rbn_id))
                            .with(libp2p::multiaddr::Protocol::P2pCircuit)
                            .with(libp2p::multiaddr::Protocol::P2p(peer_id));
                        let _ = self.swarm.dial(relay_addr);
                    }
                }

                // 3. Schedule 8-second timeout guard
                let tx = self.command_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(8)).await;
                    let _ = tx.send(NetworkCommand::HandleDiagnosticTimeout { peer_id }).await;
                });
            }
            NetworkCommand::HandleDiagnosticTimeout { peer_id } => {
                if let Some(diag) = self.pending_diagnostics.remove(&peer_id) {
                    let transport = diag.transport.unwrap_or_else(|| "None".to_string());
                    let elapsed = diag.start_time.elapsed().as_millis() as u64;
                    let status = if transport == "None" { "failed" } else { "settled" };

                    let diag_payload = format!(
                        r#"{{"peer_id":"{}","step":"timeout","status":"{}","transport":"{}","rtt_ms":{}}}"#,
                        peer_id, status, transport, elapsed
                    );
                    crate::dispatch_global_event(15, diag_payload.as_bytes());
                }
            }
            NetworkCommand::PollPeerProfile { peer_id } => {
                println!("[Mesh] Polling profile for peer: {}", peer_id);
                let payload = SignalingPayload::ProfileRequest;
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::SyncChatMessages { peer_id, chat_id, is_group } => {
                println!("[Mesh] Syncing messages for chat: {} (group={})", chat_id, is_group);
                let storage = Arc::clone(&self.storage);
                let chat_id_clone = chat_id.clone();
                let is_group_clone = is_group;
                let last_msg_id = tokio::task::spawn_blocking(move || {
                    if is_group_clone {
                        storage.get_group_messages(&chat_id_clone).ok()
                            .and_then(|msgs| msgs.last().map(|m| m.0.clone()))
                    } else {
                        storage.get_messages_for_peer(&chat_id_clone).ok()
                            .and_then(|msgs| msgs.last().map(|m| m.4.clone().unwrap_or_default()))
                    }
                }).await.unwrap_or(None);

                let storage2 = Arc::clone(&self.storage);
                let chat_id_c2 = chat_id.clone();
                let is_group_c2 = is_group;
                let last_timestamp = tokio::task::spawn_blocking(move || {
                    if is_group_c2 {
                        storage2.get_group_messages(&chat_id_c2).ok()
                            .and_then(|msgs| msgs.last().map(|m| m.3.clone()))
                            .and_then(|ts| ts.parse::<i64>().ok())
                            .unwrap_or(0)
                    } else {
                        storage2.get_messages_for_peer(&chat_id_c2).ok()
                            .and_then(|msgs| msgs.last().map(|m| m.1.clone()))
                            .and_then(|ts| ts.parse::<i64>().ok())
                            .unwrap_or(0)
                    }
                }).await.unwrap_or(0);

                let payload = SignalingPayload::ChatSyncRequest {
                    chat_id,
                    is_group,
                    last_msg_id,
                    last_timestamp,
                    limit: 100,
                };
                let _ = self.forward_to_mesh(peer_id, payload, false).await;
            }
            NetworkCommand::RequestSwarmStats => {
                // Calculate local storage contribution: min(1GB, 75% of free disk space)
                let local_storage_gb: u64 = {
                    let free_bytes: u64 = {
                        #[cfg(unix)]
                        {
                            use std::ffi::CString;
                            let path = CString::new("/").unwrap();
                            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
                            if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } == 0 {
                                (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64)
                            } else {
                                10u64 * 1024 * 1024 * 1024 // fallback 10GB
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            10u64 * 1024 * 1024 * 1024 // fallback 10GB
                        }
                    };
                    let free_gb = free_bytes / (1024 * 1024 * 1024);
                    let seventy_five_pct = (free_gb * 75) / 100;
                    std::cmp::min(1u64, seventy_five_pct)
                };

                let mut unique_nodes = self.mesh_active_peers.clone();
                let local_id = *self.swarm.local_peer_id();
                
                for pid in self.swarm.connected_peers() {
                    unique_nodes.insert(*pid);
                }
                for pid in &self.discovered_anchors {
                    unique_nodes.insert(*pid);
                }

                // Remove self from the set to calculate remote count
                unique_nodes.remove(&local_id);
                let total_nodes = unique_nodes.len() as u64 + 1;
                
                // Active Users Online: all peers we have interacted with in this session + us
                let active_users = total_nodes;
                
                // Collective Capacity: local device + remote non-RBN peers
                // RBN nodes (bootstrap_nodes) contribute 0 storage
                // Regular remote peers contribute 1 GB each
                // Local device contributes min(1GB, 75% of free disk)
                let rbn_ids: std::collections::HashSet<_> = self.bootstrap_nodes.iter().map(|(id, _)| *id).collect();
                let remote_non_rbn_count = unique_nodes.iter().filter(|pid| !rbn_ids.contains(pid)).count() as u64;
                let capacity_gb = local_storage_gb + remote_non_rbn_count;
                
                let stats = serde_json::json!({
                    "total_nodes": total_nodes,
                    "active_users": active_users,
                    "collective_capacity_gb": capacity_gb,
                    "active_transfers": self.incoming_transfers.len() + self.active_seeders.len(),
                });

                if let Ok(json) = serde_json::to_string(&stats) {
                    crate::dispatch_global_event(30, json.as_bytes()); // Event Type 30: Swarm Stats
                }
            }
        }
        Ok(())
    }

    async fn persist_session_state(storage: Arc<crate::storage::StorageService>, session_encryption_key: [u8; 32], peer_id: PeerId, state: crate::network::noise_session::NoiseSessionState) -> anyhow::Result<()> {
        use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
        use rand::RngCore;
        let encoded = bincode::serialize(&state)?;
        let key = Key::<Aes256Gcm>::from_slice(&session_encryption_key);
        let cipher = Aes256Gcm::new(key);
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let mut encrypted = cipher.encrypt(nonce, encoded.as_ref()).map_err(|_| anyhow::anyhow!("Session encryption failed"))?;
        let mut final_payload = nonce_bytes.to_vec();
        final_payload.append(&mut encrypted);
        let peer_id_str = peer_id.to_string();
        tokio::task::spawn_blocking(move || storage.save_session_state(&peer_id_str, final_payload)).await??;
        Ok(())
    }

    async fn start_peer_reaper(peer_connections: Arc<RwLock<HashMap<PeerId, Arc<RTCPeerConnection>>>>) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut closed_peers = Vec::new();
            {
                let pcs = peer_connections.read();
                for (peer, pc) in pcs.iter() {
                    if pc.connection_state() == RTCPeerConnectionState::Closed || pc.connection_state() == RTCPeerConnectionState::Failed { closed_peers.push(*peer); }
                }
            }
            if !closed_peers.is_empty() {
                let mut pcs = peer_connections.write();
                for peer in closed_peers { pcs.remove(&peer); }
            }
        }
    }

    async fn start_mailbox_cleanup(storage: Arc<crate::storage::StorageService>) {
        let mut interval = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            let _ = storage.cleanup_expired_mailbox();
        }
    }

    async fn start_message_pruning(storage: Arc<crate::storage::StorageService>) {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60)); // Check every hour
        loop {
            interval.tick().await;
            let _ = storage.prune_expired_messages();
        }
    }

    pub async fn handle_signaling_payload(&mut self, peer: PeerId, payload: SignalingPayload, is_webrtc: bool) {
        let mut queue = vec![(peer, payload, is_webrtc)];
        while let Some((p, pl, is_wtc)) = queue.pop() {
            match pl {
                SignalingPayload::Secure(secure_msg) => {
                    match secure_msg {
                        SecureMessage::Handshake(handshake_payload) => {
                            let mut success = false;
                            if let Some(session) = self.noise_sessions.get_mut(&p) {
                                if session.recv_message(&handshake_payload).is_ok() {
                                    println!("E2EE Handshake COMPLETED with peer: {}", p);
                                    let storage = Arc::clone(&self.storage);
                                    let enc_key = self.session_encryption_key;
                                    let session_state = session.get_state();
                                    tokio::spawn(async move {
                                        let _ = NetworkService::persist_session_state(storage, enc_key, p, session_state).await;
                                    });

                                    let mut data = p.to_string().into_bytes();
                                    data.push(0); 
                                    crate::dispatch_global_event(7, &data);
                                    success = true;
                                }
                            }
                            if !success {
                                // If the existing session couldn't process the handshake message
                                // (e.g. because we were in Transport mode or it was a fresh handshake request),
                                // we act as a responder and recreate the session.
                                if let Ok(mut session) = NoiseSession::responder(self.local_static_secret.to_bytes().as_slice()) {
                                    if session.recv_message(&handshake_payload).is_ok() {
                                        if let Ok(response) = session.send_message(&[]) {
                                            println!("E2EE Handshake (New/Re-key) COMPLETED as responder with peer: {}", p);
                                            let storage = Arc::clone(&self.storage);
                                            let enc_key = self.session_encryption_key;
                                            let session_state = session.get_state();
                                            tokio::spawn(async move {
                                                let _ = NetworkService::persist_session_state(storage, enc_key, p, session_state).await;
                                            });

                                            self.noise_sessions.insert(p, session);
                                            let _ = self.forward_to_mesh(p, SignalingPayload::Secure(SecureMessage::Handshake(response)), false).await;
                                        }
                                    }
                                }
                            }
                        }
                        SecureMessage::Transport(encrypted) => {
                            if let Some(session) = self.noise_sessions.get_mut(&p) {
                                match session.recv_message(&encrypted) {
                                    Ok(decrypted) => {
                                        if decrypted.starts_with(b"WEBRTC:") {
                                            if let Ok(signal) = serde_json::from_slice::<WebRtcSignal>(&decrypted[7..]) {
                                                queue.push((p, SignalingPayload::WebRtc(signal), is_wtc));
                                            }
                                        } else {
                                            if let Ok(inner_payload) = serde_json::from_slice::<SignalingPayload>(&decrypted) {
                                                queue.push((p, inner_payload, is_wtc));
                                            } else {
                                                let content_str = String::from_utf8_lossy(&decrypted).into_owned();
                                                let peer_id_str = p.to_string();
                                                let storage = Arc::clone(&self.storage);
                                                tokio::task::spawn_blocking(move || storage.store_message(&peer_id_str, &content_str, false));
                                                let timestamp = chrono::Utc::now().timestamp();
                                                let mut data = timestamp.to_be_bytes().to_vec();
                                                data.push(0); // msg_id_len = 0
                                                data.extend(&decrypted);
                                                crate::dispatch_global_event(2, &data); 
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("❌ Noise decryption FAILED for {}: {:?}", p, e);
                                        self.noise_sessions.remove(&p);
                                        let storage = Arc::clone(&self.storage);
                                        let pid_str = p.to_string();
                                        tokio::task::spawn_blocking(move || {
                                            let _ = storage.delete_session_state(&pid_str);
                                        });
                                        // Request handshake from peer
                                        let tx = self.command_tx.clone();
                                        tokio::spawn(async move {
                                            let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                                                peer_id: p, 
                                                payload: SignalingPayload::RequestHandshake 
                                            }).await;
                                        });
                                    }
                                }
                            } else {
                                println!("[Mesh] Received Transport payload from {} but no active Noise session. Requesting handshake.", p);
                                let tx = self.command_tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                                        peer_id: p, 
                                        payload: SignalingPayload::RequestHandshake 
                                    }).await;
                                });
                            }
                        }
                    }
                }
                SignalingPayload::MailboxStore { recipient_id, payload } => {
                    let is_anchor = self.swarm.behaviour().relay_server.as_ref().is_some() || self.storage.is_anchor_mode_enabled();
                    if !is_anchor {
                        println!("[Mesh] Warning: Received MailboxStore but we are NOT an anchor node. Ignoring.");
                    } else if let Ok(recipient) = recipient_id.parse::<PeerId>() {
                        // --- RELIABILITY FIX: Loopback Protection ---
                        // If we are an anchor and we receive a message for ourselves,
                        // unwrap it and handle it immediately.
                        if recipient == *self.swarm.local_peer_id() {
                            println!("[Mesh] Received MailboxStore for OURSELVES. Routing to local handler.");
                            if let Ok(inner) = serde_json::from_slice::<SignalingPayload>(&payload) {
                                // Recursive push to process the inner signaling (e.g. ChatMessage)
                                queue.push((peer, inner, false));
                            }
                        } else {
                            let _ = self.storage.store_mailbox_payload(&recipient, &peer, payload);
                            
                            // Push notification: wake up offline peer
                            if let Ok(Some((device_type, token))) = self.storage.get_push_token(&recipient_id) {
                                println!("[Mesh] 🔔 Push wake-up for offline peer {} ({})", recipient_id, device_type);
                                let client = reqwest::Client::new();
                                let peer_hash = {
                                    use sha2::{Sha256, Digest};
                                    hex::encode(Sha256::digest(recipient_id.as_bytes()))
                                };
                                let payload_json = serde_json::json!({
                                    "device_type": device_type,
                                    "token": token,
                                    "peer_id_hash": peer_hash
                                });
                                tokio::spawn(async move {
                                    let _ = client.post("https://push.introvert.network/wakeup")
                                        .json(&payload_json)
                                        .timeout(Duration::from_secs(5))
                                        .send()
                                        .await;
                                });
                            }

                            // Push upgrade for other peers...
                            if self.swarm.is_connected(&recipient) {
                                if let Ok(messages) = self.storage.drain_mailbox(&recipient) {
                                    if !messages.is_empty() {
                                        let _ = self.forward_to_mesh(recipient, SignalingPayload::MailboxDrained(messages), false).await;
                                    }
                                }
                            }
                        }
                    }
                }
                SignalingPayload::MailboxDrained(messages) => {
                    let count = messages.len();
                    println!("📦 Drained {} messages from mesh mailbox", count);
                    for msg in messages {
                        if let Ok(sender_peer) = msg.sender_id.parse::<PeerId>() {
                            if let Ok(signaling) = serde_json::from_slice::<SignalingPayload>(&msg.payload) {
                                queue.push((sender_peer, signaling, false));
                            }
                        }
                    }
                    // RECURSIVE DRAIN: If we got messages, there might be more (or our response might trigger a new one)
                    // Wait a tiny bit and fetch again.
                    if count > 0 {
                        let tx = self.command_tx.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            let _ = tx.send(NetworkCommand::FetchMailbox).await;
                        });
                    }
                }
                _ => {
                    self.handle_single_payload(p, pl, is_wtc).await;
                }
            }
        }
    }

    async fn handle_single_payload(&mut self, peer: PeerId, payload: SignalingPayload, _is_webrtc: bool) {


        match payload {
            SignalingPayload::Standard(msg) => {
                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let m = msg.clone();
                tokio::task::spawn_blocking(move || storage.store_message(&peer_id_str, &m, false));
                
                let timestamp = chrono::Utc::now().timestamp();
                let mut data = timestamp.to_be_bytes().to_vec();
                data.push(0); // msg_id_len = 0
                data.extend(msg.as_bytes());
                crate::dispatch_global_event(2, &data);
            }
            SignalingPayload::WebRtc(signal) => {
                match signal.signal_type.as_str() {
                    "offer" => {
                        self.data_channels.write().remove(&peer);
                        let old_pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer) };
                        if let Some(pc) = old_pc {
                            let _ = pc.close().await;
                        }

                        self.pending_offers.insert(peer, signal.sdp.clone());

                        let data = peer.to_string().into_bytes();
                        crate::dispatch_global_event(14, &data);
                    }
                    "answer" => {
                        let pc_opt = self.peer_connections.read().get(&peer).cloned();
                        if let Some(pc) = pc_opt { let _ = MediaManager::handle_answer(signal.sdp, pc).await; }
                    }
                    "reject" => {
                        self.data_channels.write().remove(&peer);
                        let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer) };
                        if let Some(pc) = pc {
                            let _ = pc.close().await;
                        }
                        let data = peer.to_string().into_bytes();
                        crate::dispatch_global_event(16, &data);
                    }
                    _ => {}
                }
            }
            SignalingPayload::Candidate(candidate_json) => {
                let pc_opt = self.peer_connections.read().get(&peer).cloned();
                if let Some(pc) = pc_opt {
                    if let Ok(candidate) = serde_json::from_str::<webrtc::ice_transport::ice_candidate::RTCIceCandidateInit>(&candidate_json) {
                        let _ = pc.add_ice_candidate(candidate).await;
                    }
                }
            }
            SignalingPayload::WebRtcNative(json) => {
                // Dispatch Event 15: flutter_webrtc signal (SDP offer/answer or ICE candidate)
                // Format: [peer_id_len: u8][peer_id_bytes][json_bytes]
                let peer_bytes = peer.to_string().into_bytes();
                let mut data = vec![peer_bytes.len() as u8];
                data.extend_from_slice(&peer_bytes);
                data.extend_from_slice(json.as_bytes());
                crate::dispatch_global_event(15, &data);
            }
            SignalingPayload::ChatMessage { content, msg_id, timestamp, reply_to } => {
                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);

                // Privacy gate: check if peer is a contact (verified via dual handshake)
                let contact = tokio::task::spawn_blocking({
                    let storage = storage.clone();
                    let pid = peer_id_str.clone();
                    move || storage.get_contact(&pid)
                }).await.unwrap_or(Ok(None)).unwrap_or(None);

                if contact.is_none() {
                    println!("[Privacy] Blocked individual ChatMessage from non-contact group peer: {}", peer_id_str);
                    return;
                }

                let c = content.clone();
                let mid = msg_id.clone();
                let rt = reply_to.clone();
                if !self.is_stress_test {
                    tokio::task::spawn_blocking(move || storage.store_message_with_id(&peer_id_str, &mid, &c, false, rt.as_deref()));
                }
                let ack = SignalingPayload::Acknowledgement { msg_id: msg_id.clone(), status: 1 };
                let _ = self.forward_to_mesh(peer, ack, false).await;

                // Pack [timestamp, msg_id_len, msg_id, reply_to_len, reply_to, content] for UI
                let mut data = timestamp.to_be_bytes().to_vec();
                let msg_id_bytes = msg_id.as_bytes();
                data.push(msg_id_bytes.len() as u8);
                data.extend(msg_id_bytes);

                let rt_bytes = reply_to.as_ref().map(|s| s.as_bytes()).unwrap_or(&[]);
                data.push(rt_bytes.len() as u8);
                data.extend(rt_bytes);

                data.extend(content.as_bytes());
                crate::dispatch_global_event(2, &data);
            }
            SignalingPayload::FileChunkRequest { transfer_id, chunk_index, chunk_size } => {
                println!("[Mesh] Received chunk request for {} (index {}) from {}", transfer_id, chunk_index, peer);
                
                // 1. Try active seeder first (session-specific)
                let seeder_info = self.active_seeders.get(&transfer_id).map(|s| {
                    (s.file_path.clone(), s.chunk_size, s.total_chunks, s.file_hash.clone(), s.group_id.clone())
                }).or_else(|| {
                    // Robust fallback: if exact transfer_id not found, find ANY seeder for the same hash
                    // Extract hash from transfer_id if it follows the gft_{hash}_{ts} pattern
                    let parts: Vec<&str> = transfer_id.split('_').collect();
                    if parts.len() >= 2 && parts[0] == "gft" {
                        let hash = parts[1];
                        self.active_seeders.values().find(|s| s.file_hash == hash).map(|s| {
                            (s.file_path.clone(), s.chunk_size, s.total_chunks, s.file_hash.clone(), s.group_id.clone())
                        })
                    } else {
                        None
                    }
                });

                let (path, csize, tchunks, f_hash, grp_id) = if let Some(info) = seeder_info {
                    // Use requested chunk_size if provided, otherwise fallback to seeder's registered chunk_size
                    let requested_csize = chunk_size.unwrap_or(info.1);
                    let size = std::fs::metadata(&info.0).map(|m| m.len()).unwrap_or(0) as usize;
                    let tchunks = (size as f32 / requested_csize as f32).ceil() as u32;
                    (info.0, requested_csize, tchunks, info.3, info.4)
                } else {
                    // 2. FALLBACK: Check if we have this file in our Sovereign Drive (persistent seeding)
                    let storage = self.storage.clone();
                    let tid = transfer_id.clone();
                    
                    let drive_file = tokio::task::spawn_blocking(move || {
                        // Attempt indexed hash query first by extracting hash from gft_{hash}_{ts}
                        let mut hash_opt = None;
                        let parts: Vec<&str> = tid.split('_').collect();
                        if parts.len() >= 2 && parts[0] == "gft" {
                            hash_opt = Some(parts[1].to_string());
                        }

                        if let Some(ref hash) = hash_opt {
                            if let Ok(Some(file)) = storage.get_drive_file_by_hash(hash) {
                                println!("[Mesh] Fallback seeder matched DB record by hash for {}: {:?}", tid, file.filename);
                                return Some(file);
                            }
                        }

                        let files = match storage.get_all_drive_files() {
                            Ok(f) => f,
                            Err(e) => {
                                println!("[Mesh] ❌ Fallback seeder DB error: {}", e);
                                return None;
                            }
                        };
                        println!("[Mesh] Fallback seeder checking {} drive files for transfer_id: {}", files.len(), tid);
                        files.into_iter().find(|f| {
                            let h_low = f.file_hash.to_lowercase();
                            let tid_low = tid.to_lowercase();
                            // Robust hash matching
                            if tid_low.contains(&h_low) || h_low.contains(&tid_low) || (h_low.len() > 10 && tid_low.contains(&h_low[..10])) {
                                println!("[Mesh] Fallback seeder matched DB record for {}: {:?}", tid, f.filename);
                                true
                            } else {
                                false
                            }
                        })
                    }).await.unwrap_or(None);

                    if let Some(file) = drive_file {
                        let path = file.local_path.clone();
                        let size = file.total_size;
                        let hash = file.file_hash.clone();
                        
                        let requested_csize = chunk_size.unwrap_or(64 * 1024);
                        let tchunks = (size as f32 / requested_csize as f32).ceil() as u32;
                        
                        if path.is_empty() || !std::path::Path::new(&path).exists() {
                            println!("[Mesh] ❌ Fallback seeder found drive record but file is missing on disk: {}", path);
                            return;
                        }

                        // Register seeder dynamically in active_seeders so subsequent chunks don't hit the DB
                        self.active_seeders.insert(transfer_id.clone(), ActiveSeeder {
                            peer_id: peer,
                            file_path: path.clone(),
                            file_hash: hash.clone(),
                            chunk_size: requested_csize,
                            total_chunks: tchunks,
                            _bytes_sent: 0,
                            _start_time: Instant::now(),
                            group_id: None,
                            completions: HashSet::new(),
                        });

                        // We do not have group context for fallback drive files, so None
                        (path, requested_csize, tchunks, hash, None)
                    } else {
                        println!("[Mesh] ❌ Rejected chunk request: No seeder or drive file found for {}", transfer_id);
                        return;
                    }
                };

                let tx = self.command_tx.clone();
                let tid = transfer_id.clone();
                let p_id = peer;
                let p_path = std::path::Path::new(&path);
                let filename = p_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
                let ext = p_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                let mime_type = match ext.as_str() {
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" => "image/".to_owned() + &ext,
                    "mp4" | "mov" | "avi" | "mkv" | "webm" => "video/".to_owned() + &ext,
                    "pdf" => "application/pdf".to_string(),
                    "txt" => "text/plain".to_string(),
                    _ => "application/octet-stream".to_string(),
                };

                tokio::spawn(async move {
                    if let Ok(mut file) = std::fs::File::open(&path) {
                        use std::io::Seek;
                        if file.seek(std::io::SeekFrom::Start((chunk_index * csize) as u64)).is_ok() {
                            let mut b = vec![0u8; csize as usize];
                            if let Ok(n) = file.read(&mut b) {
                                b.truncate(n);
                                let p = SignalingPayload::FileChunk { transfer_id: tid.clone(), chunk_index, total_chunks: tchunks, data_base64: general_purpose::STANDARD.encode(&b) };
                                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: p_id, payload: p }).await;

                                // Update Sender UI with upload metadata
                                let progress = FileTransferProgress {
                                    transfer_id: tid,
                                    peer_id: p_id.to_string(),
                                    filename,
                                    mime_type,
                                    file_hash: f_hash,
                                    progress: (chunk_index as f32 + 1.0) / tchunks as f32,
                                    is_complete: chunk_index + 1 == tchunks,
                                    is_verified: true,
                                    is_outgoing: true,
                                    local_path: Some(path),
                                    start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                                    speed_bps: 0.0,
                                    group_id: grp_id,
                                };

                                crate::dispatch_global_event(12, &serde_json::to_vec(&progress).unwrap_or_default());
                            }
                        }
                    }
                });
            }
            SignalingPayload::RequestHandshake => {
                println!("[Mesh] Received RequestHandshake from {}. Clearing session and initiating new handshake.", peer);
                self.noise_sessions.remove(&peer);
                let storage = Arc::clone(&self.storage);
                let pid_str = peer.to_string();
                tokio::task::spawn_blocking(move || {
                    let _ = storage.delete_session_state(&pid_str);
                });
                let tx = self.command_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(NetworkCommand::EstablishSecureSession { peer_id: peer }).await;
                });
            }
            SignalingPayload::ProfileRequest => {
                println!("[Mesh] Received ProfileRequest from {}", peer);
                if let Ok(Some(profile)) = self.storage.get_profile() {
                    let (name, handle, avatar, _) = profile;
                    let response = SignalingPayload::ProfileResponse {
                        name: name.unwrap_or_else(|| "Unknown".to_string()),
                        handle: handle.unwrap_or_else(|| "".to_string()),
                        avatar_base64: avatar,
                    };
                    let _ = self.forward_to_mesh(peer, response, false).await;
                }
            }
            SignalingPayload::ProfileResponse { name, handle, avatar_base64 } => {
                println!("[Mesh] Received ProfileResponse from {}: {} ({})", peer, name, handle);
                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let n = name.clone();
                let a = avatar_base64.clone();
                let peer_id_clone = peer_id_str.clone();
                
                tokio::task::spawn_blocking(move || {
                    // 1. Update contacts if they exist
                    if let Ok(Some(mut contact)) = storage.get_contact(&peer_id_clone) {
                        contact.global_name = Some(n.clone());
                        contact.avatar_base64 = a.clone();
                        let alias_is_empty_or_id = contact.local_alias.as_deref().map_or(true, |a| a.is_empty() || a == peer_id_clone);
                        if alias_is_empty_or_id {
                            contact.local_alias = Some(n.clone());
                        }
                        let (v, inc) = storage.get_contact_status(&peer_id_clone).ok().flatten().unwrap_or((true, false));
                        let _ = storage.upsert_sovereign_contact(&contact, v, inc);
                    }
                    
                    // 2. Update group member info across all groups
                    let _ = storage.update_group_member_profile(&peer_id_clone, &n, a.as_deref());
                });

                // Dispatch event 25: PeerProfileUpdated
                // Data format: [pid_len, pid_bytes, name_len, name_bytes, handle_len, handle_bytes, avatar_len(4), avatar_bytes]
                let mut data = vec![peer_id_str.len() as u8];
                data.extend(peer_id_str.as_bytes());
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.push(handle.len() as u8);
                data.extend(handle.as_bytes());
                
                let avatar_bytes = avatar_base64.as_deref().unwrap_or("").as_bytes();
                data.extend(&(avatar_bytes.len() as u32).to_be_bytes());
                data.extend(avatar_bytes);

                crate::dispatch_global_event(25, &data);
            }
            SignalingPayload::ChatSyncRequest { chat_id, is_group, last_msg_id, last_timestamp, limit } => {
                println!("[Mesh] Received ChatSyncRequest from {} for chat {} (group={}, after ts={})", peer, chat_id, is_group, last_timestamp);
                let storage = Arc::clone(&self.storage);
                let chat_id_c = chat_id.clone();
                let is_group_c = is_group;
                let last_ts_c = last_timestamp;
                let limit_c = limit as usize;
                let last_mid = last_msg_id.clone();

                let messages = tokio::task::spawn_blocking(move || {
                    let mut result = Vec::new();
                    if is_group_c {
                        if let Ok(msgs) = storage.get_group_messages(&chat_id_c) {
                            for m in msgs {
                                // m: (msg_id, sender_id, content, timestamp, reply_to_msg_id)
                                if let Ok(ts) = m.3.parse::<i64>() {
                                    if ts > last_ts_c {
                                        result.push(SyncMessage {
                                            msg_id: m.0,
                                            sender_id: m.1,
                                            content: m.2,
                                            timestamp: m.3,
                                            reply_to: m.4,
                                        });
                                    }
                                }
                            }
                        }
                    } else {
                        if let Ok(msgs) = storage.get_messages_for_peer(&chat_id_c) {
                            for m in msgs {
                                // m: (content, timestamp, is_me, status, msg_id, reply_to_msg_id)
                                if let Ok(ts) = m.1.parse::<i64>() {
                                    if ts > last_ts_c {
                                        let is_me_str = if m.2 { "self" } else { "peer" };
                                        result.push(SyncMessage {
                                            msg_id: m.4.unwrap_or_default(),
                                            sender_id: is_me_str.to_string(),
                                            content: m.0,
                                            timestamp: m.1,
                                            reply_to: m.5,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    result
                }).await.unwrap_or_default();

                let has_more = messages.len() > limit_c;
                let truncated = if has_more { messages[..limit_c].to_vec() } else { messages };

                println!("[Mesh] Sending {} sync messages to {}", truncated.len(), peer);
                let response = SignalingPayload::ChatSyncResponse {
                    chat_id,
                    is_group,
                    messages: truncated,
                    has_more,
                };
                let _ = self.forward_to_mesh(peer, response, false).await;
            }
            SignalingPayload::ChatSyncResponse { chat_id, is_group, messages, has_more } => {
                println!("[Mesh] Received ChatSyncResponse for {} with {} messages (has_more={})", chat_id, messages.len(), has_more);
                let storage = Arc::clone(&self.storage);
                let chat_id_c = chat_id.clone();
                let is_group_c = is_group;
                let peer_id_str = peer.to_string();

                tokio::task::spawn_blocking(move || {
                    for msg in messages {
                        if is_group_c {
                            let _ = storage.store_group_message(&chat_id_c, &msg.sender_id, &msg.msg_id, &msg.content, false, msg.reply_to.as_deref());
                        } else {
                            let is_me = msg.sender_id == "self";
                            let _ = storage.store_message_with_id(&chat_id_c, &msg.msg_id, &msg.content, is_me, msg.reply_to.as_deref());
                        }
                    }
                });

                // Dispatch event to refresh chat UI
                if is_group {
                    crate::dispatch_global_event(23, chat_id.as_bytes()); // Event 23: group updated
                } else {
                    let mut data = vec![peer_id_str.len() as u8];
                    data.extend(peer_id_str.as_bytes());
                    data.extend(0i64.to_be_bytes()); // timestamp placeholder
                    data.push(0u8); // msg_id_len = 0 (no specific message)
                    crate::dispatch_global_event(2, &data); // Event 2: new message
                }
            }
            SignalingPayload::FileTransfer { transfer_id, filename, mime_type, file_hash, total_size, is_relayed, sender_peer_id, group_id } => {
                // BUG 3 FIX: Use the actual sender's peer ID if provided, otherwise fallback to the anchor peer
                let actual_seeder_peer = if let Some(sid) = &sender_peer_id {
                    sid.parse::<PeerId>().unwrap_or(peer)
                } else {
                    peer
                };

                // LOOPBACK PROTECTION: If we are the sender of this file transfer manifest (gossiped back to us in a group chat), ignore it.
                if actual_seeder_peer == *self.swarm.local_peer_id() {
                    println!("[Mesh] Loopback FileTransfer manifest detected for transfer_id={}. Ignoring.", transfer_id);
                    return;
                }

                // Prioritize direct P2P connection: prevent broadcast manifest from demoting to relayed
                let is_connected_now = self.swarm.is_connected(&actual_seeder_peer);
                let relayed_map_snapshot = self.is_relayed_map.read().get(&actual_seeder_peer).cloned();
                let is_direct_p2p = is_connected_now && !relayed_map_snapshot.unwrap_or(false);
                let final_is_relayed = if is_direct_p2p { false } else { is_relayed };

                let chunk_size = if final_is_relayed { 64 * 1024 } else { 256 * 1024 };
                let total_chunks = (total_size as f32 / chunk_size as f32).ceil() as u32;

                // DEDUPLICATION: If we already have an active transfer for this file_hash (regardless
                // of transfer_id), merge the new sender as a provider into the existing transfer.
                // This prevents two parallel transfers of the same file from competing for seeder
                // bandwidth and causing the watchdog stall loop.
                let already_active_tid = self.incoming_transfers
                    .iter()
                    .find(|(tid, t)| t.file_hash == file_hash && t.group_id == group_id && *tid != &transfer_id)
                    .map(|(tid, _)| tid.clone());
                if let Some(existing_tid) = already_active_tid {
                    if let Some(existing) = self.incoming_transfers.get_mut(&existing_tid) {
                        if !existing.providers.contains(&actual_seeder_peer) {
                            existing.providers.push(actual_seeder_peer);
                            println!("[Mesh] Dedup: merged seeder {} into existing transfer {} (duplicate transfer_id={} ignored)",
                                actual_seeder_peer, existing_tid, transfer_id);
                        }
                    }
                    return;
                }

                let mut is_update = false;
                if let Some(existing) = self.incoming_transfers.get_mut(&transfer_id) {
                    println!("[Mesh] FileTransfer manifest update for existing transfer {}. Updating config and preserving progress.", transfer_id);
                    is_update = true;
                    let was_relayed = existing.is_relayed;
                    
                    existing.is_relayed = final_is_relayed;

                    if !existing.providers.contains(&actual_seeder_peer) {
                        existing.providers.push(actual_seeder_peer);
                    }
                    existing.last_update = Instant::now();
                    
                    // If it transitioned to relayed now, start the pull sequence from current progress
                    if final_is_relayed && !was_relayed {
                        let mut next = 0u32;
                        while existing.received_chunks.contains_key(&next) { next += 1; }
                        let limit = if existing.total_chunks > 0 {
                            std::cmp::min(next + 4, existing.total_chunks)
                        } else {
                            next + 4
                        };
                        existing.next_pull_idx = limit;
                        
                        println!("[Mesh] Transitioned to relay mode. Initiating primed pull sequence for chunks {}..{}", next, limit - 1);
                        let tx = self.command_tx.clone();
                        let tid = transfer_id.clone();
                        let selected_providers = Self::select_best_providers_static(&self.swarm, &self.is_relayed_map, &existing.providers);
                        let csize = existing.chunk_size;
                        tokio::spawn(async move {
                            for idx in next..limit {
                                let target_peer = if !selected_providers.is_empty() {
                                    selected_providers[(idx as usize) % selected_providers.len()]
                                } else {
                                    actual_seeder_peer
                                };
                                let req = SignalingPayload::FileChunkRequest { transfer_id: tid.clone(), chunk_index: idx, chunk_size: Some(csize) };
                                let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: target_peer, payload: req }).await;
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        });
                    }
                } else {
                    self.incoming_transfers.insert(transfer_id.clone(), IncomingTransfer {
                        filename: filename.clone(),
                        mime_type: mime_type.clone(),
                        file_hash: file_hash.clone(),
                        total_size,
                        total_chunks,
                        received_chunks: HashMap::new(),
                        peer_id: actual_seeder_peer,
                        providers: vec![actual_seeder_peer],
                        start_time: Instant::now(),
                        last_update: Instant::now(),
                        is_relayed: final_is_relayed,
                        group_id: group_id.clone(),
                        next_pull_idx: 4,
                        chunk_size,
                        stall_chunk_count: 0,
                    });
                }

                // SOVEREIGN SWARM: If this is a relayed (cross-network) transfer,
                // trigger a DHT search to find other providers/seeders for this file.
                if final_is_relayed {
                    let tx = self.command_tx.clone();
                    let hash = file_hash.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(NetworkCommand::FindProviders { file_hash: hash }).await;
                    });
                }

                if !is_update {
                    let progress = FileTransferProgress { 
                        transfer_id: transfer_id.clone(), 
                        peer_id: actual_seeder_peer.to_string(), 
                        filename: filename.clone(), 
                        mime_type: mime_type.clone(),
                        file_hash: file_hash.clone(),
                        progress: 0.0, 
                        is_complete: false, 
                        is_verified: false,
                        is_outgoing: false, 
                        local_path: None,
                        start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                        speed_bps: 0.0,
                        group_id: group_id.clone(),
                    };
                    let peer_id_str = actual_seeder_peer.to_string();
                    let storage = Arc::clone(&self.storage);
                    let mid = transfer_id.clone();
                    if let Some(ref gid) = group_id {
                        let gid_clone = gid.clone();
                        if let Ok(json_str) = serde_json::to_string(&progress) {
                            let c = format!("[FILE]:{}", json_str);
                            tokio::task::spawn_blocking(move || {
                                let _ = storage.store_group_message(&gid_clone, &peer_id_str, &mid, &c, false, None);
                            });
                        }
                    } else {
                        if let Ok(json_str) = serde_json::to_string(&progress) {
                            let c = format!("[FILE]:{}", json_str);
                            tokio::task::spawn_blocking(move || {
                                let _ = storage.store_message_with_id(&peer_id_str, &mid, &c, false, None);
                            });
                        }
                    }
                    let data = serde_json::to_vec(&progress).unwrap_or_default();
                    crate::dispatch_global_event(12, &data);

                    // Start pulling chunks ONLY if the sender is not pushing them directly.
                    // final_is_relayed=false means the sender will PUSH 256KB chunks directly to us.
                    // final_is_relayed=true means we must PULL chunks via FileChunkRequest.
                    if final_is_relayed {
                        let is_direct = self.swarm.is_connected(&actual_seeder_peer)
                            && self.is_relayed_map.read().get(&actual_seeder_peer).cloned() == Some(false);
                        let initial_pipeline = if is_direct { 8 } else { 4 };
                        let pacing_delay = if is_direct { 10 } else { 50 };

                        println!("[Mesh] Relay/Pull transfer detected. Initiating primed pull sequence ({} deep) for {}", initial_pipeline, transfer_id);
                        let tx = self.command_tx.clone();
                        let tid = transfer_id.clone();
                        let total_chunks_val = total_chunks;
                        let csize = chunk_size;
                        tokio::spawn(async move {
                            for i in 0..initial_pipeline {
                                if i < total_chunks_val {
                                    let req = SignalingPayload::FileChunkRequest { transfer_id: tid.clone(), chunk_index: i, chunk_size: Some(csize) };
                                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: actual_seeder_peer, payload: req }).await;
                                    tokio::time::sleep(Duration::from_millis(pacing_delay)).await;
                                }
                            }
                        });
                    } else {
                        // Direct push path: sender is pushing 256KB chunks to us.
                        // The 4-second watchdog stall recovery will automatically switch to
                        // pull mode (setting is_relayed=true) if chunks stop arriving,
                        // e.g. if the direct connection fails or the sender can't reach us.
                        println!("[Mesh] Direct push transfer detected. Waiting for 256KB chunks from sender for {}", transfer_id);
                    }
                }
            }
            SignalingPayload::FileChunk { transfer_id, chunk_index, total_chunks, data_base64 } => {
                println!("[Mesh] Received chunk {}/{} for {}", chunk_index, total_chunks, transfer_id);
                self.handle_file_chunk(peer, transfer_id, chunk_index, total_chunks, data_base64).await;
            }
            SignalingPayload::GroupManifestRequest { group_id, alias, avatar, handle } => {
                if let Ok(Some(group)) = self.storage.get_group(&group_id) {
                    let members: Vec<GroupMemberMetadata> = serde_json::from_str(&group.members_json).unwrap_or_default();
                    let requester_peer_id = peer.to_string();

                    if members.iter().any(|m| m.peer_id == requester_peer_id) {
                        // Already a member: return manifest immediately
                        let payload = SignalingPayload::GroupManifest {
                            group_id,
                            name: group.name,
                            description: group.description,
                            members,
                            secret: group.secret,
                        };
                        let _ = self.forward_to_mesh(peer, payload, false).await;
                    } else {
                        // Not a member: trigger admin approval notification (Event 26)
                        let my_peer_id = self.swarm.local_peer_id().to_string();
                        let is_admin = members.iter().any(|m| m.peer_id == my_peer_id && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                        if is_admin {
                            println!("[Mesh] Group join request from {} for group {}", requester_peer_id, group_id);
                            let mut data = group_id.clone().into_bytes();
                            data.push(0);
                            data.extend(requester_peer_id.as_bytes());
                            data.push(0);
                            data.extend(alias.clone().unwrap_or_default().as_bytes());
                            data.push(0);
                            data.extend(handle.clone().unwrap_or_default().as_bytes());
                            data.push(0);
                            data.extend(avatar.clone().unwrap_or_default().as_bytes());
                            crate::dispatch_global_event(26, &data);
                        }
                    }
                }
            }
            SignalingPayload::GroupJoinRejected { group_id, group_name, reason } => {
                println!("[Mesh] Group join request rejected for {}: {}", group_name, reason);
                let mut data = group_id.into_bytes();
                data.push(0);
                data.extend(group_name.as_bytes());
                data.push(0);
                data.extend(reason.as_bytes());
                crate::dispatch_global_event(27, &data);
            }
            SignalingPayload::GroupInvite { group_id, name, description, inviter_peer_id, group_secret_wrapped, members } => {
                println!("[Mesh] Received GroupInvite for group: {} from {}", name, inviter_peer_id);
                // Subscribe to Gossipsub topic for this group immediately to start receiving mesh traffic
                let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    eprintln!("[Mesh] Failed to subscribe to gossipsub topic for invited group {}: {:?}", group_id, e);
                }
                // Privacy-First: Store as pending invite — user must explicitly accept
                let members_json = serde_json::to_string(&members).unwrap_or_default();
                let pending = crate::storage::PendingGroupInvite {
                    group_id: group_id.clone(),
                    name: name.clone(),
                    description: description.clone(),
                    inviter_peer_id: inviter_peer_id.clone(),
                    group_secret_wrapped,
                    members_json,
                };
                let _ = self.storage.store_pending_invite(&pending);
                
                // Dispatch event 24: GroupInvitePending
                // Payload: [inviter_id_len, inviter_id_bytes, group_name_len, group_name_bytes, group_id_bytes]
                let mut data = vec![inviter_peer_id.len() as u8];
                data.extend(inviter_peer_id.as_bytes());
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend(group_id.as_bytes());
                crate::dispatch_global_event(24, &data);
            }
            SignalingPayload::GroupAction(signed_action) => {
                let members_json_res = self.storage.get_group_members(&signed_action.group_id);
                if let Ok(Some(members_json)) = members_json_res {
                    let members: Vec<GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                    match group::GroupManager::verify_action(&signed_action, &members) {
                        Ok(true) => {
                            match signed_action.action {
                                GroupAction::Message { ref content_encrypted, ref msg_id, ref reply_to } => {
                                    if let Ok(Some(group_info)) = self.storage.get_group(&signed_action.group_id) {
                                        use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                                        if content_encrypted.len() >= 12 {
                                            let nonce = Nonce::from_slice(&content_encrypted[0..12]);
                                            let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_info.secret));
                                            if let Ok(decrypted) = cipher.decrypt(nonce, &content_encrypted[12..]) {
                                                let content = String::from_utf8_lossy(&decrypted).into_owned();
                                                let is_me = signed_action.signer_peer_id == self.swarm.local_peer_id().to_string();
                                                let rt = reply_to.clone();
                                                if !self.is_stress_test {
                                                    let _ = self.storage.store_group_message(&signed_action.group_id, &signed_action.signer_peer_id, &msg_id, &content, is_me, rt.as_deref());
                                                }
                                                
                                                let mut event_data = vec![signed_action.group_id.len() as u8];
                                                event_data.extend(signed_action.group_id.as_bytes());
                                                event_data.push(signed_action.signer_peer_id.len() as u8);
                                                event_data.extend(signed_action.signer_peer_id.as_bytes());
                                                
                                                let rt_bytes = reply_to.as_ref().map(|s| s.as_bytes()).unwrap_or(&[]);
                                                event_data.push(rt_bytes.len() as u8);
                                                event_data.extend(rt_bytes);

                                                event_data.extend(content.as_bytes());
                                                
                                                // SOVEREIGN HYBRID RELIABILITY: 
                                                // If we are an anchor node, store this gossip message in the mailbox for all other group members.
                                                // This ensures that members who are currently offline will receive the message when they reconnect.
                                                if self.storage.is_anchor_mode_enabled() {
                                                    let storage_m = self.storage.clone();
                                                    let gid_m = signed_action.group_id.clone();
                                                    let payload_m = SignalingPayload::GroupAction(signed_action.clone());
                                                    let my_id_str = self.swarm.local_peer_id().to_string();
                                                    let tx_m = self.command_tx.clone();
                                                    
                                                    tokio::spawn(async move {
                                                        if let Ok(Some(members_json)) = storage_m.get_group_members(&gid_m) {
                                                            let members: Vec<GroupMemberMetadata> = serde_json::from_str(&members_json).unwrap_or_default();
                                                            for m in members {
                                                                if m.peer_id != my_id_str {
                                                                    if let Ok(m_pid) = m.peer_id.parse::<PeerId>() {
                                                                        // Push to RBN Mailbox for offline members
                                                                        let _ = tx_m.send(NetworkCommand::StoreInMailbox { 
                                                                            peer_id: m_pid, 
                                                                            payload: payload_m.clone() 
                                                                        }).await;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    });
                                                }

                                                // Anchor Auto-Pull: If we are an anchor node, automatically pull group media
                                                // to ensure it's available even if the seeder goes offline.
                                                if self.storage.is_anchor_mode_enabled() && content.starts_with("[FILE]:") {
                                                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content[7..]) {
                                                        let tid = meta.get("transfer_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                        let filename = meta.get("filename").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                                                        let mime_type = meta.get("mime_type").and_then(|v| v.as_str()).unwrap_or("application/octet-stream").to_string();
                                                        let file_hash = meta.get("file_hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                                        let total_size = meta.get("total_size").and_then(|v| v.as_u64()).unwrap_or(0);
                                                        
                                                        if !tid.is_empty() && !file_hash.is_empty() {
                                                            let tx = self.command_tx.clone();
                                                            let sid = signed_action.signer_peer_id.clone();
                                                            let gid = signed_action.group_id.clone();
                                                            let tid_clone = tid.clone();
                                                            
                                                            tokio::spawn(async move {
                                                                println!("[Registry] Anchor Auto-Pull: Initiating mesh cache for {} from {}", tid_clone, sid);
                                                                let payload = SignalingPayload::FileTransfer {
                                                                    transfer_id: tid_clone,
                                                                    filename,
                                                                    mime_type,
                                                                    file_hash,
                                                                    total_size: total_size as usize,
                                                                    is_relayed: true,
                                                                    sender_peer_id: Some(sid),
                                                                    group_id: Some(gid),
                                                                };
                                                                // Forward to ourselves to trigger the pull logic in handle_single_payload
                                                                let _ = tx.send(NetworkCommand::HandleIncomingPayload { 
                                                                    peer_id: PeerId::random(), // Dummy peer for local trigger
                                                                    payload 
                                                                }).await;
                                                            });
                                                        }
                                                    }
                                                }

                                                crate::dispatch_global_event(21, &event_data);
                                            }
                                        }
                                    }
                                },
                            GroupAction::AddMember { metadata } => {
                                let mut members = members;
                                if !members.iter().any(|m| m.peer_id == metadata.peer_id) {
                                    members.push(metadata.clone());
                                    let members_json = serde_json::to_string(&members).unwrap_or_default();
                                    let _ = self.storage.update_group_members(&signed_action.group_id, &members_json);
                                    crate::dispatch_global_event(23, signed_action.group_id.as_bytes());

                                    // Dial the new member proactively
                                    if let Ok(pid) = metadata.peer_id.parse::<PeerId>() {
                                        if pid != *self.swarm.local_peer_id() {
                                            println!("[Mesh] Proactively dialing NEW group member: {}", pid);
                                            self.dial_relay_path(pid);
                                        }
                                    }
                                }
                            }
                            GroupAction::RemoveMember { peer_id } => {
                                let mut members = members;
                                if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                                    members.remove(pos);
                                    let members_json = serde_json::to_string(&members).unwrap_or_default();
                                    let _ = self.storage.update_group_members(&signed_action.group_id, &members_json);
                                    if peer_id == self.swarm.local_peer_id().to_string() {
                                        let _ = self.storage.delete_group(&signed_action.group_id);
                                        crate::dispatch_global_event(22, signed_action.group_id.as_bytes());
                                    } else {
                                        crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                    }
                                }
                            }
                            GroupAction::UpdateRole { peer_id, new_role } => {
                                let mut members = members;
                                if let Some(pos) = members.iter().position(|m| m.peer_id == peer_id) {
                                    members[pos].role = new_role;
                                    let members_json = serde_json::to_string(&members).unwrap_or_default();
                                    let _ = self.storage.update_group_members(&signed_action.group_id, &members_json);
                                    crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                }
                            }
                            GroupAction::DeleteGroup => {
                                let _ = self.storage.delete_group(&signed_action.group_id);
                                crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                            },
                            GroupAction::Reaction { msg_id, emoji } => {
                                let sid = signed_action.signer_peer_id.clone();
                                let storage = Arc::clone(&self.storage);
                                let mid = msg_id.clone();
                                let em = emoji.clone();
                                if !self.is_stress_test {
                                    tokio::task::spawn_blocking(move || storage.add_message_reaction(&mid, &sid, &em));
                                }

                                // Pack [msg_id_len, msg_id, emoji] for UI (Event 35)
                                let mut data = vec![msg_id.len() as u8];
                                data.extend(msg_id.as_bytes());
                                data.extend(emoji.as_bytes());
                                crate::dispatch_global_event(35, &data);
                            }
                            GroupAction::SetRetention { seconds } => {
                                let signer = signed_action.signer_peer_id.clone();
                                let is_admin = members.iter().any(|m| m.peer_id == signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                if is_admin {
                                    let _ = self.storage.set_group_retention(&signed_action.group_id, seconds);
                                    crate::dispatch_global_event(23, signed_action.group_id.as_bytes()); // Refresh UI
                                }
                            }
                            GroupAction::DeleteMessage { msg_id } => {
                                // Allow if it's the sender's own message, or if signer is admin
                                let _signer = signed_action.signer_peer_id.clone();
                                let _is_admin = members.iter().any(|m| m.peer_id == _signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                // In a real implementation, we'd verify the message sender from DB.
                                // For now, we trust the signature and either the user is admin or sender.
                                // Assuming validation passed or we let the client side dictate if they can send it.
                                let _ = self.storage.delete_message(&msg_id, true, _is_admin);

                                crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                            }
                            GroupAction::EditMessage { msg_id, new_content_encrypted } => {
                                if let Ok(Some(group_info)) = self.storage.get_group(&signed_action.group_id) {
                                    use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                                    if new_content_encrypted.len() >= 12 {
                                        let nonce = Nonce::from_slice(&new_content_encrypted[0..12]);
                                        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_info.secret));
                                        if let Ok(decrypted) = cipher.decrypt(nonce, &new_content_encrypted[12..]) {
                                            let new_content = String::from_utf8_lossy(&decrypted).into_owned();
                                            let _ = self.storage.edit_message(&msg_id, &new_content, true);
                                            crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                        }
                                    }
                                }
                            }
                            GroupAction::MuteMember { peer_id } => {
                                let signer = signed_action.signer_peer_id.clone();
                                let is_admin = members.iter().any(|m| m.peer_id == signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                if is_admin {
                                    if let Ok(mut muted) = self.storage.get_group_muted_members(&signed_action.group_id) {
                                        if !muted.contains(&peer_id) {
                                            muted.push(peer_id);
                                            let json = serde_json::to_string(&muted).unwrap_or_default();
                                            let _ = self.storage.update_group_muted_members(&signed_action.group_id, &json);
                                            crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                        }
                                    }
                                }
                            }
                            GroupAction::UnmuteMember { peer_id } => {
                                let signer = signed_action.signer_peer_id.clone();
                                let is_admin = members.iter().any(|m| m.peer_id == signer && (m.role == GroupRole::Creator || m.role == GroupRole::Admin));
                                if is_admin {
                                    if let Ok(mut muted) = self.storage.get_group_muted_members(&signed_action.group_id) {
                                        if let Some(pos) = muted.iter().position(|id| id == &peer_id) {
                                            muted.remove(pos);
                                            let json = serde_json::to_string(&muted).unwrap_or_default();
                                            let _ = self.storage.update_group_muted_members(&signed_action.group_id, &json);
                                            crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
                                        }
                                    }
                                }
                            }

                            }

                    }
                    Ok(false) => {
                        eprintln!("[Mesh] ❌ GroupAction signature verification failed for signer {}", signed_action.signer_peer_id);
                    }
                    Err(e) => {
                        eprintln!("[Mesh] ❌ GroupAction verification error for signer {}: {:?}", signed_action.signer_peer_id, e);
                    }
                }
                }
            }
            SignalingPayload::GroupManifest { group_id, name, description, members, secret } => {
                let my_peer_id = self.swarm.local_peer_id().to_string();
                let is_member = members.iter().any(|m| m.peer_id == my_peer_id);
                
                if !is_member {
                    // If we are not in the manifest, we definitely shouldn't have this group.
                    // If we had it, delete it.
                    if let Ok(Some(_)) = self.storage.get_group(&group_id) {
                        println!("[Mesh] Removing group {} as we are no longer members in the received manifest", group_id);
                        let _ = self.storage.delete_group(&group_id);
                        crate::dispatch_global_event(22, group_id.as_bytes());
                    }
                    return;
                }

                // Subscribe to Gossipsub topic for this group
                let topic = libp2p::gossipsub::IdentTopic::new(group_id.clone());
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    eprintln!("[Mesh] Failed to subscribe to gossipsub topic {}: {:?}", group_id, e);
                } else {
                    println!("[Mesh] Dynamically subscribed to gossipsub topic {}", group_id);
                }

                if self.storage.is_group_deleted(&group_id) {
                    println!("[Mesh] Ignoring manifest for deleted group {}", group_id);
                    return;
                }

                let _ = self.storage.save_group_secret(&group_id, &secret);
                let members_json = serde_json::to_string(&members).unwrap_or_default();
                let _ = self.storage.upsert_group(&group_id, &name, &description, &members_json);
                crate::dispatch_global_event(23, group_id.as_bytes());
                
                let mut data = group_id.clone().into_bytes();
                data.push(0);
                data.extend(name.as_bytes());
                data.push(0);
                data.extend(members_json.as_bytes());
                data.push(0);
                data.extend(&secret);
                crate::dispatch_global_event(20, &data);

                // --- RELIABILITY FIX: Proactive Member Discovery ---
                let my_peer_id = self.swarm.local_peer_id().to_string();
                for m in members {
                    if m.peer_id == my_peer_id { continue; }
                    if let Ok(pid) = m.peer_id.parse::<PeerId>() {
                        println!("[Mesh] Proactively dialing group member {} from manifest {}", pid, name);
                        self.dial_relay_path(pid);
                    }
                }
            }
            SignalingPayload::FileTransferComplete { transfer_id } => {
                let mut local_path = None;
                let mut filename = "".to_string();
                let mut mime_type = "".to_string();
                let mut is_group_transfer = false;
                
                let mut f_hash = "".to_string();
                let mut grp_id = None;
                let mut total_members = 0;
                let mut current_completions = 0;

                if let Some(seeder) = self.active_seeders.get_mut(&transfer_id) {
                    seeder.completions.insert(peer);
                    
                    let s_path = seeder.file_path.clone();
                    local_path = Some(s_path.clone());
                    f_hash = seeder.file_hash.clone();
                    grp_id = seeder.group_id.clone();

                    let p_path = std::path::Path::new(&s_path);
                    filename = p_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                    let ext = p_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    mime_type = match ext.as_str() {
                        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" => "image/".to_owned() + &ext,
                        "mp4" | "mov" | "avi" | "mkv" | "webm" => "video/".to_owned() + &ext,
                        "pdf" => "application/pdf".to_string(),
                        "txt" => "text/plain".to_string(),
                        _ => "application/octet-stream".to_string(),
                    };
                    is_group_transfer = seeder.group_id.is_some();

                    if is_group_transfer {
                         if let Some(gid) = &grp_id {
                             if let Ok(Some(group)) = self.storage.get_group(gid) {
                                 if let Ok(members) = serde_json::from_str::<Vec<GroupMemberMetadata>>(&group.members_json) {
                                     // excluding self
                                     total_members = members.len().saturating_sub(1);
                                     current_completions = seeder.completions.len();
                                     println!("[Mesh] Group transfer {} progress: {}/{} members finished.", transfer_id, current_completions, total_members);
                                 }
                             }
                         }
                    }
                }

                // MANDATE: In 1-to-1 transfers, stop seeding once receiver confirms receipt.
                // In group transfers, we continue seeding indefinitely for the session.
                if !is_group_transfer && self.active_seeders.contains_key(&transfer_id) {
                    println!("[Mesh] 1-to-1 transfer {} complete. Removing seeder and taking off mesh.", transfer_id);
                    self.active_seeders.remove(&transfer_id);
                } else if is_group_transfer {
                    println!("[Mesh] Group member received transfer {}. Continuing to seed for the rest of the group.", transfer_id);
                }

                let (progress_ratio, outgoing_complete) = if is_group_transfer && total_members > 0 {
                    (current_completions as f32 / total_members as f32, current_completions >= total_members)
                } else if is_group_transfer && total_members == 0 {
                    // VERIFIED FIX: Group transfer but couldn't read member count from storage.
                    // Never prematurely signal outgoing_complete — wait for explicit member count.
                    // Show proportional progress based on completions seen so far (at least 1 member).
                    let ratio = current_completions as f32 / (current_completions.max(1) as f32);
                    println!("[Mesh] ⚠️ Group member count unavailable for {}. Holding verified state.", transfer_id);
                    (ratio, false)
                } else {
                    (1.0, true)
                };

                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let msg_id = transfer_id.clone();
                let progress = FileTransferProgress {
                    transfer_id: transfer_id.clone(),
                    peer_id: peer.to_string(),
                    filename,
                    mime_type,
                    file_hash: f_hash,
                    progress: progress_ratio,
                    is_complete: outgoing_complete,
                    is_verified: outgoing_complete,
                    is_outgoing: true,
                    local_path,
                    start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                    speed_bps: 0.0,
                    group_id: grp_id.clone(), // clone here so grp_id is still usable for DB routing below
                };
                if let Ok(json_str) = serde_json::to_string(&progress) {
                    let c = format!("[FILE]:{}", json_str);
                    // ROUTING FIX: Write to the correct DB table based on transfer type.
                    // Previously always wrote to store_message_with_id (direct-chat table), causing
                    // the sender's completed group file to appear in the 1-on-1 chat with the last ACKer.
                    if let Some(ref gid) = grp_id {
                        let gid_clone = gid.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_group_message(&gid_clone, &peer_id_str, &msg_id, &c, true, None);
                        });
                    } else {
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_message_with_id(&peer_id_str, &msg_id, &c, true, None);
                        });
                    }
                }
                
                // Clear RAM buffer for this transfer to prevent memory leaks
                if let Some(pending) = self.pending_messages.get_mut(&peer) {
                    pending.retain(|p| !matches!(p, SignalingPayload::FileChunk { transfer_id: tid, .. } | SignalingPayload::FileChunkRequest { transfer_id: tid, .. } if tid == &transfer_id));
                }

                let data = serde_json::to_vec(&progress).unwrap_or_default();
                crate::dispatch_global_event(12, &data);
            }
            SignalingPayload::FileTransferError { transfer_id, reason } => {
                println!("❌ File transfer error for {}: {}", transfer_id, reason);
                
                // BUG 1 FIX: Remove from active seeders to stop aggressive polling
                self.active_seeders.remove(&transfer_id);

                // Clear RAM buffer for this failed transfer to prevent memory leaks
                if let Some(pending) = self.pending_messages.get_mut(&peer) {
                    pending.retain(|p| !matches!(p, SignalingPayload::FileChunk { transfer_id: tid, .. } | SignalingPayload::FileChunkRequest { transfer_id: tid, .. } if tid == &transfer_id));
                }
            }
            SignalingPayload::MailboxDrain => {
                let is_anchor = self.swarm.behaviour().relay_server.as_ref().is_some() || self.storage.is_anchor_mode_enabled();
                if is_anchor {
                    if let Ok(messages) = self.storage.drain_mailbox(&peer) {
                        let _ = self.forward_to_mesh(peer, SignalingPayload::MailboxDrained(messages), false).await;
                    }
                } else {
                    println!("[Mesh] Warning: Received MailboxDrain but we are NOT an anchor node. Ignoring.");
                }
            }
            SignalingPayload::Acknowledgement { msg_id, status } => {
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                tokio::task::spawn_blocking(move || storage.update_message_status(&mid, status));
                let mut data = vec![status];
                data.extend(msg_id.as_bytes());
                crate::dispatch_global_event(13, &data);
            }
            SignalingPayload::TypingStart { chat_id: _ } => {
                let peer_bytes = peer.to_string().into_bytes();
                let mut data = peer_bytes.clone();
                data.push(1); // 1 = typing started
                crate::dispatch_global_event(39, &data);
            }
            SignalingPayload::TypingStop { chat_id: _ } => {
                let peer_bytes = peer.to_string().into_bytes();
                let mut data = peer_bytes.clone();
                data.push(0); // 0 = typing stopped
                crate::dispatch_global_event(39, &data);
            }
            SignalingPayload::Heartbeat { timestamp } => {
                // Store last-seen timestamp for the peer
                let peer_str = peer.to_string();
                let ts = timestamp;
                let storage = Arc::clone(&self.storage);
                tokio::task::spawn_blocking(move || {
                    let _ = storage.update_last_seen(&peer_str, ts);
                });
            }
            SignalingPayload::DirectInviteRequest(peer_identity) => {
                let is_extroverted = self.storage.is_privacy_mode_extroverted();
                if is_extroverted {
                    let peer_id = peer_identity.peer_id.clone();
                    let name = peer_identity.global_name.clone().unwrap_or_else(|| "Unknown".to_string());
                    let handle = peer_identity.handle.clone().unwrap_or_default();
                    let avatar = peer_identity.avatar_base64.clone();
                    
                    let storage = Arc::clone(&self.storage);
                    tokio::task::spawn_blocking(move || {
                        let _ = storage.upsert_sovereign_contact(&peer_identity, false, true);
                    });

                    let mut data = peer_id.into_bytes();
                    data.push(0);
                    data.extend(name.as_bytes());
                    data.push(0);
                    data.extend(handle.as_bytes());
                    data.push(0);
                    if let Some(av) = avatar { data.extend(av.as_bytes()); }
                    crate::dispatch_global_event(31, &data); // Event 31: Connection Request Received
                } else {
                    println!("[Mesh] Privacy Mode: Ignoring DirectInviteRequest from {:?} as we are INTROVERTED.", peer_identity.global_name);
                }
            }
            SignalingPayload::DirectInviteAccept(peer_identity) => {
                let peer_id = peer_identity.peer_id.clone();
                let name = peer_identity.global_name.clone().unwrap_or_else(|| "Unknown".to_string());
                let handle = peer_identity.handle.clone().unwrap_or_default();
                
                let storage = Arc::clone(&self.storage);
                tokio::task::spawn_blocking(move || {
                    let _ = storage.upsert_sovereign_contact(&peer_identity, true, false);
                });

                let mut data = peer_id.into_bytes();
                data.push(0);
                data.extend(name.as_bytes());
                data.push(0);
                data.extend(handle.as_bytes());
                crate::dispatch_global_event(32, &data); // Event 32: Connection Request Accepted
            }
            SignalingPayload::IdentifySleepState { device_type, push_token } => {
                let peer_id_str = peer.to_string();
                println!("[Registry] Registered Push Token for peer {}: {} ({})", peer_id_str, push_token, device_type);
                let _ = self.storage.save_push_token(&peer_id_str, &device_type, &push_token);
            }
            SignalingPayload::MessageReaction { msg_id, emoji } => {
                let sender_id = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                let em = emoji.clone();
                if !self.is_stress_test {
                    tokio::task::spawn_blocking(move || storage.add_message_reaction(&mid, &sender_id, &em));
                }

                // Pack [msg_id_len, msg_id, emoji] for UI
                let mut data = vec![msg_id.len() as u8];
                data.extend(msg_id.as_bytes());
                data.extend(emoji.as_bytes());
                crate::dispatch_global_event(35, &data); // Event Type 35: Message Reaction
            }
            SignalingPayload::SetRetention { seconds } => {
                let _ = self.storage.set_contact_retention(&peer.to_string(), seconds);
                let mut data = peer.to_string().into_bytes();
                data.push(0);
                crate::dispatch_global_event(36, &data); // Event 36: Retention changed
            }
            SignalingPayload::DeleteMessage { msg_id } => {
                // In P2P, if we receive DeleteMessage, the peer is deleting their own message.
                // We should technically verify if it belongs to them, but for 1-to-1, we trust the sender.
                let _ = self.storage.delete_message(&msg_id, false, false);

                let mut data = peer.to_string().into_bytes();
                data.push(0);
                crate::dispatch_global_event(37, &data); // Event 37: Message Deleted
            }
            SignalingPayload::EditMessage { msg_id, new_content } => {
                let _ = self.storage.edit_message(&msg_id, &new_content, false);
                let mut data = peer.to_string().into_bytes();
                data.push(0);
                crate::dispatch_global_event(38, &data); // Event 38: Message Edited
            }
            SignalingPayload::HandleClaimRequest { handle, peer_id, timestamp, pow_nonce } => {
                println!("[Registry] Received ClaimRequest for {} from {}", handle, peer_id);
                let claim = registry::HandleClaim { 
                    handle: handle.clone(), 
                    peer_id: peer_id.clone(), 
                    timestamp, 
                    pow_nonce, 
                    signatures: Vec::new() 
                };
                
                // 1. Verify PoW
                if !self.registry.verify_pow(&claim) {
                    println!("[Registry] ❌ Invalid PoW for handle claim: {}", handle);
                    return;
                }
                
                // 2. Check Uniqueness
                if !self.registry.is_handle_available(&handle, &peer_id) {
                    println!("[Registry] ❌ Handle {} already taken", handle);
                    return;
                }
                
                // 3. Witness claim if we are an Anchor/RBN
                let is_anchor_or_relay = self.storage.is_anchor_mode_enabled() || self.swarm.behaviour().relay_server.as_ref().is_some();
                if is_anchor_or_relay {
                    println!("[Registry] ✅ Witnessing claim for {}", handle);
                    let msg = format!("{}:{}:{}", handle, peer_id, timestamp);
                    if let Ok(sig) = self.local_keypair.sign(msg.as_bytes()) {
                         let pubkey = self.local_keypair.public().encode_protobuf();
                         let tx = self.command_tx.clone();
                         tokio::task::spawn(async move {
                             let _ = tx.send(NetworkCommand::BroadcastWitness { 
                                 handle, 
                                 peer_id, 
                                 timestamp, 
                                 pubkey,
                                 signature: sig 
                             }).await;
                         });
                    }
                }
            }
            SignalingPayload::HandleClaimWitnessed { handle, peer_id, timestamp, rbn_peer_id, rbn_pubkey, rbn_signature } => {
                println!("[Registry] Received Witness from {} for {}", rbn_peer_id, handle);
                
                // SECURITY: Verify the signature!
                let pubkey = match libp2p::identity::PublicKey::try_decode_protobuf(&rbn_pubkey) {
                    Ok(pk) => pk,
                    Err(_) => {
                        println!("[Registry] ⚠️ Rejected witness from {}: Invalid public key encoding", rbn_peer_id);
                        return;
                    }
                };

                // Verify that the public key matches the PeerId and is an authorized RBN
                let derived_pid = PeerId::from_public_key(&pubkey);
                if derived_pid.to_string() != rbn_peer_id {
                    println!("[Registry] ⚠️ Rejected witness from {}: PeerId mismatch", rbn_peer_id);
                    return;
                }

                let mut is_authorized = self.bootstrap_nodes.iter().any(|(pid, _)| pid == &derived_pid);
                
                // For local development or private meshes, allow trusting any connected anchor
                if !is_authorized && std::env::var("INTROVERT_TRUST_ALL_WITNESSES").is_ok() {
                    println!("[Registry] 🛠️ Trusting unauthorized witness due to INTROVERT_TRUST_ALL_WITNESSES");
                    is_authorized = true;
                }

                if !is_authorized {
                    println!("[Registry] ⚠️ Rejected witness from UNAUTHORIZED node: {}", rbn_peer_id);
                    return;
                }

                let msg = format!("{}:{}:{}", handle, peer_id, timestamp);
                if !pubkey.verify(msg.as_bytes(), &rbn_signature) {
                    println!("[Registry] ⚠️ INVALID signature from RBN: {}", rbn_peer_id);
                    return;
                }

                let witnesses = self.pending_claims.entry(handle.clone()).or_insert_with(HashSet::new);
                witnesses.insert(rbn_peer_id.clone());
                
                let mut connected_rbns = HashSet::new();
                for (rbn_id, _) in &self.bootstrap_nodes {
                    if self.swarm.is_connected(rbn_id) {
                        connected_rbns.insert(rbn_id);
                    }
                }
                let required_quorum = if std::env::var("INTROVERT_TRUST_ALL_WITNESSES").is_ok() || connected_rbns.len() < 2 { 1 } else { 2 };
                if witnesses.len() >= required_quorum {
                    println!("[Registry] 🏆 Quorum reached for handle: {}", handle);
                    let claim = registry::HandleClaim {
                        handle: handle.clone(),
                        peer_id: peer_id.clone(),
                        timestamp,
                        pow_nonce: 0,
                        signatures: witnesses.iter().cloned().collect(),
                    };
                    let _ = self.registry.verify_claim(&claim);
                    
                    // Publish handle mapping to DHT
                    let h_key = RecordKey::new(&handle.as_bytes());
                    let h_value = serde_json::to_string(&claim).unwrap_or_else(|_| peer_id.clone()).into_bytes();
                    let h_record = kad::Record {
                        key: h_key.clone(),
                        value: h_value,
                        publisher: Some(*self.swarm.local_peer_id()),
                        expires: None,
                    };
                    let _ = self.swarm.behaviour_mut().kademlia.put_record(h_record, kad::Quorum::One);
                    let _ = self.swarm.behaviour_mut().kademlia.start_providing(h_key);

                    // Publish reverse mapping ph_<peer_id> -> handle
                    let ph_key = RecordKey::new(&format!("ph_{}", peer_id).as_bytes());
                    let ph_record = kad::Record {
                        key: ph_key,
                        value: handle.clone().into_bytes(),
                        publisher: Some(*self.swarm.local_peer_id()),
                        expires: None,
                    };
                    let _ = self.swarm.behaviour_mut().kademlia.put_record(ph_record, kad::Quorum::One);

                    // Notify UI: [Handle\0PeerID]
                    let mut event_data = handle.clone().into_bytes();
                    event_data.push(0);
                    event_data.extend(peer_id.as_bytes());
                    crate::dispatch_global_event(34, &event_data);
                    
                    // Remove from pending to prevent double-event
                    self.pending_claims.remove(&handle);
                }
            }
            _ => {}
        }
    }

    async fn process_outgoing_file(
        peer_id: PeerId,
        file_path: String,
        is_relayed: bool,
        is_relayed_map: Arc<RwLock<HashMap<PeerId, bool>>>,
        data_channels: Arc<RwLock<HashMap<PeerId, Arc<webrtc::data_channel::RTCDataChannel>>>>,
        tx: mpsc::Sender<NetworkCommand>,
        storage: Arc<crate::storage::StorageService>,
        local_peer_id: PeerId,
        group_id: Option<String>,
        is_stress_test: bool,
        transfer_id_override: Option<String>,
    ) -> anyhow::Result<()> {

        let path = std::path::Path::new(&file_path);
        if !path.exists() { return Err(anyhow::anyhow!("File not found: {}", file_path)); }
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
        
        // Enhanced MIME detection
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let mime_type = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" => "image/".to_owned() + &ext,
            "mp4" | "mov" | "avi" | "mkv" | "webm" => "video/".to_owned() + &ext,
            "pdf" => "application/pdf".to_string(),
            "txt" => "text/plain".to_string(),
            _ => "application/octet-stream".to_string(),
        };

        // BUG 4 FIX: Streaming hash instead of full read
        use std::io::BufReader;
        let file_hash = {
            let mut hasher = Sha256::new();
            let f = std::fs::File::open(path)?;
            let mut reader = BufReader::new(f);
            std::io::copy(&mut reader, &mut hasher)?;
            format!("{:x}", hasher.finalize())
        };
        let total_size = std::fs::metadata(path)?.len() as usize;

        let transfer_id = transfer_id_override.unwrap_or_else(|| {
            format!("gft_{}_{}", file_hash, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
        });
        
        // ADAPTIVE CHUNKING: Direct P2P uses 256KB chunks, Relay uses 64KB (Sovereign Swarm Pull)
        let chunk_size = if is_relayed { 64 * 1024 } else { 256 * 1024 }; 
        let total_chunks = (total_size as f32 / chunk_size as f32).ceil() as u32;
        
        let transfer_payload = SignalingPayload::FileTransfer { 
            transfer_id: transfer_id.clone(), 
            filename: filename.clone(),
            mime_type: mime_type.clone(), 
            file_hash: file_hash.clone(),
            total_size,
            is_relayed,
            sender_peer_id: Some(local_peer_id.to_string()),
            group_id: group_id.clone(),
        };
        let initial_progress = FileTransferProgress { 
            transfer_id: transfer_id.clone(), 
            peer_id: peer_id.to_string(), 
            filename: filename.clone(), 
            mime_type: mime_type.clone(),
            file_hash: file_hash.clone(),
            progress: 0.0, 
            is_complete: false, 
            is_verified: false,
            is_outgoing: true, 
            local_path: Some(file_path.clone()),
            start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
            speed_bps: 0.0,
            group_id: group_id.clone(),
        };

        // Persistent History: Save file manifest and BROADCAST TO GROUP if applicable
        let peer_id_str = peer_id.to_string();
        let msg_id = transfer_id.clone();
        let gid_opt = group_id.clone();
        if let Ok(json_str) = serde_json::to_string(&initial_progress) {
            let content = format!("[FILE]:{}", json_str);
            let s = Arc::clone(&storage);
            if !is_stress_test {
                if let Some(gid) = gid_opt {
                    let gid_clone = gid.clone();
                    let c_clone = content.clone();
                    let tx_clone = tx.clone();
                    tokio::task::spawn_blocking(move || s.store_group_message(&gid_clone, &peer_id_str, &msg_id, &c_clone, true, None));
                    
                    // BROADCAST: For group shares, we must also gossip the manifest to the group
                    let gid_for_broadcast = gid;
                    let fname_clone = filename.clone();
                    tokio::spawn(async move {
                        println!("[Mesh] Gossiping file manifest for {} to group {}", fname_clone, gid_for_broadcast);
                        // Use standard group message broadcast mechanism
                        let _ = tx_clone.send(NetworkCommand::BroadcastGroupMessage { 
                            group_id: gid_for_broadcast, 
                            message: content,
                            reply_to: None
                        }).await;
                    });
                } else {
                    tokio::task::spawn_blocking(move || s.store_message_with_id(&peer_id_str, &msg_id, &content, true, None));
                }
            }
        }

        // --- PULL MODEL: Register as an active seeder to serve chunk requests ---
        let _ = tx.send(NetworkCommand::RegisterSeeder {
            peer_id: local_peer_id,
            transfer_id: transfer_id.clone(),
            file_path: file_path.clone(),
            file_hash: file_hash.clone(),
            chunk_size,
            total_chunks,
            group_id: group_id.clone(),
        }).await;

        let _ = tx.send(NetworkCommand::SendFileChunk { peer_id, payload: transfer_payload, progress: initial_progress.clone() }).await;

        // GOSSIPSUB BROADCAST PATH: When peer_id == local_peer_id, this call came from the
        // group broadcast announce path. We've registered as seeder and gossiped the manifest.
        // Actual chunk delivery is handled per-member by the individual SendFile calls above.
        // Return here to avoid a self-push loop.
        if peer_id == local_peer_id {
            println!("✅ Group manifest announced and seeder registered for {}. Per-member push handles delivery.", filename);
            return Ok(());
        }

        if is_relayed {
            println!("✅ File transfer manifest sent for {}. (Relay mode - waiting for chunk requests).", filename);
            // BUG 1 FIX: Immediate mailbox fetch so we see the receiver's pull requests right away
            let _ = tx.send(NetworkCommand::FetchMailbox).await;
            return Ok(());
        }
        
        // Extended delay for manifest to propagate and relay circuits to warm up
        tokio::time::sleep(Duration::from_millis(if is_relayed { 2000 } else { 500 })).await;

        // BUG 4 FIX: Read chunks from disk during push loop
        let mut file = std::fs::File::open(path)?;
        for i in 0..total_chunks {
            let start = (i * chunk_size) as usize;
            let end = std::cmp::min(start + chunk_size as usize, total_size);
            
            use std::io::Seek;
            file.seek(std::io::SeekFrom::Start(start as u64))?;
            let mut chunk_data = vec![0u8; end - start];
            file.read_exact(&mut chunk_data)?;
            
            let chunk_payload = SignalingPayload::FileChunk { transfer_id: transfer_id.clone(), chunk_index: i, total_chunks, data_base64: general_purpose::STANDARD.encode(&chunk_data) };
            
            let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
            let elapsed_s = (current_time - initial_progress.start_time_ms) as f64 / 1000.0;
            let bytes_sent = end;
            let speed_bps = if elapsed_s > 0.01 { (bytes_sent as f64 * 8.0) / elapsed_s } else { 0.0 };

            let progress = FileTransferProgress { 
                transfer_id: transfer_id.clone(), 
                peer_id: peer_id.to_string(), 
                filename: filename.clone(), 
                mime_type: mime_type.clone(),
                file_hash: file_hash.clone(),
                progress: (i + 1) as f32 / total_chunks as f32, 
                is_complete: i + 1 == total_chunks, 
                is_verified: false,
                is_outgoing: true, 
                local_path: Some(file_path.clone()),
                start_time_ms: initial_progress.start_time_ms,
                speed_bps,
                group_id: group_id.clone(),
            };
            
            // Since SendFileChunk is handled via the command channel, we can't easily wait here.
            // But the actual forward_to_mesh now drops chunks rather than buffering them infinitely.
            // To avoid overloading the channel itself, we simply apply a pacing delay.
            let _ = tx.send(NetworkCommand::SendFileChunk { peer_id, payload: chunk_payload.clone(), progress: progress.clone() }).await;
            
            // ADAPTIVE PACING: Direct P2P/WebRTC uses 20ms, Relay uses 250ms (checked dynamically)
            let current_relayed = is_relayed_map.read().get(&peer_id).cloned().unwrap_or(is_relayed);
            let has_webrtc = {
                let dc_store_read = data_channels.read();
                if let Some(dc) = dc_store_read.get(&peer_id) {
                    dc.ready_state() == RTCDataChannelState::Open
                } else {
                    false
                }
            };
            let is_direct = !current_relayed || has_webrtc;
            tokio::time::sleep(Duration::from_millis(if is_direct { 20 } else { 250 })).await;
        }
        


        println!("✅ File transfer chunks sent for {}. Waiting for verification from peer...", filename);
        Ok(())
    }

    fn select_best_providers_static(
        swarm: &Swarm<IntrovertBehaviour>,
        is_relayed_map: &Arc<RwLock<HashMap<PeerId, bool>>>,
        providers: &[PeerId],
    ) -> Vec<PeerId> {
        let is_connected_fn = |p: &PeerId| swarm.is_connected(p);
        let is_relayed_fn = |p: &PeerId| is_relayed_map.read().get(p).cloned().unwrap_or(true);

        let mut direct = Vec::new();
        let mut relayed = Vec::new();
        for p in providers {
            if is_connected_fn(p) {
                if !is_relayed_fn(p) {
                    direct.push(*p);
                } else {
                    relayed.push(*p);
                }
            }
        }
        if !direct.is_empty() {
            direct
        } else if !relayed.is_empty() {
            relayed
        } else {
            providers.to_vec()
        }
    }
}
