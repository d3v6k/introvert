use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use crate::storage::StorageService;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandleClaim {
    pub handle: String,
    pub peer_id: String,
    pub timestamp: i64,
    pub pow_nonce: u64,
    pub signatures: Vec<String>, // RBN signatures: "rbn_peer_id:sig_hex"
}

pub struct RegistryManager {
    storage: Arc<StorageService>,
    difficulty: usize, // number of leading zeros in hex
}

impl RegistryManager {
    pub fn new(storage: Arc<StorageService>) -> Self {
        Self {
            storage,
            difficulty: 6, // "000000" prefix (24 bits, ~16M iterations avg)
        }
    }

    /// Generates a Proof-of-Work nonce for a handle claim.
    pub fn generate_pow(&self, handle: &str, peer_id: &str, timestamp: i64) -> u64 {
        let mut nonce = 0;
        let prefix = "0".repeat(self.difficulty);
        
        loop {
            let hash = self.calculate_hash(handle, peer_id, timestamp, nonce);
            if hash.starts_with(&prefix) {
                return nonce;
            }
            nonce += 1;
        }
    }

    /// Verifies if a claim's PoW is valid.
    pub fn verify_pow(&self, claim: &HandleClaim) -> bool {
        // Validate timestamp is within +/- 5 minutes of current time
        let now = chrono::Utc::now().timestamp();
        let staleness = (now - claim.timestamp).abs();
        if staleness > 300 {
            return false; // Claim is stale or from the future
        }
        
        let prefix = "0".repeat(self.difficulty);
        let hash = self.calculate_hash(&claim.handle, &claim.peer_id, claim.timestamp, claim.pow_nonce);
        hash.starts_with(&prefix)
    }

    fn calculate_hash(&self, handle: &str, peer_id: &str, timestamp: i64, nonce: u64) -> String {
        let mut hasher = Sha256::new();
        hasher.update(handle.as_bytes());
        hasher.update(peer_id.as_bytes());
        hasher.update(timestamp.to_be_bytes());
        hasher.update(nonce.to_be_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Checks if a handle is available or already owned by the same peer.
    pub fn is_handle_available(&self, handle: &str, peer_id: &str) -> bool {
        match self.storage.get_handle_claim(handle) {
            Ok(Some((existing_peer, _, _, verified))) => {
                // If verified and owned by someone else, it's NOT available.
                // If verified and owned by ME, it's available (idempotent).
                // If NOT verified, we treat it as potentially available (conflict resolved by RBNs).
                if verified && existing_peer != peer_id {
                    false
                } else {
                    true
                }
            }
            Ok(None) => true,
            Err(_) => false,
        }
    }

    /// Marks a handle as verified in the local registry.
    pub fn verify_claim(&self, claim: &HandleClaim) -> anyhow::Result<()> {
        let sigs_json = serde_json::to_string(&claim.signatures)?;
        self.storage.insert_handle_claim(&claim.handle, &claim.peer_id, claim.timestamp, &sigs_json, true)?;
        Ok(())
    }
}
