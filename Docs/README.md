# Introvert: Sovereign P2P Mesh

Introvert is a privacy-first, decentralized communication ecosystem. It eliminates central servers entirely by utilizing true Peer-to-Peer (P2P) networking, end-to-end encryption (E2EE), and a dynamic, sovereign Solana-based token economy. The network operates via a crowdsourced, incentivized, self-healing mesh layer, with entry points dynamically coordinated on-chain — transforming Introvert from an isolated chat application into a zero-knowledge, autonomous utility network.

## 🌟 Core Value Pillars
*   **Open-Source & P2P:** A fully transparent codebase audited by the community. Serverless, direct edge-to-edge routing over standard libp2p.
*   **End-to-End Encryption:** Locally encrypted messaging, groups, and drive data using the Noise Protocol frame (`Noise_IK_25519_ChaChaPoly_BLAKE2s`) and AES-256-GCM.
*   **Eco-Friendly / Green Credentials:** Zero carbon-heavy datacenters. Utilizes existing idle consumer hardware coupled with Solana's green Proof-of-History consensus.
*   **Zero Spam:** Sybil-resistant, balance-gated rate-limiting filters that increase the economic cost of abuse.
*   **Bleeding-Edge Tech:** Powered by the self-healing **Intro-Claw Maintenance Engine** and the bandwidth-saving binary **Intro Codec** (saving 25% overhead).
*   **User Rewards:** Earn $INTR tokens daily based on contribution weight via dynamic pool-clearing rewards.
*   **Community-Powered RBNs:** Root Bootstrap Nodes are operated by community hosts to maintain a decentralized network directory.

## Core Features
- **Zero-Knowledge Privacy:** End-to-end encryption (E2EE) using the Noise Protocol (Noise_IK_25519_ChaChaPoly_BLAKE2s).
- **Sovereign Identity:** Deterministic identity derived from a 32-byte master seed via HKDF-SHA256. No phone number, email, or central authority required.
- **Dynamic Blockchain Bootstrapping:** Eliminates hardcoded bootstrap IPs. Clients discover RBN nodes dynamically via Solana on-chain registry queries, making the network resistant to DNS/IP blacklisting.
- **Token Gating Engine:** Structural Sybil resistance requiring 100,000 $INTR minimum for edge routing (Event Code 22) and 2,000,000 $INTR for RBN operators.
- **Autonomous Escrow Vault:** Unified Program-Derived Address (PDA) on Solana holding all network stakes and emissions — no single key controls the vault.
- **Squads V4 Governance:** 3-of-5 multisig controls contract upgrades, ensuring full legal separation for the software publisher.
- **Messenger-Grade Hardening:** libp2p v0.56 mesh standardized on **Port 443 (HTTPS Bypass)** for global reachability through carrier firewalls.
- **Dark Mesh Isolation:** Completely shielded from global DHT noise via custom `/introvert/kad/1.0.0` protocols and client-only mode for edge devices.
- **Real-time Delivery/Read Receipts:** Functional 'Acknowledgement' protocol for WhatsApp-style UI ticks (Sent, Delivered, Read).
- **Real-time P2P Push:** RBN-driven push logic that eliminates polling delays for connected peers.
- **Relay-Aware Connectivity:** Automatic construction of relay paths via RBN nodes, ensuring Mac-to-Android and multi-network reliability.
- **Introvert Codec:** A custom hybrid JSON-binary codec (`/introvert/signaling/2.0.0`) that eliminates Base64 overhead for `FileChunk` data, providing ~25% wire data savings.
- **Direct Dial Auto-Upgrades:** Connections automatically upgrade from relayed to direct P2P when direct addresses are discovered.
- **Persistent History:** Encrypted local storage using SQLCipher (AES-256-CBC) with CRDT-based synchronization.
- **Economic Incentives:** Built-in Solana-based $INTR token economy (100M fixed supply, Mint: `NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn`). 50% allocation (50M) for ecosystem rewards over 10 years. Dynamic pool-clearing daily rewards. Gasless transactions via Treasury Fee Payer. See `Docs/INTROVERT_TOKEN_WHITEPAPER.md`.
- **Sovereign Drive:** Encrypted file storage with automatic organization into context-aware subfolders.
- **Encrypted Groups:** Gossipsub-based mesh group rooms with signed actions, role management, and shared secrets.
- **Mesh Reactions:** Decentralized emoji reaction propagation across the mesh network.
- **Magic Wormhole Onboarding:** Zero-config device pairing via 2-word codes.
- **WebSocket Tunnel:** Loopback tunnel client for NAT traversal via RBN WebSocket proxies.
- **Handle Registry:** PoW-based INR handle claims with RBN witness consensus.
- **Intro-Claw AI Engine:** Local automation engine with 12 maintenance modules, BERT-based semantic intent matching (all-MiniLM-L6-v2), natural language assistant, network recon & healing, and optional Hybrid AI mode.
- **Green Energy & Sustainability:** A "Zero-Data-Center" architecture utilizing existing consumer hardware.

