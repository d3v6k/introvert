use serde::{Serialize, Deserialize};
use crate::network::{GroupRole, GroupMemberMetadata, SignedGroupAction, GroupAction};
use crate::network::e2ee::TreeKemState;
use anyhow::{Result, anyhow};
use libp2p::identity::PeerId;

pub struct GroupManager;

/// Extended group state with TreeKEM E2EE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupE2eeState {
    /// The TreeKEM state for key management
    pub tree_kem: TreeKemState,
    /// Whether TreeKEM is enabled for this group
    pub enabled: bool,
}

impl GroupManager {
    /// Signs a group action using the node's private key.
    /// This provides non-repudiation and proof of authority in the mesh.
    pub fn sign_action(
        group_id: String,
        action: GroupAction,
        keypair: &libp2p::identity::Keypair,
    ) -> Result<SignedGroupAction> {
        let payload = serde_json::to_vec(&(&group_id, &action))?;
        let signature = keypair.sign(&payload).map_err(|e| anyhow!("Sign failed: {}", e))?;
        
        Ok(SignedGroupAction {
            group_id,
            action,
            signer_peer_id: PeerId::from(keypair.public()).to_string(),
            signature,
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
        })
    }

    /// Verifies that a group action was signed by a legitimate admin.
    pub fn verify_action(
        signed: &SignedGroupAction,
        members: &[GroupMemberMetadata],
    ) -> Result<bool> {
        // 1. Verify Member exists and has appropriate role
        let member = members.iter().find(|m| m.peer_id == signed.signer_peer_id)
            .ok_or_else(|| anyhow!("Sender {} is not a group member", signed.signer_peer_id))?;
            
        // Creator-only action validation
        if matches!(signed.action, GroupAction::DeleteGroup) && member.role != GroupRole::Creator {
            return Err(anyhow!("Permission Denied: Only the main creator can delete the group"));
        }

        // Admins and Creators can do anything. Members can only send messages, remove themselves, and manage their own messages/reactions.
        if member.role == GroupRole::Member {
            match &signed.action {
                GroupAction::Message { .. } => {}, // Allowed
                GroupAction::Reaction { .. } => {}, // Allowed
                GroupAction::EditMessage { .. } => {}, // Allowed (verifying ownership is handled later/implicitly)
                GroupAction::DeleteMessage { .. } => {}, // Allowed (verifying ownership is handled later/implicitly)
                GroupAction::RemoveMember { peer_id } if peer_id == &signed.signer_peer_id => {}, // Allowed to self-remove
                _ => return Err(anyhow!("Permission Denied: Only admins can perform control actions")),
            }
        }

        // 2. Verify Signature
        let public_key = libp2p::identity::PublicKey::try_decode_protobuf(&member.pubkey)
            .map_err(|_| anyhow!("Could not decode public key for member {}", member.peer_id))?;
        
        let payload = serde_json::to_vec(&(&signed.group_id, &signed.action))?;
        if public_key.verify(&payload, &signed.signature) {
            Ok(true)
        } else {
            Err(anyhow!("Invalid cryptographic signature from peer {}", signed.signer_peer_id))
        }
    }

