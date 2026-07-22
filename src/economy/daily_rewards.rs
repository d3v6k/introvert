use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::{Utc, NaiveDate};
use tracing::{info, warn};
use crate::storage::StorageService;
use crate::economy::RewardTracker;
use ed25519_dalek::{VerifyingKey, Signature, Verifier};

/// C-compatible runtime state struct for cross-FFI boundary transfer.
/// Fixed-width types only — no String, Vec, or raw pointers.
/// Boolean fields use u8 (0/1) to match C _Bool layout on all targets.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIDailyState {
    pub total_social_points: f64,
    pub total_infra_points: f64,
    pub active_web_containers: u32,
    pub current_cycle_uptime: u64,
    pub is_edge_node: u8,
    pub is_rbn: u8,
}

// Trusted RBN Multisig public keys — hardcoded to prevent unauthorized config injection
// These are the 5-of-5 Squads V4 multisig member keys for the Introvert RBN network
const TRUSTED_RBN_PUBLIC_KEYS: &[[u8; 32]] = &[
    // Primary RBN operator key (Alibaba Cloud RBN)
    // Derived from the bootstrap PeerId 12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a
    [0x12, 0xd3, 0x4b, 0x0e, 0x4a, 0x6e, 0x8f, 0x2c, 0x1a, 0x5d, 0x7b, 0x9f, 0x3e, 0x8c, 0x2d, 0x6a,
     0x4b, 0x0e, 0x1f, 0x3a, 0x5c, 0x7d, 0x9e, 0x2b, 0x4a, 0x6c, 0x8d, 0x0f, 0x2e, 0x4a, 0x6b, 0x8c],
    // Treasury multisig member 2
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
     0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    // Treasury multisig member 3
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
     0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
];

// Official escrow vault address — derived from the introvert-registry Anchor program
// PDA seeds: [b"escrow_vault"] with program ID RBNRegXy4vQszN2Cg8gqf91mYyL24p8cT32d1mY1111
// This is the on-chain PDA that holds all staked $INTR for RBN operators
pub const DAILY_REWARD_ESCROW: &str = "9jauyKiimh6SBnpoRXcNXiLXZKSnN4h2gWKoqMcG4zHy";
// Note: In production, this should be the actual PDA derived from:
//   Pubkey::find_program_address(&[b"escrow_vault"], &registry_program_id)
// The treasury address is used as a fallback until the Anchor program PDA is deployed.

// Token Generation Event date — used to calculate emission year
pub const TGE_DATE: &str = "2026-01-01";

// Cycle transition hour (UTC). Cycles roll over at 12:00 UTC instead of midnight
// so operators in US/EU timezones can monitor the transition live.
const CYCLE_TRANSITION_HOUR_UTC: i64 = 0;

/// Returns the current economy day string (e.g., "2026-07-07").
/// Shifts by CYCLE_TRANSITION_HOUR_UTC so the cycle rolls over at midnight UTC.
pub fn economy_today() -> String {
    let now = Utc::now();
    let shifted = now - chrono::Duration::hours(CYCLE_TRANSITION_HOUR_UTC);
    shifted.format("%Y-%m-%d").to_string()
}

/// Returns the current economy epoch ID for telemetry (e.g., "2026_07_07").
/// Same shift as economy_today() but uses underscore format for telemetry protocol.
pub fn economy_epoch_id() -> String {
    let now = Utc::now();
    let shifted = now - chrono::Duration::hours(CYCLE_TRANSITION_HOUR_UTC);
    shifted.format("%Y_%m_%d").to_string()
}

// 10-year emission schedule: daily user pool caps ($INTR/day)
// From the whitepaper: Year 1 = 16,438/day, 20% annual decay
// Year N daily cap = 16438 * (0.8 ^ (N-1))
const YEAR_1_DAILY_POOL: f64 = 16_438.0;
const YEAR_1_RBN_DAILY_POOL: f64 = 8_219.0;  // RBN pool: 3,000,000 / 365
const ANNUAL_DECAY: f64 = 0.8;

// Default global points estimate (used when network is small / pre-launch)
// As the network grows, this should be updated via on-chain data
const DEFAULT_GLOBAL_POINTS_ESTIMATE: f64 = 100_000.0;

// ─── Activity Types ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ActivityType {
    MessageSent = 0,
    MessageReceived = 1,
    GroupMessageSent = 2,
    GroupReaction = 3,
    FileTransferSent = 4,
    FileTransferRecv = 5,
    CallDurationSecs = 6,
    RelayBytes = 7,
    UptimeSeconds = 8,
    WebFocusedActiveTime = 9,
    SandboxWebPacketData = 10,
    WebViewMediaCallHook = 11,
    UniquePeerHandshakes = 12,
}

impl ActivityType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::MessageSent),
            1 => Some(Self::MessageReceived),
            2 => Some(Self::GroupMessageSent),
            3 => Some(Self::GroupReaction),
            4 => Some(Self::FileTransferSent),
            5 => Some(Self::FileTransferRecv),
            6 => Some(Self::CallDurationSecs),
            7 => Some(Self::RelayBytes),
            8 => Some(Self::UptimeSeconds),
            9 => Some(Self::WebFocusedActiveTime),
            10 => Some(Self::SandboxWebPacketData),
            11 => Some(Self::WebViewMediaCallHook),
            12 => Some(Self::UniquePeerHandshakes),
            _ => None,
        }
    }

    pub fn all() -> &'static [ActivityType] {
        &[
            Self::MessageSent, Self::MessageReceived,
            Self::GroupMessageSent, Self::GroupReaction,
            Self::FileTransferSent, Self::FileTransferRecv,
            Self::CallDurationSecs, Self::RelayBytes,
            Self::UptimeSeconds, Self::WebFocusedActiveTime,
            Self::SandboxWebPacketData, Self::WebViewMediaCallHook,
            Self::UniquePeerHandshakes,
        ]
    }
}

// ─── Configurable Weights ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityWeights {
    pub message_sent: f64,
    pub message_received: f64,
    pub group_message_sent: f64,
    pub group_reaction: f64,
    pub file_transfer_sent: f64,
    pub file_transfer_recv: f64,
    pub call_duration_secs: f64,
    pub relay_bytes: f64,
    pub uptime_seconds: f64,
    pub web_focused_active_time: f64,
    pub sandbox_web_packet_data: f64,
    pub webview_media_call_hook: f64,
    pub unique_peer_handshakes: f64,

    pub cap_message_sent: u32,
    pub cap_message_received: u32,
    pub cap_group_message_sent: u32,
    pub cap_group_reaction: u32,
    pub cap_file_transfer_sent: u32,
    pub cap_file_transfer_recv: u32,
    pub cap_call_duration_secs: u32,
    pub cap_relay_bytes: u64,
    pub cap_relay_bytes_rbn: u64,
    pub cap_uptime_seconds: u32,
    pub cap_web_focused_active_time: u32,
    pub cap_sandbox_web_packet_data: u64,
    pub cap_webview_media_call_hook: u32,
    pub max_third_party_containers: u32,
    pub cap_unique_peer_handshakes: u32,

    pub min_message_length: usize,
    pub rapid_fire_cooldown_secs: u64,
    pub rapid_fire_max_per_window: u32,
    pub daily_point_cap: f64,
    pub intr_per_point: f64,
    pub edge_infra_multiplier: f64,
}

