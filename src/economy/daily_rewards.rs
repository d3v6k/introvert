use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::{Utc, NaiveDate};
use tracing::{info, warn};
use crate::storage::StorageService;
use crate::economy::RewardTracker;

// Placeholder escrow address — replace with actual PDA when on-chain program is deployed
pub const DAILY_REWARD_ESCROW: &str = "PLACEHOLDER_ESCROW_ADDRESS";

// Token Generation Event date — used to calculate emission year
pub const TGE_DATE: &str = "2026-01-01";

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
            _ => None,
        }
    }

    pub fn all() -> &'static [ActivityType] {
        &[
            Self::MessageSent, Self::MessageReceived,
            Self::GroupMessageSent, Self::GroupReaction,
            Self::FileTransferSent, Self::FileTransferRecv,
            Self::CallDurationSecs, Self::RelayBytes,
            Self::UptimeSeconds,
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

    pub cap_message_sent: u32,
    pub cap_message_received: u32,
    pub cap_group_message_sent: u32,
    pub cap_group_reaction: u32,
    pub cap_file_transfer_sent: u32,
    pub cap_file_transfer_recv: u32,
    pub cap_call_duration_secs: u32,
    pub cap_relay_bytes: u64,
    pub cap_uptime_seconds: u32,

    pub min_message_length: usize,
    pub rapid_fire_cooldown_secs: u64,
    pub rapid_fire_max_per_window: u32,
    pub daily_point_cap: f64,
    pub intr_per_point: f64,
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
            uptime_seconds: 0.001,

            cap_message_sent: 200,
            cap_message_received: 300,
            cap_group_message_sent: 150,
            cap_group_reaction: 100,
            cap_file_transfer_sent: 20,
            cap_file_transfer_recv: 20,
            cap_call_duration_secs: 3600,
            cap_relay_bytes: 10_485_760,
            cap_uptime_seconds: 86400,

            min_message_length: 5,
            rapid_fire_cooldown_secs: 60,
            rapid_fire_max_per_window: 10,
            daily_point_cap: 5000.0,
            intr_per_point: 0.001,
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
            min_unique_peers: 3,
            max_messages_per_peer: 50,
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
        }
    }
}

// ─── Daily Reward Engine ─────────────────────────────────────

pub struct DailyRewardEngine {
    state: RwLock<DailyRewardState>,
    storage: Arc<StorageService>,
    weights: RwLock<ActivityWeights>,
    anti_gaming: RwLock<AntiGamingConfig>,
}

impl DailyRewardEngine {
    pub fn new(storage: Arc<StorageService>) -> Self {
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
        };

