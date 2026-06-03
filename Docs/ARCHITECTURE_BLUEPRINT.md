# Introvert Architecture Blueprint

## 1. System Overview
Introvert is a decentralized, P2P communication system designed for high-privacy and carrier-grade reachability. It consists of a high-performance Rust core (`libintrovert`) and a modern Flutter-based user interface. The system avoids central servers entirely, relying on a distributed mesh of user nodes and Root Bootstrap Nodes (RBNs).

## 2. Component Layers

### A. Core Engine (Rust)
The engine is responsible for all heavy lifting:
- **Networking Swarm:** Managed by `libp2p`, handling discovery, routing (Kademlia), and transport (TCP/QUIC/WebRTC).
- **Security Enclave:** Handles deterministic key derivation (HKDF), E2EE Noise sessions, and SQLCipher management.
- **Protocol Logic:** Implements the messaging lifecycle, file transfer pacing, and mailbox synchronization.
- **FFI Bridge:** An asynchronous interface providing thread-safe communication with the Dart/Flutter layer.

### B. UI Layer (Flutter)
The frontend handles user interaction and presentation:
- **Main Shell:** Orchestrates the app lifecycle and background service status.
- **Chat Engine:** Manages real-time message rendering, media previews, and transfer progress.
- **Native Interface:** Uses `dart:ffi` with `NativeCallable` to listen for Rust events without blocking the UI thread.
- **State Management:** Reactive updates based on event codes dispatched from the Rust core.

### C. Persistent Storage (SQLCipher)
All data is stored in an encrypted SQLite database:
- **Messages:** Thread-indexed history with functional status ticks.
- **Contacts:** Verified identities with permanent public keys.
- **Mailbox:** A zero-knowledge store-and-forward buffer for offline peers.
- **Introvert Drive:** Metadata for personal files stored locally and backed up across the mesh swarm.
- **Mesh Chunks:** A 1GB communal storage commitment (per node) for torrent-like file distribution.
- **Session Cache:** Persisted Noise handshake states to minimize re-handshakes.

### D. Sovereign Group Mesh (Phase 5)
Architecture for decentralized multi-user communication:
- **Propagation:** Uses `libp2p-gossipsub` to broadcast messages across unique topics (hash of Group ID).
- **Decentralized Admin Model:** Only the group Creator or appointed Admins can perform control actions (Add/Remove). These actions are **cryptographically signed** using the admin's Ed25519 key.
- **Group Privacy:** E2EE using AES-256-GCM with a shared master secret. 
- **Mesh Discovery:** Supports "Join by Code" via Kademlia DHT. Manifests are encrypted with a human passphrase and stored on Anchor nodes.

### E. Sovereign Swarm & Mesh Storage (Latest)
Decentralized file storage and retrieval strategy:
- **Hybrid P2P Engine:** 
    - **Direct/WebRTC:** Utilizes a high-speed sequential **PUSH** model (256KB chunks @ 20ms) to maximize local throughput.
    - **Relayed/Swarm:** Automatically falls back to an ultra-stable **Redundancy-Filtered PULL** model for cross-network reliability.
- **Relay Performance Profile:**
    - **Chunk Size:** 16KB (MTU Safe) for maximum packet delivery success on mobile networks.
    - **Pacing:** 250ms sender delay to prevent relay circuit saturation.
    - **Sliding Window:** 2 chunks in-flight to maintain a lean, non-congestive flow.
- **DHT-Based Discovery:** Uses Kademlia `start_providing` to announce file availability based on SHA-256 hashes. Receivers query the mesh to find all available seeders.
- **Distributed Seeding & Mandates:** 
    - **1-to-1 Mode:** Seeding is strictly limited to the sender-receiver pair and stops upon receipt confirmation to preserve individual privacy.
    - **Group Mode:** Every group member that verifies a file via SHA-256 automatically becomes a provider on the DHT, creating a resilient group-wide mesh.
- **Redundancy Filtered Pull:**
    - The engine tracks pending chunk requests in RAM.
    - During network transitions, older redundant requests are purged to prevent "Thundering Herd" congestion upon reconnection.
- **Auto-Resumption:** The pull-based engine proactively tracks missing chunks and handles seeder timeouts via an 8-second watchdog, allowing transfers to self-heal and resume gracefully.
- **Mailbox Isolation:** Large data payloads are strictly RAM-buffered and prohibited from entering the persistent anchor mailbox, ensuring the signaling path remains clear.
- **Self-Cleaning Mesh:** In Group Mode, once all members confirm receipt and verification, the mesh triggers a `MeshCleanup` signal, purging chunks from participating anchors based on a 1GB LRU quota.


## 3. Data Flow

1.  **Identity Derivation:** Seed (32 bytes) -> HKDF-SHA256 -> {libp2p Key, X25519 Static Key, Solana Wallet, Storage Key}.
2.  **Peer Discovery:** mDNS (Local) + Kademlia DHT (Global) -> Verified Contact Lookup -> Auto-Dial.
3.  **Messaging:** UI Input -> FFI Send -> Rust Encryption -> libp2p Signaling -> Remote Node.
4.  **Event Loop:** Rust Swarm Event -> Event Type Mapping -> Global Dispatch -> Dart Stream -> UI Update.

## 4. Logical Modules

- `src/identity.rs`: The root of sovereignty.
- `src/network/mod.rs`: The heartbeat of the system.
- `src/storage.rs`: The source of truth.
- `src/media/mod.rs`: Low-latency VoIP and streaming.
- `src/economy/mod.rs`: Incentive tracking and Solana integration.
