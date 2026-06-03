# Introvert Master Plan: Sovereign P2P Architecture

## 1. Vision & Core Philosophy
Project Introvert is a privacy-first, decentralized communication platform. It eliminates central servers by utilizing Peer-to-Peer (P2P) networking, end-to-end encryption (E2EE), and a sovereign Solana-based token economy.

## 2. Technical Stack
- **Core Engine:** Rust (`libintrovert`).
- **User Interface:** Flutter (Dart).
- **FFI Bridge:** Asynchronous, non-blocking bridge using Tokio `spawn_blocking` and Dart `NativeCallable.listener`.
- **Identity:** Deterministic HKDF-SHA256 derivation from a 32-byte master seed.
- **Persistence:** SQLCipher (Encrypted SQLite) with thread-safe `Mutex` handles.
- **Networking:** libp2p (v0.56) with Kademlia DHT and WebRTC-rs data channels. Standardized on **Port 443 (HTTPS Bypass)** for global NAT traversal.

---

## 3. Execution Roadmap

### Phase 1: Foundational Hardening [COMPLETE]
Establish an unbreakable, non-blocking core foundation.
- [x] **Deterministic Identity:** Implement `NodeIdentity` using HKDF-SHA256 for domain-separated keys (P2P vs. Storage).
- [x] **Encrypted Persistence:** Initialize `SQLCipher` with high-integrity key management.
- [x] **Async FFI Bridge:** Transition to a non-blocking architecture using `tokio::task::spawn_blocking` to protect the UI thread.
- [x] **Callback Synchronization:** Implement `NativeCallable.listener` in Dart to handle Rust background task results via `Completers`.

### Phase 2: P2P Networking Restoration [COMPLETE]
Restore real-time communication capabilities using the hardened foundation.
- [x] **libp2p Swarm:** Re-introduce the libp2p (v0.56) swarm loop for Peer Discovery and Kademlia DHT routing.
- [x] **WebRTC Data Channels:** Implement `webrtc-rs` for direct browser-compatible data transport, bypassing complex hole-punching protocols.
- [x] **Signaling Pipeline:** Bridge network events (Peer Join/Leave, Incoming Data) to the Flutter UI via the `FfiCallback` model.

### Phase 3: Anchor Services (Decentralized Mailbox) [COMPLETE]
Implement store-and-forward capabilities for offline messaging.
- [x] **Zero-Knowledge Storage:** Encrypted blob storage on Anchor nodes.
- [x] **Mailbox Protocol:** Asynchronous message retrieval using P2P signaling.
- [x] **Resource Incentives:** Define the proof-of-work/storage requirements for Anchor participation.

### Phase 4: High-Fidelity VoIP & Media [COMPLETE]
Restore low-latency voice and video communication.
- [x] **WebRTC Integration:** Full implementation of `webrtc-rs` for peer-to-peer media transport.
- [x] **Opus/VP8 Codecs:** Native hardware-accelerated encoding/decoding for clear audio and video.
- [x] **Signaling Pipeline:** Encrypted SDP/ICE exchange via the libp2p signaling plane.
- [x] **Global Connectivity Hardening:** Standardized RBN nodes on **Port 443 (TCP/UDP)** to bypass carrier firewalls and improve mobile NAT traversal.
- [x] **Encrypted Receipts:** Functional Noise-encrypted 'Acknowledgement' protocol for instant delivery and read ticks across the mesh.
- [x] **Aggressive Relay Dialing:** Automatic construction of Port 443 relay paths when direct P2P is blocked, ensuring reliable cross-network delivery.
- [x] **Optimized File Transfers:** Hardened chunk-delivery engine (adaptive 128KB chunks) for stable media exchange.
- [x] **Modern UI Transformation:** Human-readable aliases, bubble-side avatars, and WhatsApp-style status ticks.
- [x] **Connection Persistence:** Added libp2p Keep-Alive (Ping) to prevent relay socket timeouts.

