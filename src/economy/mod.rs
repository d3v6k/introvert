use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;
use crate::storage::StorageService;

pub mod solana;
pub mod daily_rewards;
pub mod balance_gating;
pub mod ledger_cron;

use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardProof {
    pub provider_pubkey: String, // The Relay/Node providing the service
    pub consumer_peer_id: String, // The Peer consuming the service
    pub relayed_bytes: u64,
    pub timestamp: u64,
}

struct EconomyState {
    outbound_relayed_bytes: u64,
    mailbox_storage_bytes_seconds: u64,
    uptime_seconds: u64,
    pending_per_consumer: HashMap<String, u64>,
    pending_daily_reward_nano_intr: u64,  // Daily rewards tracked in nano-INTR (1 INTR = 1,000,000,000 nano-INTR, matching Solana 9-decimal SPL)
    last_claim_timestamp: u64,
}

pub struct RewardTracker {
    state: Arc<RwLock<EconomyState>>,
    storage: Option<Arc<StorageService>>,
    threshold: u64,
    cooldown_secs: u64,
    start_time: std::time::Instant,
    /// Shared metrics bridge: DailyRewardEngine writes, RewardTracker reads for telemetry
    pub shared_metrics: Arc<RwLock<[u64; 13]>>,
}

impl RewardTracker {
    pub fn new(storage: Option<Arc<StorageService>>, shared_metrics: Arc<RwLock<[u64; 13]>>) -> Self {
        let initial_bytes = if let Some(ref s) = storage {
            s.get_total_relayed_from_db().unwrap_or(0)
        } else {
            0
        };

        Self {
            state: Arc::new(RwLock::new(EconomyState {
                outbound_relayed_bytes: initial_bytes,
                mailbox_storage_bytes_seconds: 0,
                uptime_seconds: 0,
                pending_per_consumer: HashMap::new(),
                pending_daily_reward_nano_intr: 0,
                last_claim_timestamp: 0,
            })),
            storage,
            threshold: 10_000_000_000,
            cooldown_secs: 300,
            start_time: std::time::Instant::now(),
            shared_metrics,
        }
    }

    pub fn record_relay(&self, consumer_peer_id: &str, bytes: u64) {
        let mut state = self.state.write();
        state.outbound_relayed_bytes += bytes;
        
        let entry = state.pending_per_consumer.entry(consumer_peer_id.to_string()).or_insert(0);
        *entry += bytes;

        if let Some(ref s) = self.storage {
            if let Err(e) = s.log_reward(bytes) {
                tracing::error!("[Economy] Failed to log reward: {}", e);
            }
        }
    }

    /// Records mailbox storage usage. Anchor nodes earn yield based on bytes * seconds.
    pub fn record_mailbox_storage(&self, bytes: u64, seconds: u64) {
        let product = bytes.saturating_mul(seconds);
        let mut state = self.state.write();
        state.mailbox_storage_bytes_seconds = state.mailbox_storage_bytes_seconds.saturating_add(product);
        
        if let Some(ref s) = self.storage {
            if let Err(e) = s.record_mailbox_storage(product) {
                tracing::error!("[Economy] Failed to record mailbox storage: {}", e);
            }
        }
    }

    /// Updates uptime. Nodes with > 99% uptime receive 'Availability Yield' multiplier.
    pub fn update_uptime(&self) {
        let mut state = self.state.write();
        state.uptime_seconds = self.start_time.elapsed().as_secs();
    }

    /// Prepares a reward proof for a specific consumer, applying Availability Yield if applicable.
    pub fn prepare_reward_proof(&self, provider_pubkey: &str, consumer_peer_id: &str) -> Option<(u64, Vec<u8>)> {
        let state = self.state.read();
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut pending_bytes = *state.pending_per_consumer.get(consumer_peer_id).unwrap_or(&0);

        // Availability Yield Logic (v3.0.1): If node uptime >= 22 hours, apply 1.5x multiplier
        // Note: The 1.5x is applied to the uptime WEIGHT in daily_rewards.rs score_activities_static()
        // Here it is applied to pending_bytes for the relay-based reward proof system
        if state.uptime_seconds >= 79200 {
            pending_bytes = (pending_bytes as f64 * 1.5) as u64;
        }

        // Check threshold and cooldown
        if pending_bytes >= self.threshold && (now - state.last_claim_timestamp >= self.cooldown_secs) {
            let proof = RewardProof {
                provider_pubkey: provider_pubkey.to_string(),
                consumer_peer_id: consumer_peer_id.to_string(),
                relayed_bytes: pending_bytes,
                timestamp: now,
            };
            
            let bytes = serde_json::to_vec(&proof).ok()?;
            Some((pending_bytes, bytes))
        } else {
            None
        }
    }

