use libp2p::{
    kad::{self, Record, RecordKey, QueryId},
    request_response,
    swarm::SwarmEvent,
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
use std::collections::HashMap;
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

use crate::media::{MediaManager, WebRtcSignal};
use crate::identity::SovereignIdentity;
use noise_session::NoiseSession;
pub use behaviour::{IntrovertBehaviour, IntrovertBehaviourEvent};
use config::get_bootstrap_nodes;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupAction {
    Message { content_encrypted: Vec<u8>, msg_id: String },
    AddMember { metadata: GroupMemberMetadata },
    RemoveMember { peer_id: String },
    UpdateRole { peer_id: String, new_role: GroupRole },
    DeleteGroup,
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
    ChatMessage { 
        content: String, 
        msg_id: String, 
        #[serde(default)]
        timestamp: i64 
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
    FileChunkRequest { transfer_id: String, chunk_index: u32 },
    FileChunk { transfer_id: String, chunk_index: u32, total_chunks: u32, data_base64: String },
    FileTransferComplete { transfer_id: String },
    FileTransferError { transfer_id: String, reason: String },
    DeleteMessage { msg_id: String },
    // Group Mesh
    GroupManifestRequest { group_id: String },
    GroupInvite { group_id: String, name: String, description: String, inviter_peer_id: String, group_secret_wrapped: Vec<u8>, members: Vec<GroupMemberMetadata> },
    GroupAction(SignedGroupAction),
    GroupManifest { group_id: String, name: String, description: String, members: Vec<GroupMemberMetadata>, secret: [u8; 32] },
    RequestHandshake,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingRequest(pub SignalingPayload);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingResponse(pub String);

// --- Network Commands ---

pub enum NetworkCommand {
    Dial { peer_id: PeerId, address: Option<Multiaddr> },
    ListenOn { address: Multiaddr },
    SendSignaling { peer_id: PeerId, msg_id: String, message: String },
    InitiateWebRtc { peer_id: PeerId },
    StartMediaStream { peer_id: PeerId, media_type: u8 },
    CloseWebRtc { peer_id: PeerId },
    WebRtcFailed { peer_id: PeerId },
    RenegotiateWebRtc { peer_id: PeerId },
    AddAddress { peer_id: PeerId, address: Multiaddr },
    EstablishSecureSession { peer_id: PeerId },
    FetchMailbox,
    UpdateAnchorStatus { enabled: bool },
    SendFile { peer_id: PeerId, file_path: String, group_id: Option<String> },
    SendFileFinalize { peer_id: PeerId, file_path: String, has_dc_already: bool, group_id: Option<String> },
    SendFileChunk { peer_id: PeerId, payload: SignalingPayload, progress: FileTransferProgress },
    SendAcknowledgement { peer_id: PeerId, msg_id: String, status: u8 },
    ForwardMeshSignaling { peer_id: PeerId, payload: SignalingPayload },
    HandleIncomingPayload { peer_id: PeerId, payload: SignalingPayload },
    HandleIncomingWebRtcPayload { peer_id: PeerId, payload: SignalingPayload },
    AddGroupMember { group_id: String, peer_id: String },
    RemoveGroupMember { group_id: String, peer_id: String },
    UpdateGroupRole { group_id: String, peer_id: String, role: GroupRole },
    PublishGroupManifest { group_id: String, code: String },
    JoinMeshByCode { code: String },
    AcceptGroupInvite { group_id: String },
    DeclineGroupInvite { group_id: String },
    ForceMeshRefresh,
    RegisterSeeder { peer_id: PeerId, transfer_id: String, file_path: String, file_hash: String, chunk_size: u32, total_chunks: u32, group_id: Option<String> },
    UnregisterSeeder { transfer_id: String },
    FindProviders { file_hash: String },
    /// Force-store a payload in the anchor mailbox for a peer, bypassing all direct delivery.
    /// Used when direct relay sends fail — avoids the direct-retry loop.
    StoreInMailbox { peer_id: PeerId, payload: SignalingPayload },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferProgress {
    pub transfer_id: String,
    pub peer_id: String,
    pub filename: String,
    pub mime_type: String,
    pub progress: f32, // 0.0 to 1.0
    pub is_complete: bool,
    pub is_verified: bool,
    pub is_outgoing: bool,
    pub local_path: Option<String>,
    pub start_time_ms: u64,
    pub speed_bps: f64,
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
}

struct ActiveSeeder {
    peer_id: PeerId,
    file_path: String,
    file_hash: String,
    chunk_size: u32,
    total_chunks: u32,
    bytes_sent: usize,
    start_time: Instant,
    group_id: Option<String>,
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
    is_relayed_map: HashMap<PeerId, bool>,
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
    ) -> anyhow::Result<Self> {
        let local_static_public = PublicKey::from(&local_static_secret);
        let local_peer_id = PeerId::from(keypair.public());

        macro_rules! build_swarm {
            ($builder:expr) => {
                {
                    let mut yamux_config = libp2p::yamux::Config::default();
                    yamux_config.set_max_num_streams(1024);
                    $builder
                        .with_relay_client(libp2p::noise::Config::new, move || yamux_config.clone())?
                        .with_behaviour(|keypair: &libp2p::identity::Keypair, relay_client| {
                            IntrovertBehaviour::new(local_peer_id, keypair.public(), relay_client, enable_mdns, enable_relay_server, max_connections)
                        })?
                        .with_swarm_config(|c: libp2p::swarm::Config| {
                            c.with_idle_connection_timeout(Duration::from_secs(120))
                        })
                        .build()
                }
            };
        }

        let builder = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                || {
                    let mut yamux_config = libp2p::yamux::Config::default();
                    yamux_config.set_max_num_streams(1024);
                    yamux_config
                },
            )?
            .with_quic();

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
        
        Ok(Self { 
            swarm, 
            command_rx,
            command_tx,
            storage,
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
            is_relayed_map: HashMap::new(),
            relay_dial_limiter: HashMap::new(),
            outbound_tracker: HashMap::new(),
            inflight_requests: HashMap::new(),
            liveness_interval_secs,
            downloads_dir,
            local_keypair: keypair,
            resolved_group_codes: HashMap::new(),
            anchor_mappings: HashMap::new(),
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

        let local_peer_id = *self.swarm.local_peer_id();
        let pubkey_record = Record {
            key: RecordKey::new(&local_peer_id.to_bytes()),
            value: self.local_static_public.to_bytes().to_vec(),
            publisher: Some(local_peer_id),
            expires: None,
        };
        let _ = self.swarm.behaviour_mut().kademlia.put_record(pubkey_record, kad::Quorum::One);

        // Pre-populate anchors with known RBN nodes
        for (peer_id, addr) in get_bootstrap_nodes() {
            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
            if !self.discovered_anchors.contains(&peer_id) {
                self.discovered_anchors.push(peer_id);
            }
            let _ = self.swarm.dial(addr);
        }
        
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
        self.perform_mailbox_fetch().await;

        if self.swarm.behaviour().relay_server.as_ref().is_some() {
            println!("[Network] RBN Mode: Automatically providing Anchor Node service.");
            let key = RecordKey::new(&ANCHOR_PROVIDER_KEY);
            let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
        }

        let mut republication_interval = tokio::time::interval(Duration::from_secs(5 * 60)); // 5 mins
        let mut liveness_interval = tokio::time::interval(Duration::from_secs(self.liveness_interval_secs));
        let mut contact_refresh_interval = tokio::time::interval(Duration::from_secs(30));
        let mut anchor_discovery_interval = tokio::time::interval(Duration::from_secs(2 * 60));
        let mut mailbox_fetch_interval = tokio::time::interval(Duration::from_secs(30));
        let mut fast_poll_interval = tokio::time::interval(Duration::from_secs(1)); // Fast poll when transfers are active
        let mut status_check_interval = tokio::time::interval(Duration::from_secs(15)); // Check local status every 15s
        let mut pull_retry_interval = tokio::time::interval(Duration::from_secs(4));
        let mut lease_interval = tokio::time::interval(Duration::from_secs(60 * 60));
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(10));


        let mut last_status = 0u8;
        let mut last_fast_mailbox_fetch = Instant::now() - Duration::from_secs(60);

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    let peers = self.swarm.connected_peers().count();
                    println!("[Swarm Heartbeat] Connected peers: {}", peers);
                }
                _ = fast_poll_interval.tick() => {
                    let has_active_incoming = self.incoming_transfers.values().any(|t| t.is_relayed);
                    let has_active_seeding = !self.active_seeders.is_empty();
                    let has_relay_peers = self.is_relayed_map.values().any(|&r| r);
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
                        let stalled: Vec<(String, PeerId, Vec<PeerId>, u32, u32)> = self.incoming_transfers.iter()
                            .filter(|(_, t)| {
                                let is_relayed_conn = self.is_relayed_map.get(&t.peer_id).cloned().unwrap_or(false);
                                (t.is_relayed || is_relayed_conn) && t.last_update.elapsed() > Duration::from_secs(8)
                            })
                            .map(|(tid, t)| {
                                // Find the first missing chunk index
                                let mut next = 0u32;
                                while t.received_chunks.contains_key(&next) { next += 1; }
                                (tid.clone(), t.peer_id, t.providers.clone(), next, t.total_chunks)
                            })
                            .collect();
                        
                        for (tid, peer, providers, first_missing_idx, total_chunks) in stalled {
                            // If total_chunks is known, make sure we don't request past the end
                            // Use 2-deep window to maximize reliability on recovery
                            let limit = if total_chunks > 0 {
                                std::cmp::min(first_missing_idx + 2, total_chunks)
                            } else {
                                first_missing_idx + 2
                            };

                            if first_missing_idx < limit {
                                println!("[Mesh] Transfer {} stalled. Retrying PULL for chunks {}..{} from {} providers", 
                                         tid, first_missing_idx, limit - 1, providers.len());
                                
                                // REDUNDANCY FILTER: Remove old requests for this transfer from RAM buffer
                                if let Some(pending) = self.pending_messages.get_mut(&peer) {
                                    pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: ref id, .. } if id == &tid));
                                }
                                
                                let tx = self.command_tx.clone();
                                let tid_clone = tid.clone();
                                tokio::spawn(async move {
                                    for idx in first_missing_idx..limit {
                                        let target_peer = if !providers.is_empty() {
                                            providers[(idx as usize) % providers.len()]
                                        } else {
                                            peer
                                        };
                                        let req = SignalingPayload::FileChunkRequest { 
                                            transfer_id: tid_clone.clone(), 
                                            chunk_index: idx 
                                        };
                                        let _ = tx.send(NetworkCommand::ForwardMeshSignaling { 
                                            peer_id: target_peer, 
                                            payload: req 
                                        }).await;
                                        tokio::time::sleep(Duration::from_millis(100)).await;
                                    }
                                });
                            }
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

                    // Periodically broadcast connection status of all currently connected peers
                    // to keep the Flutter UI in sync, especially on screen transitions.
                    let connected_peers: Vec<PeerId> = self.swarm.connected_peers().cloned().collect();
                    for peer_id in connected_peers {
                        let is_relayed = self.is_relayed_map.get(&peer_id).cloned().unwrap_or(false);
                        let status: u8 = if is_relayed { 1 } else { 0 }; // 0 = Direct P2P, 1 = Relay Active
                        let mut data = peer_id.to_string().into_bytes();
                        data.push(b':');
                        data.push(status);
                        crate::dispatch_global_event(8, &data);
                    }
                }
                _ = mailbox_fetch_interval.tick() => {
                    self.perform_mailbox_fetch().await;
                    for (_, addr) in get_bootstrap_nodes() {
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
                    if self.swarm.connected_peers().count() == 0 {
                        println!("[Network] Zero peers detected. Forcing mesh re-entry...");
                        for (peer_id, addr) in get_bootstrap_nodes() {
                            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
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
        if let Some(transfer) = self.incoming_transfers.get_mut(&transfer_id) {
            transfer.total_chunks = total_chunks;
            transfer.last_update = Instant::now();
            if let Ok(chunk_data) = general_purpose::STANDARD.decode(data_base64) {
                // RELIABILITY: Only trigger N+4 if this is a NEW chunk.
                // This prevents duplicate requests from retries or re-deliveries.
                let is_new_chunk = transfer.received_chunks.insert(chunk_index, chunk_data).is_none();
                
                let progress_val = transfer.received_chunks.len() as f32 / total_chunks as f32;
                let is_complete = transfer.received_chunks.len() as u32 == total_chunks;
                
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
                        
                        // SOVEREIGN SWARM: Seeding logic depends on group context
                        if let Some(ref gid) = transfer.group_id {
                            println!("[Mesh] Group transfer complete. Joining swarm as seeder for group: {}", gid);
                            let key = RecordKey::new(&transfer.file_hash.as_bytes());
                            let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);

                            let safe_filename = Self::sanitize_filename(&transfer.filename);
                            let path = format!("{}/introvert_{}", self.downloads_dir, safe_filename);
                            
                            // Register as active seeder to serve chunk requests for this group
                            let _ = self.command_tx.send(NetworkCommand::RegisterSeeder {
                                peer_id: *self.swarm.local_peer_id(),
                                transfer_id: transfer_id.clone(),
                                file_path: path.clone(),
                                file_hash: transfer.file_hash.clone(),
                                chunk_size: if transfer.is_relayed { 32 * 1024 } else { 256 * 1024 },
                                total_chunks,
                                group_id: Some(gid.clone()),
                            }).await;
                        } else {
                            println!("[Mesh] 1-to-1 transfer complete. Skipping mesh seeding to preserve privacy.");
                        }

                        let safe_filename = Self::sanitize_filename(&transfer.filename);
                        let path = format!("{}/introvert_{}", self.downloads_dir, safe_filename);

                        if std::fs::write(&path, full_data).is_ok() { 
                            local_path = Some(path); 
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
                } else if is_new_chunk && !transfer.filename.starts_with("ERROR:") {
                    let is_relayed_conn = self.is_relayed_map.get(&peer).cloned().unwrap_or(false);
                    if transfer.is_relayed || is_relayed_conn {
                        // SOVEREIGN SWARM: Stable windowed pull (N+2) distributed across providers.
                        // This maintains exactly 2 chunks in flight, balancing load across all seeders.
                        let next_idx = chunk_index + 2;
                        if next_idx < total_chunks {
                            let providers = transfer.providers.clone();
                            let target_peer = if !providers.is_empty() {
                                providers[(next_idx as usize) % providers.len()]
                            } else {
                                peer
                            };

                            let tx = self.command_tx.clone();
                            let tid = transfer_id.clone();
                            tokio::spawn(async move {
                                let req = SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: next_idx };
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
                    progress: progress_val, 
                    is_complete, 
                    is_verified,
                    is_outgoing: false, 
                    local_path: local_path.clone(),
                    start_time_ms: 0,
                    speed_bps,
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
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_group_message(&gid_clone, &peer_str, &tid_clone, &content);
                        });
                    }
                } else {
                    if let Ok(json_str) = serde_json::to_string(&progress) {
                        let content = format!("[FILE]:{}", json_str);
                        let storage = Arc::clone(&self.storage);
                        let peer_str = peer.to_string();
                        let tid_clone = transfer_id.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_message_with_id(&peer_str, &tid_clone, &content, false);
                        });
                    }
                }
                
                // CRITICAL FIX: Always remove from incoming transfers when complete (success or fail) to prevent memory leak
                if is_complete { self.incoming_transfers.remove(&transfer_id); }
            }
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
                        for (peer_id, addr) in list {
                            println!("mDNS discovered peer: {} at address: {}", peer_id, addr);
                            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                            println!("Dialing peer discovered via mDNS: {}", peer_id);
                            let _ = self.swarm.dial(peer_id);
                        }
                    }
                    IntrovertBehaviourEvent::Autonat(autonat::Event::StatusChanged { old, new }) => {
                        println!("[AutoNAT] Reachability changed: {:?} -> {:?}", old, new);
                        
                        // Clear all WebRTC connections since our network interface changed
                        self.data_channels.write().clear();
                        let pcs: Vec<Arc<RTCPeerConnection>> = self.peer_connections.write().drain().map(|(_, pc)| pc).collect();
                        for pc in pcs {
                            let _ = pc.close().await;
                        }

                        // PROACTIVE MESH REBUILD: If we just moved networks, re-dial bootstrap nodes
                        for (_, addr) in get_bootstrap_nodes() {
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
                        
                        // Add addresses to both Kademlia AND the swarm's direct address book
                        // This is critical for the Relay Client to find the relay server.
                        for addr in &info.listen_addrs {
                            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr.clone());
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
                                Ok(id) => println!("[Mesh] Relay listen request SUCCESS. Address: {}, Listener ID: {:?}", relay_addr, id),
                                Err(e) => println!("[Mesh] Relay listen request FAILED on {}: {:?}", relay_addr, e),
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
                                    .map(|p| (*p, self.is_relayed_map.get(p).cloned().unwrap_or(false)))
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
                        for transfer in self.incoming_transfers.values_mut() {
                            if transfer.file_hash == key_str {
                                for pid in &filtered_providers {
                                     if !transfer.providers.contains(pid) { transfer.providers.push(*pid); }
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
                                    tokio::spawn(async move {
                                        let req = SignalingPayload::GroupManifestRequest { group_id: gid };
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
                        
                        // Store the resolved mapping
                        self.resolved_group_codes.insert(key_str.clone(), value_str.clone());

                        // If we have providers already discovered for this key, query them immediately
                        if let Some(providers) = self.active_providers.get(&key_str).cloned() {
                            for peer_id in providers {
                                let tx = self.command_tx.clone();
                                let gid = value_str.clone();
                                tokio::spawn(async move {
                                    let req = SignalingPayload::GroupManifestRequest { group_id: gid };
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
                                let is_relay_target = self.is_relayed_map.get(&target_peer).cloned().unwrap_or(false);
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
                            self.is_relayed_map.remove(&peer);
                        }
                    }
                    IntrovertBehaviourEvent::RequestResponse(request_response::Event::ResponseSent { .. }) => {}
                    IntrovertBehaviourEvent::Ping(_) => {
                        // Suppress noisy ping logs
                    }
                    _ => {
                        println!("[Swarm Debug] Unhandled behaviour event: {:?}", b_event);
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("[Swarm] New listen address: {}", address);
            }
            SwarmEvent::ListenerError { listener_id, error, .. } => {
                println!("[Swarm] Listener error ({:?}): {:?}", listener_id, error);
            }
            SwarmEvent::ListenerClosed { listener_id, reason, .. } => {
                println!("[Swarm] Listener closed ({:?}): {:?}", listener_id, reason);
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

                let is_relayed = endpoint.is_relayed();
                self.is_relayed_map.insert(peer_id, is_relayed);

                let status: u8 = if is_relayed { 1 } else { 0 };
                let mut data = peer_id.to_string().into_bytes();
                data.push(b':');
                data.push(status);
                crate::dispatch_global_event(8, &data);
                
                if is_relayed { self.reward_tracker.record_relay(&peer_id.to_string(), 1024); }
                
                let data = peer_id.to_bytes();
                crate::dispatch_global_event(1, &data); 

                // Flush pending messages on connection — but RATE-LIMITED to prevent thundering herd
                // on relay circuits. File chunks are paced: max 4 in-flight at 50ms intervals.
                if let Some(payloads) = self.pending_messages.remove(&peer_id) {
                    let is_relayed = self.is_relayed_map.get(&peer_id).cloned().unwrap_or(false);
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
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                // Clean up WebRTC resources immediately on connection loss to prevent stale ghost channels
                self.data_channels.write().remove(&peer_id);
                self.anchor_mappings.remove(&peer_id);
                self.inflight_requests.remove(&peer_id); // CRITICAL: Clear in-flight tracker on disconnect
                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
                if let Some(pc) = pc {
                    let _ = pc.close().await;
                }

                if !self.swarm.is_connected(&peer_id) {
                    self.is_relayed_map.remove(&peer_id);
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
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(pid) = peer_id {
                    if pid == *self.swarm.local_peer_id() { return Ok(()); }
                    println!("[Swarm] Outgoing connection error for peer {}: {:?}", pid, error);
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

        // 1. Dial ALL port 443 RBN nodes (matching stable v11 robustness)
        for (rbn_id, rbn_addr) in get_bootstrap_nodes() {
            if rbn_addr.to_string().contains("443") {
                let relay_addr = rbn_addr.clone()
                    .with(libp2p::multiaddr::Protocol::P2p(rbn_id))
                    .with(libp2p::multiaddr::Protocol::P2pCircuit)
                    .with(libp2p::multiaddr::Protocol::P2p(recipient_id));

                let _ = self.swarm.dial(relay_addr);
            }
        }

        // 2. Dial via ONE additional active connected anchor (if physical IP is known)
        let mut anchor_ids = Vec::new();
        if let Ok(verified_anchors) = self.storage.fetch_all_anchor_nodes() {
            for node in verified_anchors {
                if let Ok(pid) = node.peer_id.parse::<PeerId>() {
                    if !anchor_ids.contains(&pid) { anchor_ids.push(pid); }
                }
            }
        }
        for pid in &self.discovered_anchors {
            if !anchor_ids.contains(pid) { anchor_ids.push(*pid); }
        }

        let mut extra_dial_count = 0;
        for anchor_id in anchor_ids {
            if self.swarm.is_connected(&anchor_id) && extra_dial_count < 1 {
                // Skip if this is already a bootstrap RBN (already dialed above)
                if get_bootstrap_nodes().iter().any(|(id, _)| id == &anchor_id) { continue; }

                if let Some(addr) = self.anchor_mappings.get(&anchor_id) {
                    let relay_addr = addr.clone()
                        .with(libp2p::multiaddr::Protocol::P2p(anchor_id))
                        .with(libp2p::multiaddr::Protocol::P2pCircuit)
                        .with(libp2p::multiaddr::Protocol::P2p(recipient_id));
                    
                    println!("[Mesh] Dialing recipient {} via extra anchor relay: {}", recipient_id, relay_addr);
                    let _ = self.swarm.dial(relay_addr);
                    extra_dial_count += 1;
                }
            }
        }

        // 3. Also attempt direct dial as fallback
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
            // HYBRID ROUTING: Direct P2P uses WebRTC for everything (max speed).
            // Relayed transfers avoid WebRTC for ALL File Payloads to use the robust libp2p stack.
            let is_file_payload = matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. } | SignalingPayload::FileTransfer { .. });
            let is_relayed_conn = self.is_relayed_map.get(&recipient_id).cloned().unwrap_or(false);
            let skip_webrtc = is_relayed_conn && is_file_payload;

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
                println!("[Mesh] Peer {} is connected. Attempting direct delivery...", recipient_str);
                let mut sent = false;
                // If it's a message/ack that can be encrypted, try Noise.
                // NOTE: FileChunk is intentionally excluded from Noise on relay connections:
                // relay transport is already encrypted (libp2p Noise), and adding app-level
                // Noise causes double-JSON-base64 overhead (~83% extra wire cost per chunk).
                let noise_eligible = match &payload {
                    SignalingPayload::Standard(_) | SignalingPayload::ChatMessage { .. } | SignalingPayload::Acknowledgement { .. } => true,
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
                    // Enforce in-flight concurrency limit for relay connections (max 2 concurrent)
                    // CRITICAL: ONLY limit actual FileChunks, NOT Requests (control traffic)
                    let is_chunk_data = matches!(payload, SignalingPayload::FileChunk { .. });
                    if is_chunk_data && is_relayed_conn {
                        let inflight = self.inflight_requests.get(&recipient_id).cloned().unwrap_or(0);
                        if inflight >= 2 {
                            // Back-pressure: put back in pending to be sent when slots free up
                            println!("[Mesh] Relay in-flight limit reached for {}. Buffering chunk.", recipient_str);
                            self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
                            return Ok(());
                        }
                    }
                    
                    println!("[Mesh] Sending PLAIN payload to {}", recipient_str);
                    let req_id = self.swarm.behaviour_mut().request_response.send_request(&recipient_id, SignalingRequest(payload.clone()));
                    self.outbound_tracker.insert(req_id, (recipient_id, payload.clone()));
                    
                    if is_chunk_data && is_relayed_conn {
                        *self.inflight_requests.entry(recipient_id).or_insert(0) += 1;
                    }
                }
                return Ok(());
            }

            // 3. Active Relay Dialing (Messenger Strategy)
            // If not connected, construct and dial the relay path via RBN
            self.dial_relay_path(recipient_id);
        }
        // 4. Fallback: Persistent Mesh Storage (Mailbox)

        // WebRTC signaling is transient and should never be stored in persistent mailboxes.
        if matches!(payload, SignalingPayload::WebRtc(_) | SignalingPayload::Candidate(_) | SignalingPayload::Offer(_) | SignalingPayload::Answer(_)) {
            println!("[Mesh] Buffering real-time signaling for {} in RAM...", recipient_str);
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
                SignalingPayload::GroupManifest { .. }
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
            } else if let SignalingPayload::FileChunkRequest { transfer_id, chunk_index } = &payload {
                pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: tid, chunk_index: idx } if tid == transfer_id && idx == chunk_index));
            }

            pending.push(payload.clone());

            // Dial mesh to find anchors
            for pid in anchor_ids { let _ = self.swarm.dial(pid); }
            for (_, addr) in get_bootstrap_nodes() { let _ = self.swarm.dial(addr); }

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
            NetworkCommand::SendSignaling { peer_id, msg_id, message } => {
                let peer_id_str = peer_id.to_string();
                let content_str = message.clone();
                let storage = Arc::clone(&self.storage);
                let mid = msg_id.clone();
                let c = content_str.clone();
                tokio::task::spawn_blocking(move || storage.store_message_with_id(&peer_id_str, &mid, &c, true));
                self.reward_tracker.record_message_activity(&peer_id.to_string());
                
                let timestamp = chrono::Utc::now().timestamp();
                let payload = SignalingPayload::ChatMessage { content: message, msg_id, timestamp };
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
            NetworkCommand::RemoveGroupMember { group_id, peer_id } => {
                println!("[Mesh] Removing member {} from group {}", peer_id, group_id);
                if let Ok(Some(group_info)) = self.storage.get_group(&group_id) {
                    let mut members: Vec<GroupMemberMetadata> = serde_json::from_str(&group_info.members_json).unwrap_or_default();
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
                                // If we removed ourselves, delete the group locally and notify the mesh
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
            NetworkCommand::AcceptGroupInvite { group_id } => {
                println!("[Mesh] Accepting group invite for: {}", group_id);
                if let Ok(Some(invite)) = self.storage.get_pending_invite(&group_id) {
                    if let Ok(group_secret) = group::GroupManager::unwrap_group_secret(&invite.group_secret_wrapped, &self.local_static_secret) {
                        let _ = self.storage.save_group_secret(&group_id, &group_secret);
                        let _ = self.storage.upsert_group(&group_id, &invite.name, &invite.description, &invite.members_json);
                        let _ = self.storage.delete_pending_invite(&group_id);
                        crate::dispatch_global_event(23, group_id.as_bytes());
                        println!("[Mesh] ✅ Group invite accepted: {}", invite.name);
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
            NetworkCommand::RegisterSeeder { peer_id, transfer_id, file_path, file_hash, chunk_size, total_chunks, group_id } => {
                self.active_seeders.insert(transfer_id, ActiveSeeder {
                    peer_id,
                    file_path,
                    file_hash: file_hash.clone(),
                    chunk_size,
                    total_chunks,
                    bytes_sent: 0,
                    start_time: Instant::now(),
                    group_id,
                });

                // SOVEREIGN SWARM: Announce that we are providing this file hash to the mesh
                let key = RecordKey::new(&file_hash.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.start_providing(key);
            }
            NetworkCommand::UnregisterSeeder { transfer_id } => {
                self.active_seeders.remove(&transfer_id);
            }
            NetworkCommand::FindProviders { file_hash } => {
                println!("[Mesh] Searching Sovereign Swarm for providers of file: {}", file_hash);
                let key = RecordKey::new(&file_hash.as_bytes());
                let _ = self.swarm.behaviour_mut().kademlia.get_providers(key);
            }
            NetworkCommand::SendFile { peer_id, file_path, group_id } => {
                let already_direct = self.swarm.is_connected(&peer_id)
                    && self.is_relayed_map.get(&peer_id).cloned() == Some(false);
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
                        let _ = tx_webrtc.send(NetworkCommand::InitiateWebRtc { peer_id: pid_webrtc }).await;
                    });
                }

                let dc_store = Arc::clone(&self.data_channels);
                let tx = self.command_tx.clone();

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
                    let _ = tx.send(NetworkCommand::SendFileFinalize { peer_id, file_path, has_dc_already, group_id }).await;
                });
            }
            NetworkCommand::SendFileFinalize { peer_id, file_path, has_dc_already: _, group_id } => {
                let is_connected_now = self.swarm.is_connected(&peer_id);
                let relayed_map_snapshot = self.is_relayed_map.get(&peer_id).cloned();
                let tx = self.command_tx.clone();
                let storage = self.storage.clone();
                let local_peer_id = *self.swarm.local_peer_id();

                tokio::spawn(async move {
                    let is_relayed = if is_connected_now {
                        relayed_map_snapshot.unwrap_or(true)
                    } else {
                        true
                    };

                    let _ = Self::process_outgoing_file(peer_id, file_path, is_relayed, tx, storage, local_peer_id, group_id).await;
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
                for (peer_id, addr) in get_bootstrap_nodes() {
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
            NetworkCommand::InitiateWebRtc { peer_id } => {
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
            NetworkCommand::StartMediaStream { peer_id, media_type } => {
                let pc_clone = { let pcs = self.peer_connections.read(); pcs.get(&peer_id).cloned() };
                if let Some(pc) = pc_clone { MediaManager::add_media_tracks(pc, media_type).await?; }
            }
            NetworkCommand::CloseWebRtc { peer_id } => { 
                self.data_channels.write().remove(&peer_id);
                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
                if let Some(pc) = pc { let _ = pc.close().await; } 
            }
            NetworkCommand::WebRtcFailed { peer_id } => {
                println!("Peer Connection State has changed: failed");
                self.data_channels.write().remove(&peer_id);
                let pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer_id) };
                if let Some(pc) = pc { let _ = pc.close().await; }

                // RESTORE/UPDATE STATUS: WebRTC channel failed, fall back to current libp2p link state
                let is_connected = self.swarm.is_connected(&peer_id);
                let status: u8 = if is_connected {
                    if self.is_relayed_map.get(&peer_id).cloned().unwrap_or(false) {
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
                    if let Ok(recipient) = recipient_id.parse::<PeerId>() {
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
                        // Clean up existing WebRTC resources first to avoid stale/ghost channels
                        self.data_channels.write().remove(&peer);
                        let old_pc = { let mut pcs = self.peer_connections.write(); pcs.remove(&peer) };
                        if let Some(pc) = old_pc {
                            let _ = pc.close().await;
                        }

                        if let Ok((pc, mut dc_rx)) = MediaManager::create_peer_connection(false, Arc::clone(&self.reward_tracker), peer, self.command_tx.clone()).await {
                            let dc_store = Arc::clone(&self.data_channels);
                            tokio::spawn(async move {
                                if let Some(dc) = dc_rx.recv().await {
                                    dc_store.write().insert(peer, dc);
                                }
                            });

                            if let Ok(answer_sdp) = MediaManager::handle_offer(signal.sdp, Arc::clone(&pc)).await {
                                self.peer_connections.write().insert(peer, pc);
                                let response = WebRtcSignal { signal_type: "answer".to_owned(), sdp: answer_sdp };
                                let _ = self.forward_to_mesh(peer, SignalingPayload::WebRtc(response), false).await;
                            }
                        }
                    }
                    "answer" => {
                        let pc_opt = self.peer_connections.read().get(&peer).cloned();
                        if let Some(pc) = pc_opt { let _ = MediaManager::handle_answer(signal.sdp, pc).await; }
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
            SignalingPayload::ChatMessage { content, msg_id, timestamp } => {
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
                tokio::task::spawn_blocking(move || storage.store_message_with_id(&peer_id_str, &mid, &c, false));
                let ack = SignalingPayload::Acknowledgement { msg_id: msg_id.clone(), status: 1 };
                let _ = self.forward_to_mesh(peer, ack, false).await;
                
                // Pack [timestamp, msg_id_len, msg_id, content] for UI
                let mut data = timestamp.to_be_bytes().to_vec();
                let msg_id_bytes = msg_id.as_bytes();
                let msg_id_len = msg_id_bytes.len() as u8;
                data.push(msg_id_len);
                data.extend(msg_id_bytes);
                data.extend(content.as_bytes());
                crate::dispatch_global_event(2, &data);
            }
            SignalingPayload::FileChunkRequest { transfer_id, chunk_index } => {
                println!("[Mesh] Received chunk request for {} (index {}) from {}", transfer_id, chunk_index, peer);
                if let Some(seeder) = self.active_seeders.get_mut(&transfer_id) {
                    let path = seeder.file_path.clone();
                    let csize = seeder.chunk_size;
                    let tchunks = seeder.total_chunks;
                    let tx = self.command_tx.clone();
                    let tid = transfer_id.clone();
                    let p_id = peer;
                    
                    seeder.bytes_sent += csize as usize;
                    self.reward_tracker.record_relay(&peer.to_string(), csize as u64);
                    let elapsed = seeder.start_time.elapsed().as_secs_f64();
                    let speed_bps = if elapsed > 0.01 { (seeder.bytes_sent as f64 * 8.0) / elapsed } else { 0.0 };

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

                                    // Update Sender UI with upload speed and correct metadata
                                    let progress = FileTransferProgress {
                                        transfer_id: tid,
                                        peer_id: p_id.to_string(),
                                        filename,
                                        mime_type,
                                        progress: (chunk_index as f32 + 1.0) / tchunks as f32,
                                        is_complete: chunk_index + 1 == tchunks,
                                        is_verified: true,
                                        is_outgoing: true,
                                        local_path: Some(path),
                                        start_time_ms: 0,
                                        speed_bps,
                                    };
                                    crate::dispatch_global_event(12, &serde_json::to_vec(&progress).unwrap_or_default());
                                }
                            }
                        }
                    });
                }
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
            SignalingPayload::FileTransfer { transfer_id, filename, mime_type, file_hash, total_size, is_relayed, sender_peer_id, group_id } => {
                // BUG 3 FIX: Use the actual sender's peer ID if provided, otherwise fallback to the anchor peer
                let actual_seeder_peer = if let Some(sid) = &sender_peer_id {
                    sid.parse::<PeerId>().unwrap_or(peer)
                } else {
                    peer
                };

                self.incoming_transfers.insert(transfer_id.clone(), IncomingTransfer {
                    filename: filename.clone(),
                    mime_type: mime_type.clone(),
                    file_hash: file_hash.clone(),
                    total_size,
                    total_chunks: 0,
                    received_chunks: HashMap::new(),
                    peer_id: actual_seeder_peer,
                    providers: vec![actual_seeder_peer],
                    start_time: Instant::now(),
                    last_update: Instant::now(),
                    is_relayed,
                    group_id: group_id.clone(),
                });

                // SOVEREIGN SWARM: If this is a relayed (cross-network) transfer,
                // trigger a DHT search to find other providers/seeders for this file.
                if is_relayed {
                    let tx = self.command_tx.clone();
                    let hash = file_hash.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(NetworkCommand::FindProviders { file_hash: hash }).await;
                    });
                }

                let progress = FileTransferProgress { 
                    transfer_id: transfer_id.clone(), 
                    peer_id: actual_seeder_peer.to_string(), 
                    filename: filename.clone(), 
                    mime_type: mime_type.clone(),
                    progress: 0.0, 
                    is_complete: false, 
                    is_verified: false,
                    is_outgoing: false, 
                    local_path: None,
                    start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                    speed_bps: 0.0,
                };
                let peer_id_str = actual_seeder_peer.to_string();
                let storage = Arc::clone(&self.storage);
                let mid = transfer_id.clone();
                if let Some(ref gid) = group_id {
                    let gid_clone = gid.clone();
                    if let Ok(json_str) = serde_json::to_string(&progress) {
                        let c = format!("[FILE]:{}", json_str);
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_group_message(&gid_clone, &peer_id_str, &mid, &c);
                        });
                    }
                } else {
                    if let Ok(json_str) = serde_json::to_string(&progress) {
                        let c = format!("[FILE]:{}", json_str);
                        tokio::task::spawn_blocking(move || {
                            let _ = storage.store_message_with_id(&peer_id_str, &mid, &c, false);
                        });
                    }
                }
                let data = serde_json::to_vec(&progress).unwrap_or_default();
                crate::dispatch_global_event(12, &data);

                // Start pulling chunks ONLY if the sender is not pushing them directly
                if is_relayed {
                    println!("[Mesh] Relay transfer detected. Initiating primed pull sequence (4 deep) for {}", transfer_id);
                    let tx = self.command_tx.clone();
                    let tid = transfer_id.clone();
                    tokio::spawn(async move {
                        for i in 0..4 {
                            let req = SignalingPayload::FileChunkRequest { transfer_id: tid.clone(), chunk_index: i };
                            let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: actual_seeder_peer, payload: req }).await;
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    });
                } else {
                    println!("[Mesh] Direct transfer detected. Waiting for chunks to be pushed for {}", transfer_id);
                }
            }
            SignalingPayload::FileChunk { transfer_id, chunk_index, total_chunks, data_base64 } => {
                println!("[Mesh] Received chunk {}/{} for {}", chunk_index, total_chunks, transfer_id);
                self.handle_file_chunk(peer, transfer_id, chunk_index, total_chunks, data_base64).await;
            }
            SignalingPayload::GroupManifestRequest { group_id } => {
                if let Ok(Some(group)) = self.storage.get_group(&group_id) {
                    let members = serde_json::from_str(&group.members_json).unwrap_or_default();
                    let payload = SignalingPayload::GroupManifest {
                        group_id,
                        name: group.name,
                        description: group.description,
                        members,
                        secret: group.secret,
                    };
                    let _ = self.forward_to_mesh(peer, payload, false).await;
                }
            }
            SignalingPayload::GroupInvite { group_id, name, description, inviter_peer_id, group_secret_wrapped, members } => {
                println!("[Mesh] Received GroupInvite for group: {} from {}", name, inviter_peer_id);
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
                    if let Ok(true) = group::GroupManager::verify_action(&signed_action, &members) {
                        match signed_action.action {
                            GroupAction::Message { content_encrypted, msg_id } => {
                                if let Ok(Some(group_info)) = self.storage.get_group(&signed_action.group_id) {
                                    use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
                                    if content_encrypted.len() >= 12 {
                                        let nonce = Nonce::from_slice(&content_encrypted[0..12]);
                                        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&group_info.secret));
                                        if let Ok(decrypted) = cipher.decrypt(nonce, &content_encrypted[12..]) {
                                            let content = String::from_utf8_lossy(&decrypted).into_owned();
                                            let _ = self.storage.store_group_message(&signed_action.group_id, &signed_action.signer_peer_id, &msg_id, &content);
                                            
                                            let mut event_data = vec![signed_action.group_id.len() as u8];
                                            event_data.extend(signed_action.group_id.as_bytes());
                                            event_data.push(signed_action.signer_peer_id.len() as u8);
                                            event_data.extend(signed_action.signer_peer_id.as_bytes());
                                            event_data.extend(content.as_bytes());
                                            crate::dispatch_global_event(21, &event_data);
                                        }
                                    }
                                }
                            }
                            GroupAction::AddMember { metadata } => {
                                let mut members = members;
                                if !members.iter().any(|m| m.peer_id == metadata.peer_id) {
                                    members.push(metadata);
                                    let members_json = serde_json::to_string(&members).unwrap_or_default();
                                    let _ = self.storage.update_group_members(&signed_action.group_id, &members_json);
                                    crate::dispatch_global_event(23, signed_action.group_id.as_bytes());
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
                                crate::dispatch_global_event(22, signed_action.group_id.as_bytes());
                            }
                        }
                    }
                }
            }
            SignalingPayload::GroupManifest { group_id, name, description, members, secret } => {
                let _ = self.storage.save_group_secret(&group_id, &secret);
                let members_json = serde_json::to_string(&members).unwrap_or_default();
                let _ = self.storage.upsert_group(&group_id, &name, &description, &members_json);
                crate::dispatch_global_event(23, group_id.as_bytes());
                
                let mut data = group_id.into_bytes();
                data.push(0);
                data.extend(name.as_bytes());
                data.push(0);
                data.extend(members_json.as_bytes());
                data.push(0);
                data.extend(&secret);
                crate::dispatch_global_event(20, &data);
            }
            SignalingPayload::FileTransferComplete { transfer_id } => {
                let mut local_path = None;
                let mut filename = "".to_string();
                let mut mime_type = "".to_string();
                let mut is_group_transfer = false;
                
                if let Some(seeder) = self.active_seeders.get(&transfer_id) {
                    let path_str = seeder.file_path.clone();
                    local_path = Some(path_str.clone());
                    let p_path = std::path::Path::new(&path_str);
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
                }

                // MANDATE: In 1-to-1 transfers, stop seeding once receiver confirms receipt.
                // In group transfers, we continue seeding until the whole group is satisfied.
                if !is_group_transfer {
                    println!("[Mesh] 1-to-1 transfer {} complete. Removing seeder and taking off mesh.", transfer_id);
                    self.active_seeders.remove(&transfer_id);
                } else {
                    println!("[Mesh] Group member received transfer {}. Continuing to seed for the rest of the group.", transfer_id);
                }

                let peer_id_str = peer.to_string();
                let storage = Arc::clone(&self.storage);
                let msg_id = transfer_id.clone();
                let progress = FileTransferProgress {
                    transfer_id: transfer_id.clone(),
                    peer_id: peer.to_string(),
                    filename,
                    mime_type,
                    progress: 1.0,
                    is_complete: true,
                    is_verified: true,
                    is_outgoing: true,
                    local_path,
                    start_time_ms: 0,
                    speed_bps: 0.0,
                };
                if let Ok(json_str) = serde_json::to_string(&progress) {
                    let c = format!("[FILE]:{}", json_str);
                    tokio::task::spawn_blocking(move || storage.store_message_with_id(&peer_id_str, &msg_id, &c, true));
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
                if let Ok(messages) = self.storage.drain_mailbox(&peer) {
                    let _ = self.forward_to_mesh(peer, SignalingPayload::MailboxDrained(messages), false).await;
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
            _ => {}
        }
    }

    async fn process_outgoing_file(
        peer_id: PeerId, 
        file_path: String, 
        is_relayed: bool, 
        tx: mpsc::Sender<NetworkCommand>, 
        storage: Arc<crate::storage::StorageService>,
        local_peer_id: PeerId,
        group_id: Option<String>,
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

        let transfer_id = format!("{}_{}", filename, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs());
        
        // ADAPTIVE CHUNKING: Direct P2P uses 256KB chunks, Relay uses 16KB (Reduced for extreme reliability)
        let chunk_size = if is_relayed { 16 * 1024 } else { 256 * 1024 }; 
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
            progress: 0.0, 
            is_complete: false, 
            is_verified: false,
            is_outgoing: true, 
            local_path: Some(file_path.clone()),
            start_time_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
            speed_bps: 0.0,
        };

        // Persistent History: Save file manifest
        let peer_id_str = peer_id.to_string();
        let msg_id = transfer_id.clone();
        let gid_opt = group_id.clone();
        if let Ok(json_str) = serde_json::to_string(&initial_progress) {
            let content = format!("[FILE]:{}", json_str);
            let s = Arc::clone(&storage);
            if let Some(gid) = gid_opt {
                tokio::task::spawn_blocking(move || s.store_group_message(&gid, &peer_id_str, &msg_id, &content));
            } else {
                tokio::task::spawn_blocking(move || s.store_message_with_id(&peer_id_str, &msg_id, &content, true));
            }
        }

        // --- PULL MODEL: Register as an active seeder to serve chunk requests ---
        let _ = tx.send(NetworkCommand::RegisterSeeder {
            peer_id,
            transfer_id: transfer_id.clone(),
            file_path: file_path.clone(),
            file_hash: file_hash.clone(),
            chunk_size,
            total_chunks,
            group_id: group_id.clone(),
        }).await;

        let _ = tx.send(NetworkCommand::SendFileChunk { peer_id, payload: transfer_payload, progress: initial_progress.clone() }).await;

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
            let speed_bps = if elapsed_s > 0.1 { (bytes_sent as f64 * 8.0) / elapsed_s } else { 0.0 };

            let progress = FileTransferProgress { 
                transfer_id: transfer_id.clone(), 
                peer_id: peer_id.to_string(), 
                filename: filename.clone(), 
                mime_type: mime_type.clone(),
                progress: (i + 1) as f32 / total_chunks as f32, 
                is_complete: i + 1 == total_chunks, 
                is_verified: false,
                is_outgoing: true, 
                local_path: Some(file_path.clone()),
                start_time_ms: initial_progress.start_time_ms,
                speed_bps,
            };
            
            // Loop until the chunk is successfully accepted by the networking queue
            loop {
                // To safely check if it succeeded, we'd need to change how the channel works.
                // Since SendFileChunk is handled via the command channel, we can't easily wait here.
                // But the actual forward_to_mesh now drops chunks rather than buffering them infinitely.
                // To avoid overloading the channel itself, we simply apply a pacing delay.
                let _ = tx.send(NetworkCommand::SendFileChunk { peer_id, payload: chunk_payload.clone(), progress: progress.clone() }).await;
                break;
            }
            
            // ADAPTIVE PACING: Direct P2P uses 20ms, Relay uses 250ms
            tokio::time::sleep(Duration::from_millis(if is_relayed { 250 } else { 20 })).await;
        }
        


        println!("✅ File transfer chunks sent for {}. Waiting for verification from peer...", filename);
        Ok(())
    }
}
