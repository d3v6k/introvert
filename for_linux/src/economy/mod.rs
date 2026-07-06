use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;
use crate::storage::StorageService;
use ed25519_dalek::{SigningKey, Signer};
use sha2::{Sha256, Digest};

pub mod solana;
pub mod daily_rewards;

use std::collections::HashMap;

// Re-export TelemetryEnvelope from daily_rewards
pub use daily_rewards::TelemetryEnvelope;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardProof {
    pub provider_pubkey: String, // The Relay/Node providing the service
    pub consumer_peer_id: String, // The Peer consuming the service
    pub relayed_bytes: u64,
    pub timestamp: u64,
    pub nonce: u64,              // Monotonic per-provider replay protection
    pub signature: Vec<u8>,      // Ed25519 signature over the proof
}

/// Tracks all 13 activity metrics for TelemetryEnvelope packaging.
#[derive(Debug, Clone)]
struct ActivityMetrics {
    message_sent: u64,
    message_received: u64,
    group_message_sent: u64,
    group_reaction: u64,
    file_transfer_sent: u64,
    file_transfer_recv: u64,
    call_duration_secs: u64,
    relay_bytes: u64,
    uptime_seconds: u64,
    web_focused_active_time: u64,
    sandbox_web_packet_data: u64,
    webview_media_call_hook: u64,
    unique_peer_handshakes: u64,
}

impl ActivityMetrics {
    fn new() -> Self {
        Self {
            message_sent: 0,
            message_received: 0,
            group_message_sent: 0,
            group_reaction: 0,
            file_transfer_sent: 0,
            file_transfer_recv: 0,
            call_duration_secs: 0,
            relay_bytes: 0,
            uptime_seconds: 0,
            web_focused_active_time: 0,
            sandbox_web_packet_data: 0,
            webview_media_call_hook: 0,
            unique_peer_handshakes: 0,
        }
    }

    fn to_array(&self) -> [u64; 13] {
        [
            self.message_sent,
            self.message_received,
            self.group_message_sent,
            self.group_reaction,
            self.file_transfer_sent,
            self.file_transfer_recv,
            self.call_duration_secs,
            self.relay_bytes,
            self.uptime_seconds,
            self.web_focused_active_time,
            self.sandbox_web_packet_data,
            self.webview_media_call_hook,
            self.unique_peer_handshakes,
        ]
    }
}

