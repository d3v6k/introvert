# Introvert: Open-Source, Sovereign P2P Mesh Messenger

**Version:** 0.35.2 | **Status:** Beta | **Last Updated:** 2026-07-21

Introvert is a privacy-focused, decentralized communication system that eliminates central servers entirely. Operating via a crowdsourced, self-healing Peer-to-Peer (P2P) mesh network, it ensures zero-knowledge, autonomous, and censorship-resistant utility for global users.

---

## 🌟 Core Value Pillars

### 1. 📂 Open Source & Peer-to-Peer (P2P)
*   **100% Transparent Codebase:** The entire core (Rust) and client UI (Flutter) are open-source, allowing auditability and eliminating hidden backdoors.
*   **Serverless Network Swarm:** Utilizes standard libp2p configurations to route events directly between edge peers, avoiding centralized hops or single points of failure.

### 2. 🔐 End-to-End Encryption (E2EE)
*   **Cryptographic Sovereignty:** All messaging, file transfers, and group chats are encrypted locally before transmission using the Noise Protocol framework (`Noise_IK_25519_ChaChaPoly_BLAKE2s`) and AES-256-GCM. Intermediary nodes relay ciphertext without any visibility into plaintext or keys.

### 3. 🌱 Eco-Friendly & Green Credentials (Zero Data Centers)
*   **Idle Consumer Hardware:** Introvert replaces carbon-heavy, energy-guzzling centralized datacenters with existing, idle consumer devices.
*   **Carbon-Neutral Consensus:** Built on the Solana blockchain, which operates via Proof-of-History (PoH) consensus—consuming negligible energy compared to Proof-of-Work networks.

### 4. 🚫 Zero Spam
*   **Economic Gating:** Built-in rate limits, balance-based gating, and local anti-spam filters ensure bad actors cannot spam the network without incurring costs, achieving structural Sybil resistance.

### 5. ⚡ Bleeding-Edge Tech
*   **Intro-Claw Maintenance Engine:** A local automation suite that manages database optimization, storage compaction, secure garbage collection, and local performance tuning.
*   **Intro Codec:** A custom binary signaling protocol (`/introvert/signaling/2.0.0`) that eliminates Base64 overhead for `FileChunk` data, reducing wire bandwidth consumption by 25%.

### 6. 🎁 User Rewards
*   **Dynamic Participation Pool:** Earn $INTR tokens daily based on your social contribution (messages, reactions, calls) and infrastructure contributions (bytes relayed, node uptime) via a pool-clearing rewards engine.

### 7. 🖥️ Community-Powered RBNs
*   **Decentralized Directory:** Root Bootstrap Nodes (RBNs) are run entirely by community operators, receiving emissions for coordinating network discovery and relaying data.

---

## 🛠️ Tech Stack
*   **Core Engine:** Rust (libp2p, SQLite/SQLCipher, WebRTC, Solana SDK)
*   **User Interface:** Flutter (Dart) with a high-performance native FFI bridge
*   **Governance:** Squads V4 (3-of-5) Multisig for program upgrade authority
*   **Staking PDA:** Unified keyless escrow registry on Solana Mainnet

---

## 🛡️ Media Safety Module

Introvert incorporates an **on-device Media Safety Module** that inspects all media files before they enter the mesh network. Content is validated locally on the sender's device — **no data is transmitted to external servers** for analysis.

### Detection Layers

| Layer | Technology | What It Catches |
|-------|-----------|-----------------|
| **Perceptual Hash (PDQ)** | Custom Rust implementation — 64x64 grayscale resize, 8x8 block DCT, median-threshold to 256-bit hash | Known CSAM and illegal imagery via perceptual hash matching (hamming distance ≤ 10) |
| **Executable Masquerade Detection** | Magic byte analysis (PE `MZ`, ELF `7F 45 4C 46`, Mach-O `FEEDFACE/FACF`) | Malware disguised as images/videos — blocks `.exe`/`.elf`/`.dylib` files with media extensions |
| **Shannon Entropy Analysis** | Byte-frequency entropy calculation (threshold: 7.95 bits/byte) | Steganography and encrypted payloads in image files (passive logging, no hard block) |
| **TFLite Classifier** | TensorFlow Lite on-device inference (224x224 RGB tensor) | Explicit content, violent/gore, and malware payload classification (model integration pending) |

### How It Works

1. User selects media to send
2. `UploadController` intercepts the file before encryption
3. Rust `inspect_media()` computes PDQ hash, checks blocklist, validates magic bytes
4. If `knownViolationBlocked` → file is rejected locally, never enters the mesh
5. If `approved` → file proceeds through AES-256-GCM encryption and P2P transmission

### Privacy

- All analysis runs **entirely on-device** — no cloud APIs, no external lookups
- PDQ hashes are computed locally and compared against a local blocklist
- No content, hashes, or metadata are transmitted to any server
- The blocklist is bundled with the app and can be updated via app releases

### Libraries & Modules

| Component | Implementation |
|-----------|---------------|
| PDQ Perceptual Hash | Custom Rust (`src/safety.rs`) — `image` crate for decode/resize, manual DCT |
| Entropy Analysis | Custom Rust — Shannon entropy on raw bytes |
| Executable Detection | Custom Rust — magic byte header inspection |
| TFLite Classifier | Dart (`tflite_safety_classifier.dart`) — `tflite_flutter` (model loading pending) |
| Upload Gate | Dart `UploadController` — integrated into all 6 send call sites |
| FFI Bridge | Dart `native_hash_bridge.dart` → Rust `inspect_media()` |

---

## 📥 Download

| Platform | Link |
|----------|------|
| Android | [app-release.apk](https://github.com/d3v6k/introvert/releases/download/v0.35.2/app-release.apk) |
| macOS | [Introvert-macOS.dmg](https://github.com/d3v6k/introvert/releases/download/v0.35.2/Introvert-macOS.dmg) |

For details on configuration and build steps, refer to [Docs/README.md](./Docs/README.md).