    /// Explicitly commits a claimed amount for a consumer.
    pub fn commit_reward_claim(&self, consumer_peer_id: &str, bytes_claimed: u64) {
        let mut state = self.state.write();
        if let Some(pending) = state.pending_per_consumer.get_mut(consumer_peer_id) {
            *pending = pending.saturating_sub(bytes_claimed);
        }
        state.last_claim_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    pub fn get_total_relayed(&self) -> u64 {
        self.state.read().outbound_relayed_bytes
    }

    pub fn get_pending_rewards(&self) -> u64 {
        self.state.read().pending_per_consumer.values().sum()
    }

    pub fn get_pending_consumers(&self) -> Vec<String> {
        self.state.read().pending_per_consumer.keys().cloned().collect()
    }

    pub fn get_last_claim_timestamp(&self) -> u64 {
        self.state.read().last_claim_timestamp
    }

    pub fn needs_seed_balance(&self) -> bool {
        if let Some(ref s) = self.storage {
            !s.is_seed_claimed()
        } else {
            false
        }
    }

    pub fn prepare_seed_request(&self, user_address: &str) -> Option<String> {
        // Prepare a request for the initial onboarding seed balance
        Some(format!("SEED_REQUEST:{}", user_address))
    }

    pub fn commit_seed_claimed(&self) {
        if let Some(ref s) = self.storage {
            let _ = s.set_seed_claimed(true);
        }
    }

    pub fn record_message_activity(&self, peer_id: &str) {
        // Phase III: Activity Yield
        // For now, we record a flat 1KB activity credit
        self.record_relay(peer_id, 1024);
    }

    /// Checks if the node's identity lease is valid.
    /// A valid lease requires:
    /// 1. Balance >= minimum threshold (100,000 INTR for edge nodes)
    /// 2. Last claim was within the last 30 days (lease renewal window)
    pub fn is_lease_valid(&self, balance: u64) -> bool {
        const MIN_BALANCE_NANO: u64 = 100_000_000_000_000; // 100,000 INTR in nano-INTR
        const LEASE_RENEWAL_SECS: u64 = 2_592_000; // 30 days in seconds

        // Check minimum balance
        if balance < MIN_BALANCE_NANO {
            tracing::warn!("[Economy] Lease invalid: balance {} < minimum {}", balance, MIN_BALANCE_NANO);
            return false;
        }

        // Check lease renewal window
        let state = self.state.read();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if state.last_claim_timestamp > 0 {
            let elapsed = now.saturating_sub(state.last_claim_timestamp);
            if elapsed > LEASE_RENEWAL_SECS {
                tracing::warn!("[Economy] Lease expired: last claim was {} seconds ago (max {})", elapsed, LEASE_RENEWAL_SECS);
                return false;
            }
        }

        true
    }

    /// DEV ONLY: Overrides the internal start time to simulate long uptimes.
    pub fn simulate_uptime(&self, seconds: u64) {
        let mut state = self.state.write();
        state.uptime_seconds = seconds;
    }

    /// Records a daily reward amount into the pending claim pool.
    /// Called by DailyRewardEngine at cycle close.
    /// Tracks in nano-INTR units (1 INTR = 1,000,000,000 nano-INTR), matching Solana's 9-decimal SPL standard.
    pub fn record_daily_reward(&self, intr_amount: f64) {
        let nano_intr = (intr_amount * 1_000_000_000.0) as u64;
        if nano_intr == 0 { return; }
        let mut state = self.state.write();
        state.pending_daily_reward_nano_intr += nano_intr;
        tracing::info!("[Economy] Daily reward recorded: {:.9} INTR ({} nano-INTR)", intr_amount, nano_intr);
    }

    /// Returns pending daily rewards in INTR units.
    pub fn get_pending_daily_reward_intr(&self) -> f64 {
        let state = self.state.read();
        state.pending_daily_reward_nano_intr as f64 / 1_000_000_000.0
    }
}



/// Telemetry data for sending to RBN.
#[derive(Debug, Clone)]
pub struct TelemetryData {
    pub peer_id: String,
    pub metrics: [u64; 13],
    pub timestamp: u64,
}

impl RewardTracker {
    /// Packages current activity metrics into a TelemetryData for the RBN.
    pub fn package_telemetry(&self, peer_id: &str) -> TelemetryData {
        let state = self.state.read();
        let mut metrics = *self.shared_metrics.read();
        // Overlay relay_bytes and uptime from RewardTracker (more accurate for these)
        metrics[7] = state.outbound_relayed_bytes;
        metrics[8] = state.uptime_seconds;
        TelemetryData {
            peer_id: peer_id.to_string(),
            metrics,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}
impl Default for RewardTracker {
    fn default() -> Self {
        Self::new(None, Arc::new(RwLock::new([0u64; 13])))
    }
}

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

pub fn derive_ata(wallet: &str, mint: &str) -> Option<String> {
    use std::str::FromStr;
    let owner = solana_sdk::pubkey::Pubkey::from_str(wallet).ok()?;
    let mint_pubkey = solana_sdk::pubkey::Pubkey::from_str(mint).ok()?;
    let token_program = solana_sdk::pubkey::Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").ok()?;
    let ata_program = solana_sdk::pubkey::Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").ok()?;
    let (ata, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint_pubkey.as_ref()],
        &ata_program,
    );
    Some(ata.to_string())
}

impl RewardTracker {
    pub fn package_signed_telemetry(
        &self,
        peer_id: &str,
        solana_wallet: &str,
        solana_ata: &str,
        epoch_id: &str,
        signing_key: &ed25519_dalek::SigningKey,
        is_rbn: bool,
        is_edge_node: bool,
        prestige_tier: u8,
    ) -> TelemetryEnvelope {
        use ed25519_dalek::Signer;
        use sha2::{Sha256, Digest};
        
        let state = self.state.read();
        let mut metrics = *self.shared_metrics.read();
        // Overlay relay_bytes and uptime from RewardTracker (more accurate for these)
        metrics[7] = state.outbound_relayed_bytes;
        metrics[8] = state.uptime_seconds;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Calculate proof hash for relay bytes verification
        let proof_hash = if !is_rbn && metrics[7] > 0 {
            let preimage = format!("RelayBytes:{}:{}", metrics[7], peer_id);
            let mut hasher = Sha256::new();
            hasher.update(preimage.as_bytes());
            hex::encode(hasher.finalize())
        } else {
            String::new()
        };

        // Fetch unique peers from database
        let mut unique_peers = Vec::new();
        if let Some(ref s) = self.storage {
            if let Ok(contacts) = s.get_all_contacts() {
                unique_peers = contacts.iter().map(|c| c.peer_id.clone()).collect();
            }
        }

        // Build message to sign: epoch_id || peer_id || solana_wallet || solana_ata ||
        // metrics[0..13] || is_rbn || is_edge_node || prestige_tier || timestamp
        let mut message = Vec::new();
        message.extend_from_slice(epoch_id.as_bytes());
        message.extend_from_slice(peer_id.as_bytes());
        message.extend_from_slice(solana_wallet.as_bytes());
        message.extend_from_slice(solana_ata.as_bytes());
        for m in &metrics {
            message.extend_from_slice(&m.to_le_bytes());
        }
        message.push(is_rbn as u8);
        message.push(is_edge_node as u8);
        message.push(prestige_tier);
        message.extend_from_slice(&timestamp.to_le_bytes());

        // Sign with Ed25519
        let signature = signing_key.sign(&message);

        TelemetryEnvelope {
            peer_id: peer_id.to_string(),
            solana_wallet: solana_wallet.to_string(),
            solana_ata: solana_ata.to_string(),
            epoch_id: epoch_id.to_string(),
            metrics,
            unique_peers,
            is_rbn,
            is_edge_node,
            prestige_tier,
            proof_hash,
            client_signature: signature.to_bytes().to_vec(),
            timestamp,
        }
    }
}
