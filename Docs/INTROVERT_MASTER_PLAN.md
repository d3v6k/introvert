# INTROVERT: The Sovereign Mesh
### Master Plan & Technical Specification v3.0

## 1. Vision & Objectives
Introvert is a privacy-first, decentralized communication platform designed to eliminate reliance on centralized servers. It establishes a sovereign P2P mesh where users own their identity, data, and bandwidth.

**Core Objectives:**
*   **Zero-Knowledge Identity:** Single BIP-39 seed derives Libp2p PeerID, SQLCipher encryption key, and Solana Wallet.
*   **Resilient Networking:** Hybrid routing using mDNS (local), Kademlia DHT (global), and Peer-to-Peer Circuit Relays (asynchronous).
*   **Sovereign Economy:** Integrated Solana SPL Token (`INTR`) rewards for users who provide relay and storage services to the mesh.
*   **Hardened Privacy:** Mandatory Noise IK (X25519) E2EE for all signaling and WebRTC streams.

**Core Philosophy & Value Pillars:**
*   **Open-Source & Peer-to-Peer:** A fully transparent codebase audited by the community. Serverless, direct edge-to-edge routing over standard libp2p.
*   **End-to-End Encryption:** Locally encrypted messaging, groups, and drive data using the Noise Protocol frame (`Noise_IK_25519_ChaChaPoly_BLAKE2s`) and AES-256-GCM.
*   **Eco-Friendly / Green Credentials:** Zero carbon-heavy datacenters. Utilizes existing idle consumer hardware coupled with Solana's green Proof-of-History consensus.
*   **Zero Spam:** Sybil-resistant, balance-gated rate-limiting filters that increase the economic cost of abuse.
*   **Bleeding-Edge Tech:** Powered by the self-healing **Intro-Claw Maintenance Engine** and the bandwidth-saving binary **Intro Codec** (saving 25% overhead).
*   **User Rewards:** Earn $INTR tokens daily based on contribution weight via dynamic pool-clearing rewards.
*   **Community-Powered RBNs:** Root Bootstrap Nodes are operated by community hosts to maintain a decentralized network directory.

---

## 2. Component Architecture & File Mapping

### A. Core Engine (Rust - `libintrovert`)
The high-performance backbone handling networking, crypto, and storage.
| Component | Strategic Purpose | File Location |
| :--- | :--- | :--- |
| **FFI Bridge** | Logic exported to Flutter; handles memory leak-and-reclaim. | `src/lib.rs` |
| **Network Swarm** | Libp2p engine; handles QUIC, TCP, and Relay V2. | `src/network/mod.rs` |
| **Introvert Codec**| Custom hybrid JSON-Binary codec for /signaling/2.0.0. | `src/network/codec.rs` |
| **Kademlia DHT** | Zero-knowledge peer discovery and X25519 key publishing. | `src/network/config.rs` |
| **Wormhole** | Magic Wormhole PAKE for secure one-time onboarding. | `src/network/wormhole.rs` |
| **Storage Engine** | SQLCipher integration with 7-day TTL for mailboxes. | `src/storage.rs` |
| **Economy Layer** | Reward tracking and work-proof generation. | `src/economy/mod.rs` |
| **Solana Engine** | SPL Token (INTR) balance and gasless claim logic. | `src/economy/solana.rs` |
| **Media Plane** | WebRTC stack for encrypted Voice/Video. | `src/media/mod.rs` |
| **Identity** | BIP-39 mnemonic and domain-separated key derivation. | `src/identity.rs` |

### B. User Interface (Flutter - `introvert_tests`)
The WhatsApp-style front-end providing a polished user experience.
| Component | Strategic Purpose | File Location |
| :--- | :--- | :--- |
| **FFI Client** | The Dart-to-Rust bridge; manages unified event streams. | `lib/src/native/introvert_client.dart` |
| **Main Shell** | Primary UI navigation (Chats, Calls, Settings). | `lib/src/ui/main_shell.dart` |
| **Chat View** | Real-time messaging with liveness indicators. | `lib/views/chat_screen.dart` |
| **Identity Hub** | Sovereign Earnings HUD and INTR Wallet management. | `lib/src/ui/widgets/rewards_hud.dart` |
| **Onboarding** | Mnemonic generation and wallet restoration. | `lib/src/ui/onboarding_screen.dart` |
| **Video Renderer** | Zero-copy native texture rendering for WebRTC. | `lib/src/ui/video_player.dart` |

