# Introvert System Overview

## Architecture Summary

Introvert is a decentralized, peer-to-peer communication system consisting of three primary components:

1. **Rust Core Engine (`libintrovert`)** — High-performance networking, encryption, and storage
2. **Flutter UI** — Cross-platform user interface (Android, iOS, macOS)
3. **RBN Daemon (`introvertd`)** — Root Bootstrap Node for network anchoring

## Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Flutter UI Layer                         │
├─────────────┬─────────────┬─────────────┬───────────────────────┤
│  Chat Tab   │  Drive Tab  │  Settings   │  Call Screen          │
└──────┬──────┴──────┬──────┴──────┬──────┴───────────┬───────────┘
       │             │             │                  │
       └─────────────┼─────────────┼──────────────────┘
                     │ FFI Bridge  │
       ┌─────────────▼─────────────▼───────────────────┐
       │           Rust FFI Bridge Core                 │
       │  (50+ exported C functions, async callbacks)   │
       └─────────────────────┬─────────────────────────┘
                             │
       ┌─────────────────────▼─────────────────────────┐
       │           Rust Core Engine                     │
       ├──────────────┬──────────────┬─────────────────┤
       │   Network    │   Storage    │   Economy       │
       │  (libp2p)    │  (SQLCipher) │  (Solana)       │
       └──────┬───────┴──────┬───────┴────────┬────────┘
              │              │                │
              ▼              ▼                ▼
       [Mesh Network]  [Encrypted DB]  [SOL Chain]
```

## Data Flow

### Message Send
1. User types message in Flutter UI
2. `IntrovertClient.sendMessage()` called via FFI
3. Rust core encrypts with Noise session
4. libp2p sends via Request-Response protocol
5. Delivery status events dispatched back to UI

### File Transfer
1. User selects file in Flutter UI
2. File chunked (64KB) and encrypted
3. Chunks sent via signaling or WebRTC
4. Recipient reassembles and verifies SHA-256
5. File stored in Sovereign Drive

### Group Message
1. User sends message in group chat
2. `GroupAction::Message` signed with Ed25519
3. Gossipsub broadcasts to group members
4. Each member decrypts with group secret
5. Message stored in `group_messages` table

## Security Layers

| Layer | Technology | Purpose |
|-------|------------|---------|
| Transport | Noise IK | Encrypts all libp2p traffic |
| Storage | SQLCipher | AES-256-CBC encrypted database |
| File | AES-GCM | Encrypts file payloads |
| Group | Symmetric key | Per-group encryption |
| Identity | HKDF-SHA256 | Deterministic key derivation |

## Network Topology

```
                    ┌──────────────────┐
                    │   Root Bootstrap │
                    │   Node (RBN)     │
                    │   Port 443       │
                    └────────┬─────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
        ┌─────▼─────┐ ┌─────▼─────┐ ┌─────▼─────┐
        │  Client    │ │  Client   │ │  Client   │
        │  (Mobile)  │ │  (Desktop)│ │  (Mobile) │
        └─────┬─────┘ └─────┬─────┘ └─────┬─────┘
              │              │              │
        ┌─────▼─────┐ ┌─────▼─────┐ ┌─────▼─────┐
        │  Direct    │ │  Relay    │ │  Direct   │
        │  P2P       │ │  (RBN)    │ │  P2P      │
        └───────────┘ └───────────┘ └───────────┘
```

## Key Metrics

| Metric | Value |
|--------|-------|
| Codebase Size | ~15,000 lines (Rust) + ~10,000 lines (Dart) |
| FFI Functions | 50+ exported |
| Database Tables | 18 |
| Event Codes | 0-38 (39 total) |
| Supported Platforms | Android, iOS, macOS, Linux |
| Max File Size | 1GB+ |
| Direct Transfer Speed | 14+ Mbps |
| Relay Transfer Speed | 0.3-1 Mbps |

## Build System

| Target | Command | Output |
|--------|---------|--------|
| macOS | `make mac` | `libintrovert.dylib` |
| Android | `make android` | `libintrovert.so` (arm64 + x86_64) |
| iOS | `make ios` | `libintrovert.a` (device + sim) |
| Linux RBN | `deploy_local_rbn.sh` | `introvertd` (ELF x86_64) |

## Technology Stack

| Component | Technology | Version |
|-----------|------------|---------|
| Backend | Rust | 1.75+ |
| Frontend | Flutter/Dart | 3.22+ |
| P2P Networking | libp2p | 0.56 |
| Encryption | Noise/snow | 0.9 |
| Database | SQLCipher | 0.31 |
| Blockchain | Solana SDK | 4.0 |
| Media | WebRTC | 0.11 |
| Onboarding | Magic Wormhole | 0.7 |