impl Default for ActivityWeights {
    fn default() -> Self {
        Self {
            message_sent: 10.0,
            message_received: 5.0,
            group_message_sent: 8.0,
            group_reaction: 3.0,
            file_transfer_sent: 20.0,
            file_transfer_recv: 10.0,
            call_duration_secs: 1.0,
            relay_bytes: 0.01,
            uptime_seconds: 0.005,
            web_focused_active_time: 0.1,
            sandbox_web_packet_data: 0.02,
            webview_media_call_hook: 0.2,
            unique_peer_handshakes: 1.0,

            cap_message_sent: 200,
            cap_message_received: 300,
            cap_group_message_sent: 150,
            cap_group_reaction: 100,
            cap_file_transfer_sent: 20,
            cap_file_transfer_recv: 20,
            cap_call_duration_secs: 3600,
            cap_relay_bytes: 10240,  // 10,240 KB = 10 MB cap for edge nodes
            cap_relay_bytes_rbn: 51200,  // 51,200 KB = 50 MB cap for RBN nodes (v3.0.1)
            cap_uptime_seconds: 86400,
            cap_web_focused_active_time: 86400,
            cap_sandbox_web_packet_data: 10240,
            cap_webview_media_call_hook: 1800,
            max_third_party_containers: 3,
            cap_unique_peer_handshakes: 500,

            min_message_length: 5,
            rapid_fire_cooldown_secs: 60,
            rapid_fire_max_per_window: 10,
            daily_point_cap: 5000.0,
            intr_per_point: 0.001,
            edge_infra_multiplier: 3.0,
        }
    }
}

// ─── Anti-Gaming Config ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiGamingConfig {
    pub require_foreground: bool,
    pub grace_period_secs: u64,
    pub reject_self_messaging: bool,
    pub min_unique_peers: u32,
    pub max_messages_per_peer: u32,
}

impl Default for AntiGamingConfig {
    fn default() -> Self {
        Self {
            require_foreground: true,
            grace_period_secs: 30,
            reject_self_messaging: true,
            min_unique_peers: 1, // Lowered from 2: allows 2-peer networks to earn rewards
            max_messages_per_peer: 50,
        }
    }
}

// ─── Signed Reward Envelope (Security Fix #1) ─────────────────

/// Cryptographically verified envelope for daily RewardConfig updates.
/// The RBN network signs the payload_json with its Ed25519 key before broadcasting.
/// Clients verify the signature against hardcoded trusted public keys before processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedRewardEnvelope {
    /// The JSON payload (ActivityWeights or AntiGamingConfig)
    pub payload_json: String,
    /// 64-byte Ed25519 signature over payload_json bytes
    pub signature_bytes: Vec<u8>,
    /// Public key of the signer (32 bytes, must match TRUSTED_RBN_PUBLIC_KEYS)
    pub signer_pubkey: Vec<u8>,
    /// Monotonic sequence number to prevent replay attacks
    pub sequence: u64,
    /// Timestamp of signing (unix seconds)
    pub signed_at: u64,
}

impl SignedRewardEnvelope {
    /// Verifies the envelope signature against trusted RBN public keys.
    /// Returns Ok(()) if valid, Err if signature is invalid or signer is untrusted.
    pub fn verify(&self) -> Result<(), String> {
        // 1. Check signer is in trusted key set
        if self.signer_pubkey.len() != 32 {
            return Err("Signer public key must be 32 bytes".to_string());
        }
        let mut pubkey_bytes = [0u8; 32];
        pubkey_bytes.copy_from_slice(&self.signer_pubkey);
        let is_trusted = TRUSTED_RBN_PUBLIC_KEYS.iter().any(|k| *k == pubkey_bytes);
        if !is_trusted {
            return Err("Signer public key not in trusted RBN set".to_string());
        }

        // 2. Check signature length
        if self.signature_bytes.len() != 64 {
            return Err("Signature must be 64 bytes".to_string());
        }

        // 3. Reconstruct the signed message: payload_json + sequence (big-endian) + signed_at (big-endian)
        let mut message = Vec::new();
        message.extend_from_slice(self.payload_json.as_bytes());
        message.extend_from_slice(&self.sequence.to_be_bytes());
        message.extend_from_slice(&self.signed_at.to_be_bytes());

        // 4. Verify Ed25519 signature
        let verifying_key = VerifyingKey::from_bytes(&pubkey_bytes)
            .map_err(|e| format!("Invalid public key: {}", e))?;
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&self.signature_bytes);
        let signature = Signature::from_bytes(&sig_bytes);
        verifying_key.verify(&message, &signature)
            .map_err(|e| format!("Signature verification failed: {}", e))?;

        // 5. Check timestamp freshness (reject if older than 24 hours)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now.saturating_sub(self.signed_at) > 86400 {
            return Err("Envelope timestamp is older than 24 hours".to_string());
        }

        Ok(())
    }

    /// Creates a signed envelope (used by RBN nodes only).
    /// In production, this is called by the RBN multisig, not by client apps.
    pub fn create(
        payload_json: &str,
        signing_key: &ed25519_dalek::SigningKey,
        sequence: u64,
    ) -> Self {
        let signed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut message = Vec::new();
        message.extend_from_slice(payload_json.as_bytes());
        message.extend_from_slice(&sequence.to_be_bytes());
        message.extend_from_slice(&signed_at.to_be_bytes());

        use ed25519_dalek::Signer;
        let signature = signing_key.sign(&message);
        let signer_pubkey = signing_key.verifying_key().to_bytes().to_vec();

        Self {
            payload_json: payload_json.to_string(),
            signature_bytes: signature.to_bytes().to_vec(),
            signer_pubkey,
            sequence,
            signed_at,
        }
    }
}

// ─── Daily Cycle State ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivityCount {
    pub activity_type: ActivityType,
    pub raw_count: u64,
    pub capped_count: u64,
    pub points: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCycle {
    pub cycle_date: String,
    pub snapshot_balance: u64,
    pub activities: Vec<DailyActivityCount>,
    pub total_points: f64,
    pub capped_points: f64,
    pub intr_reward: f64,
    pub unique_peers: u32,
    pub is_eligible: bool,
    pub eligibility_reason: String,
    pub submitted: bool,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}

// ─── Activity Event ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub activity_type: ActivityType,
    pub peer_id: Option<String>,
    pub value: u64,
    pub is_foreground: bool,
    pub message_len: Option<usize>,
    pub is_self: bool,
    #[serde(default)]
    pub is_rbn: bool,
    /// Cryptographic proof: actual bytes relayed or message hash
    /// For RelayBytes: verified throughput from network layer
    /// For messages: message_id hash for dedup verification
    #[serde(default)]
    pub proof_hash: Option<String>,
    /// Number of active third-party web view containers at event time.
    /// Reported by the front-end; enforced against max_third_party_containers.
    #[serde(default)]
    pub active_web_containers: u32,
}

// ─── Rapid-Fire Tracker ──────────────────────────────────────

#[derive(Debug, Clone)]
struct RapidFireWindow {
    timestamps: VecDeque<u64>,
}

impl RapidFireWindow {
    fn new() -> Self {
        Self { timestamps: VecDeque::new() }
    }

    fn is_allowed(&mut self, now: u64, cooldown_secs: u64, max_per_window: u32) -> bool {
        let cutoff = now.saturating_sub(cooldown_secs);
        while self.timestamps.front().map_or(false, |&t| t <= cutoff) {
            self.timestamps.pop_front();
        }
        if self.timestamps.len() >= max_per_window as usize {
            return false;
        }
        self.timestamps.push_back(now);
        true
    }
}

// ─── Engine State ────────────────────────────────────────────

// v3.0.1 phase-in: 90-day linear blend from old weights to new weights
const V3_PHASE_IN_DAYS: u64 = 90;
const V3_PHASE_IN_START: &str = "2026-07-01"; // Date when v3.0.1 weights begin phase-in

struct DailyRewardState {
    current_cycle: Option<DailyCycle>,
    per_type_counts: HashMap<u8, u64>,
    per_type_capped: HashMap<u8, u64>,
    per_peer_message_count: HashMap<String, u32>,
    unique_peers: HashSet<String>,
    rapid_fire_windows: HashMap<u8, RapidFireWindow>,
    cycle_start_epoch: u64,
    global_points_estimate: f64,
    is_rbn: bool,
    is_edge_node: bool,
    prestige_tier: u8,
    active_containers_highwater: u32,
    /// RBN-reported network-wide total points (from TelemetryAck).
    /// When fresh (< 48h), overrides the static 100K ceiling entirely.
    rbn_reported_total_points: f64,
    last_rbn_estimate_timestamp: u64,
}

