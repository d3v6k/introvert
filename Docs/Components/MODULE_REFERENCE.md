# Module Reference

## Rust Core (`src/`)

### `lib.rs` (3414 lines)
The FFI boundary. Exports 50+ `#[no_mangle] pub extern "C"` functions callable from Dart:
- `introvert_engine_start` / `introvert_engine_stop` — Lifecycle management
- `introvert_network_start_production` — Starts the libp2p swarm with callback
- `introvert_network_send_message` — Sends encrypted messages via signaling
- `introvert_wormhole_start` / `introvert_wormhole_join` — Magic Wormhole onboarding
- `introvert_file_start_transfer` / `introvert_file_get_progress` — File transfer management
- `introvert_storage_*` — CRUD operations for messages, contacts, groups, drive, reactions
- `introvert_network_*` — Network operations (tunnel, anchor, handle, swarm stats, etc.)

### `main.rs` (186 lines)
Headless daemon entry point (`introvertd`). CLI via `clap`:
```
introvertd --seed-file <path> --db-path <path> --port 443 --relay --max-connections 1000000
```

### `identity.rs` (90 lines)
- `NodeIdentity::from_seed(seed)` — HKDF-SHA256 derivation of Ed25519 keypair
- `NodeIdentity::derive_storage_key(seed)` — SQLCipher key derivation
- `NodeIdentity::derive_static_noise(seed)` — X25519 static key for Noise IK
- `SovereignIdentity` — Serializable identity package for Wormhole exchange

### `storage.rs` (1338 lines)
SQLCipher database with 18 tables. Key methods:
- `StorageService::new(path, key)` — Opens encrypted database
- `StorageService::new_ephemeral()` — In-memory database for testing
- `bootstrap()` — Creates all tables and indexes
- CRUD methods for messages, contacts, groups, drive, reactions, handles, etc.

### `network/mod.rs` (4928 lines)
The heart of the mesh networking layer:
- `NetworkService` — Manages libp2p swarm lifecycle
- `SignalingRequest`/`SignalingResponse` — JSON protocol types
- `GroupAction` / `SignedGroupAction` — Group mesh operation types
- `SecureMessage` — Handshake and transport message envelopes
- Message dispatch, file transfer, mailbox sync, peer management

### `network/behaviour.rs` (150 lines)
`IntrovertBehaviour` — Composite NetworkBehaviour for libp2p:
- `kademlia` — Custom protocol `/introvert/kad/1.0.0`, 24h TTL, 5x replication
- `request_response` — JSON signaling protocol
- `gossipsub` — Group mesh messaging
- `mdns` — Local network discovery (toggleable)
- `dcutr` — Direct Connection Upgrade Through Relay
- `relay_client` / `relay_server` — Circuit relay v2
- `autonat` — NAT detection
- `identify` — Custom protocol `/introvert/id/1.0.0`
- `ping` / `connection_limits` — Health and limits

### `network/config.rs` (32 lines)
Legacy bootstrap node configuration. Global RBN at `47.89.252.80:443`. Supports `INTROVERT_EXTRA_BOOTSTRAP` env var. **Note:** As of Phase 2, bootstrap nodes are discovered dynamically from the Solana `introvert-registry` program. This file serves as fallback only when Solana RPC is unreachable.

### `network/noise_session.rs` (154 lines)
Noise IK handshake using `snow` crate. Pattern: `Noise_IK_25519_ChaChaPoly_BLAKE2s`.

### `network/group.rs` (136 lines)
Group mesh manager. Signs and verifies group actions using libp2p keys. Role-based permission enforcement (Creator > Admin > Member).

### `network/registry.rs` (83 lines)
PoW-based handle registry. Difficulty: 4 leading hex zeros. Verifies claims against RBN witness signatures.

### `network/tunnel.rs` (158 lines)
WebSocket tunnel for NAT traversal. Bridges local TCP loopback to remote RBN WebSocket endpoint. Bidirectional proxy with async tasks.

### `network/wormhole.rs` (189 lines)
Magic Wormhole onboarding. Creates/joins 2-word pairing codes via `relay.magic-wormhole.io`. Exchanges `SovereignIdentity` packages.

### `media/mod.rs` (363 lines)
WebRTC media transport. Creates peer connections with VP8 video and Opus audio codecs. Manages SDP signaling, data channels, and track handling.

### `economy/mod.rs` (187 lines)
`RewardTracker` — Monitors relayed bytes with 1MB threshold and 5-minute cooldown. Persists to `reward_log` table.

### `economy/solana.rs` (155 lines)
`SolanaIncentiveEngine` — Solana RPC client for $INTR token operations: balance queries, transfer construction, treasury co-signing.

---

## Flutter/Dart (`lib/`)

### `main.dart` (233 lines)
App entry point. Initializes:
1. Theme loading
2. IntrovertClient FFI initialization
3. Sandbox path resolution (iOS/macOS)
4. Identity check (existing seed → start engine, or → onboarding)
5. Provider setup (SyncStateNotifier, IntrovertClient, IdentityManager)

### `src/native/introvert_client.dart` (1560 lines)
The FFI bridge. Key components:
- `FfiResult` struct — C-compatible return type
- `IntrovertClient` class — Wraps all native function calls
- Event streams: `networkEventStream`, `mediaFrameStream`, `swarmStatsStream`
- Sandbox path resolution for iOS/macOS UUID changes
- Automatic cleanup with `_freeBinary` in try/finally blocks

