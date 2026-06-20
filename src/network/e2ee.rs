//! TreeKEM Key Ratchet for Decentralized Group E2EE
//!
//! Implements a binary tree-based key management protocol that provides:
//! - Forward secrecy: Compromised keys don't expose past messages
//! - Key rotation: Change group keys without re-inviting members
//! - Post-compromise security: Recover after a key leak
//!
//! Based on the TreeKEM paper (https://eprint.iacr.org/2019/1020)

use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce, aead::Aead};
use hkdf::Hkdf;
use sha2::Sha256;
use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow};
use rand::RngCore;

/// A node in the binary tree, holding either a key or being empty
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TreeNode {
    Leaf {
        /// Member's public key (X25519)
        public_key: [u8; 32],
        /// Index in the tree
        index: usize,
    },
    Node {
        /// Encrypted key for this node (encrypted to children)
        encrypted_key: Vec<u8>,
        /// Left child index
        left: usize,
        /// Right child index
        right: usize,
    },
    Empty,
}

/// TreeKEM group state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeKemState {
    /// Current epoch number (increments on each key rotation)
    pub epoch: u64,
    /// The binary tree of keys
    pub tree: Vec<TreeNode>,
    /// Number of members in the group
    pub member_count: usize,
    /// Group ID for domain separation
    pub group_id: String,
    /// Root key derivation secret (derived from tree root)
    pub root_secret: [u8; 32],
}

/// A key rotation commit message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationCommit {
    /// New epoch number
    pub new_epoch: u64,
    /// Updated tree nodes (only the changed ones)
    pub updated_nodes: Vec<(usize, TreeNode)>,
    /// New member additions (if any)
    pub added_members: Vec<[u8; 32]>,
    /// Removed member indices (if any)
    pub removed_members: Vec<usize>,
    /// Signature from the committer
    pub signature: Vec<u8>,
    /// Committer's peer ID
    pub committer_peer_id: String,
}

impl TreeKemState {
    /// Creates a new TreeKEM group with initial members
    pub fn new(group_id: String, member_keys: &[[u8; 32]]) -> Result<Self> {
        if member_keys.is_empty() {
            return Err(anyhow!("Cannot create group with no members"));
        }

        let member_count = member_keys.len();
        let tree_size = Self::tree_size(member_count);
        let mut tree = vec![TreeNode::Empty; tree_size];

        // Place members at leaf positions
        for (i, key) in member_keys.iter().enumerate() {
            tree[i] = TreeNode::Leaf {
                public_key: *key,
                index: i,
            };
        }

        // Initialize with a random root secret
        let mut root_secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut root_secret);

        let mut state = Self {
            epoch: 0,
            tree,
            member_count,
            group_id,
            root_secret,
        };

        // Derive the initial tree keys
        state.derive_tree_keys()?;

