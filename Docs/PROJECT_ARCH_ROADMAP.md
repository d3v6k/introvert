# Introvert Project Architectural Roadmap

**Current Status:** Phase 2 (Signaling Complete, Data Transport Integration In Progress)
**Execution Phase:** Phase 3: WebRTC Data Plane

## I. Built & Stabilized (100% Verified)

The following components have been rebuilt from the ground up, hardened for production stability, and verified via end-to-end integration tests.

### 1. Identity & Storage (Phase 1)
- **Sovereign Identity:** Deterministic key derivation via HKDF-SHA256 from a single 32-byte master seed, providing robust domain separation.
- **Encrypted Persistence:** Thread-safe SQLCipher implementation using `parking_lot::Mutex` and `tokio::task::spawn_blocking` for zero-jank message logging.

### 2. Bidirectional Signaling Plane (Phase 2)
- **libp2p Signaling Plane:** Lightweight libp2p v0.53 swarm utilizing Kademlia DHT for peer discovery and Request-Response for JSON-based signaling exchange.
- **Outbound Signaling:** Exposing the `introvert_network_send_message` entry point to dispatch signaling strings (SDP/ICE) from Dart.
- **Bidirectional Event Flow:** Seamlessly routing raw SDP and ICE metadata via libp2p Request-Response protocols without any redundant hole-punching layers.
- **Background Event Loop:** Persistent Tokio background task for the libp2p swarm, ensuring networking logic never blocks the engine or UI.

### 3. Asynchronous FFI Bridge
- **End-to-End Memory Safety:** 100% verified ownership hand-off using `CString::into_raw` and explicit `introvert_free_string` cleanup in Dart.
- **Async Pattern:** Standardized usage of `NativeCallable.listener` and Dart `Completers` for non-blocking communication between Flutter and Rust.

---

## II. In-Progress Development (Phase 3)

We are currently bridging the signaling plane to the high-performance data transport layer.

### 1. WebRTC Data Plane (`src/network/webrtc_manager.rs`)
- **Framework:** Integrating the standalone `webrtc` crate to handle raw user-space P2P streams.
- **Transport:** Establishing direct, serverless data tunnels for real-time messaging, converting signaling metadata into user-space data channels for chat.

---

## III. Planned (Phase 3+)

### Phase 3: Anchor Mailbox Services
- **Goal:** Decentralized store-and-forward mailbox nodes for asynchronous messaging.
- **Security:** Zero-knowledge encrypted blob storage on sovereign Anchor nodes.

### Phase 4: Solana Reward Economy
- **Goal:** Activating the `RewardTracker` and connecting it directly to network events for the sovereign `INTR` token economy.
- **Mechanism:** Tracking work-proofs for message delivery and enabling gasless rewards (INTR token transfers) on Solana Mainnet from the Treasury ATA.

---

## IV. Technical Integrity Log
- **2026-05-04:** Phase 1 (Foundational Hardening) successfully concluded.
- **2026-05-04:** Phase 2 (Signaling Restoration) established. Bidirectional P2P signaling loop verified.
- **2026-05-04:** Phase 2 Conclusion: Outbound signaling and memory-safe FFI bridge validated.
- **2026-05-04:** Commencing Phase 3 (WebRTC Data Plane) integration.
