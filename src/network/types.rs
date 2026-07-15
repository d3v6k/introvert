use serde::{Serialize, Deserialize};
use libp2p::{PeerId, Multiaddr};
use crate::media::WebRtcSignal;
use crate::identity::SovereignIdentity;

pub const ANCHOR_PROVIDER_KEY: &[u8] = b"/introvert/anchor_nodes";
pub const RBN_PEER_ID: &str = "12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a";
pub const RBN_WS_URL: &str = "wss://47.89.252.80/tunnel";
pub const RBN_WS_URL_PLAIN: &str = "ws://47.89.252.80:80/tunnel";

// --- Message Priority Levels ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    /// Background sync, file transfers, bulk operations
    Low = 0,
    /// Normal messages, reactions, edits
    Normal = 1,
    /// Calls, typing indicators, time-sensitive data
    Urgent = 2,
}

impl Default for MessagePriority {
    fn default() -> Self {
        MessagePriority::Normal
    }
}

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
    MailboxStore { recipient_id: String, payload: Vec<u8>, #[serde(default)] original_msg_id: Option<String> },
    MailboxDrain,
    MailboxDrained(Vec<MailboxMessage>),
    Acknowledgement { msg_id: String, status: u8 },
    MailboxStored { recipient_id: String, original_msg_id: String },
    Handshake(SovereignIdentity),
    Offer(WebRtcSignal),
    Answer(WebRtcSignal),
    Candidate(String),
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
        rbn_pubkey: Vec<u8>,
        rbn_signature: Vec<u8>,
    },
    HandleResolveRequest {
        handle: String,
    },
    HandleResolveResponse {
        handle: String,
        peer_id: String,
        verified: bool,
    },
    IdentifySleepState {
        device_type: String,
        push_token: String,
    },
    TypingStart { chat_id: String },
    TypingStop { chat_id: String },
    Heartbeat { timestamp: i64 },
    FileTransfer { 
        transfer_id: String, 
        filename: String, 
        mime_type: String, 
        file_hash: String, 
        total_size: u64, 
        #[serde(default)]
        is_relayed: bool,
        #[serde(default)]
        sender_peer_id: Option<String>,
        #[serde(default)]
        group_id: Option<String>,
    },
    FileChunkRequest { transfer_id: String, chunk_index: u32, #[serde(default)] chunk_size: Option<u32>, #[serde(default)] relay_hint: Option<String> },
    FileChunk { transfer_id: String, chunk_index: u32, total_chunks: u32, data_base64: String },
    FileTransferComplete { transfer_id: String },
    FileTransferError { transfer_id: String, reason: String },
    TransitFileChunk { target_peer: String, chunk: Box<SignalingPayload> },
    DeleteMessage { msg_id: String },
    EditMessage { msg_id: String, new_content: String },
    SetRetention { seconds: u32 },
    MessageReaction { msg_id: String, emoji: String },
    GroupManifestRequest { group_id: String, alias: Option<String>, avatar: Option<String>, #[serde(default)] handle: Option<String>, #[serde(default)] requester_static_key: Option<Vec<u8>> },
    GroupInvite { group_id: String, name: String, description: String, inviter_peer_id: String, group_secret_wrapped: Vec<u8>, members: Vec<GroupMemberMetadata> },
    GroupAction(SignedGroupAction),
    GroupManifest { group_id: String, name: String, description: String, members: Vec<GroupMemberMetadata> },
    GroupJoinRejected { group_id: String, group_name: String, reason: String },
    RequestHandshake,
    ProfileRequest,
    ProfileResponse {
        name: String,
        handle: String,
        avatar_base64: Option<String>,
        #[serde(default)]
        prestige_tier: u8,
    },
    ChatSyncRequest {
        chat_id: String,
        is_group: bool,
        known_msg_ids: Vec<String>,
        limit: u32,
    },
    ChatSyncResponse {
        chat_id: String,
        is_group: bool,
        messages: Vec<SyncMessage>,
        missing_ids: Vec<String>,
        #[serde(default)]
        is_relay: bool,
    },
    /// Client telemetry submission for reward processing
    TelemetryEnvelope {
        peer_id: String,
        solana_wallet: String,
        solana_ata: String,
        epoch_id: String,
        metrics: [u64; 13],
        unique_peers: Vec<String>,
        is_rbn: bool,
        is_edge_node: bool,
        prestige_tier: u8,
        proof_hash: String,
        client_signature: Vec<u8>,
        timestamp: u64,
    },
    /// RBN acknowledgment that telemetry was received and processed
    TelemetryAck {
        peer_id: String,
        epoch_id: String,
        total_points: f64,
        timestamp: u64,
    },
}

/// Wrapper for SignalingPayload with priority level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrioritizedPayload {
    pub priority: MessagePriority,
    pub payload: SignalingPayload,
}

impl PrioritizedPayload {
    pub fn new(payload: SignalingPayload, priority: MessagePriority) -> Self {
        Self { priority, payload }
    }

    pub fn urgent(payload: SignalingPayload) -> Self {
        Self::new(payload, MessagePriority::Urgent)
    }

    pub fn normal(payload: SignalingPayload) -> Self {
        Self::new(payload, MessagePriority::Normal)
    }

    pub fn low(payload: SignalingPayload) -> Self {
        Self::new(payload, MessagePriority::Low)
    }

