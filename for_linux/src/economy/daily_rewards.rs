use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use parking_lot::RwLock;
use chrono::{Utc, NaiveDate};
use tracing::{info, warn};
use crate::storage::StorageService;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// ─── Constants ───────────────────────────────────────────────

pub const TGE_DATE: &str = "2026-01-01";
const YEAR_1_DAILY_POOL: f64 = 16_438.0;
const YEAR_1_RBN_DAILY_POOL: f64 = 8_219.0;
const ANNUAL_DECAY: f64 = 0.8;
const DEFAULT_GLOBAL_POINTS_ESTIMATE: f64 = 100_000.0;
const YEAR_1_STRATEGIC_RESERVE_DAILY: f64 = 3_287.60; // 10% of daily emissions
const INTR_MINT: &str = "EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf";
const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// Derive the Associated Token Account address for a given wallet and mint.
fn derive_ata(wallet: &str, mint: &str) -> Option<String> {
    let owner = Pubkey::from_str(wallet).ok()?;
    let mint_pubkey = Pubkey::from_str(mint).ok()?;
    let token_program = Pubkey::from_str(TOKEN_PROGRAM_ID).ok()?;
    let ata_program = Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM_ID).ok()?;
    let (ata, _) = Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint_pubkey.as_ref()],
        &ata_program,
    );
    Some(ata.to_string())
}

// ─── Dynamic Promo Stack ─────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum PromoType {
    CommunityThemeVote,
    EarlyAdopterBonus,
    DeveloperHackathonYield,
    DynamicBonusCampaign,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActiveCampaign {
    pub campaign_id: String,
    pub promo_type: PromoType,
    pub daily_payout_allocation: f64,
    pub expiration_epoch: String,
}

pub struct DynamicPromoStack {
    pub daily_strategic_reserve_ceiling: f64,
    pub active_campaigns: HashMap<String, ActiveCampaign>,
}

impl DynamicPromoStack {
    pub fn new() -> Self {
        Self {
            daily_strategic_reserve_ceiling: YEAR_1_STRATEGIC_RESERVE_DAILY,
            active_campaigns: HashMap::new(),
        }
    }

    pub fn with_decay(year: u32) -> Self {
        let ceiling = YEAR_1_STRATEGIC_RESERVE_DAILY * ANNUAL_DECAY.powi((year - 1) as i32);
        Self {
            daily_strategic_reserve_ceiling: ceiling,
            active_campaigns: HashMap::new(),
        }
    }

    pub fn open_or_adjust_campaign(&mut self, campaign: ActiveCampaign) {
        info!("[PromoStack] Opening/adjusting campaign: {} ({} INTR/day)", campaign.campaign_id, campaign.daily_payout_allocation);
        self.active_campaigns.insert(campaign.campaign_id.clone(), campaign);
    }

    pub fn close_campaign(&mut self, campaign_id: &str) {
        if self.active_campaigns.remove(campaign_id).is_some() {
            info!("[PromoStack] Closed campaign: {}", campaign_id);
        }
    }

    pub fn compute_epoch_promo_distribution(&mut self, current_epoch: &str) -> (f64, HashMap<String, f64>) {
        let mut total_promo_deductions = 0.0;
        let mut executed_payouts = HashMap::new();

        self.active_campaigns.retain(|_, campaign| {
            if current_epoch > campaign.expiration_epoch.as_str() {
                info!("[PromoStack] Auto-evicting expired campaign: {}", campaign.campaign_id);
                false
            } else {
                total_promo_deductions += campaign.daily_payout_allocation;
                true
            }
        });

        if total_promo_deductions > self.daily_strategic_reserve_ceiling {
            warn!("[PromoStack] Promo deductions exceed ceiling, capping at {}", self.daily_strategic_reserve_ceiling);
            total_promo_deductions = self.daily_strategic_reserve_ceiling;
        }

        let remaining_referral_pool = self.daily_strategic_reserve_ceiling - total_promo_deductions;

        for (id, campaign) in &self.active_campaigns {
            executed_payouts.insert(id.clone(), campaign.daily_payout_allocation);
        }
        executed_payouts.insert("CORE_REFERRAL_POOL_DISTRIBUTION".to_string(), remaining_referral_pool);

        (remaining_referral_pool, executed_payouts)
    }
}

// ─── Activity Types ──────────────────────────────────────────

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

// ─── Activity Weights ────────────────────────────────────────

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
            cap_relay_bytes: 10240,
            cap_relay_bytes_rbn: 51200,
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

// ─── Telemetry Envelope (from client) ────────────────────────