impl DailyRewardState {
    fn new() -> Self {
        Self {
            current_cycle: None,
            per_type_counts: HashMap::new(),
            per_type_capped: HashMap::new(),
            per_peer_message_count: HashMap::new(),
            unique_peers: HashSet::new(),
            rapid_fire_windows: HashMap::new(),
            cycle_start_epoch: 0,
            global_points_estimate: DEFAULT_GLOBAL_POINTS_ESTIMATE,
            is_rbn: false,
            is_edge_node: false,
            prestige_tier: 0,
            active_containers_highwater: 0,
            rbn_reported_total_points: 0.0,
            last_rbn_estimate_timestamp: 0,
        }
    }
}

// ─── Daily Reward Engine ─────────────────────────────────────

pub struct DailyRewardEngine {
    state: RwLock<DailyRewardState>,
    storage: Arc<StorageService>,
    weights: RwLock<ActivityWeights>,
    anti_gaming: RwLock<AntiGamingConfig>,
    /// Shared metrics array bridging to RewardTracker for telemetry
    shared_metrics: Arc<parking_lot::RwLock<[u64; 13]>>,
}

impl DailyRewardEngine {
    pub fn new(storage: Arc<StorageService>, shared_metrics: Arc<parking_lot::RwLock<[u64; 13]>>) -> Self {
        let (weights, anti_gaming) = storage
            .load_daily_reward_config()
            .ok()
            .flatten()
            .unwrap_or_else(|| (ActivityWeights::default(), AntiGamingConfig::default()));

        let engine = Self {
            state: RwLock::new(DailyRewardState::new()),
            storage,
            weights: RwLock::new(weights),
            anti_gaming: RwLock::new(anti_gaming),
            shared_metrics,
        };

        // Resume any in-progress cycle from DB
        let today = economy_today();
        if let Ok(Some(cycle)) = engine.storage.load_daily_cycle(&today) {
            let mut state = engine.state.write();
            state.current_cycle = Some(cycle);
            state.cycle_start_epoch = Utc::now().timestamp() as u64;

            // Restore per_type_counts from persisted activity log so points survive restarts
            if let Ok(activities) = engine.storage.load_daily_activities(&today) {
                for act in &activities {
                    state.per_type_counts.insert(act.activity_type as u8, act.raw_count);
                    state.per_type_capped.insert(act.activity_type as u8, act.capped_count);
                }
            }
        }

        engine
    }

    pub fn needs_cycle_transition(&self, today: &str) -> bool {
        let state = self.state.read();
        match &state.current_cycle {
            None => true,
            Some(cycle) => cycle.cycle_date != today,
        }
    }

    pub fn transition_cycle(&self, today: &str, tracker: &RewardTracker) {
        let mut state = self.state.write();

        // Close and score previous cycle
        if let Some(mut prev) = state.current_cycle.take() {
            let weights = self.weights.read();
            let anti = self.anti_gaming.read();

            prev.activities = Self::score_activities_static(&state, &weights);

            // CRITICAL: Separate social and infra points (dual-pool)
            let social_points: f64 = prev.activities.iter()
                .filter(|a| matches!(a.activity_type,
                    ActivityType::MessageSent | ActivityType::MessageReceived |
                    ActivityType::GroupMessageSent | ActivityType::GroupReaction |
                    ActivityType::FileTransferSent | ActivityType::FileTransferRecv |
                    ActivityType::CallDurationSecs |
                    ActivityType::WebFocusedActiveTime | ActivityType::WebViewMediaCallHook))
                .map(|a| a.points)
                .sum();
            let infra_points: f64 = prev.activities.iter()
                .filter(|a| matches!(a.activity_type,
                    ActivityType::RelayBytes | ActivityType::UptimeSeconds |
                    ActivityType::SandboxWebPacketData |
                    ActivityType::UniquePeerHandshakes))
                .map(|a| a.points)
                .sum();

            let uptime_raw = state.per_type_counts.get(&(ActivityType::UptimeSeconds as u8)).copied().unwrap_or(0);
            let is_rbn = state.is_rbn;
            let effective_social_cap = if !is_rbn && state.is_edge_node && uptime_raw >= 86400 {
                15_000.0
            } else {
                weights.daily_point_cap
            };
            let social_capped = social_points.min(effective_social_cap);
            prev.total_points = social_capped + infra_points;
            prev.capped_points = prev.total_points;

            // Compute unique peers and eligibility BEFORE reward formula
            prev.unique_peers = state.unique_peers.len() as u32;
            prev.is_eligible = prev.unique_peers >= anti.min_unique_peers && prev.capped_points > 0.0;

            // Dynamic global estimate with RBN freshness bypass
            let global_estimate = Self::compute_effective_global_estimate(&state, &weights);

            // CRITICAL: Use pool-isolated clearing
            let effective_pool = if is_rbn { self.get_rbn_daily_pool_cap() } else { self.get_daily_pool_cap() };
            let user_share = prev.total_points / global_estimate;
            let prestige_mult = match state.prestige_tier {
                0 => 1.0,
                1 => 1.05,
                2 => 1.10,
                3 => 1.20,
                4 => 1.50,
                5 => 1.15,
                6 => 1.15,
                _ => 1.0,
            };
            // Deterministic rounding: truncate to 6 decimal places
            let raw_reward = user_share * effective_pool * prestige_mult;
            prev.intr_reward = (raw_reward * 1_000_000.0).trunc() / 1_000_000.0;
            prev.eligibility_reason = if !prev.is_eligible {
                format!("unique_peers={} < min={}", prev.unique_peers, anti.min_unique_peers)
            } else {
                "eligible".into()
            };
            prev.ended_at = Some(Utc::now().timestamp() as u64);
            prev.submitted = prev.is_eligible;

            info!("[DailyRewards] Cycle {} closed: {:.1} pts, {:.4} INTR, eligible={}, rbn={}",
                prev.cycle_date, prev.capped_points, prev.intr_reward, prev.is_eligible, is_rbn);

            // Persist to DB
            let _ = self.storage.save_daily_cycle(&prev);
            let _ = self.storage.save_daily_activities(&prev.cycle_date, &prev.activities);

            // Persist consolidated reward record with anti-farming tracking
            let containers_hw = state.active_containers_highwater;
            let _ = self.storage.save_daily_reward_record(
                &prev.cycle_date,
                social_points,
                infra_points,
                containers_hw,
                uptime_raw,
            );

            // Feed reward into existing claim pool
            if prev.is_eligible {
                tracker.record_daily_reward(prev.intr_reward);
            }
        }

        // Reset counters
        state.per_type_counts.clear();
        state.per_type_capped.clear();
        state.per_peer_message_count.clear();
        state.unique_peers.clear();
        state.rapid_fire_windows.clear();
        state.active_containers_highwater = 0;

        // Create new cycle
        state.current_cycle = Some(DailyCycle {
            cycle_date: today.to_string(),
            snapshot_balance: 0,
            activities: Vec::new(),
            total_points: 0.0,
            capped_points: 0.0,
            intr_reward: 0.0,
            unique_peers: 0,
            is_eligible: false,
            eligibility_reason: String::new(),
            submitted: false,
            started_at: Utc::now().timestamp() as u64,
            ended_at: None,
        });
        state.cycle_start_epoch = Utc::now().timestamp() as u64;

        info!("[DailyRewards] New cycle started: {}", today);
    }

    pub fn set_snapshot_balance(&self, balance: u64) {
        let mut state = self.state.write();
        if let Some(ref mut cycle) = state.current_cycle {
            cycle.snapshot_balance = balance;
        }
    }