### C. System & Build Infrastructure
Toolchain configurations for cross-platform deployment.
| Component | Strategic Purpose | File Location |
| :--- | :--- | :--- |
| **Android Build** | Standalone APK pipeline with OpenSSL injection. | `build_standalone_apk.sh` |
| **Linux Runner** | GTK window management and native lib loading. | `linux/runner/my_application.cc` |
| **Cargo Config** | Linker and target-specific optimizations. | `.cargo/config.toml` |
| **Manifest** | Android permissions (Internet, Camera, Mic, Storage). | `android/app/src/main/AndroidManifest.xml` |

---

## 3. The Sovereign Token Economy (INTR)
All incentives are anchored to the official on-chain SPL mint.
*   **Token:** Introvert Token (`INTR`)
*   **Mint:** `NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn`
*   **Decimals:** 9
*   **Logic:**
    1.  **Availability Yield:** Users earn INTR for staying online and maintaining DHT records.
    2.  **Relay Proofs:** Bytes relayed for other peers generate signed work-proofs.
    3.  **Gasless Claims:** Treasury co-signers (`api.introvert.network`) handle SOL fees for reward payouts.

---

## 4. Networking Protocol Details
1.  **Discovery:** mDNS scans local LAN; Kademlia DHT scans global Root Bootstrap Nodes (RBNs).
2.  **Liveness:** Background heartbeats (every 30s) dial verified contacts to maintain active P2P links.
3.  **Signaling:**
    *   **Direct:** 1-to-1 QUIC/TCP stream.
    *   **Relay:** Fallback to circuit-relay via connected RBNs if NAT traversal fails.
    *   **Mailbox:** Offline messages are stored on 3 nearest DHT neighbors for 7 days (encrypted).
    *   **Introvert Codec (v2.0.0):** Custom hybrid JSON-Binary codec that bypasses Base64 encoding for `FileChunk` data, providing ~25% wire savings.

---

## 5. Deployment Pipeline
1.  **Rust Build:** `cargo build --release` (targets `x86_64` and `aarch64-android`).
2.  **Optimization:** `llvm-strip` removes debug symbols to keep the APK under 45MB.
3.  **FFI Sync:** `libintrovert.so` is manually injected into Flutter's ephemeral and release paths.
4.  **Flutter Assembly:** `flutter build apk --split-per-abi` for production deployment.

***

## 6. Build & Deployment Matrix (Requirements for Code Changes)

When code changes are made to the codebase, follow this matrix to determine which rebuild, rerun, or upload actions are required:

| Scope of Code Change | Cargo Rebuild (`make mac` / `make android`) | Flutter Run (App Relaunch) | RBN Upload / Redeploy |
| :--- | :--- | :--- | :--- |
| **Rust Client Core (`src/`)** | **YES** | **YES** | **NO** |
| **Dart/Flutter UI (`lib/`)** | **NO** | **YES** (or Hot Reload/Restart) | **NO** |
| **Assets / Configs (`assets/`)** | **NO** | **YES** | **NO** |
| **RBN Daemon Core (`for_linux/`)** | **NO** (unless compiling local tests) | **NO** | **YES** (Recompile and redeploy `introvertd` to RBN servers) |

### Protocol Upgrades & Chunk Size Changes
When changing chunk size or pull pipeline window sizes:
*   **Active Client Transfers**: Requires a **Cargo Rebuild** (to repackage the native libraries) and **Flutter Run** on the client devices. Redeploying the RBN is **NOT** required because the RBN acts as a transparent circuit relayer and does not inspect or validate chunk sizes.
*   **Offline Caching / Node Mode**: Requires **RBN Upload & Redeploy** to RBN servers. The daemon itself runs the file prefetcher/seeder engine in Node Mode and must align on chunk sizes to serve offline cache clients.

***

This plan is now integrated into the project's memory and is ready for the next phase: **Full Sovereign Network Launch.**
