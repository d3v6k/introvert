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

### B. User Interface — Channel A (Flutter - `introvert`)
The WhatsApp-style front-end providing a polished user experience. **Pure P2P Telemetry Diagnostic Utility** — store-compliant, no financial code.
| Component | Strategic Purpose | File Location |
| :--- | :--- | :--- |
| **FFI Client** | The Dart-to-Rust bridge; manages unified event streams. | `lib/src/native/introvert_client.dart` |
| **Main Shell** | Primary UI navigation (Chats, Calls, Settings). | `lib/src/ui/main_shell.dart` |
| **Chat View** | Real-time messaging with liveness indicators. | `lib/views/chat_screen.dart` |
| **Node Performance Hub** | Telemetry diagnostics and network metrics display. | `lib/views/node_performance_hub.dart` |
| **Node Dashboard** | Live node performance counters (Path Weight, Allocation Multiplier). | `lib/src/ui/widgets/rewards_hud.dart` |
| **Onboarding** | Mnemonic generation and identity restoration. | `lib/src/ui/onboarding_screen.dart` |
| **Video Renderer** | Zero-copy native texture rendering for WebRTC. | `lib/src/ui/video_player.dart` |

### B2. Companion App — Channel B (React Native + Expo - `introvert-ledger`)
Independent binary for financial tracking and on-chain interactions. **ZERO shared sandbox with Channel A.**
| Component | Strategic Purpose |
| :--- | :--- |
| **RBN Oracle Client** | Connects to public RBN API endpoints over HTTPS using scanned Peer ID |
| **WalletConnect Integration** | Self-custody Solana wallet connection (Phantom, Solflare, Backpack) |
| **PDA Escrow Interface** | On-chain escrow interactions and claim payload generation |
| **Financial Tracking** | Participation weight charts, allocation history, telemetry rewards |

### C. System & Build Infrastructure
Toolchain configurations for cross-platform deployment.
| Component | Strategic Purpose | File Location |
| :--- | :--- | :--- |
| **Android Build** | Standalone APK pipeline with OpenSSL injection. | `build_standalone_apk.sh` |
| **Linux Runner** | GTK window management and native lib loading. | `linux/runner/my_application.cc` |
| **Cargo Config** | Linker and target-specific optimizations. | `.cargo/config.toml` |
| **Manifest** | Android permissions (Internet, Camera, Mic, Storage). | `android/app/src/main/AndroidManifest.xml` |

---

## 3. The Sovereign Telemetry Economy (Channel Isolation)
All incentive calculations are performed on RBN infrastructure. The Flutter client (Channel A) collects raw telemetry only — no financial logic. The Introvert Ledger App (Channel B, React Native + Expo) handles all on-chain interactions.

*   **Telemetry Unit:** Arbitrary tracking points (`pts`) in the Flutter client — no financial terminology.
*   **On-Chain Settlement:** Processed via Solana Escrow PDA by the RBN oracle batch system.
*   **Mint:** `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` (unified across all infrastructure)
*   **Logic:**
    1.  **Telemetry Collection (Channel A):** Flutter app records `bytes_relayed`, `uptime_seconds`, `active_containers` via `introvert_daily_reward_tick` FFI. Encrypted, signed with Ed25519, transmitted via v2 binary codec.
    2.  **RBN Processing:** RBNs validate telemetry against anti-gaming models, compute participation weights, build Merkle Trees.
    3.  **On-Chain Settlement (Channel B):** Introvert Ledger App queries RBN oracle API, generates claim payloads, treasury co-signs and pays SOL gas fees.

---

## 4. Networking Protocol Details
1.  **Discovery:** mDNS scans local LAN; Kademlia DHT scans global Root Bootstrap Nodes (RBNs).
2.  **Liveness:** Background heartbeats (every 30s) dial verified contacts to maintain active P2P links.
3.  **Signaling:**
    *   **Direct:** 1-to-1 QUIC/TCP stream.
    *   **Relay:** Fallback to circuit-relay via connected RBNs if NAT traversal fails.
    *   **Mailbox:** Offline messages are stored on 3 nearest DHT neighbors for 7 days (encrypted).
    *   **Introvert Codec (v2.0.0):** Custom hybrid JSON-Binary codec that bypasses Base64 encoding for `FileChunk` data, providing ~25% wire savings.
4.  **Resilience & Self-Healing (Intro-Claw):**
    *   **VPN Adaptive Pathing:** Automatically detects VPN connection transitions (type 5). Native engine filters private RFC-1918 and loopback bootstrap addresses, prioritizing WebSocket tunnel-only fallback routing.
    *   **Intelligent RBN Blacklisting:** Failed RBN dials are tracked. Stale/unreachable nodes are blacklisted with exponential back-off cooldowns (2 min → 10 min → 1 hour) to keep the reconnect ladder unblocked.
    *   **Session-Aware Connection Optimization:** The engine tracks the current chat or group session context. Bypasses cooling periods to aggressively attempt direct DCUtR upgrades for relayed chat partners and proactively heals connections to offline chat/group targets.
    *   **App Launch Warm-Up:** Forces immediate mesh refreshes and dials top contacts on startup.

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