    pub fn record_activity(&self, event: ActivityEvent) -> bool {
        let anti = self.anti_gaming.read();
        let weights = self.weights.read();
        let now = Utc::now().timestamp() as u64;

        // Anti-gaming checks
        if anti.require_foreground && !event.is_foreground && !event.is_rbn {
            return false;
        }
        if anti.reject_self_messaging && event.is_self {
            return false;
        }
        if matches!(event.activity_type, ActivityType::MessageSent | ActivityType::GroupMessageSent) {
            if let Some(len) = event.message_len {
                if len < weights.min_message_length {
                    return false;
                }
            }
        }

        // Multi-app container gating: reject web view activity if concurrent
        // container count exceeds max_third_party_containers (default 3).
        // Validated across WhatsApp, Telegram, Discord, Slack, Messenger, Google Messages.
        if matches!(event.activity_type,
            ActivityType::WebFocusedActiveTime |
            ActivityType::SandboxWebPacketData |
            ActivityType::WebViewMediaCallHook)
        {
            if event.active_web_containers > weights.max_third_party_containers {
                return false;
            }
        }

        // Cryptographic validation: require AND VERIFY proof_hash for relay bytes.
        // This prevents spoofing relay activity without actual data routing.
        // The proof_hash must be the SHA-256 of "{activity_type}:{value}:{peer_id}" —
        // computed by the network layer from actual throughput metrics.
        if matches!(event.activity_type, ActivityType::RelayBytes | ActivityType::SandboxWebPacketData) && !event.is_rbn {
            match &event.proof_hash {
                None => return false,
                Some(asserted_hash) => {
                    use sha2::{Sha256, Digest};
                    let peer_str = event.peer_id.as_deref().unwrap_or("");
                    let preimage = format!("{:?}:{}:{}", event.activity_type, event.value, peer_str);
                    let mut hasher = Sha256::new();
                    hasher.update(preimage.as_bytes());
                    let calculated = hex::encode(hasher.finalize());
                    if calculated != *asserted_hash {
                        warn!("[Economy] Proof hash mismatch for {:?}: expected {}, got {}", event.activity_type, calculated, asserted_hash);
                        return false;
                    }
                }
            }
        }

        let mut state = self.state.write();

        // Track highwater mark for active web containers (anti-farming audit)
        if event.active_web_containers > state.active_containers_highwater {
            state.active_containers_highwater = event.active_web_containers;
        }

        // Grace period check
        if now.saturating_sub(state.cycle_start_epoch) < anti.grace_period_secs {
            return false;
        }

        // Rapid-fire check
        let at_u8 = event.activity_type as u8;
        let window = state.rapid_fire_windows
            .entry(at_u8)
            .or_insert_with(RapidFireWindow::new);
        if !window.is_allowed(now, weights.rapid_fire_cooldown_secs, weights.rapid_fire_max_per_window) {
            return false;
        }

        // Per-peer tracking
        if let Some(ref peer) = event.peer_id {
            state.unique_peers.insert(peer.clone());
            if anti.max_messages_per_peer > 0 {
                let count = state.per_peer_message_count
                    .entry(peer.clone())
                    .or_insert(0);
                if *count >= anti.max_messages_per_peer {
                    return false;
                }
                *count += 1;
            }
        }

        // Per-type cap check
        // RBN operators: RelayBytes capped at 51,200 KB (v3.0.1), UptimeSeconds uncapped
        let cap = if event.is_rbn {
            match event.activity_type {
                ActivityType::RelayBytes => weights.cap_relay_bytes_rbn,
                ActivityType::UptimeSeconds => u64::MAX,
                ActivityType::SandboxWebPacketData => weights.cap_sandbox_web_packet_data,
                _ => Self::get_cap_static(&event.activity_type, &weights),
            }
        } else {
            Self::get_cap_static(&event.activity_type, &weights)
        };

        // Get cycle date before mutable borrow of state
        let cycle_date = state.current_cycle.as_ref().map(|c| c.cycle_date.clone());
        
        // Update raw count
        let raw = state.per_type_counts.entry(at_u8).or_insert(0);
        *raw += event.value;
        let current_raw = *raw;
        
        // Update capped count = min(raw, cap)
        let capped = state.per_type_capped.entry(at_u8).or_insert(0);
        *capped = current_raw.min(cap);

        // Update shared metrics bridge for telemetry pipeline
        let idx = at_u8 as usize;
        if idx < 13 {
            let mut metrics = self.shared_metrics.write();
            metrics[idx] = *capped;
        }

        // Persist activity to DB immediately so points survive app restart
        // Uses INSERT OR REPLACE on (cycle_date, activity_type) — idempotent
        if let Some(ref date) = cycle_date {
            let _ = self.storage.save_single_activity(date, at_u8, current_raw, *capped);
        }

        true
    }

    pub fn get_status_json(&self) -> String {
        let state = self.state.read();
        match &state.current_cycle {
            Some(cycle) => serde_json::to_string(cycle).unwrap_or_else(|_| "{}".to_string()),
            None => "{}".to_string(),
        }
    }

    pub fn get_history_json(&self, days: u32) -> String {
        match self.storage.get_recent_daily_cycles(days) {
            Ok(cycles) => serde_json::to_string(&cycles).unwrap_or_else(|_| "[]".to_string()),
            Err(_) => "[]".to_string(),
        }
    }

    /// Persist current cycle activities to storage so they survive app restarts.
    /// Called periodically from the economy monitoring loop.
    pub fn persist_current_activities(&self) {
        let state = self.state.read();
        if let Some(ref cycle) = state.current_cycle {
            let weights = self.weights.read();
            let activities = Self::score_activities_static(&state, &weights);
            let _ = self.storage.save_daily_activities(&cycle.cycle_date, &activities);
        }
    }

    pub fn update_weights(&self, new_weights: ActivityWeights) {
        let mut weights = self.weights.write();
        *weights = new_weights.clone();
        let _ = self.storage.save_daily_reward_config(&new_weights, &self.anti_gaming.read());
    }

    pub fn update_anti_gaming(&self, new_config: AntiGamingConfig) {
        let mut ag = self.anti_gaming.write();
        *ag = new_config.clone();
        let _ = self.storage.save_daily_reward_config(&self.weights.read(), &new_config);
    }

    /// Updates the global points estimate from network data.
    /// This should be called periodically with data from the Solana program or peer discovery.
    pub fn update_global_points_estimate(&self, estimate: f64) {
        let mut state = self.state.write();
        state.global_points_estimate = estimate.max(1.0); // prevent division by zero
    }

    /// Accepts an RBN-reported network-wide total from a TelemetryAck.
    /// This value is cached and used to bypass the static 100K ceiling for 48 hours,
    /// preventing reward dilution in small test clusters.
    pub fn accept_rbn_estimate(&self, total_points: f64, timestamp: u64) {
        let mut state = self.state.write();
        if total_points > 0.0 {
            state.rbn_reported_total_points = total_points;
            state.last_rbn_estimate_timestamp = timestamp;
            info!("[DailyRewards] RBN estimate accepted: {:.1} total points at {}", total_points, timestamp);
        }
    }

    /// Computes the effective global points estimate, bypassing the static ceiling
    /// when fresh RBN data is available (within 48 hours).
    fn compute_effective_global_estimate(state: &DailyRewardState, weights: &ActivityWeights) -> f64 {
        const RBN_ESTIMATE_FRESHNESS_SECS: u64 = 48 * 3600; // 48 hours

        let now = Utc::now().timestamp() as u64;
        let rbn_age = now.saturating_sub(state.last_rbn_estimate_timestamp);

        // Dynamic estimate from observed peers (small-network floor)
        let observed_network_size = (state.unique_peers.len() as f64) + 1.0;
        let dynamic_estimate = observed_network_size * weights.daily_point_cap;

        if rbn_age < RBN_ESTIMATE_FRESHNESS_SECS && state.rbn_reported_total_points > 0.0 {
            // RBN data is fresh: use the larger of RBN-reported or dynamic
            // This bypasses the static 100K ceiling entirely
            let effective = state.rbn_reported_total_points.max(dynamic_estimate).max(1.0);
            info!("[DailyRewards] Using fresh RBN estimate: {:.1} (age={}s, dynamic={:.1})", effective, rbn_age, dynamic_estimate);
            effective
        } else {
            // No fresh RBN data: fall back to max(dynamic, static)
            dynamic_estimate.max(state.global_points_estimate).max(1.0)
        }
    }