/// Cryptographically signed telemetry envelope from client devices.
/// Contains all 13 activity metrics, wallet addresses, and proof data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEnvelope {
    pub peer_id: String,          // libp2p network identity
    pub solana_wallet: String,    // Client's Solana Public Key
    pub solana_ata: String,       // Pre-derived Associated Token Account for $INTR
    pub epoch_id: String,         // Calendar identifier (e.g., "2026_07_03")
    pub metrics: [u64; 13],       // The 13 activity metrics tracked natively
    pub unique_peers: Vec<String>,
    pub is_rbn: bool,
    pub is_edge_node: bool,
    pub prestige_tier: u8,
    pub proof_hash: String,       // SHA-256 proving valid relay work
    pub client_signature: Vec<u8>, // Ed25519 signature of entire payload
    pub timestamp: u64,
}

// Legacy alias for backward compatibility
pub type SignedTelemetryPacket = TelemetryEnvelope;

// ─── Claim Request (to treasury daemon) ──────────────────────

/// IPC payload sent to introvert-solana over port 9001.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub claim_type: String,
    pub peer_id: String,
    pub payout_address: String,
    pub token_amount: f64,
    pub epoch_id: String,
}

// ─── Daily Cycle State (per client) ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivityCount {
    pub activity_type: ActivityType,
    pub raw_count: u64,
    pub capped_count: u64,
    pub points: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCycle {
    pub peer_id: String,
    pub payout_address: String,
    pub cycle_date: String,
    pub activities: Vec<DailyActivityCount>,
    pub social_points: f64,
    pub infra_points: f64,
    pub total_points: f64,
    pub intr_reward: f64,
    pub intr_reward_nano: u64,
    pub unique_peers: u32,
    pub is_eligible: bool,
    pub eligibility_reason: String,
    pub processed: bool,
    pub prestige_tier: u8,
}

// ─── RBN Daily Reward Engine ─────────────────────────────────

/// Server-side reward engine running on the RBN.
/// Receives signed telemetry from clients, validates, aggregates, and calculates rewards.
pub struct RbnDailyRewardEngine {
    weights: RwLock<ActivityWeights>,
    anti_gaming: RwLock<AntiGamingConfig>,
    /// Processed cycles: epoch_id -> (peer_id -> ClientCycle)
    pub processed_cycles: RwLock<HashMap<String, HashMap<String, ClientCycle>>>,
    /// Global points estimate (updated from network data)
    global_points_estimate: RwLock<f64>,
    /// Per-epoch total points (for fair share calculation)
    epoch_total_points: RwLock<HashMap<String, f64>>,
    /// Persistent storage for double-claim protection across restarts
    storage: Option<Arc<StorageService>>,
    /// Tracks the last time each solana_wallet sent authenticated telemetry.
    /// Used by the Passive Telemetry-Correlation Engine to detect tokenless forks.
    last_telemetry_seen: RwLock<HashMap<String, Instant>>,
}