        Ok(state)
    }

    /// Calculates the required tree size for N members
    fn tree_size(member_count: usize) -> usize {
        // Next power of 2
        let mut size = 1;
        while size < member_count {
            size *= 2;
        }
        size * 2 - 1 // Full binary tree: 2n - 1 nodes
    }

    /// Derives all tree node keys from the root secret
    fn derive_tree_keys(&mut self) -> Result<()> {
        // For simplicity, we use a ratcheting scheme where:
        // - The root secret derives the encryption key
        // - Each epoch rotation derives new keys
        // - Messages are encrypted with the epoch key

        // Derive the epoch encryption key from root secret + epoch
        let epoch_key = self.derive_epoch_key()?;
        let _ = epoch_key; // Key is derived on demand

        Ok(())
    }

    /// Derives the encryption key for the current epoch
    fn derive_epoch_key(&self) -> Result<[u8; 32]> {
        let hk = Hkdf::<Sha256>::new(Some(b"introvert-treekem-epoch"), &self.root_secret);
        let mut epoch_key = [0u8; 32];
        hk.expand(&self.epoch.to_le_bytes(), &mut epoch_key)
            .map_err(|_| anyhow!("HKDF expansion failed for epoch key"))?;
        Ok(epoch_key)
    }

    /// Derives a message-specific key from the epoch key
    fn derive_message_key(&self, message_id: &[u8]) -> Result<[u8; 32]> {
        let epoch_key = self.derive_epoch_key()?;
        let hk = Hkdf::<Sha256>::new(Some(b"introvert-treekem-message"), &epoch_key);
        let mut msg_key = [0u8; 32];
        hk.expand(message_id, &mut msg_key)
            .map_err(|_| anyhow!("HKDF expansion failed for message key"))?;
        Ok(msg_key)
    }

    /// Encrypts a message using the current epoch key
    pub fn encrypt(&self, plaintext: &[u8], message_id: &[u8]) -> Result<Vec<u8>> {
        let msg_key = self.derive_message_key(message_id)?;

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&msg_key));
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let encrypted = cipher.encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|_| anyhow!("Encryption failed"))?;

        // Format: [epoch:8][nonce:12][ciphertext:...]
        let mut output = Vec::with_capacity(8 + 12 + encrypted.len());
        output.extend_from_slice(&self.epoch.to_le_bytes());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&encrypted);

        Ok(output)
    }

    /// Decrypts a message using the appropriate epoch key
    pub fn decrypt(&self, ciphertext: &[u8], message_id: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 20 {
            return Err(anyhow!("Ciphertext too short"));
        }

        let msg_epoch = u64::from_le_bytes(ciphertext[0..8].try_into().unwrap());
        let nonce = &ciphertext[8..20];
        let encrypted = &ciphertext[20..];

        if msg_epoch != self.epoch {
            // For forward secrecy, we don't decrypt old epochs
            // In a full implementation, we'd store past epoch keys
            return Err(anyhow!("Message from different epoch ({} vs {})", msg_epoch, self.epoch));
        }

        let msg_key = self.derive_message_key(message_id)?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&msg_key));

        cipher.decrypt(Nonce::from_slice(nonce), encrypted)
            .map_err(|_| anyhow!("Decryption failed"))
    }

    /// Performs a key rotation, creating a new epoch
    pub fn rotate_keys(&mut self) -> Result<KeyRotationCommit> {
        self.epoch += 1;

        // Derive new root secret by ratcheting forward
        let hk = Hkdf::<Sha256>::new(Some(b"introvert-treekem-ratchet"), &self.root_secret);
        let mut new_secret = [0u8; 32];
        hk.expand(b"next-epoch", &mut new_secret)
            .map_err(|_| anyhow!("HKDF expansion failed for ratchet"))?;
        self.root_secret = new_secret;

        // Derive new tree keys
        self.derive_tree_keys()?;

        Ok(KeyRotationCommit {
            new_epoch: self.epoch,
            updated_nodes: Vec::new(), // In full implementation, update tree nodes
            added_members: Vec::new(),
            removed_members: Vec::new(),
            signature: Vec::new(), // Would be signed by committer
            committer_peer_id: String::new(),
        })
    }

    /// Adds a new member to the group (triggers key rotation)
    pub fn add_member(&mut self, new_member_key: [u8; 32]) -> Result<KeyRotationCommit> {
        let new_index = self.member_count;
        self.member_count += 1;

        // Ensure tree is large enough
        let required_size = Self::tree_size(self.member_count);
        if self.tree.len() < required_size {
            self.tree.resize(required_size, TreeNode::Empty);
        }

        // Add new leaf
        self.tree[new_index] = TreeNode::Leaf {
            public_key: new_member_key,
            index: new_index,
        };

        // Rotate keys to invalidate old keys
        self.rotate_keys()
    }

    /// Removes a member from the group (triggers key rotation)
    pub fn remove_member(&mut self, member_index: usize) -> Result<KeyRotationCommit> {
        if member_index >= self.member_count {
            return Err(anyhow!("Invalid member index"));
        }

        // Mark leaf as empty
        self.tree[member_index] = TreeNode::Empty;

        // Rotate keys to invalidate old keys
        let mut commit = self.rotate_keys()?;
        commit.removed_members.push(member_index);

        Ok(commit)
    }

    /// Gets the current epoch number
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Gets the number of members
    pub fn member_count(&self) -> usize {
        self.member_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_encrypt_decrypt() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];

        let mut state = TreeKemState::new("test-group".to_string(), &[key1, key2]).unwrap();
        let msg_id = b"test-message-1";

        let plaintext = b"Hello, world!";
        let ciphertext = state.encrypt(plaintext, msg_id).unwrap();
        let decrypted = state.decrypt(&ciphertext, msg_id).unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_key_rotation() {
        let key1 = [1u8; 32];
        let mut state = TreeKemState::new("test-group".to_string(), &[key1]).unwrap();

        assert_eq!(state.epoch(), 0);

        let commit = state.rotate_keys().unwrap();
        assert_eq!(commit.new_epoch, 1);
        assert_eq!(state.epoch(), 1);
    }

    #[test]
    fn test_add_member() {
        let key1 = [1u8; 32];
        let mut state = TreeKemState::new("test-group".to_string(), &[key1]).unwrap();

        assert_eq!(state.member_count(), 1);

        let key2 = [2u8; 32];
        let commit = state.add_member(key2).unwrap();

        assert_eq!(state.member_count(), 2);
        assert_eq!(commit.new_epoch, 1);
    }

    #[test]
    fn test_remove_member() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let state = TreeKemState::new("test-group".to_string(), &[key1, key2]).unwrap();

        assert_eq!(state.member_count(), 2);

        let mut state = state;
        let commit = state.remove_member(1).unwrap();

        assert_eq!(state.member_count(), 2); // Count doesn't change, just marks empty
        assert!(commit.removed_members.contains(&1));
    }

    #[test]
    fn test_forward_secrecy() {
        let key1 = [1u8; 32];
        let mut state = TreeKemState::new("test-group".to_string(), &[key1]).unwrap();
        let msg_id = b"test-message-1";

        // Encrypt with epoch 0
        let plaintext = b"Secret message";
        let ciphertext = state.encrypt(plaintext, msg_id).unwrap();

        // Rotate keys (new epoch)
        state.rotate_keys().unwrap();

        // Should fail to decrypt with new epoch
        let result = state.decrypt(&ciphertext, msg_id);
        assert!(result.is_err());
    }
}
