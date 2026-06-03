# Security & Encryption Enclave

## 1. Zero-Knowledge Foundation
Introvert is built on the principle that no intermediary, including RBN nodes, should ever be able to read user data.

## 2. Deterministic Identity Derivation
Identity is not stored as a single file; it is derived mathematically from a **32-byte Master Seed**.

### HKDF-SHA256 Derivation Paths
Using distinct salt strings to ensure domain separation:
- **libp2p Identity:** `b"introvert_p2p_identity"` -> Ed25519 Keypair (PeerId + **p2p_pubkey**).
- **E2EE Identity:** `b"introvert_e2ee_identity"` -> X25519 Static Secret (Noise IK).
- **Storage Key:** `b"introvert_storage_key"` -> 256-bit SQLCipher Key.
- **Solana Wallet:** `b"introvert_solana_wallet"` -> Ed25519 Signing Key.
- **Session Encryption:** `b"introvert_session_encryption"` -> Key for persisting session blobs.

## 3. Mandatory End-to-End Encryption
Introvert enforces **Strict E2EE** for all sensitive payloads. Plaintext fallback is architecturally prohibited for Chat, Group actions, and File Metadata.

### A. Point-to-Point (Noise IK)
Standard E2EE for 1-on-1 chats. If a Noise session is not active, payloads are automatically buffered in the `pending_messages` enclave while a secure handshake is initiated. Data never leaves the node in plaintext.

### B. Sovereign Group Mesh (AES-GCM)
Multi-user encryption using a shared Group Master Secret.
- **Verification:** All group control actions (Add/Remove) are signed with the admin's Ed25519 `p2p_pubkey` and ordered via **Lamport Clocks** to prevent malicious mesh injection or replay attacks.
- **Key Distribution:** New keys are individually wrapped (X25519 DH) for each member when a group is formed or rotated.
- **Forward Secrecy:** Evicting a member triggers immediate group secret rotation and re-distribution.

## 4. Persistent Storage Security (SQLCipher)
Local data is protected using SQLCipher (SQLite with AES-256 encryption).
- **Thread Safety:** All database access is wrapped in a thread-safe `Mutex` or `RwLock`.
- **Integrity:** Non-blocking storage updates prevent swarm loop starvation during high-frequency writes.
- **Convergence:** Lamport Clock logic ensures that history stays consistent across multiple devices without a central server.

## 5. Zero-Knowledge Mailbox
When a peer is offline, encrypted messages are stored on an **Anchor Node**.
- **Indexing:** Messages are indexed using a truncated hash of the recipient's PeerId.
- **Privacy:** The Anchor node knows *who* has a message waiting but cannot read the content, as it remains encrypted with the recipient's Noise session keys.
- **Authentication:** Only the node possessing the private key corresponding to the PeerId can drain the mailbox.

## 6. FFI Memory Safety
- **Binary Buffers:** Memory transferred between Rust and Dart is explicitly managed. Rust allocates memory using `libc::malloc`, and the Dart layer is responsible for calling `introvert_free_binary` once the data is copied.
- **Opaque Pointers:** The `Engine` state is stored as a global `Lazy<RwLock<Option<Arc<Engine>>>>`, ensuring safe access across FFI boundaries.
