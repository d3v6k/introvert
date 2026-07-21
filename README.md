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

## 📥 Download

| Platform | Link |
|----------|------|
| Android | [app-release.apk](https://github.com/d3v6k/introvert/releases/download/v0.35.2/app-release.apk) |
| macOS | [Introvert-macOS.dmg](https://github.com/d3v6k/introvert/releases/download/v0.35.2/Introvert-macOS.dmg) |

For details on configuration and build steps, refer to [Docs/README.md](./Docs/README.md).