### `src/native/identity_manager.dart`
Manages seed storage via `SharedPreferences`. Handles mnemonic generation and seed persistence.

### `src/native/alert_service.dart`
Notification permissions and background service management for incoming calls.

### `src/ui/main_shell.dart` (3087 lines)
WhatsApp-style main UI:
- 3 tabs: Chats, Drive, Settings
- Global network event listener
- App lifecycle management (background service start/stop)
- Incoming call handling via `flutter_callkit_incoming`
- Update service integration
- Debug overlay support

### `src/ui/drive_tab.dart` (627 lines)
Sovereign Drive UI. Features:
- File listing with search
- Upload/download with progress tracking
- Swarm capacity display
- Active transfer monitoring
- Auto-refresh every 5 seconds

### `src/ui/video_player.dart`
Video playback widget using `video_player` package with play/pause, seek, and duration display.

### `src/ui/update_service.dart`
Background update checker. Fetches update JSON from configured endpoint. Supports `config_url_override` for server migration.

### `src/ui/connection_diagnostics_overlay.dart`
Debug overlay showing network status, connection type, and peer information.

### `src/services/webrtc_call_service.dart`
Manages WebRTC call lifecycle: incoming call detection, call acceptance/rejection, media stream management.

### `src/repository/sync_repository.dart`
Repository pattern for data synchronization between Rust storage and Flutter UI.

### `views/chat_screen.dart`
1-on-1 chat interface with message bubbles, reactions, replies, file attachments, and call buttons.

### `views/group_chat_screen.dart`
Group chat interface with member management, admin controls, and group settings.

### `views/profile_screen.dart`
Contact profile view with avatar, handle, connection status, and action buttons.

### `views/call_screen.dart`
Active call UI with video/audio controls, mute, speaker, and end call.

### `views/media_gallery_viewer.dart`
Full-screen media gallery with swipe navigation, pinch-to-zoom for images, and video playback controls.

### `views/wallet_dashboard.dart`
Solana $INTR token balance and transaction history.

### `views/location_picker_screen.dart`
Map-based location sharing using `flutter_map` with CartoDB Voyager tiles.

### `blueprint_ui.dart` (315 lines)
Reusable UI components:
- `SovereignAvatar` — Circular avatar with connection indicator
- Various styled widgets for consistent theming

### `theme/app_theme.dart` (112 lines)
Theme system with 5 built-in themes. Persists selection via SharedPreferences.

---

## Intro-Claw / Automation Engine

### `src/intro_claw.rs` (1500+ lines)
Core automation engine. Contains:
- **12 maintenance modules**: BatteryThrottler, DatabasePruner, MediaLifecycleManager, ConnectionOptimizer, MessageBatcher, PredictivePrefetcher, SyncPrioritizer, DuplicateSuppressor, ConnectionHealthScorer, StorageQuotaManager, AdaptiveChunkSizer
- **Tick loop**: 5-minute interval via `NetworkCommand::IntroClawTick`
- **Assistant query engine**: `parse_assistant_query()`, `execute_assistant_query()` — natural language → structured search
- **Network recon**: `run_network_recon()` — builds markdown report of mesh state, peer routing, connections, anchors
- **Network healer**: `build_heal_plan()`, `render_heal_report()` — 5-strategy connection recovery
- **Hybrid LLM integration**: `llm_query()`, `process_assistant_query_hybrid()` — async reqwest POST to OpenAI-compatible endpoints

### `src/embedding.rs` (300+ lines)
Local text embedding engine. Contains:
- **EmbeddingEngine**: Singleton with keyword matching + BERT vector similarity
- **BERT inference**: `BertInference::encode()` — tokenization → candle BERT forward pass → mean pooling → L2 normalization → 384-dim vectors
- **Cosine similarity**: `cosine_similarity()` for vector comparison
- **12 action intents**: Pre-defined phrases for each automation module
- **Thread priority**: `libc::setpriority(PRIO_PROCESS, 0, 10)` for low-priority background execution

### `lib/src/ui/assistant_tab.dart` (800+ lines)
Assistant chat UI. Contains:
- **Chat interface**: Message bubbles, suggestion chips, input bar
- **RECON button**: Network reconnaissance with terminal-style milestone animation
- **HEAL button**: Network healing with multi-strategy recovery
- **Info panel**: Modal bottom sheet explaining Intro-Claw capabilities
- **Recon report rendering**: Monospaced markdown code blocks on black background

### `for_linux/src/fcm.rs` (230+ lines)
Firebase Cloud Messaging push service for the RBN. Contains:
- **FcmPushService**: JWT-based OAuth2 token generation, FCM v1 API calls
- **Service account loading**: From `FIREBASE_SERVICE_ACCOUNT_PATH` or `/opt/introvert/config/firebase-service-account.json`
- **Token caching**: 55-minute lifetime with auto-refresh

## Scripts

### `scripts/build_android.sh` (148 lines)
Android cross-compilation pipeline. Auto-detects NDK, pre-builds OpenSSL, compiles for arm64-v8a and x86_64.

### `deploy_local_rbn.sh` (69 lines)
Local cross-compile and deploy. Uses `cargo-zigbuild` for Linux binary, SCP for upload, SSH for systemd management.

### `deploy_rbn.sh` (74 lines)
Remote build machine deployment. Syncs source, compiles remotely, deploys to production RBN.

### `Makefile` (46 lines)
Master build orchestration: `make mac`, `make android`, `make ios`, `make all`, `make clean`.
