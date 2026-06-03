# Introvert Master Plan: Sovereign P2P Architecture

## 1. Vision & Core Philosophy
Project Introvert is a privacy-first, decentralized communication platform. It eliminates central servers by utilizing Peer-to-Peer (P2P) networking, end-to-end encryption (E2EE), and a sovereign Solana-based token economy.

## 2. Technical Stack
- **Core Engine:** Rust (`libintrovert`).
- **User Interface:** Flutter (Dart).
- **FFI Bridge:** Asynchronous, non-blocking bridge using Tokio `spawn_blocking` and Dart `NativeCallable.listener`.
- **Identity:** Deterministic HKDF-SHA256 derivation from a 32-byte master seed.
- **Persistence:** SQLCipher (Encrypted SQLite) with thread-safe `Mutex` handles.
- **Networking:** libp2p (v0.53) with Kademlia DHT and WebRTC-rs data channels.

---

## 3. Execution Roadmap

### Phase 1: Foundational Hardening [COMPLETE]
Establish an unbreakable, non-blocking core foundation.
- [x] **Deterministic Identity:** Implement `NodeIdentity` using HKDF-SHA256 for domain-separated keys (P2P vs. Storage).
- [x] **Encrypted Persistence:** Initialize `SQLCipher` with high-integrity key management.
- [x] **Async FFI Bridge:** Transition to a non-blocking architecture using `tokio::task::spawn_blocking` to protect the UI thread.
- [x] **Callback Synchronization:** Implement `NativeCallable.listener` in Dart to handle Rust background task results via `Completers`.

### Phase 2: P2P Networking Restoration [IN PROGRESS]
Restore real-time communication capabilities using the hardened foundation.
- [ ] **libp2p Swarm:** Re-introduce the libp2p (v0.53) swarm loop for Peer Discovery and Kademlia DHT routing.
- [ ] **WebRTC Data Channels:** Implement `webrtc-rs` for direct browser-compatible data transport, bypassing complex hole-punching protocols.
- [ ] **Signaling Pipeline:** Bridge network events (Peer Join/Leave, Incoming Data) to the Flutter UI via the `FfiCallback` model.

### Phase 3: Anchor Services (Decentralized Mailbox) [PLANNED]
Implement store-and-forward capabilities for offline messaging.
- [ ] **Zero-Knowledge Storage:** Encrypted blob storage on Anchor nodes.
- [ ] **Mailbox Protocol:** Asynchronous message retrieval using P2P signaling.
- [ ] **Resource Incentives:** Define the proof-of-work/storage requirements for Anchor participation.

### Phase 4: Solana Economy & Incentives [PLANNED]
Activate the `INTR` token economy to sustain the network.
- [ ] **Reward Tracker:** Integrate the `economy` module to track message delivery and generate work proofs.
- [ ] **Solana Mainnet Integration:** Enable SPL-token balance checks and gasless reward payouts via Treasury ATA.
- [ ] **Gasless Relayers:** Implement transaction delegation for zero-friction user onboarding.

---

## 4. System Integrity Status

| Component | Status | Technology |
| :--- | :--- | :--- |
| Identity Core | STABLE | HKDF-SHA256 |
| Local Storage | STABLE | SQLCipher + Mutex |
| FFI Bridge | STABLE | Async Callbacks |
| P2P Swarm | IN PROGRESS | libp2p v0.53 |
| Media / VoIP | PLANNED | WebRTC / Opus |
| Token Economy | PLANNED | Solana / SPL-Token |

## 5. Audit Log
- **2026-05-04:** Phase 1 Hardening complete. Resolved Tokio starvation and FFI memory safety. Foundation verified end-to-end.
- **2026-05-01:** Initial skeleton core established.