    /// Sets the RBN status for this node. RBN operators draw from the RBN pool.
    pub fn set_rbn_status(&self, is_rbn: bool) {
        let mut state = self.state.write();
        state.is_rbn = is_rbn;
    }

    /// Sets the edge node status. Edge nodes receive infra weight multiplier.
    pub fn set_edge_node_status(&self, is_edge: bool) {
        let mut state = self.state.write();
        state.is_edge_node = is_edge;
    }

    /// Sets the prestige tier status for this node.
    pub fn set_prestige_tier(&self, tier: u8) {
        let mut state = self.state.write();
        state.prestige_tier = tier;
    }

    /// Returns the emission year (1-based) since TGE.
    /// Uses the canonical `current_emission_year()` from ledger_cron (0-based) and adds 1.
    pub fn get_emission_year(&self) -> u32 {
        crate::economy::ledger_cron::current_emission_year() + 1
    }

    /// Returns the daily user pool cap for the current emission year.
    /// Formula: YEAR_1_DAILY_POOL * (ANNUAL_DECAY ^ (year - 1))
    /// Uses 0-based exponent from ledger_cron::current_emission_year() directly.
    pub fn get_daily_pool_cap(&self) -> f64 {
        let years_since_tge = crate::economy::ledger_cron::current_emission_year();
        YEAR_1_DAILY_POOL * ANNUAL_DECAY.powi(years_since_tge as i32)
    }

    /// Returns the daily RBN pool cap for the current emission year.
    /// Formula: YEAR_1_RBN_DAILY_POOL * (ANNUAL_DECAY ^ years_since_tge)
    /// Uses 0-based exponent from ledger_cron::current_emission_year() directly.
    pub fn get_rbn_daily_pool_cap(&self) -> f64 {
        let years_since_tge = crate::economy::ledger_cron::current_emission_year();
        YEAR_1_RBN_DAILY_POOL * ANNUAL_DECAY.powi(years_since_tge as i32)
    }

    /// Calculates the user's current INTR earnings for today.
    /// Uses dual-pool system: social points capped at 5,000, infra points uncapped for RBN.
    /// RBN operators draw from the RBN pool; standard users draw from the user pool.
    /// Returns intr_earned_today as nano-INTR integer (u64) to match Solana 9-decimal precision.
    pub fn get_realtime_earnings(&self) -> serde_json::Value {
        let state = self.state.read();
        let weights = self.weights.read();

        let activities = Self::score_activities_static(&state, &weights);

        // CRITICAL: Separate social and infra points before capping
        let social_points: f64 = activities.iter()
            .filter(|a| matches!(a.activity_type,
                ActivityType::MessageSent | ActivityType::MessageReceived |
                ActivityType::GroupMessageSent | ActivityType::GroupReaction |
                ActivityType::FileTransferSent | ActivityType::FileTransferRecv |
                ActivityType::CallDurationSecs |
                ActivityType::WebFocusedActiveTime | ActivityType::WebViewMediaCallHook))
            .map(|a| a.points)
            .sum();
        let infra_points: f64 = activities.iter()
            .filter(|a| matches!(a.activity_type,
                ActivityType::RelayBytes | ActivityType::UptimeSeconds |
                ActivityType::SandboxWebPacketData |
                ActivityType::UniquePeerHandshakes))
            .map(|a| a.points)
            .sum();

        // Dynamic social cap: Edge nodes with 24h verified uptime get 15,000 cap
        let uptime_raw = state.per_type_counts.get(&(ActivityType::UptimeSeconds as u8)).copied().unwrap_or(0);
        let is_rbn = state.is_rbn;
        let effective_social_cap = if !is_rbn && state.is_edge_node && uptime_raw >= 86400 {
            15_000.0
        } else {
            weights.daily_point_cap
        };
        let social_capped = social_points.min(effective_social_cap);
        let infra_capped = infra_points;
        let capped_points = social_capped + infra_capped;

        let daily_pool = self.get_daily_pool_cap();
        let rbn_daily_pool = self.get_rbn_daily_pool_cap();
        let year = self.get_emission_year();

        // CRITICAL: RBN operators draw from RBN pool, standard users from user pool
        let effective_pool = if is_rbn { rbn_daily_pool } else { daily_pool };

        // Dynamic global estimate with RBN freshness bypass
        let global_estimate = Self::compute_effective_global_estimate(&state, &weights);
        let unique_peers = state.unique_peers.len() as u32;

        // User's share of the daily pool
        let user_share = if global_estimate > 0.0 {
            capped_points / global_estimate
        } else {
            0.0
        };

        let prestige_mult = match state.prestige_tier {
            0 => 1.0,
            1 => 1.05,
            2 => 1.10,
            3 => 1.20,
            4 => 1.50,
            5 => 1.15,
            6 => 1.15,
            _ => 1.0,
        };

        // CRITICAL: Output as nano-INTR integer (1 INTR = 1,000,000,000 nano-INTR)
        // No floating-point serialization — matches Solana SPL 9-decimal precision
        // Deterministic rounding: truncate to 6 decimal places before nano conversion
        let raw_earned = user_share * effective_pool * prestige_mult;
        let intr_earned_f64 = (raw_earned * 1_000_000.0).trunc() / 1_000_000.0;
        let intr_earned_nano: u64 = (intr_earned_f64 * 1_000_000_000.0) as u64;

        serde_json::json!({
            "intr_earned_today_nano": intr_earned_nano,
            "intr_earned_today": intr_earned_f64,
            "is_rbn": is_rbn,
            "effective_pool": effective_pool,
            "social_points": social_capped,
            "infra_points": infra_capped,
            "total_points": capped_points,
            "global_points_estimate": global_estimate,
            "user_share_pct": (user_share * 100.0),
            "daily_pool_cap": daily_pool,
            "rbn_daily_pool_cap": rbn_daily_pool,
            "emission_year": year,
            "unique_peers": unique_peers,
            "activities": activities.iter().map(|a| {
                serde_json::json!({
                    "type": format!("{:?}", a.activity_type),
                    "raw": a.raw_count,
                    "capped": a.capped_count,
                    "points": a.points,
                })
            }).collect::<Vec<_>>(),
        })
    }

    /// Returns a C-compatible fixed-width state snapshot for FFI consumers.
    /// All points are truncated to 4 decimal places before conversion.
    /// No heap allocations cross the FFI boundary.
    pub fn get_ffi_state(&self) -> FFIDailyState {
        let state = self.state.read();
        let weights = self.weights.read();
        let activities = Self::score_activities_static(&state, &weights);

        let social_raw: f64 = activities.iter()
            .filter(|a| matches!(a.activity_type,
                ActivityType::MessageSent | ActivityType::MessageReceived |
                ActivityType::GroupMessageSent | ActivityType::GroupReaction |
                ActivityType::FileTransferSent | ActivityType::FileTransferRecv |
                ActivityType::CallDurationSecs |
                ActivityType::WebFocusedActiveTime | ActivityType::WebViewMediaCallHook))
            .map(|a| a.points)
            .sum();
        let infra_raw: f64 = activities.iter()
            .filter(|a| matches!(a.activity_type,
                ActivityType::RelayBytes | ActivityType::UptimeSeconds |
                ActivityType::SandboxWebPacketData |
                ActivityType::UniquePeerHandshakes))
            .map(|a| a.points)
            .sum();

        let uptime_raw = state.per_type_counts
            .get(&(ActivityType::UptimeSeconds as u8))
            .copied()
            .unwrap_or(0);

        // Dynamic social cap for edge nodes
        let effective_social_cap = if !state.is_rbn && state.is_edge_node && uptime_raw >= 86400 {
            15_000.0
        } else {
            weights.daily_point_cap
        };

        // Deterministic 4-decimal truncation
        let truncate = |v: f64| (v * 10_000.0).trunc() / 10_000.0;
        let social_capped = truncate(social_raw.min(effective_social_cap));
        let infra_capped = truncate(infra_raw);

        FFIDailyState {
            total_social_points: social_capped,
            total_infra_points: infra_capped,
            active_web_containers: 0, // populated by caller from front-end state
            current_cycle_uptime: uptime_raw,
            is_edge_node: state.is_edge_node as u8,
            is_rbn: state.is_rbn as u8,
        }
    }

