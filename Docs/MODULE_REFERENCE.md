# Module Reference & Codebase Map

## 1. Rust Backend (`src/`)

### `main.rs`
The entry point for the standalone daemon (`introvertd`). Orchestrates the initialization of the `NetworkService` and `StorageService` for RBN/headless environments.

### `lib.rs`
The library entry point for the FFI bridge. Contains all `#[no_mangle]` functions used by Flutter. Manages the global `ENGINE` instance.

### `identity.rs`
Logic for sovereign identity. Implements `NodeIdentity` and HKDF-based deterministic derivation.

### `storage.rs`
Persistence layer. Manages SQLCipher database connections, schema migrations, and high-performance message/contact queries.

### `network/`
- `mod.rs`: The central hub. Contains the `NetworkService` event loop, command handling, and signaling payload processing.
- `behaviour.rs`: Defines the `libp2p::NetworkBehaviour` (Kademlia, Request-Response, Mdns, Relay, etc.).
- `noise_session.rs`: Wrapper around the `snow` crate for end-to-end encrypted sessions.
- `wormhole.rs`: Magic Wormhole integration for secure, code-based peer introduction.
- `group.rs`: Core logic for decentralized groups. Manages Gossipsub topic mapping and **cryptographic authority verification** (Signed Actions).
- `config.rs`: Network constants and bootstrap node lists.

### `media/`
- `mod.rs`: WebRTC stack integration. Handles track creation, SDP negotiation, and ICE candidate gathering.

### `economy/`
- `mod.rs`: Reward tracking and work proof generation.
- `solana.rs`: Interaction with the Solana blockchain (SPL-tokens, Treasury ATA).

## 2. Flutter Frontend (`lib/`)

### `main.dart`
App entry point. Initializes the `IntrovertClient` and determines if the user needs onboarding.

### `src/native/introvert_client.dart`
The FFI bridge. Maps C-style function pointers to Dart methods and exposes the `eventStream`, `transferStream`, and new `Drive` FFI calls.

### `src/ui/`
- `main_shell.dart`: The primary navigation and status HUD. Updated to include **Mesh Status Indicators** and bottom-nav for Chats, Drive, and Settings.
- `drive_tab.dart`: New interface for personal/mesh file storage. Displays **Dynamic Mesh Capacity** (updated every 15 mins).
- `onboarding_screen.dart`: Revamped flow providing explicit **"Create New Identity"** and **"Recover from Seed"** paths.
- `widgets/`:
    - `sovereign_avatar.dart`: Renders deterministic identicons or base64 avatars.
    - `file_transfer_bubble.dart`: Complex widget for rendering transfer progress and media previews.
    - `rewards_hud.dart`: Displays $INTR balance and relay stats.

### `views/`
- `chat_screen.dart`: The core messaging interface. Revamped to include **Encrypted Call** (VoIP/Video) directly in the header.
- `contact_screen.dart`: Contact management and Wormhole invitations.

## 3. Scripts & Tooling
- `Makefile`: Central build orchestration.
- `scripts/build_android.sh`: NDK cross-compilation environment setup.
- `for_linux/build_linux.sh`: Native ELF compilation for RBNs.