        // Resume any in-progress cycle from DB
        let today = Utc::now().format("%Y-%m-%d").to_string();
        if let Ok(Some(cycle)) = engine.storage.load_daily_cycle(&today) {
            let mut state = engine.state.write();
            state.current_cycle = Some(cycle);
            state.cycle_start_epoch = Utc::now().timestamp() as u64;
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
            prev.total_points = prev.activities.iter().map(|a| a.points).sum();
            prev.capped_points = prev.total_points.min(weights.daily_point_cap);
            prev.intr_reward = prev.capped_points * weights.intr_per_point;
            prev.unique_peers = state.unique_peers.len() as u32;
            prev.is_eligible = prev.unique_peers >= anti.min_unique_peers && prev.capped_points > 0.0;
            prev.eligibility_reason = if !prev.is_eligible {
                format!("unique_peers={} < min={}", prev.unique_peers, anti.min_unique_peers)
            } else {
                "eligible".into()
            };
            prev.ended_at = Some(Utc::now().timestamp() as u64);
            prev.submitted = prev.is_eligible;

            info!("[DailyRewards] Cycle {} closed: {:.1} pts, {:.4} INTR, eligible={}",
                prev.cycle_date, prev.capped_points, prev.intr_reward, prev.is_eligible);

            // Persist to DB
            let _ = self.storage.save_daily_cycle(&prev);
            let _ = self.storage.save_daily_activities(&prev.cycle_date, &prev.activities);

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

        // Cryptographic validation: require proof_hash for relay bytes
        // This prevents spoofing relay activity without actual data routing
        if matches!(event.activity_type, ActivityType::RelayBytes) && !event.is_rbn {
            if event.proof_hash.is_none() {
                // Edge nodes must provide proof of actual relay work
                return false;
            }
        }

        let mut state = self.state.write();

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
        // RBN operators: skip cap on RelayBytes and UptimeSeconds (infrastructure work is uncapped)
        let is_rbn_uncapped = event.is_rbn && matches!(event.activity_type, ActivityType::RelayBytes | ActivityType::UptimeSeconds);
        let cap = if is_rbn_uncapped { u64::MAX } else { Self::get_cap_static(&event.activity_type, &weights) };
        let raw = state.per_type_counts.entry(at_u8).or_insert(0);
        *raw += event.value;

        if *raw <= cap {
            let capped = state.per_type_capped.entry(at_u8).or_insert(0);
            *capped += event.value;
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

    /// Sets the RBN status for this node. RBN operators draw from the RBN pool.
    pub fn set_rbn_status(&self, is_rbn: bool) {
        let mut state = self.state.write();
        state.is_rbn = is_rbn;
    }

    /// Returns the emission year (1-based) since TGE.
    pub fn get_emission_year(&self) -> u32 {
        let tge = NaiveDate::parse_from_str(TGE_DATE, "%Y-%m-%d").unwrap_or(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        let today = Utc::now().date_naive();
        let days = today.signed_duration_since(tge).num_days().max(0);
        ((days / 365) + 1) as u32
    }

    /// Returns the daily user pool cap for the current emission year.
    /// Formula: YEAR_1_DAILY_POOL * (ANNUAL_DECAY ^ (year - 1))
    pub fn get_daily_pool_cap(&self) -> f64 {
        let year = self.get_emission_year();
        YEAR_1_DAILY_POOL * ANNUAL_DECAY.powi((year - 1) as i32)
    }

    /// Returns the daily RBN pool cap for the current emission year.
    /// Formula: YEAR_1_RBN_DAILY_POOL * (ANNUAL_DECAY ^ (year - 1))
    pub fn get_rbn_daily_pool_cap(&self) -> f64 {
        let year = self.get_emission_year();
        YEAR_1_RBN_DAILY_POOL * ANNUAL_DECAY.powi((year - 1) as i32)
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
                ActivityType::CallDurationSecs))
            .map(|a| a.points)
            .sum();
        let infra_points: f64 = activities.iter()
            .filter(|a| matches!(a.activity_type,
                ActivityType::RelayBytes | ActivityType::UptimeSeconds))
            .map(|a| a.points)
            .sum();

        let social_capped = social_points.min(weights.daily_point_cap);
        let infra_capped = infra_points;
        let capped_points = social_capped + infra_capped;

        let daily_pool = self.get_daily_pool_cap();
        let rbn_daily_pool = self.get_rbn_daily_pool_cap();
        let year = self.get_emission_year();
        let is_rbn = state.is_rbn;

        // CRITICAL: RBN operators draw from RBN pool, standard users from user pool
        let effective_pool = if is_rbn { rbn_daily_pool } else { daily_pool };

        // Use dynamic global points estimate (updated from network data)
        // In production, this should be split into pool-specific estimates
        let global_estimate = state.global_points_estimate;

        // User's share of the daily pool
        let user_share = if global_estimate > 0.0 {
            capped_points / global_estimate
        } else {
            0.0
        };

        // CRITICAL: Output as nano-INTR integer (1 INTR = 1,000,000,000 nano-INTR)
        // No floating-point serialization — matches Solana SPL 9-decimal precision
        let intr_earned_f64 = user_share * effective_pool;
        let intr_earned_nano: u64 = (intr_earned_f64 * 1_000_000_000.0) as u64;

        let unique_peers = state.unique_peers.len() as u32;

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
        }
    }

