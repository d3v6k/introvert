use libp2p::identity::{ed25519, Keypair, PeerId};
use anyhow::Result;
use hkdf::Hkdf;
use sha2::Sha256;
use serde::{Serialize, Deserialize};

/// The canonical identity package exchanged during the Magic Wormhole handshake.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SovereignIdentity {
    pub peer_id: String,        // Permanent libp2p PeerId
    pub p2p_pubkey: Vec<u8>,    // Permanent Ed25519 public key (Protobuf encoded)
    pub static_key: [u8; 32],   // Permanent X25519 public key for Noise IK
    pub solana_address: String, // User's reward destination
    pub global_name: Option<String>,
    pub local_alias: Option<String>,
    pub avatar_base64: Option<String>,
    pub is_anchor_capable: bool, // Node supports mailbox storage
    pub retention_seconds: u32,
    pub handle: Option<String>,
}

pub struct NodeIdentity {
    pub keypair: Keypair,
    pub peer_id: PeerId,
    pub seed: [u8; 32], // Stored for secure domain-separated derivation
}

impl NodeIdentity {
    /// Derives the libp2p identity (Keypair/PeerId) from a 32-byte master seed using HKDF-SHA256.
    pub fn from_seed(seed: [u8; 32]) -> Result<Self> {
        let hk = Hkdf::<Sha256>::new(None, &seed);
        let mut okm = [0u8; 32];
        
        // Expand to derive p2p secret
        hk.expand(b"introvert_p2p_identity", &mut okm)
            .map_err(|_| anyhow::anyhow!("HKDF expansion failed for identity"))?;

        let secret = ed25519::SecretKey::try_from_bytes(&mut okm)?;
        let ed_kp = ed25519::Keypair::from(secret);
        let keypair = Keypair::from(ed_kp);
        let peer_id = PeerId::from_public_key(&keypair.public());

        Ok(Self { keypair, peer_id, seed })
    }

    /// Derives a distinct 32-byte storage key for SQLCipher from the master seed.
    pub fn derive_storage_key(seed: [u8; 32]) -> Result<[u8; 32]> {
        let hk = Hkdf::<Sha256>::new(None, &seed);
        let mut storage_key = [0u8; 32];
        
        hk.expand(b"introvert_storage_key", &mut storage_key)
            .map_err(|_| anyhow::anyhow!("HKDF expansion failed for storage key"))?;

        Ok(storage_key)
    }

    /// Derives a mathematically distinct ed25519 keypair for the Solana wallet layer.
    pub fn derive_solana_keypair(seed: [u8; 32]) -> Result<ed25519_dalek::SigningKey> {
        let hk = Hkdf::<Sha256>::new(None, &seed);
        let mut okm = [0u8; 32];
        
        // Fix: Use dedicated salt string to break the link to libp2p node ID
        hk.expand(b"introvert_solana_wallet", &mut okm)
            .map_err(|_| anyhow::anyhow!("HKDF expansion failed for solana key"))?;

        Ok(ed25519_dalek::SigningKey::from_bytes(&okm))
    }

    /// Derives a mathematically distinct X25519 static secret for E2EE Noise sessions.
    pub fn derive_e2ee_key(seed: [u8; 32]) -> Result<x25519_dalek::StaticSecret> {
        let hk = Hkdf::<Sha256>::new(None, &seed);
        let mut okm = [0u8; 32];
        
        hk.expand(b"introvert_e2ee_identity", &mut okm)
            .map_err(|_| anyhow::anyhow!("HKDF expansion failed for E2EE key"))?;

        Ok(x25519_dalek::StaticSecret::from(okm))
    }

    /// Derives a 32-byte key for encrypting ephemeral session blobs before storage.
    pub fn derive_session_encryption_key(seed: [u8; 32]) -> Result<[u8; 32]> {
        let hk = Hkdf::<Sha256>::new(None, &seed);
        let mut okm = [0u8; 32];
        
        hk.expand(b"introvert_session_encryption", &mut okm)
            .map_err(|_| anyhow::anyhow!("HKDF expansion failed for session encryption key"))?;

        Ok(okm)
    }
}