## 7. Project Architecture Overview

### Repository Separation

Introvert operates as two distinct codebases with clear responsibilities:

| Repository | Purpose | Location |
|------------|---------|----------|
| **introvert** | RBN code — Flutter app, Rust networking core (libp2p), relay daemon, client UI | `/Users/dev/Development/introvert/` |
| **introvert-daemon** | Economy code — Solana staking validation, token payouts, treasury management | `/Users/dev/Documents/introvert-token/introvert-daemon/` |

### Why Separate?

- **Dependency isolation**: The Solana SDK (100+ crates) conflicts with libp2p's dependency tree. Separate projects prevent version mismatches.
- **Deployment independence**: RBN nodes and economy daemons scale separately and deploy to different infrastructure.
- **Security boundary**: Treasury keypairs and payout logic are isolated from the networking layer.

---

### introvert (RBN Code)

The main Introvert application and relay infrastructure:

| Component | Purpose |
|-----------|---------|
| **Flutter App** | User interface for messaging, calls, and node management |
| **Rust Core (`libintrovert`)** | libp2p networking, Kademlia DHT, QUIC/TCP transport, relay circuit |
| **RBN Daemon (`introvertd`)** | Root Bootstrap Node for peer discovery and relay services |
| **Intro-Claw Engine** | Self-healing maintenance, adaptive chunking, DCUtR upgrades |

---

### introvert-daemon (Economy Code)

Automated staking validation and token payout system:

| Component | Purpose |
|-----------|---------|
| **introvert-p2p** | libp2p swarm for peer identity verification |
| **introvert-solana** | Solana Mainnet RPC, ATA audit, transfer_checked payouts |
| **introvert-keygen** | Treasury keypair generation utility |

#### Architecture

```
[Client Device]
       │
       │ TelemetryEnvelope (Ed25519 signed)
       │ 13 activity metrics + wallet addresses
       ▼
[introvert-p2p on Alibaba]
       │
       │ RbnDailyRewardEngine calculates payout
       │ Double-claim guard checks [epoch_id:peer_id]
       │
       │ ClaimRequest JSON over port 9001
       ▼
[introvert-solana on Alibaba]
       │
       │ Verifies claim, derives ATA
       │ transfer_checked to client ATA
       ▼
[Solana Mainnet] → [Client receives $INTR]
```

#### TelemetryEnvelope Structure

```rust
pub struct TelemetryEnvelope {
    pub peer_id: String,          // libp2p network identity
    pub solana_wallet: String,    // Client's Solana Public Key
    pub solana_ata: String,       // Pre-derived Associated Token Account
    pub epoch_id: String,         // Calendar identifier (e.g., "2026_07_03")
    pub metrics: [u64; 13],       // The 13 activity metrics
    pub unique_peers: Vec<String>,
    pub is_rbn: bool,
    pub is_edge_node: bool,
    pub prestige_tier: u8,
    pub proof_hash: String,       // SHA-256 proving valid relay work
    pub client_signature: Vec<u8>, // Ed25519 signature
    pub timestamp: u64,
}
```

#### DynamicPromoStack (Customizable Campaign Layer)

The DynamicPromoStack allows runtime promotion adjustments without code rebuilds:

```rust
pub struct DynamicPromoStack {
    pub daily_strategic_reserve_ceiling: f64, // 3,287.60 INTR (Year 1)
    pub active_campaigns: HashMap<String, ActiveCampaign>,
}

pub struct ActiveCampaign {
    pub campaign_id: String,
    pub promo_type: PromoType,
    pub daily_payout_allocation: f64,
    pub expiration_epoch: String,
}
```

**Math Model:**
```
[Strategic Reserve Daily Ceiling: 3,287.60 INTR]
                    │
                    ├──► [- Minus] Active Campaigns (e.g., Theme: 1,000 INTR)
                    │
                    └──► [= Equals] Referral Pool (2,287.60 INTR)
```

#### Treasury Wallets
- Mac: `GNNEC8q9urd6rBLeNrgGLME17T7winqqEes36cMh6wu8`
- Alibaba: `DZWeLhjPeH3q4Z45HyTh5BbWXiuXdHKK7od4yR9wGLQm`

#### $INTR Mint
`EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf`

#### Deployment Targets
- **Mac Mini**: Launchd services with auto-restart
- **Alibaba RBN (47.89.252.80)**: Systemd services with auto-restart

#### Documentation
See `Docs/Operations/DAEMON_DEPLOYMENT_GUIDE.md` for complete deployment instructions.

***

This plan is now integrated into the project's memory and is ready for the next phase: **Full Sovereign Network Launch.**
