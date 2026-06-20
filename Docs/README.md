# Introvert: Sovereign P2P Mesh

Introvert is a privacy-first, decentralized communication ecosystem. It eliminates central intermediaries by utilizing a high-performance peer-to-peer (P2P) mesh architecture, ensuring data ownership and network control remain exclusively with the user.

## Core Features
- **Zero-Knowledge Privacy:** End-to-end encryption (E2EE) using the Noise Protocol (Noise_IK_25519_ChaChaPoly_BLAKE2s).
- **Sovereign Identity:** Deterministic identity derived from a 32-byte master seed via HKDF-SHA256. No phone number, email, or central authority required.
- **Messenger-Grade Hardening:** libp2p v0.56 mesh standardized on **Port 443 (HTTPS Bypass)** for global reachability through carrier firewalls.
- **Dark Mesh Isolation:** Completely shielded from global DHT noise via custom `/introvert/kad/1.0.0` protocols and client-only mode for edge devices.
- **Real-time Delivery/Read Receipts:** Functional 'Acknowledgement' protocol for WhatsApp-style UI ticks (Sent, Delivered, Read).
- **Real-time P2P Push:** RBN-driven push logic that eliminates polling delays for connected peers.
- **Relay-Aware Connectivity:** Automatic construction of relay paths via RBN nodes, ensuring Mac-to-Android and multi-network reliability.
- **Direct Dial Auto-Upgrades:** Connections automatically upgrade from relayed to direct P2P when direct addresses are discovered.
- **Persistent History:** Encrypted local storage using SQLCipher (AES-256-CBC) with CRDT-based synchronization.
- **Economic Incentives:** Built-in Solana-based $INTR token economy (Mint: `NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn`) for network contributors.
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
- **Networking:** Port 443 TCP/UDP (QUIC), WebSocket tunnel fallback
- **Storage:** SQLCipher encrypted database (18 tables)
- **Identity:** HKDF-SHA256 deterministic derivation from master seed

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