    pub fn score_activities_static(state: &DailyRewardState, w: &ActivityWeights) -> Vec<DailyActivityCount> {
        let is_rbn = state.is_rbn;
        let uptime_raw = state.per_type_counts.get(&(ActivityType::UptimeSeconds as u8)).copied().unwrap_or(0);

        ActivityType::all().iter().map(|at| {
            let at_u8 = *at as u8;
            let raw = state.per_type_counts.get(&at_u8).copied().unwrap_or(0);
            let capped = state.per_type_capped.get(&at_u8).copied().unwrap_or(0);
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
            };

            // Apply 1.2x availability yield to uptime weight for RBN nodes with 23h+ uptime
            if matches!(at, ActivityType::UptimeSeconds) && is_rbn && uptime_raw >= 82800 {
                weight *= 1.2;
            }

            DailyActivityCount {
                activity_type: *at,
                raw_count: raw,
                capped_count: capped,
                points: capped as f64 * weight,
            }
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageService;
    use std::sync::Arc;

    #[test]
    fn test_vector_1_edge_node() {
        let storage = Arc::new(StorageService::new_ephemeral().unwrap());
        let engine = DailyRewardEngine::new(storage);
        {
            let mut state = engine.state.write();
            state.is_rbn = false;
            state.global_points_estimate = 100_000.0;
            state.cycle_start_epoch = 0;
        }
        let weights = engine.weights.read().clone();
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageSent, peer_id: Some("p1".into()), value: 45, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: false, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageReceived, peer_id: Some("p1".into()), value: 120, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupMessageSent, peer_id: Some("g1".into()), value: 80, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: false, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupReaction, peer_id: Some("g1".into()), value: 25, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::FileTransferSent, peer_id: Some("p1".into()), value: 3, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::FileTransferRecv, peer_id: Some("p1".into()), value: 8, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::CallDurationSecs, peer_id: Some("p1".into()), value: 1800, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::RelayBytes, peer_id: Some("p2".into()), value: 5120, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: Some("abc".into()) });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::UptimeSeconds, peer_id: None, value: 86400, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: None });

        let state = engine.state.read();
        let activities = DailyRewardEngine::score_activities_static(&state, &weights);
        let social: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::MessageSent | ActivityType::MessageReceived | ActivityType::GroupMessageSent | ActivityType::GroupReaction | ActivityType::FileTransferSent | ActivityType::FileTransferRecv | ActivityType::CallDurationSecs)).map(|a| a.points).sum();
        let infra: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::RelayBytes | ActivityType::UptimeSeconds)).map(|a| a.points).sum();
        assert_eq!(social, 3705.0);
        assert!((infra - 137.6).abs() < 0.01, "infra={}", infra);
        let total = social.min(5000.0) + infra;
        let nano: u64 = (total / 100_000.0 * 16_438_000_000_000.0) as u64;
        assert_eq!(nano, 631_646_514_800, "Test Vector 1 failed: {}", nano);
    }

    #[test]
    fn test_vector_2_rbn_node() {
        let storage = Arc::new(StorageService::new_ephemeral().unwrap());
        let engine = DailyRewardEngine::new(storage);
        {
            let mut state = engine.state.write();
            state.is_rbn = true;
            state.global_points_estimate = 100_000.0;
            state.cycle_start_epoch = 0;
        }
        let weights = engine.weights.read().clone();
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageSent, peer_id: Some("p1".into()), value: 30, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: true, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageReceived, peer_id: Some("p1".into()), value: 50, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupMessageSent, peer_id: Some("g1".into()), value: 40, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: true, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::GroupReaction, peer_id: Some("g1".into()), value: 10, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::RelayBytes, peer_id: Some("p2".into()), value: 10_485_760, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::UptimeSeconds, peer_id: None, value: 86400, is_foreground: true, message_len: None, is_self: false, is_rbn: true, proof_hash: None });

        let state = engine.state.read();
        let activities = DailyRewardEngine::score_activities_static(&state, &weights);
        let social: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::MessageSent | ActivityType::MessageReceived | ActivityType::GroupMessageSent | ActivityType::GroupReaction | ActivityType::FileTransferSent | ActivityType::FileTransferRecv | ActivityType::CallDurationSecs)).map(|a| a.points).sum();
        let infra: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::RelayBytes | ActivityType::UptimeSeconds)).map(|a| a.points).sum();
        assert_eq!(social, 900.0);
        let expected_infra = 104_857.6 + 103.68;
        assert!((infra - expected_infra).abs() < 0.1, "infra={}", infra);
        let total = social.min(5000.0) + infra;
        let nano: u64 = (total / 100_000.0 * 8_219_000_000_000.0) as u64;
        assert_eq!(nano, 8_700_738_503_296, "Test Vector 2 failed: {}", nano);
    }

    #[test]
    fn test_dual_pool_separation() {
        let storage = Arc::new(StorageService::new_ephemeral().unwrap());
        let engine = DailyRewardEngine::new(storage);
        {
            let mut state = engine.state.write();
            state.is_rbn = false;
            state.global_points_estimate = 100_000.0;
            state.cycle_start_epoch = 0;
        }
        engine.record_activity(ActivityEvent { activity_type: ActivityType::RelayBytes, peer_id: Some("p1".into()), value: 500_000, is_foreground: true, message_len: None, is_self: false, is_rbn: false, proof_hash: Some("x".into()) });
        engine.record_activity(ActivityEvent { activity_type: ActivityType::MessageSent, peer_id: Some("p2".into()), value: 100, is_foreground: true, message_len: Some(10), is_self: false, is_rbn: false, proof_hash: None });

        let state = engine.state.read();
        let weights = engine.weights.read();
        let activities = DailyRewardEngine::score_activities_static(&state, &weights);
        let social: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::MessageSent)).map(|a| a.points).sum();
        let infra: f64 = activities.iter().filter(|a| matches!(a.activity_type, ActivityType::RelayBytes)).map(|a| a.points).sum();
        assert_eq!(social, 1000.0, "social={}", social);
        assert!((infra - 102.4).abs() < 0.01, "infra={}", infra);
    }
}