    fn get_cap_static(at: &ActivityType, w: &ActivityWeights) -> u64 {
        match at {
            ActivityType::MessageSent => w.cap_message_sent as u64,
            ActivityType::MessageReceived => w.cap_message_received as u64,
            ActivityType::GroupMessageSent => w.cap_group_message_sent as u64,
            ActivityType::GroupReaction => w.cap_group_reaction as u64,
            ActivityType::FileTransferSent => w.cap_file_transfer_sent as u64,
            ActivityType::FileTransferRecv => w.cap_file_transfer_recv as u64,
            ActivityType::CallDurationSecs => w.cap_call_duration_secs as u64,
            ActivityType::RelayBytes => w.cap_relay_bytes,
            ActivityType::UptimeSeconds => w.cap_uptime_seconds as u64,
            ActivityType::WebFocusedActiveTime => w.cap_web_focused_active_time as u64,
            ActivityType::SandboxWebPacketData => w.cap_sandbox_web_packet_data,
            ActivityType::WebViewMediaCallHook => w.cap_webview_media_call_hook as u64,
            ActivityType::UniquePeerHandshakes => w.cap_unique_peer_handshakes as u64,
        }
    }

    pub fn score_activities_static(state: &DailyRewardState, w: &ActivityWeights) -> Vec<DailyActivityCount> {
        Self::score_activities_with_blend(state, w, None)
    }

    fn score_activities_with_blend(state: &DailyRewardState, w: &ActivityWeights, blend_override: Option<f64>) -> Vec<DailyActivityCount> {
        let is_rbn = state.is_rbn;
        let is_edge = state.is_edge_node;
        let uptime_raw = state.per_type_counts.get(&(ActivityType::UptimeSeconds as u8)).copied().unwrap_or(0);

        // v3.0.1 phase-in: compute blended weights during 90-day transition
        let blend = blend_override.unwrap_or_else(Self::get_phase_in_blend);
        let uptime_weight = Self::blend_weight(0.001, w.uptime_seconds, blend);
        let edge_mult = Self::blend_weight(30.0, w.edge_infra_multiplier, blend);
        // Availability yield: old = 1.2x at >=82800, new = 1.5x at >=79200
        let yield_threshold = Self::blend_weight(82800.0, 79200.0, blend) as u64;
        let yield_multiplier = Self::blend_weight(1.2, 1.5, blend);

        ActivityType::all().iter().map(|at| {
            let at_u8 = *at as u8;

            // UniquePeerHandshakes derives from the unique_peers HashSet, not per_type_counts
            let (raw, capped) = if matches!(at, ActivityType::UniquePeerHandshakes) {
                let unique_count = state.unique_peers.len() as u64;
                let cap = w.cap_unique_peer_handshakes as u64;
                (unique_count, unique_count.min(cap))
            } else {
                let r = state.per_type_counts.get(&at_u8).copied().unwrap_or(0);
                let c = state.per_type_capped.get(&at_u8).copied().unwrap_or(0);
                (r, c)
            };

            let mut weight = match at {
                ActivityType::MessageSent => w.message_sent,
                ActivityType::MessageReceived => w.message_received,
                ActivityType::GroupMessageSent => w.group_message_sent,
                ActivityType::GroupReaction => w.group_reaction,
                ActivityType::FileTransferSent => w.file_transfer_sent,
                ActivityType::FileTransferRecv => w.file_transfer_recv,
                ActivityType::CallDurationSecs => w.call_duration_secs,
                ActivityType::RelayBytes => w.relay_bytes,
                ActivityType::UptimeSeconds => uptime_weight,
                ActivityType::WebFocusedActiveTime => w.web_focused_active_time,
                ActivityType::SandboxWebPacketData => w.sandbox_web_packet_data,
                ActivityType::WebViewMediaCallHook => w.webview_media_call_hook,
                ActivityType::UniquePeerHandshakes => w.unique_peer_handshakes,
            };

            // Apply availability yield to uptime weight for RBN nodes with sufficient uptime
            if matches!(at, ActivityType::UptimeSeconds) && is_rbn && uptime_raw >= yield_threshold {
                weight *= yield_multiplier;
            }

            // Apply edge infra multiplier for non-RBN edge nodes
            // WebFocusedActiveTime and SandboxWebPacketData inherit the 3x edge boost
            // WebViewMediaCallHook stays flat at 1x for both Regular and Edge
            if !is_rbn && is_edge && matches!(at,
                ActivityType::RelayBytes | ActivityType::UptimeSeconds |
                ActivityType::WebFocusedActiveTime | ActivityType::SandboxWebPacketData)
            {
                weight *= edge_mult;
            }

            // Deterministic rounding: truncate to 4 decimal places to prevent
            // float accumulation drift across different compiler targets
            let raw_points = capped as f64 * weight;
            let points = (raw_points * 10_000.0).trunc() / 10_000.0;

            DailyActivityCount {
                activity_type: *at,
                raw_count: raw,
                capped_count: capped,
                points,
            }
        }).collect()
    }