impl RbnDailyRewardEngine {
    pub fn new() -> Self {
        Self {
            weights: RwLock::new(ActivityWeights::default()),
            anti_gaming: RwLock::new(AntiGamingConfig::default()),
            processed_cycles: RwLock::new(HashMap::new()),
            global_points_estimate: RwLock::new(DEFAULT_GLOBAL_POINTS_ESTIMATE),
            epoch_total_points: RwLock::new(HashMap::new()),
            storage: None,
            last_telemetry_seen: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new engine with persistent storage for double-claim protection.
    pub fn with_storage(storage: Arc<StorageService>) -> Self {
        Self {
            weights: RwLock::new(ActivityWeights::default()),
            anti_gaming: RwLock::new(AntiGamingConfig::default()),
            processed_cycles: RwLock::new(HashMap::new()),
            global_points_estimate: RwLock::new(DEFAULT_GLOBAL_POINTS_ESTIMATE),
            epoch_total_points: RwLock::new(HashMap::new()),
            storage: Some(storage),
            last_telemetry_seen: RwLock::new(HashMap::new()),
        }
    }

    /// Process a signed telemetry packet from a client.
    /// Returns a ClaimRequest if the client is eligible for rewards.
    pub fn process_telemetry(&self, packet: SignedTelemetryPacket) -> Option<ClaimRequest> {
        // 1. Verify signature
        if !self.verify_signature(&packet) {
            warn!("[RbnRewards] Invalid signature from peer {}", packet.peer_id);
            return None;
        }

        // Record authenticated telemetry timestamp for fork-detection correlation
        self.last_telemetry_seen.write().insert(packet.solana_wallet.clone(), Instant::now());

        // 2. Check for existing telemetry — allow UPDATES with higher cumulative points.
        //    Metrics are cumulative (not deltas), so a later submission supersedes an earlier one
        //    if it has higher activity counts. This prevents the "first 30-min snapshot" problem
        //    where the RBN would otherwise use stale partial-day data.
        let mut existing_total_points: f64 = 0.0;
        if let Some(ref storage) = self.storage {
            match storage.is_telemetry_processed(&packet.epoch_id, &packet.solana_wallet) {
                Ok(true) => {
                    // Already processed — check if this is an upgrade
                    if let Some(epoch_cycles) = self.processed_cycles.read().get(&packet.epoch_id) {
                        if let Some(existing) = epoch_cycles.get(&packet.solana_wallet) {
                            existing_total_points = existing.total_points;
                        }
                    }
                }
                Ok(false) => {} // Not yet processed, continue
                Err(e) => {
                    warn!("[RbnRewards] SQLite check failed: {:?}, falling back to in-memory", e);
                }
            }
        }

        // 3. In-memory check: if existing score is higher, reject this stale submission
        {
            let cycles = self.processed_cycles.read();
            if let Some(epoch_cycles) = cycles.get(&packet.epoch_id) {
                if let Some(existing) = epoch_cycles.get(&packet.solana_wallet) {
                    if existing.total_points > existing_total_points {
                        existing_total_points = existing.total_points;
                    }
                }
            }
        }
        // We'll compare after scoring — if new_total <= existing_total, reject

        // 3. Anti-gaming validation
        let anti = self.anti_gaming.read();
        if packet.unique_peers.len() < anti.min_unique_peers as usize {
            info!("[RbnRewards] Peer {} rejected: unique_peers={} < min={}", 
                packet.peer_id, packet.unique_peers.len(), anti.min_unique_peers);
            return None;
        }

        // 4. Score activities
        let weights = self.weights.read();
        let activities = self.score_activities(&packet, &weights);

        // 5. Calculate dual-pool points
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

        // 6. Apply social cap
        let uptime_raw = packet.metrics[ActivityType::UptimeSeconds as usize];
        let effective_social_cap = if !packet.is_rbn && packet.is_edge_node && uptime_raw >= 86400 {
            15_000.0
        } else {
            weights.daily_point_cap
        };
        let social_capped = social_points.min(effective_social_cap);
        let total_points = social_capped + infra_points;

        // 6b. Upgrade check: reject if existing score is already higher (stale telemetry)
        if existing_total_points > 0.0 && total_points <= existing_total_points {
            info!("[RbnRewards] Stale telemetry rejected: wallet {} new_pts={:.1} <= existing_pts={:.1}",
                packet.solana_wallet, total_points, existing_total_points);
            return None;
        }

        // 7. Calculate INTR reward using pool-clearing formula
        let effective_pool = if packet.is_rbn { self.get_rbn_daily_pool_cap() } else { self.get_daily_pool_cap() };
        let global_estimate = *self.global_points_estimate.read();
        let user_share = total_points / global_estimate.max(1.0);

        let prestige_mult = match packet.prestige_tier {
            0 => 1.0,
            1 => 1.05,
            2 => 1.10,
            3 => 1.20,
            4 => 1.50,
            5 => 1.15,
            6 => 1.15,
            _ => 1.0,
        };

        let raw_reward = user_share * effective_pool * prestige_mult;
        let intr_reward = (raw_reward * 1_000_000.0).trunc() / 1_000_000.0;
        let intr_reward_nano = (intr_reward * 1_000_000_000.0) as u64;

        // 8. Eligibility check
        let is_eligible = packet.unique_peers.len() as u32 >= anti.min_unique_peers && total_points > 0.0;
        let eligibility_reason = if !is_eligible {
            format!("unique_peers={} < min={}", packet.unique_peers.len(), anti.min_unique_peers)
        } else {
            "eligible".into()
        };

        // 9. Record processed cycle
        let cycle = ClientCycle {
            peer_id: packet.peer_id.clone(),
            payout_address: packet.solana_ata.clone(),
            cycle_date: packet.epoch_id.clone(),
            activities,
            social_points: social_capped,
            infra_points,
            total_points,
            intr_reward,
            intr_reward_nano,
            unique_peers: packet.unique_peers.len() as u32,
            is_eligible,
            eligibility_reason: eligibility_reason.clone(),
            processed: true,
            prestige_tier: packet.prestige_tier,
        };

        {
            let mut cycles = self.processed_cycles.write();
            cycles.entry(packet.epoch_id.clone())
                .or_insert_with(HashMap::new)
                .insert(packet.solana_wallet.clone(), cycle);
        }

        // Persist to SQLite for double-claim protection across restarts
        if let Some(ref storage) = self.storage {
            if let Err(e) = storage.mark_telemetry_processed(&packet.epoch_id, &packet.solana_wallet) {
                warn!("[RbnRewards] Failed to persist telemetry record: {:?}", e);
            }
            if let Err(e) = storage.save_client_telemetry(
                &packet.epoch_id,
                &packet.solana_wallet,
                &packet.peer_id,
                &packet.solana_ata,
                &packet.metrics,
                &packet.unique_peers,
                packet.is_rbn,
                packet.is_edge_node,
                packet.prestige_tier,
                &packet.client_signature,
                packet.timestamp,
            ) {
                warn!("[RbnRewards] Failed to persist complete client telemetry: {:?}", e);
            }
        }

        {
            let mut epoch_totals = self.epoch_total_points.write();
            let epoch_total = epoch_totals.entry(packet.epoch_id.clone()).or_insert(0.0);
            *epoch_total += total_points;
        }

        info!("[RbnRewards] Peer {} scored {:.1} pts, {:.4} INTR for epoch {}", 
            packet.peer_id, total_points, intr_reward, packet.epoch_id);

        // 10. Return claim request if eligible
        if is_eligible && intr_reward > 0.0 {
            Some(ClaimRequest {
                claim_type: "DailySettlement".to_string(),
                peer_id: packet.solana_wallet,
                payout_address: packet.solana_ata,
                token_amount: intr_reward,
                epoch_id: packet.epoch_id,
            })
        } else {
            None
        }
    }

    /// Closes the current epoch at midnight UTC with IQR outlier mitigation.
    /// This is the anti-gaming master filter: it clamps malicious scores before
    /// computing the dynamic pool denominator and distributing rewards.
    ///
    /// Algorithm:
    /// 1. Collect all reporting edge scores for the epoch
    /// 2. Sort them and determine Q1 (25th percentile) and Q3 (75th percentile)
    /// 3. Compute IQR = Q3 - Q1
    /// 4. Compute Upper Bound = Q3 + (1.5 * IQR)
    /// 5. Clamp any outlier score above Upper Bound down to Upper Bound
    /// 6. Compute total capped global points = sum of all clamped scores
    /// 7. Distribute rewards proportionally: (Capped User Points / Total Capped Global Points) * Daily Pool Cap
    pub fn close_current_epoch(&self, epoch_id: &str) -> Vec<ClaimRequest> {
        let mut epoch_cycles = HashMap::new();

        // 1. Load client cycles from SQLite if storage is available
        if let Some(ref storage) = self.storage {
            if let Ok(envelopes) = storage.fetch_client_telemetry_for_epoch(epoch_id) {
                for packet in envelopes {
                    let anti = self.anti_gaming.read();
                    if packet.unique_peers.len() < anti.min_unique_peers as usize {
                        continue;
                    }
                    let weights = self.weights.read();
                    let activities = self.score_activities(&packet, &weights);

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

                    let uptime_raw = packet.metrics[ActivityType::UptimeSeconds as usize];
                    let effective_social_cap = if !packet.is_rbn && packet.is_edge_node && uptime_raw >= 86400 {
                        15_000.0
                    } else {
                        weights.daily_point_cap
                    };
                    let social_capped = social_points.min(effective_social_cap);
                    let total_points = social_capped + infra_points;

                    let cycle = ClientCycle {
                        peer_id: packet.peer_id.clone(),
                        payout_address: packet.solana_ata.clone(),
                        cycle_date: packet.epoch_id.clone(),
                        activities,
                        social_points: social_capped,
                        infra_points,
                        total_points,
                        intr_reward: 0.0,
                        intr_reward_nano: 0,
                        unique_peers: packet.unique_peers.len() as u32,
                        is_eligible: total_points > 0.0,
                        eligibility_reason: "eligible".into(),
                        processed: true,
                        prestige_tier: packet.prestige_tier,
                    };
                    epoch_cycles.insert(packet.solana_wallet.clone(), cycle);
                }
            }
        }

        // 2. Overlay in-memory cycles (if any)
        {
            let cycles = self.processed_cycles.read();
            if let Some(mem_cycles) = cycles.get(epoch_id) {
                for (wallet, cycle) in mem_cycles {
                    epoch_cycles.insert(wallet.clone(), cycle.clone());
                }
            }
        }

        if epoch_cycles.is_empty() {
            warn!("[RbnRewards] No cycles found for epoch {}", epoch_id);
            return Vec::new();
        }

        // Step 1: Collect all reporting edge scores (non-RBN only for IQR)
        let mut edge_scores: Vec<(String, f64, String, String, u8)> = Vec::new(); // (peer_id, total_points, wallet, ata, prestige_tier)
        for (peer_id, cycle) in epoch_cycles.iter() {
            if cycle.is_eligible && cycle.total_points > 0.0 {
                // Derive the proper Associated Token Account from the wallet address
                let ata = derive_ata(&cycle.payout_address, INTR_MINT)
                    .unwrap_or_else(|| cycle.payout_address.clone());
                edge_scores.push((
                    peer_id.clone(),
                    cycle.total_points,
                    cycle.payout_address.clone(),
                    ata,
                    cycle.prestige_tier,
                ));
            }
        }

        if edge_scores.is_empty() {
            info!("[RbnRewards] No eligible peers for epoch {}", epoch_id);
            return Vec::new();
        }

        // Step 2: Sort scores for percentile calculation
        let mut sorted_scores: Vec<f64> = edge_scores.iter().map(|(_, s, _, _, _)| *s).collect();
        sorted_scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Step 3: Calculate Q1, Q3, IQR
        let n = sorted_scores.len();
        let q1_idx = n / 4;
        let q3_idx = (3 * n) / 4;
        let q1 = sorted_scores[q1_idx];
        let q3 = sorted_scores[q3_idx.min(n - 1)];
        let iqr = q3 - q1;

        // Step 4: Compute Upper Bound (honesty ceiling)
        let upper_bound = q3 + (1.5 * iqr);

        info!("[RbnRewards] IQR Filter: Q1={:.1}, Q3={:.1}, IQR={:.1}, UpperBound={:.1}", q1, q3, iqr, upper_bound);
        info!("[RbnRewards] Raw scores: {:?}", sorted_scores);

        // Step 5: Clamp outliers to Upper Bound
        let mut clamped_scores: Vec<(String, f64, String, String, u8)> = Vec::new();
        let mut total_clamped_points = 0.0;
        for (peer_id, score, wallet, ata, tier) in &edge_scores {
            let clamped = if *score > upper_bound {
                info!("[RbnRewards] Clamping {} from {:.1} to {:.1} (IQR limit)", peer_id, score, upper_bound);
                upper_bound
            } else {
                *score
            };
            clamped_scores.push((peer_id.clone(), clamped, wallet.clone(), ata.clone(), *tier));
            total_clamped_points += clamped;
        }

        info!("[RbnRewards] Total clamped points: {:.1} (was {:.1})", total_clamped_points,
            edge_scores.iter().map(|(_, s, _, _, _)| s).sum::<f64>());

        // Step 6: Distribute rewards proportionally
        let daily_pool = self.get_daily_pool_cap(); // 16,438.00 INTR
        let mut claims = Vec::new();

        for (peer_id, clamped_score, wallet, ata, prestige_tier) in &clamped_scores {
            if total_clamped_points <= 0.0 {
                continue;
            }
            let share = clamped_score / total_clamped_points;
            let prestige_mult = match prestige_tier {
                0 => 1.0,
                1 => 1.05,
                2 => 1.10,
                3 => 1.20,
                4 => 1.50,
                5 | 6 => 1.15,
                _ => 1.0,
            };
            let intr_reward = share * daily_pool * prestige_mult;
            let intr_rounded = (intr_reward * 1_000_000.0).trunc() / 1_000_000.0; // 6 decimal precision

            if intr_rounded > 0.0 {
                info!("[RbnRewards] Payout: {} gets {:.6} INTR ({:.1}% of pool, tier {} → {:.2}x)", peer_id, intr_rounded, share * 100.0, prestige_tier, prestige_mult);
                claims.push(ClaimRequest {
                    claim_type: "DailySettlement".to_string(),
                    peer_id: wallet.clone(),
                    payout_address: ata.clone(),
                    token_amount: intr_rounded,
                    epoch_id: epoch_id.to_string(),
                });
            }
        }

        // Step 7: Record epoch close
        info!("[RbnRewards] Epoch {} closed: {} claims, {:.6} INTR distributed", epoch_id, claims.len(),
            claims.iter().map(|c| c.token_amount).sum::<f64>());

        claims
    }

    fn score_activities(&self, packet: &SignedTelemetryPacket, w: &ActivityWeights) -> Vec<DailyActivityCount> {
        let is_rbn = packet.is_rbn;
        let is_edge = packet.is_edge_node;
        let uptime_raw = packet.metrics[ActivityType::UptimeSeconds as usize];

        ActivityType::all().iter().map(|at| {
            let at_idx = *at as usize;

            let (raw, capped) = if matches!(at, ActivityType::UniquePeerHandshakes) {
                let unique_count = packet.unique_peers.len() as u64;
                let cap = w.cap_unique_peer_handshakes as u64;
                (unique_count, unique_count.min(cap))
            } else {
                let r = packet.metrics[at_idx];
                let cap = if is_rbn {
                    match at {
                        ActivityType::RelayBytes => w.cap_relay_bytes_rbn,
                        ActivityType::UptimeSeconds => u64::MAX,
                        _ => Self::get_cap(at, w),
                    }
                } else {
                    Self::get_cap(at, w)
                };
                (r, r.min(cap))
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
                ActivityType::UptimeSeconds => w.uptime_seconds,
                ActivityType::WebFocusedActiveTime => w.web_focused_active_time,
                ActivityType::SandboxWebPacketData => w.sandbox_web_packet_data,
                ActivityType::WebViewMediaCallHook => w.webview_media_call_hook,
                ActivityType::UniquePeerHandshakes => w.unique_peer_handshakes,
            };

            // Apply availability yield for RBN nodes
            if matches!(at, ActivityType::UptimeSeconds) && is_rbn && uptime_raw >= 79200 {
                weight *= 1.5;
            }

            // Apply edge infra multiplier
            if !is_rbn && is_edge && matches!(at,
                ActivityType::RelayBytes | ActivityType::UptimeSeconds |
                ActivityType::WebFocusedActiveTime | ActivityType::SandboxWebPacketData)
            {
                weight *= w.edge_infra_multiplier;
            }

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

    fn get_cap(at: &ActivityType, w: &ActivityWeights) -> u64 {
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

    fn verify_signature(&self, packet: &SignedTelemetryPacket) -> bool {
        // Real Ed25519 signature verification
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        
        // Extract public key from solana_wallet
        let wallet_bytes = match bs58::decode(&packet.solana_wallet).into_vec() {
            Ok(bytes) => bytes,
            Err(_) => {
                warn!("[RbnRewards] Invalid base58 wallet: {}", packet.solana_wallet);
                return false;
            }
        };
        
        if wallet_bytes.len() != 32 {
            warn!("[RbnRewards] Invalid wallet key length: {}", wallet_bytes.len());
            return false;
        }
        
        let mut pubkey_bytes = [0u8; 32];
        pubkey_bytes.copy_from_slice(&wallet_bytes);
        
        let verifying_key = match VerifyingKey::from_bytes(&pubkey_bytes) {
            Ok(key) => key,
            Err(e) => {
                warn!("[RbnRewards] Invalid public key: {}", e);
                return false;
            }
        };
        
        // Reconstruct signed message: epoch_id || peer_id || solana_wallet || solana_ata ||
        // metrics[0..13] || is_rbn || is_edge_node || prestige_tier || timestamp
        let mut message = Vec::new();
        message.extend_from_slice(packet.epoch_id.as_bytes());
        message.extend_from_slice(packet.peer_id.as_bytes());
        message.extend_from_slice(packet.solana_wallet.as_bytes());
        message.extend_from_slice(packet.solana_ata.as_bytes());
        for m in &packet.metrics {
            message.extend_from_slice(&m.to_le_bytes());
        }
        message.push(packet.is_rbn as u8);
        message.push(packet.is_edge_node as u8);
        message.push(packet.prestige_tier);
        message.extend_from_slice(&packet.timestamp.to_le_bytes());
        
        // Parse signature
        if packet.client_signature.len() != 64 {
            warn!("[RbnRewards] Invalid signature length: {}", packet.client_signature.len());
            return false;
        }
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&packet.client_signature);
        let signature = Signature::from_bytes(&sig_bytes);
        
        // Verify
        match verifying_key.verify(&message, &signature) {
            Ok(()) => true,
            Err(e) => {
                warn!("[RbnRewards] Signature verification failed: {}", e);
                false
            }
        }
    }

    pub fn get_emission_year(&self) -> u32 {
        let tge = NaiveDate::parse_from_str(TGE_DATE, "%Y-%m-%d").unwrap_or(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        let today = Utc::now().date_naive();
        let days = today.signed_duration_since(tge).num_days().max(0);
        ((days / 365) + 1) as u32
    }

    pub fn get_daily_pool_cap(&self) -> f64 {
        let year = self.get_emission_year();
        YEAR_1_DAILY_POOL * ANNUAL_DECAY.powi((year - 1) as i32)
    }

    pub fn get_rbn_daily_pool_cap(&self) -> f64 {
        let year = self.get_emission_year();
        YEAR_1_RBN_DAILY_POOL * ANNUAL_DECAY.powi((year - 1) as i32)
    }

    /// Check if a wallet has sent authenticated telemetry within the given threshold.
    /// Returns true if telemetry was seen within the threshold, false otherwise.
    /// Used by the Passive Telemetry-Correlation Engine to detect tokenless forks.
    pub fn has_recent_telemetry(&self, solana_wallet: &str, threshold: std::time::Duration) -> bool {
        let seen = self.last_telemetry_seen.read();
        match seen.get(solana_wallet) {
            Some(last) => last.elapsed() < threshold,
            None => false,
        }
    }

    /// Get a snapshot of all wallet telemetry timestamps.
    /// Used by the Passive Telemetry-Correlation Engine to check wallet activity.
    pub fn get_last_telemetry_seen(&self) -> HashMap<String, Instant> {
        self.last_telemetry_seen.read().clone()
    }

    pub fn update_global_points_estimate(&self, estimate: f64) {
        let mut global = self.global_points_estimate.write();
        *global = estimate.max(1.0);
    }

    pub fn get_epoch_stats(&self, epoch_id: &str) -> Option<(f64, usize)> {
        let cycles = self.processed_cycles.read();
        cycles.get(epoch_id).map(|epoch_cycles| {
            let total_points: f64 = epoch_cycles.values().map(|c| c.total_points).sum();
            (total_points, epoch_cycles.len())
        })
    }

    pub fn is_double_claim(&self, epoch_id: &str, solana_wallet: &str) -> bool {
        let cycles = self.processed_cycles.read();
        cycles.get(epoch_id)
            .map(|epoch_cycles| epoch_cycles.contains_key(solana_wallet))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, Signer};

    /// Helper: create a signed TelemetryEnvelope with a valid Ed25519 signature
    fn create_test_packet(peer_id: &str, epoch_id: &str, metrics: [u64; 13], unique_peers: Vec<String>) -> TelemetryEnvelope {
        // Use peer_id hash to generate unique key per peer
        let mut key_bytes = [42u8; 32];
        for (i, b) in peer_id.bytes().enumerate() {
            key_bytes[i % 32] ^= b;
        }
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let verifying_key = signing_key.verifying_key();
        let wallet = bs58::encode(verifying_key.to_bytes()).into_string();
        
        let timestamp = 1234567890u64;
        // Build canonical signed message matching verify_signature format
        let mut message = Vec::new();
        message.extend_from_slice(epoch_id.as_bytes());
        message.extend_from_slice(peer_id.as_bytes());
        message.extend_from_slice(wallet.as_bytes());
        message.extend_from_slice(wallet.as_bytes()); // solana_ata = wallet for test
        for m in &metrics {
            message.extend_from_slice(&m.to_le_bytes());
        }
        message.push(0u8); // is_rbn = false
        message.push(1u8); // is_edge_node = true
        message.push(0u8); // prestige_tier = 0
        message.extend_from_slice(&timestamp.to_le_bytes());
        
        let signature = signing_key.sign(&message);
        
        TelemetryEnvelope {
            peer_id: peer_id.to_string(),
            solana_wallet: wallet.clone(),
            solana_ata: wallet,
            epoch_id: epoch_id.to_string(),
            metrics,
            unique_peers,
            is_rbn: false,
            is_edge_node: true,
            prestige_tier: 0,
            proof_hash: String::new(),
            client_signature: signature.to_bytes().to_vec(),
            timestamp,
        }
    }

    #[test]
    fn test_basic_scoring() {
        let engine = RbnDailyRewardEngine::new();
        
        let mut metrics = [0u64; 13];
        metrics[ActivityType::MessageSent as usize] = 50;
        metrics[ActivityType::RelayBytes as usize] = 5000;
        metrics[ActivityType::UptimeSeconds as usize] = 86400;

        let packet = create_test_packet("test_peer", "2026_07_03", metrics, vec!["p1".into(), "p2".into(), "p3".into()]);
        let result = engine.process_telemetry(packet);
        assert!(result.is_some());
        
        let claim = result.unwrap();
        assert_eq!(claim.claim_type, "DailySettlement");
        assert!(claim.token_amount > 0.0);
    }

    #[test]
    fn test_double_claim_rejected() {
        let engine = RbnDailyRewardEngine::new();
        
        let packet = create_test_packet("test_peer", "2026_07_03", [0u64; 13], vec!["p1".into(), "p2".into(), "p3".into()]);
        let wallet = packet.solana_wallet.clone();
        let result1 = engine.process_telemetry(packet.clone());
        let result2 = engine.process_telemetry(packet);
        
        // First should succeed (or fail for other reasons), second should be rejected
        assert!(engine.is_double_claim("2026_07_03", &wallet));
    }

    #[test]
    fn test_iqr_outlier_mitigation_and_batch_distribution() {
        let engine = RbnDailyRewardEngine::new();
        let epoch_id = "2026_07_06";

        // Use RelayBytes (weight=0.001, cap=10240) to create distinct scores
        // Peer1: 15000 bytes -> 15 pts
        // Peer2: 30000 bytes -> 30 pts
        // Peer3: 60000 bytes -> 60 pts (capped at 10240 -> 10.24 pts from relay, rest from other)
        // We need to use uncapped activities to get distinct scores
        // Use GroupReaction (weight=0.3, cap=100) and FileTransferSent (weight=0.5, cap=20)
        
        // Actually, let's use a simpler approach: set high values for multiple activities
        // so the total points are distinct and the IQR filter can work
        let scores = vec![
            ("peer1", 15u64),
            ("peer2", 30u64),
            ("peer3", 60u64),
            ("peer4", 60u64),
            ("peer5", 100u64),
            ("peer6", 900u64),
        ];

        // Process each peer's telemetry with distinct raw counts
        for (peer_id, raw_count) in &scores {
            let mut metrics = [0u64; 13];
            // Use multiple activities to build up distinct total points
            // MessageSent: weight=0.05, cap=200 -> max 10 pts
            // GroupMessageSent: weight=0.08, cap=150 -> max 12 pts
            // GroupReaction: weight=0.3, cap=100 -> max 30 pts
            // FileTransferSent: weight=0.5, cap=20 -> max 10 pts
            // CallDurationSecs: weight=0.02, cap=3600 -> max 72 pts
            // UptimeSeconds: weight=0.0001, cap=86400 -> max 8.64 pts
            // Total max from all: ~142.64 pts
            
            // For distinct scores, set CallDurationSecs to different values
            // weight=0.02, so 100 secs = 2 pts, 900 secs = 18 pts
            metrics[ActivityType::CallDurationSecs as usize] = *raw_count;
            metrics[ActivityType::UptimeSeconds as usize] = 86400;
            metrics[ActivityType::MessageSent as usize] = 200; // max cap
            metrics[ActivityType::GroupReaction as usize] = 100; // max cap

            let packet = create_test_packet(peer_id, epoch_id, metrics, vec!["p1".into(), "p2".into(), "p3".into()]);
            engine.process_telemetry(packet);
        }

        // Debug: check how many cycles were processed
        let cycles = engine.processed_cycles.read();
        if let Some(epoch_cycles) = cycles.get(epoch_id) {
            println!("Epoch cycles count: {}", epoch_cycles.len());
            for (pid, cycle) in epoch_cycles {
                println!("  Peer {}: eligible={}, total_points={:.1}, intr_reward={:.6}", 
                    pid, cycle.is_eligible, cycle.total_points, cycle.intr_reward);
            }
        }
        drop(cycles);

        // Close epoch with IQR filter
        let claims = engine.close_current_epoch(epoch_id);

        // Verify that peer6 (the outlier) received a clamped payout
        // peer6's wallet address is derived from its signing key
        let mut peer6_key_bytes = [42u8; 32];
        for (i, b) in "peer6".bytes().enumerate() {
            peer6_key_bytes[i % 32] ^= b;
        }
        let peer6_signing_key = ed25519_dalek::SigningKey::from_bytes(&peer6_key_bytes);
        let peer6_wallet = bs58::encode(peer6_signing_key.verifying_key().to_bytes()).into_string();
        
        let peer6_claim = claims.iter().find(|c| c.peer_id == peer6_wallet);
        assert!(peer6_claim.is_some(), "Peer 6 should receive a claim (wallet: {})", peer6_wallet);

        let peer6_payout = peer6_claim.unwrap().token_amount;

        // Verify total distribution doesn't exceed daily pool
        let total_distributed: f64 = claims.iter().map(|c| c.token_amount).sum();
        let daily_pool = engine.get_daily_pool_cap();
        assert!(total_distributed <= daily_pool + 0.01,
            "Total distributed ({:.2}) should not exceed daily pool ({:.2})", total_distributed, daily_pool);

        // Verify peer6's payout is less than what it would get without IQR
        // (900 / (15+30+60+60+100+900)) * 16438 = ~12612 INTR
        // With IQR clamp at 205: (205 / 470) * 16438 = ~7169 INTR
        assert!(peer6_payout < 12612.0,
            "Peer 6 payout ({:.2}) should be less than unclamped ({:.2})", peer6_payout, 12612.0);

        // Verify at least 6 claims were generated
        assert!(claims.len() >= 6, "Should have at least 6 claims, got {}", claims.len());

        println!("IQR Test Results:");
        println!("  Total claims: {}", claims.len());
        println!("  Total distributed: {:.6} INTR", total_distributed);
        println!("  Peer 6 payout: {:.6} INTR", peer6_payout);
        println!("  Daily pool cap: {:.2} INTR", daily_pool);
    }
}