    /// Get the inherent priority of a payload type
    pub fn inherent_priority(payload: &SignalingPayload) -> MessagePriority {
        match payload {
            // Urgent: calls, typing, time-sensitive
            SignalingPayload::TypingStart { .. } |
            SignalingPayload::TypingStop { .. } |
            SignalingPayload::Heartbeat { .. } => MessagePriority::Urgent,

            // Low: file transfers, sync, bulk operations
            SignalingPayload::FileChunk { .. } |
            SignalingPayload::FileChunkRequest { .. } |
            SignalingPayload::FileTransfer { .. } |
            SignalingPayload::ChatSyncRequest { .. } |
            SignalingPayload::ChatSyncResponse { .. } => MessagePriority::Low,

            // Normal: everything else
            _ => MessagePriority::Normal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    pub msg_id: String,
    pub sender_id: String,
    pub content: String,
    pub timestamp: String,
    pub reply_to: Option<String>,
}

/// JSON-serialized request wrapper used by request_response::json::Behaviour
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalingRequest(pub SignalingPayload);

/// JSON-serialized response wrapper used by request_response::json::Behaviour
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
    ForwardWebRtcNative { peer_id: PeerId, json: String },
    HandleIncomingPayload { peer_id: PeerId, payload: SignalingPayload },
    HandleIncomingWebRtcPayload { peer_id: PeerId, payload: SignalingPayload },
    ResolveHandle { handle: String },
    SendDirectInvite { peer_id: PeerId, identity: SovereignIdentity, is_accept: bool },
    ClaimHandle { handle: String },
    BroadcastWitness { handle: String, peer_id: String, timestamp: i64, pubkey: Vec<u8>, signature: Vec<u8> },
    AddGroupMember { group_id: String, peer_id: String },
    RemoveGroupMember { group_id: String, peer_id: String, members_json: Option<String> },
    SetConnectivityType { connectivity_type: u8 },
    UpdateGroupRole { group_id: String, peer_id: String, role: GroupRole },
    PublishGroupManifest { group_id: String, code: String },
    JoinMeshByCode { code: String },
    AcceptGroupInvite { group_id: String },
    DeclineGroupInvite { group_id: String },
    ApproveGroupJoin { group_id: String, requester_peer_id: String, alias: Option<String>, avatar: Option<String>, handle: Option<String> },
    RejectGroupJoin { group_id: String, requester_peer_id: String, reason: String },
    TestManualRbn { address: String },
    VerifyManualRbnConnection { address: String, multiaddr: Multiaddr },
    BroadcastGroupMessage { group_id: String, message: String, reply_to: Option<String> },
    PublishGossipsub { topic: String, data: Vec<u8> },
    SubscribeGossipsub { group_id: String },
    ForceMeshRefresh,
    ActivateTunnel,
    /// Add a peer to the verified_rbns set after cryptographic test-dial
    /// confirms the on-chain PeerId matches the node at the claimed IP.
    AddVerifiedRbn { peer_id: PeerId },
    SendManualTelemetry,
    RegisterSeeder { peer_id: PeerId, transfer_id: String, file_path: String, file_hash: String, chunk_size: u32, total_chunks: u32, group_id: Option<String> },
    UnregisterSeeder { transfer_id: String },
    FindProviders { file_hash: String },
    StoreInMailbox { peer_id: PeerId, payload: SignalingPayload },
    ClearMailboxForPeer { peer_id: PeerId },
    LookupPeerHandle { peer_id: String },
    CancelFileTransfer { transfer_id: String },
    RecheckConnection { peer_id: PeerId },
    HandleDiagnosticTimeout { peer_id: PeerId },
    RequestSwarmStats,
    PollPeerProfile { peer_id: PeerId },
    SyncChatMessages { peer_id: PeerId, chat_id: String, is_group: bool, is_full: bool },
    RelaySyncedMessages { chat_id: String, messages: Vec<SyncMessage> },
    IntroClawTick {
        battery_pct: i32,
        is_background: bool,
        connected_peers: Vec<String>,
        mdns_discovered: Vec<String>,
        is_mobile_data: bool,
        network_type: String,
    },
    IntroClawSetActive { active: bool },
    IntroClawSetNodeMode { enabled: bool },
    IntroClawNetworkRecon {
        result_tx: tokio::sync::oneshot::Sender<String>,
    },
    IntroClawNetworkHeal {
        peer_id: PeerId,
        result_tx: tokio::sync::oneshot::Sender<String>,
    },
    IntroClawGetActivityLog {
        result_tx: tokio::sync::oneshot::Sender<String>,
    },
    IntroClawVoipStartCall {
        peer_id: String,
        is_video: bool,
    },
    IntroClawVoipEndCall,
    IntroClawVoipRecordSample {
        rtt_ms: u64,
        packet_loss_pct: f64,
        jitter_ms: u64,
        bitrate_kbps: u64,
        is_relayed: bool,
        codec: String,
    },
    IntroClawVoipGetQuality {
        result_tx: tokio::sync::oneshot::Sender<String>,
    },
    IntroClawVoipGetDowngradeRecommendation {
        result_tx: tokio::sync::oneshot::Sender<String>,
    },
    IntroClawSetActiveChat {
        chat_id: String,
        peer_id: Option<String>,
        is_group: bool,
    },
    IntroClawClearActiveChat,
    IntroClawSetActiveGroupMembers {
        members: Vec<String>,
    },
    IntroClawOnAppLaunch {
        result_tx: tokio::sync::oneshot::Sender<()>,
    },
    SetAppIdleState { is_idle: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferProgress {
    pub transfer_id: String,
    pub peer_id: String,
    pub filename: String,
    pub mime_type: String,
    pub file_hash: String,
    pub progress: f32,
    pub is_complete: bool,
    pub is_verified: bool,
    pub is_outgoing: bool,
    pub local_path: Option<String>,
    pub start_time_ms: u64,
    pub speed_bps: f64,
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

pub type FfiNetworkCallback = extern "C" fn(event_type: i32, data_ptr: *const u8, data_len: usize);