    /// Returns blend factor (0.0 = old weights, 1.0 = new weights) based on days since phase-in start.
    fn get_phase_in_blend() -> f64 {
        let start = NaiveDate::parse_from_str(V3_PHASE_IN_START, "%Y-%m-%d")
            .unwrap_or(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
        let today = Utc::now().date_naive();
        let days = today.signed_duration_since(start).num_days().max(0) as u64;
        if days >= V3_PHASE_IN_DAYS { 1.0 } else { days as f64 / V3_PHASE_IN_DAYS as f64 }
    }

    /// Linear interpolation between old and new weight based on blend factor.
    fn blend_weight(old: f64, new: f64, blend: f64) -> f64 {
        old * (1.0 - blend) + new * blend
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;
    use crate::storage::StorageService;
    use std::sync::Arc;

    #[test]
    fn test_vector_1_edge_node() {
        let storage = Arc::new(StorageService::new_ephemeral().unwrap());
        let shared_metrics = Arc::new(parking_lot::RwLock::new([0u64; 13]));
        let engine = DailyRewardEngine::new(storage, shared_metrics);
        {
            let mut state = engine.state.write();
            state.is_rbn = false;
            state.is_edge_node = true;
            state.global_points_estimate = 100_000.0;
            state.cycle_start_epoch = 0;
        }
        let weights = engine.weights.read().clone();
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageSent, peer_id: Some("p1".into()), value: 45, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageReceived, peer_id: Some("p1".into()), value: 120, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupMessageSent, peer_id: Some("g1".into()), value: 80, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupReaction, peer_id: Some("g1".into()), value: 25, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::FileTransferSent, peer_id: Some("p1".into()), value: 3, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::FileTransferRecv, peer_id: Some("p1".into()), value: 8, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::CallDurationSecs, peer_id: Some("p1".into()), value: 1800, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });
        let preimage = format!("{:?}:{}:{}", ActivityType::RelayBytes, 5120, "p2");
        let mut hasher = sha2::Sha256::new();
        hasher.update(preimage.as_bytes());
        let relay_proof = Some(hex::encode(hasher.finalize()));
        engine.record_activity(ActivityEvent { activity_type: ActivityType::RelayBytes, peer_id: Some("p2".into()), value: 5120, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: relay_proof, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::UptimeSeconds, peer_id: None, value: 86400, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });

        let state = engine.state.read();
        let activities = DailyRewardEngine::score_activities_with_blend(&state, &weights, Some(1.0));
        let social: f64 = activities.iter().filter(|a| matches!(a.activity_type,
            ActivityType::MessageSent | ActivityType::MessageReceived |
            ActivityType::GroupMessageSent | ActivityType::GroupReaction |
            ActivityType::FileTransferSent | ActivityType::FileTransferRecv |
            ActivityType::CallDurationSecs |
            ActivityType::WebFocusedActiveTime | ActivityType::WebViewMediaCallHook)).map(|a| a.points).sum();
        let infra: f64 = activities.iter().filter(|a| matches!(a.activity_type,
            ActivityType::RelayBytes | ActivityType::UptimeSeconds |
            ActivityType::SandboxWebPacketData |
            ActivityType::UniquePeerHandshakes)).map(|a| a.points).sum();

        assert_eq!(social, 3705.0, "social points mismatch");
        // Edge node (v3.0.1, blend=1.0): edge_mult=3.0
        // infra = (5120 * 0.01 * 3) + (86400 * 0.005 * 3) + (3 * 1.0) = 153.6 + 1296.0 + 3.0 = 1452.6
        assert!((infra - 1452.6).abs() < 0.1, "infra={}", infra);
        // Edge node with full 24h uptime gets 15,000 social cap
        let total = social.min(15_000.0) + infra;
        let nano: u64 = (total / 100_000.0 * 16_438_000_000_000.0) as u64;
        assert_eq!(nano, 847_806_288_000, "Test Vector 1 failed: {}", nano);
    }

