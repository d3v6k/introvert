use libp2p::{
    kad::{self, Record, RecordKey, QueryId},
    request_response,
    swarm::SwarmEvent,
    core::transport::ListenerId,
    identity::Keypair,
    PeerId, Swarm, Multiaddr,
    futures::{StreamExt, FutureExt},
};
use base64::{Engine as _, engine::general_purpose};
use sha2::{Sha256, Digest};
use std::time::{Duration, Instant};
use std::io::Read;
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use chrono::Utc;
use parking_lot::{RwLock, Mutex};
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;
use libp2p::{autonat, identify};
use x25519_dalek::{StaticSecret, PublicKey};
use tracing::{info, warn, error, debug};

pub mod noise_session;
pub mod wormhole;
pub mod behaviour;
pub mod config;
pub mod group;
pub mod registry;
pub mod tunnel;
pub mod codec; // Binary codec for /introvert/signaling/2.0.0 (inactive on client until Phase 2)

use crate::media::{MediaManager, WebRtcSignal};
use crate::identity::SovereignIdentity;
use noise_session::NoiseSession;
use codec::{IntrovertCodec, BinarySignalingRequest, BinarySignalingResponse};
pub use behaviour::{IntrovertBehaviour, IntrovertBehaviourEvent};

pub const ANCHOR_PROVIDER_KEY: &[u8] = b"/introvert/anchor_nodes";
pub const RBN_PEER_ID: &str = "12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a";
pub const RBN_WS_URL: &str = "wss://47.89.252.80/tunnel";

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