    /// Wraps (encrypts) the group symmetric secret for a recipient static public key using ECDH and AES-GCM.
    pub fn wrap_group_secret(group_secret: &[u8; 32], recipient_static_key: &[u8; 32]) -> Result<Vec<u8>> {
        let ephemeral_secret = x25519_dalek::EphemeralSecret::random_from_rng(&mut rand::thread_rng());
        let ephemeral_public = x25519_dalek::PublicKey::from(&ephemeral_secret);
        let recipient_public = x25519_dalek::PublicKey::from(*recipient_static_key);
        let shared_secret = ephemeral_secret.diffie_hellman(&recipient_public);
        
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(None, shared_secret.as_bytes());
        let mut aes_key = [0u8; 32];
        hk.expand(b"introvert_group_secret_wrap", &mut aes_key)
            .map_err(|e| anyhow!("HKDF failed: {:?}", e))?;
            
        use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
        use rand::RngCore;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&aes_key));
        let encrypted = cipher.encrypt(Nonce::from_slice(&nonce_bytes), group_secret.as_slice())
            .map_err(|e| anyhow!("Encryption failed: {:?}", e))?;
            
        let mut wrapped = Vec::new();
        wrapped.extend_from_slice(ephemeral_public.as_bytes());
        wrapped.extend_from_slice(&nonce_bytes);
        wrapped.extend_from_slice(&encrypted);
        Ok(wrapped)
    }

    /// Unwraps (decrypts) a group symmetric secret using our local static private key.
    pub fn unwrap_group_secret(wrapped: &[u8], local_static_secret: &x25519_dalek::StaticSecret) -> Result<[u8; 32]> {
        if wrapped.len() < 44 {
            return Err(anyhow!("Wrapped secret too short"));
        }
        let mut ephemeral_bytes = [0u8; 32];
        ephemeral_bytes.copy_from_slice(&wrapped[0..32]);
        let ephemeral_public = x25519_dalek::PublicKey::from(ephemeral_bytes);
        
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&wrapped[32..44]);
        
        let shared_secret = local_static_secret.diffie_hellman(&ephemeral_public);
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(None, shared_secret.as_bytes());
        let mut aes_key = [0u8; 32];
        hk.expand(b"introvert_group_secret_wrap", &mut aes_key)
            .map_err(|e| anyhow!("HKDF failed: {:?}", e))?;
            
        use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit, aead::Aead};
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&aes_key));
        let decrypted = cipher.decrypt(Nonce::from_slice(&nonce_bytes), &wrapped[44..])
            .map_err(|e| anyhow!("Decryption failed: {:?}", e))?;
            
        let mut secret = [0u8; 32];
        if decrypted.len() != 32 {
            return Err(anyhow!("Decrypted secret is not 32 bytes"));
        }
        secret.copy_from_slice(&decrypted);
        Ok(secret)
    }

    /// Creates a new TreeKEM-enabled group
    pub fn create_e2ee_group(
        group_id: String,
        member_keys: &[[u8; 32]],
    ) -> Result<GroupE2eeState> {
        let tree_kem = TreeKemState::new(group_id, member_keys)?;
        Ok(GroupE2eeState {
            tree_kem,
            enabled: true,
        })
    }

    /// Encrypts a group message using TreeKEM
    pub fn encrypt_group_message(
        e2ee_state: &GroupE2eeState,
        plaintext: &[u8],
        message_id: &[u8],
    ) -> Result<Vec<u8>> {
        if !e2ee_state.enabled {
            return Err(anyhow!("TreeKEM not enabled for this group"));
        }
        e2ee_state.tree_kem.encrypt(plaintext, message_id)
    }

    /// Decrypts a group message using TreeKEM
    pub fn decrypt_group_message(
        e2ee_state: &GroupE2eeState,
        ciphertext: &[u8],
        message_id: &[u8],
    ) -> Result<Vec<u8>> {
        if !e2ee_state.enabled {
            return Err(anyhow!("TreeKEM not enabled for this group"));
        }
        e2ee_state.tree_kem.decrypt(ciphertext, message_id)
    }

    /// Performs key rotation for the group
    pub fn rotate_group_keys(
        e2ee_state: &mut GroupE2eeState,
    ) -> Result<crate::network::e2ee::KeyRotationCommit> {
        if !e2ee_state.enabled {
            return Err(anyhow!("TreeKEM not enabled for this group"));
        }
        e2ee_state.tree_kem.rotate_keys()
    }

    /// Adds a new member to the TreeKEM group
    pub fn add_member_e2ee(
        e2ee_state: &mut GroupE2eeState,
        member_key: [u8; 32],
    ) -> Result<crate::network::e2ee::KeyRotationCommit> {
        if !e2ee_state.enabled {
            return Err(anyhow!("TreeKEM not enabled for this group"));
        }
        e2ee_state.tree_kem.add_member(member_key)
    }

    /// Removes a member from the TreeKEM group
    pub fn remove_member_e2ee(
        e2ee_state: &mut GroupE2eeState,
        member_index: usize,
    ) -> Result<crate::network::e2ee::KeyRotationCommit> {
        if !e2ee_state.enabled {
            return Err(anyhow!("TreeKEM not enabled for this group"));
        }
        e2ee_state.tree_kem.remove_member(member_index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupState {
    pub metadata: GroupMetadataInternal,
    pub shared_secret: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMetadataInternal {
    pub group_id: String,
    pub name: String,
    pub members: Vec<GroupMemberMetadata>,
}