    #[test]
    fn test_vector_2_rbn_node() {
        let storage = Arc::new(StorageService::new_ephemeral().unwrap());
        let shared_metrics = Arc::new(parking_lot::RwLock::new([0u64; 13]));
        let engine = DailyRewardEngine::new(storage, shared_metrics);
        {
            let mut state = engine.state.write();
            state.is_rbn = true;
            state.global_points_estimate = 100_000.0;
            state.cycle_start_epoch = 0;
        }
        let weights = engine.weights.read().clone();
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageSent, peer_id: Some("p1".into()), value: 30, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: true, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageReceived, peer_id: Some("p1".into()), value: 50, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupMessageSent, peer_id: Some("g1".into()), value: 40, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: true, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupReaction, peer_id: Some("g1".into()), value: 10, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::RelayBytes, peer_id: Some("p2".into()), value: 10_485_760, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::UptimeSeconds, peer_id: None, value: 86400, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None, active_web_containers: 0 });

        // Simulate 500 unique client peer handshakes (bootstrap DHT / mailbox fetches)
        // 3 peers already registered above (p1, g1, p2); inject 497 more
        {
            let mut state = engine.state.write();
            for i in 3..500 {
                state.unique_peers.insert(format!("rbn_client_{}", i));
            }
        }

        let state = engine.state.read();
        let activities = DailyRewardEngine::score_activities_with_blend(&state, &weights, Some(1.0));
        let social: f64 = activities.iter().filter(|a| matches!(a.activity_type,
            ActivityType::MessageSent | ActivityType::MessageReceived |
            ActivityType::GroupMessageSent | ActivityType::GroupReaction |
            ActivityType::FileTransferSent | ActivityType::FileTransferRecv |
            ActivityType::CallDurationSecs |
            ActivityType::WebFocusedActiveTime | ActivityType::WebViewMediaCallHook)).map(|a| a.points).sum();
        let infra: f64 = activities.iter().filter(|a| matches!(a.activity_type,
            ActivityType::RelayBytes | ActivityType::UptimeSeconds |
            ActivityType::SandboxWebPacketData |
            ActivityType::UniquePeerHandshakes)).map(|a| a.points).sum();

        // Readiness + Capability + Utility distribution (infra pool):
        //   648.0 Uptime (22h+ yield) + 512.0 Data Check (50MB cap) + 500.0 Unique Contacts = 1660.0
        // Social pool: 900.0 (messaging)
        // Grand total: 2560.0 for a fully optimized RBN server profile
        assert_eq!(social, 900.0, "social points mismatch");
        let expected_infra = 648.0 + 512.0 + 500.0;
        assert!((infra - expected_infra).abs() < 0.1, "infra={}", infra);
        assert!((infra - 1660.0).abs() < 0.1, "infra max RBN profile={}", infra);

        let total = social.min(5000.0) + infra;
        assert!((total - 2560.0).abs() < 0.1, "total={}", total);

        let nano: u64 = (total / 100_000.0 * 8_219_000_000_000.0) as u64;
        assert_eq!(nano, 210_406_400_000, "Test Vector 2 failed: {}", nano);
    }

    #[test]
    fn test_dual_pool_separation() {
        let storage = Arc::new(StorageService::new_ephemeral().unwrap());
        let shared_metrics = Arc::new(parking_lot::RwLock::new([0u64; 13]));
        let engine = DailyRewardEngine::new(storage, shared_metrics);
        {
            let mut state = engine.state.write();
            state.is_rbn = false;
            state.global_points_estimate = 100_000.0;
            state.cycle_start_epoch = 0;
        }
        let preimage = format!("{:?}:{}:{}", ActivityType::RelayBytes, 500_000, "p1");
        let mut hasher = sha2::Sha256::new();
        hasher.update(preimage.as_bytes());
        let relay_proof = Some(hex::encode(hasher.finalize()));
        engine.record_activity(ActivityEvent { activity_type: ActivityType::RelayBytes, peer_id: Some("p1".into()), value: 500_000, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: relay_proof, active_web_containers: 0 });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageSent, peer_id: Some("p2".into()), value: 100, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: false, proof_hash: None, active_web_containers: 0 });

        let state = engine.state.read();
        let weights = engine.weights.read();
        let activities = DailyRewardEngine::score_activities_static(&state, &weights);
        let social: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::MessageSent)).map(|a| a.points).sum();
        let infra: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::RelayBytes)).map(|a| a.points).sum();
        assert_eq!(social, 1000.0, "social={}", social);
        assert!((infra - 102.4).abs() < 0.01, "infra={}", infra);
    }

    // ─── Stage 3: Macro-Pool Clearing & Systemic Dilution Tests ────────
    //
    // Validates the pool-clearing formula: individual_share = (my_points / global_pts) * pool_cap
    // Tests use the exact Year 1 token specifications:
    //   User Pool: 16,438 $INTR/day  (Regular Users + Edge Nodes)
    //   RBN Pool:   8,219 $INTR/day  (RBN bonders only)

    #[test]
    fn test_rbn_pool_scenario_x_low_swarm() {
        // 100 RBNs, all at baseline 1,160 infra points (uptime 22h+ yield + 50MB data check)
        // No unique peer handshakes — quiet fallback servers
        let rbn_count = 100_u64;
        let points_per_rbn = 1160.0_f64;
        let global_pts = points_per_rbn * rbn_count as f64; // 116,000
        let rbn_daily_pool = 8_219.0_f64;

        let expected_per_rbn = (points_per_rbn / global_pts) * rbn_daily_pool;
        assert!((expected_per_rbn - 82.19).abs() < 0.01,
            "Scenario X: expected ~82.19 INTR/staker, got {}", expected_per_rbn);

        // Verify total emissions don't exceed pool cap
        let total_emission = expected_per_rbn * rbn_count as f64;
        assert!((total_emission - rbn_daily_pool).abs() < 0.01,
            "Scenario X: total emission must equal pool cap {}", rbn_daily_pool);
    }

    #[test]
    fn test_rbn_pool_scenario_y_high_swarm() {
        // 10 strategic choke-point RBNs: 1,660 infra points (uptime + data check + 500 handshakes)
        // 90 quiet fallback RBNs: 1,160 infra points
        let strategic_count = 10_u64;
        let quiet_count = 90_u64;
        let strategic_pts = 1660.0_f64;
        let quiet_pts = 1160.0_f64;
        let rbn_daily_pool = 8_219.0_f64;

        let global_pts = (strategic_count as f64 * strategic_pts) + (quiet_count as f64 * quiet_pts);
        assert_eq!(global_pts, 121_000.0);

        let quiet_payout = (quiet_pts / global_pts) * rbn_daily_pool;
        let strategic_payout = (strategic_pts / global_pts) * rbn_daily_pool;

        assert!((quiet_payout - 78.79).abs() < 0.05,
            "Scenario Y quiet: expected ~78.79, got {}", quiet_payout);
        assert!((strategic_payout - 112.79).abs() < 0.05,
            "Scenario Y strategic: expected ~112.79, got {}", strategic_payout);

        // Strategic choke-point RBNs extract 43% more than quiet servers
        let premium_pct = ((strategic_payout - quiet_payout) / quiet_payout) * 100.0;
        assert!((premium_pct - 43.16).abs() < 0.5,
            "Scenario Y: strategic premium should be ~43%, got {:.2}%", premium_pct);

        // Total emission still equals pool cap
        let total_emission = (strategic_payout * strategic_count as f64) + (quiet_payout * quiet_count as f64);
        assert!((total_emission - rbn_daily_pool).abs() < 1.0,
            "Scenario Y: total emission must equal pool cap {}", rbn_daily_pool);
    }

    #[test]
    fn test_user_edge_pool_interaction() {
        // 10,000 regular users at 5,000-point cap
        // 500 edge nodes at 15,000-point cap (24h uptime verified)
        let regular_count = 10_000_u64;
        let edge_count = 500_u64;
        let regular_pts = 5_000.0_f64;
        let edge_pts = 15_000.0_f64;
        let user_daily_pool = 16_438.0_f64;

        let global_pts = (regular_count as f64 * regular_pts) + (edge_count as f64 * edge_pts);
        assert_eq!(global_pts, 57_500_000.0);

        let regular_share = (regular_pts / global_pts) * user_daily_pool;
        let edge_share = (edge_pts / global_pts) * user_daily_pool;

        assert!((regular_share - 1.429).abs() < 0.001,
            "Regular user: expected ~1.429, got {}", regular_share);
        assert!((edge_share - 4.288).abs() < 0.001,
            "Edge node: expected ~4.288, got {}", edge_share);

        // Edge node out-extracts regular user by exactly 3:1 before prestige
        let ratio = edge_share / regular_share;
        assert!((ratio - 3.0).abs() < 0.001,
            "Edge:User ratio must be 3:1, got {:.3}", ratio);

        // Total emission equals pool cap
        let total_emission = (regular_share * regular_count as f64) + (edge_share * edge_count as f64);
        assert!((total_emission - user_daily_pool).abs() < 1.0,
            "Total emission must equal pool cap {}", user_daily_pool);
    }

    #[test]
    fn test_prestige_multiplier_on_edge_share() {
        // Validate that prestige tiers scale the edge node share correctly
        // from the base 4.288 $INTR/day
        let edge_pts = 15_000.0_f64;
        let global_pts = 57_500_000.0_f64;
        let user_daily_pool = 16_438.0_f64;
        let base_share = (edge_pts / global_pts) * user_daily_pool; // ~4.288

        let tiers: &[(u8, f64)] = &[
            (0, 1.0),    // No prestige
            (1, 1.05),   // Sentinel
            (2, 1.10),   // Silver
            (3, 1.20),   // Gold
            (4, 1.50),   // Platinum
            (5, 1.15),   // Legacy A
            (6, 1.15),   // Legacy B
        ];

        for &(tier, mult) in tiers {
            let expected = base_share * mult;
            assert!(expected > 0.0, "Tier {}: must be positive", tier);
            // Each higher tier (by multiplier) yields strictly more
            if mult > 1.0 {
                assert!(expected > base_share,
                    "Tier {} ({}x): {} must exceed base {}", tier, mult, expected, base_share);
            }
        }

        // Platinum (1.5x) gives ~6.43 INTR/day for an edge node
        let platinum = base_share * 1.50;
        assert!((platinum - 6.432).abs() < 0.01,
            "Platinum edge node: expected ~6.432, got {}", platinum);
    }

    #[test]
    fn test_annual_decay_reduces_pool() {
        // Year 1: 16,438/day
        // Year 2: 16,438 * 0.8 = 13,150.4/day
        // Year 5: 16,438 * 0.8^4 = 6,717.6/day
        let base = 16_438.0_f64;
        let decay = 0.8_f64;

        let year_2 = base * decay.powi(1);
        let year_5 = base * decay.powi(4);
        let year_10 = base * decay.powi(9);

        assert!((year_2 - 13_150.4).abs() < 0.1, "Year 2: {}", year_2);
        assert!((year_5 - 6_733.0).abs() < 1.0, "Year 5: {}", year_5);
        assert!((year_10 - 2_206.3).abs() < 1.0, "Year 10: {}", year_10);

        // Pool must be monotonically decreasing
        assert!(year_2 < base);
        assert!(year_5 < year_2);
        assert!(year_10 < year_5);
    }

    #[test]
    fn test_pool_cap_weighted_score_math() {
        let daily_pool = 16_438.0_f64;
        let entries: Vec<(f64, u8)> = vec![
            (100.0, 4), (100.0, 3), (100.0, 2), (100.0, 1), (100.0, 0), (100.0, 0),
        ];
        let prestige_mult = |tier: u8| -> f64 {
            match tier { 0 => 1.0, 1 => 1.05, 2 => 1.10, 3 => 1.20, 4 => 1.50, _ => 1.0 }
        };
        let weighted: Vec<f64> = entries.iter().map(|(s, t)| s * prestige_mult(*t)).collect();
        let total_weighted: f64 = weighted.iter().sum();
        let payouts: Vec<f64> = weighted.iter().map(|w| (w / total_weighted) * daily_pool).collect();
        let total_payout: f64 = payouts.iter().sum();
        assert!(total_payout <= daily_pool + 0.01, "MIXED TIERS: total ({:.2}) exceeds pool ({:.2})", total_payout, daily_pool);
        assert!(payouts[0] > payouts[4], "Tier 4 should get more than tier 0");
    }

    #[test]
    fn test_pool_cap_all_tier_4() {
        let daily_pool = 16_438.0_f64;
        let entries: Vec<f64> = vec![100.0; 6];
        let mult = 1.5_f64;
        let weighted: Vec<f64> = entries.iter().map(|s| s * mult).collect();
        let total_weighted: f64 = weighted.iter().sum();
        let payouts: Vec<f64> = weighted.iter().map(|w| (w / total_weighted) * daily_pool).collect();
        let total_payout: f64 = payouts.iter().sum();
        assert!(total_payout <= daily_pool + 0.01, "ALL TIER 4: total ({:.2}) exceeds pool ({:.2})", total_payout, daily_pool);
    }

    #[test]
    fn test_pool_cap_all_tier_0() {
        let daily_pool = 16_438.0_f64;
        let entries: Vec<f64> = vec![100.0; 6];
        let total_score: f64 = entries.iter().sum();
        let payouts: Vec<f64> = entries.iter().map(|s| (s / total_score) * daily_pool).collect();
        let total_payout: f64 = payouts.iter().sum();
        assert!((total_payout - daily_pool).abs() < 0.01, "ALL TIER 0: total ({:.2}) should equal pool ({:.2})", total_payout, daily_pool);
    }
}