### Phase 5: Sovereign Group Mesh & Swarm [STABLE - PHASE 1]
Extend point-to-point sovereignty to decentralized multi-user environments and high-speed swarms.
- [x] **DHT Seeder Discovery:** Use libp2p Kademlia `start_providing` to announce file availability.
- [x] **Participating Seeding:** Every node that verifies a file automatically joins the swarm as a provider.
- [x] **Pipelined Pull Model:** Implemented 4-deep chunk pipelining to hide relay latency during cross-network transfers.
- [ ] **Gossipsub Integration:** Implement `libp2p-gossipsub` for efficient multi-point message propagation without central relays.
- [ ] **Decentralized Group E2EE:** Implement MLS (Messaging Layer Security) or TreeKEM-based group key rotation.

### Phase 6: Solana Economy & Incentives [IN PROGRESS]
Activate the `INTR` token economy to sustain the network.
- [x] **Reward Tracker:** Integrate the `economy` module to track message delivery and generate work proofs.
- [x] **Seeder Incentives:** Track data served via Sovereign Swarm to reward long-term file providers.
- [ ] **Solana Mainnet Integration:** Enable SPL-token balance checks and gasless reward payouts via Treasury ATA.

---

## 4. System Integrity Status

| Component | Status | Technology |
| :--- | :--- | :--- |
| Identity Core | STABLE | HKDF-SHA256 |
| Local Storage | STABLE | SQLCipher + Mutex |
| FFI Bridge | STABLE | Async Callbacks |
| P2P Swarm | STABLE | libp2p v0.56 |
| Sovereign Swarm | STABLE | DHT / Pull-Pipelining |
| Media / VoIP | STABLE | WebRTC / Opus |
| Token Economy | IN PROGRESS | Solana / SPL-Token |

## 5. Audit Log
- **2026-06-01:** Stable Version 11 [PASS]. Integrated cross-network Smart Hybrid Pull Model (64KB chunks @ 50ms interval, 4-deep pipelining) achieving stable 1 MB/s transfer speeds. Resolved seeder resolution bug by tracking `sender_peer_id` in manifest, preventing infinite loop chunk requests to the anchor. Corrected `is_relayed_map` checks to preserve the direct P2P pathway. Added local `cargo-zigbuild` cross-compilation for Linux RBN daemons and automated static OpenSSL linking with `bundled-sqlcipher-vendored-openssl` across platforms.
- **2026-05-29:** Deep System Audit & Sovereign Swarm Phase 1 [PASS]. Synchronized networking core with RBN Daemon. Resolved `UnexpectedEof` protocol desync and 1MB relay ceiling. Optimized cross-network fallback to 64KB chunks @ 50ms with 4-deep pipelining. Implemented DHT-based seeder discovery and auto-seeding. Strengthened Node Mode loopback and offline status reporting.
- **2026-05-28:** Sovereign Swarm Redesign [PASS]. Implemented ground-up distributed pull engine for cross-network file transfers. Multi-source chunk assembly and participating seeding verified. Hardened messaging with Mandatory E2EE. Resolved swarm loop starvation by moving all storage operations to non-blocking tokio tasks.
- **2026-05-24:** Stable Version 9. Implemented recursive signaling handling to fix media rendering (base64/JSON issue). Fixed false offline status by improving connection tracking. Optimized file transfer performance with adaptive pacing and chunking (256KB for direct P2P). Restored outgoing thumbnails by preserving state in UI.
- **2026-05-24:** Wormhole & Storage UX Polish [PASS]. Added 30s timeout to Magic Wormhole code generation to prevent Android hangs. Implemented Chat Deletion (wipe history) and ensured Peer Deletion purges all message history.
- **2026-05-24:** Messenger-Grade Hardening [PASS]. Implemented functional delivery receipts, aggressive relay-first dialing, and Port 443 P2P Push logic. Standardized native ELF RBN deployment.
- **2026-05-23:** Global Connectivity Audit [PASS]. Standardized mesh on Port 443. Implemented Hard Networking Reset for mobile handovers and fixed silent encryption deadlocks.
- **2026-05-19:** Full Deep System Audit [PASS]. Verified NAT Traversal (DCUtR), Economic Cohesion, and 100k-node scaling. Phase 2 & 3 marked complete.
- **2026-05-18:** Anchor Node Opt-in and Gasless Reward cosigning verified.
- **2026-05-04:** Phase 1 Hardening complete. Resolved Tokio starvation and FFI memory safety. Foundation verified end-to-end.
- **2026-05-01:** Initial skeleton core established.