## Technical Stack
- **Backend:** Rust (libp2p v0.56, SQLite/SQLCipher, Noise IK, WebRTC, Solana SDK 4.0)
- **Frontend:** Flutter (Dart) with dart:ffi bridge
- **Networking:** Port 443 TCP/UDP (QUIC), WebSocket tunnel fallback, dynamic Solana-based bootstrapping, and custom **Introvert Codec** (v2.0.0 protocol)
- **Storage:** SQLCipher encrypted database (18 tables)
- **Identity:** HKDF-SHA256 deterministic derivation from master seed (Zero Phone/Email)
- **Consensus & Economy:** Solana Mainnet-Beta via unified PDA escrow vault, Squads V4 (3-of-5) Multisig governance

## Getting Started

### Prerequisites
- **Rust:** `rustup` stable (1.75+)
- **Flutter:** 3.22+ (stable channel)
- **Android NDK:** v28.2.13676358
- **CMake & LLVM:** For native cryptography bindings
- **CocoaPods:** For iOS/macOS plugin management

### Build Native Core
```bash
make mac      # For macOS (produces libintrovert.dylib)
make android  # For Android (arm64 + x86_64 .so files)
make ios      # For iOS (device + simulator .a static libraries)
```

### Launch Application
```bash
flutter pub get
flutter run
```

### RBN Deployment
```bash
# Option 1: Local cross-compile and deploy (recommended)
brew install zig && cargo install cargo-zigbuild
./deploy_local_rbn.sh

# Option 2: Remote compilation on build machine
./deploy_rbn.sh
```

## Project Structure
```
/introvert
├── Docs/                    # Technical blueprints & rebuild guides
├── android/                 # Android build config (com.example.introvert_tests)
├── ios/                     # iOS build config (CocoaPods, P2P entitlements)
├── macos/                   # macOS build config (CocoaPods, ephemeral libs)
├── lib/                     # Flutter UI (Dart)
│   ├── main.dart            # App entry point, initialization
│   ├── blueprint_ui.dart    # Reusable UI components (SovereignAvatar, etc.)
│   ├── src/
│   │   ├── native/          # FFI Bridge (introvert_client.dart, identity_manager.dart)
│   │   ├── ui/              # Main shell, drive tab, update service, video player
│   │   ├── services/        # WebRTC call service
│   │   └── repository/      # Sync repository
│   ├── views/               # Chat, group chat, profile, call, media gallery, wallet
│   └── theme/               # App styling (5 themes: Introvert Dark, Nordic Fog, etc.)
├── src/                     # Rust Core Engine
│   ├── lib.rs               # FFI C-bindings (3414 lines, 50+ exported functions)
│   ├── main.rs              # Headless daemon entry point (introvertd)
│   ├── identity.rs          # Deterministic HKDF identity derivation
│   ├── storage.rs           # SQLCipher persistence (18 tables, 1338 lines)
│   ├── network/             # libp2p swarm, signaling, groups, registry, tunnel
│   ├── media/               # WebRTC implementation
│   └── economy/             # Reward tracker + Solana incentive engine
├── scripts/                 # Automation (Android build, cmake wrapper)
├── for_linux/               # RBN Daemon source tree (Linux Native)
├── plugins/                 # Local plugins (pdf_render_maintained)
├── assets/                  # Images, audio (introvert_ping.m4a)
├── Makefile                 # Master build orchestration
├── Cargo.toml               # Rust dependency management
├── Cargo.lock               # Rust lockfile
├── pubspec.yaml             # Flutter dependency management
└── pubspec.lock             # Flutter lockfile
```

---
**Own your words. Own your network. Own your future.**