struct EconomyState {
    outbound_relayed_bytes: u64,
    mailbox_storage_bytes_seconds: u64,
    uptime_seconds: u64,
    pending_per_consumer: HashMap<String, u64>,
    last_claim_timestamp: u64,
    proof_nonce: u64,
    metrics: ActivityMetrics,
    unique_peers: Vec<String>,
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
                proof_nonce: 0,
                metrics: ActivityMetrics::new(),
                unique_peers: Vec::new(),
            })),
            storage,
            threshold: 1_048_576, // 1MB
            cooldown_secs: 300, // 5 minutes
            start_time: std::time::Instant::now(),
        }
    }

    // ─── Activity Recording Methods ─────────────────────────────

    /// Records a sent message event.
    pub fn record_message_sent(&self, peer_id: &str) {
        let mut state = self.state.write();
        state.metrics.message_sent += 1;
        self.add_unique_peer(&mut state, peer_id);
    }

    /// Records a received message event.
    pub fn record_message_received(&self, peer_id: &str) {
        let mut state = self.state.write();
        state.metrics.message_received += 1;
        self.add_unique_peer(&mut state, peer_id);
    }

    /// Records a group message sent event.
    pub fn record_group_message_sent(&self, peer_id: &str) {
        let mut state = self.state.write();
        state.metrics.group_message_sent += 1;
        self.add_unique_peer(&mut state, peer_id);
    }

    /// Records a group reaction event.
    pub fn record_group_reaction(&self, peer_id: &str) {
        let mut state = self.state.write();
        state.metrics.group_reaction += 1;
        self.add_unique_peer(&mut state, peer_id);
    }

    /// Records a file transfer sent event.
    pub fn record_file_transfer_sent(&self, peer_id: &str) {
        let mut state = self.state.write();
        state.metrics.file_transfer_sent += 1;
        self.add_unique_peer(&mut state, peer_id);
    }

    /// Records a file transfer received event.
    pub fn record_file_transfer_recv(&self, peer_id: &str) {
        let mut state = self.state.write();
        state.metrics.file_transfer_recv += 1;
        self.add_unique_peer(&mut state, peer_id);
    }

    /// Records call duration in seconds.
    pub fn record_call_duration(&self, seconds: u64) {
        let mut state = self.state.write();
        state.metrics.call_duration_secs += seconds;
    }

    /// Records relay bytes and updates the activity metric.
    pub fn record_relay_activity(&self, peer_id: &str, bytes: u64) {
        let mut state = self.state.write();
        state.metrics.relay_bytes += bytes;
        state.outbound_relayed_bytes += bytes;
        self.add_unique_peer(&mut state, peer_id);

        let entry = state.pending_per_consumer.entry(peer_id.to_string()).or_insert(0);
        *entry += bytes;

        if let Some(ref s) = self.storage {
            let _ = s.log_reward(bytes);
        }
    }

    // ─── TelemetryEnvelope Packaging ──────────────────────────

    /// Packages current metrics into a signed TelemetryEnvelope for the RBN.
    /// 
    /// # Arguments
    /// * `peer_id` - libp2p network identity
    /// * `solana_wallet` - Client's Solana Public Key (base58)
    /// * `solana_ata` - Pre-derived Associated Token Account
    /// * `epoch_id` - Calendar identifier (e.g., "2026_07_03")
    /// * `signing_key` - Ed25519 signing key for signature
    /// * `is_rbn` - Whether this node is an RBN
    /// * `is_edge_node` - Whether this node is an edge node
    /// * `prestige_tier` - Prestige tier (0-6)
    pub fn package_telemetry(
        &self,
        peer_id: &str,
        solana_wallet: &str,
        solana_ata: &str,
        epoch_id: &str,
        signing_key: &SigningKey,
        is_rbn: bool,
        is_edge_node: bool,
        prestige_tier: u8,
    ) -> TelemetryEnvelope {
        let state = self.state.read();
        let metrics = state.metrics.to_array();
        let unique_peers = state.unique_peers.clone();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Calculate proof hash for relay bytes verification
        let proof_hash = if !is_rbn && state.metrics.relay_bytes > 0 {
            let preimage = format!("RelayBytes:{}:{}", state.metrics.relay_bytes, peer_id);
            let mut hasher = Sha256::new();
            hasher.update(preimage.as_bytes());
            hex::encode(hasher.finalize())
        } else {
            String::new()
        };

        // Build message to sign: epoch_id + metrics + timestamp
        let mut message = Vec::new();
        message.extend_from_slice(epoch_id.as_bytes());
        for m in &metrics {
            message.extend_from_slice(&m.to_le_bytes());
        }
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

    /// Sends a TelemetryEnvelope to the RBN daemon via TCP.
    pub async fn send_telemetry_to_rbn(&self, envelope: &TelemetryEnvelope, rbn_addr: &str) -> Result<(), String> {
        let mut stream = tokio::net::TcpStream::connect(rbn_addr).await
            .map_err(|e| format!("Failed to connect to RBN: {}", e))?;
        
        use tokio::io::AsyncWriteExt;
        let payload = serde_json::to_string(envelope)
            .map_err(|e| format!("Failed to serialize envelope: {}", e))?;
        let msg = format!("{}\n", payload);
        
        stream.write_all(msg.as_bytes()).await
            .map_err(|e| format!("Failed to send to RBN: {}", e))?;
        
        Ok(())
    }

    /// Records web focused active time in seconds.
    pub fn record_web_focused_active_time(&self, seconds: u64) {
        let mut state = self.state.write();
        state.metrics.web_focused_active_time += seconds;
    }

    /// Records sandbox web packet data in bytes.
    pub fn record_sandbox_web_packet_data(&self, bytes: u64) {
        let mut state = self.state.write();
        state.metrics.sandbox_web_packet_data += bytes;
    }

    /// Records WebView media call hook duration in seconds.
    pub fn record_webview_media_call_hook(&self, seconds: u64) {
        let mut state = self.state.write();
        state.metrics.webview_media_call_hook += seconds;
    }

    fn add_unique_peer(&self, state: &mut EconomyState, peer_id: &str) {
        if !state.unique_peers.contains(&peer_id.to_string()) {
            state.unique_peers.push(peer_id.to_string());
        }
    }

    pub fn record_relay(&self, consumer_peer_id: &str, bytes: u64) {
        let mut state = self.state.write();
        state.outbound_relayed_bytes += bytes;
        state.metrics.relay_bytes += bytes;
        
        let entry = state.pending_per_consumer.entry(consumer_peer_id.to_string()).or_insert(0);
        *entry += bytes;

        self.add_unique_peer(&mut state, consumer_peer_id);

        if let Some(ref s) = self.storage {
            let _ = s.log_reward(bytes);
        }
    }

    pub fn record_message_activity(&self, _peer_id: &str) {
        // Phase III: Activity Yield
        // For now, we record a flat 1KB activity credit
        self.record_relay(_peer_id, 1024);
    }

    /// Records mailbox storage usage. Anchor nodes earn yield based on bytes * seconds.
    pub fn record_mailbox_storage(&self, bytes: u64, seconds: u64) {
        let mut state = self.state.write();
        state.mailbox_storage_bytes_seconds += bytes * seconds;
        
        if let Some(ref s) = self.storage {
            let _ = s.record_mailbox_storage(bytes * seconds);
        }
    }

    /// Updates uptime. Nodes with high uptime receive 'Availability Yield' multiplier.
    pub fn update_uptime(&self) {
        let mut state = self.state.write();
        let uptime = self.start_time.elapsed().as_secs();
        state.uptime_seconds = uptime;
        state.metrics.uptime_seconds = uptime;
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
        if state.uptime_seconds >= 79200 {
            pending_bytes = (pending_bytes as f64 * 1.5) as u64;
        }

        // Check threshold and cooldown
        if pending_bytes >= self.threshold && (now - state.last_claim_timestamp >= self.cooldown_secs) {
            // Increment nonce for replay protection
            let mut state_mut = self.state.write();
            state_mut.proof_nonce += 1;
            let nonce = state_mut.proof_nonce;
            drop(state_mut);

            let proof = RewardProof {
                provider_pubkey: provider_pubkey.to_string(),
                consumer_peer_id: consumer_peer_id.to_string(),
                relayed_bytes: pending_bytes,
                timestamp: now,
                nonce,
                signature: Vec::new(),
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

    /// Minimum INTR balance required to operate as an edge node (100,000 INTR).
    const MINIMUM_IDENTITY_LEASE: u64 = 100_000_000_000_000; // 100,000 INTR * 10^9 decimals

    pub fn is_lease_valid(&self, balance: u64) -> bool {
        balance >= Self::MINIMUM_IDENTITY_LEASE
    }

    /// DEV ONLY: Overrides the internal start time to simulate long uptimes.
    #[cfg(test)]
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
