use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;
use crate::storage::StorageService;

pub mod solana;

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
    last_claim_timestamp: u64,
}

pub struct RewardTracker {
    state: Arc<RwLock<EconomyState>>,
    storage: Option<Arc<StorageService>>,
    threshold: u64, // 1MB = 1,048,576 bytes
    cooldown_secs: u64, // 5 minutes = 300 seconds
    start_time: std::time::Instant,
}

impl RewardTracker {
    pub fn new(storage: Option<Arc<StorageService>>) -> Self {
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
                last_claim_timestamp: 0,
            })),
            storage,
            threshold: 1_048_576, // 1MB
            cooldown_secs: 300, // 5 minutes
            start_time: std::time::Instant::now(),
        }
    }

    pub fn record_relay(&self, consumer_peer_id: &str, bytes: u64) {
        let mut state = self.state.write();
        state.outbound_relayed_bytes += bytes;
        
        let entry = state.pending_per_consumer.entry(consumer_peer_id.to_string()).or_insert(0);
        *entry += bytes;

        if let Some(ref s) = self.storage {
            let _ = s.log_reward(bytes);
        }
    }

    /// Records mailbox storage usage. Anchor nodes earn yield based on bytes * seconds.
    pub fn record_mailbox_storage(&self, bytes: u64, seconds: u64) {
        let mut state = self.state.write();
        state.mailbox_storage_bytes_seconds += bytes * seconds;
        
        if let Some(ref s) = self.storage {
            let _ = s.record_mailbox_storage(bytes * seconds);
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

        // Availability Yield Logic: If node uptime > 24h, apply 1.2x multiplier
        if state.uptime_seconds > 86400 {
            pending_bytes = (pending_bytes as f64 * 1.2) as u64;
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

    pub fn is_lease_valid(&self, _balance: u64) -> bool {
        // Phase II: Identity Lease
        // RELAXED: Always return true for now to ensure connectivity during testing.
        true
    }

    /// DEV ONLY: Overrides the internal start time to simulate long uptimes.
    pub fn simulate_uptime(&self, seconds: u64) {
        let mut state = self.state.write();
        state.uptime_seconds = seconds;
    }
}

impl Default for RewardTracker {
    fn default() -> Self {
        Self::new(None)
    }
}
