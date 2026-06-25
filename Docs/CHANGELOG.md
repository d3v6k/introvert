# Changelog

All notable changes to Introvert will be documented in this file.

## [0.15.0] - 2026-06-25 — STABLE v39 "Relay Resiliency"

### Added
- **ListenerClosed Relay Auto-Recovery**: Added `SwarmEvent::ListenerClosed` handler to client networking mod (`src/network/mod.rs`) that automatically removes dropped relay reservation listeners and requests a fresh circuit relay reservation if the connection to the RBN remains active.
- **RBN Code Sync**: Updated RBN (Relayed Byte Network) daemon source code (`for_linux/src/network/mod.rs`) with v2.0.0 protocol binary codec compatibility.
- **Makefile All Targets**: Verified full workspace cross-compilation pipeline (`make all`) compiling macOS (`libintrovert.dylib`), iOS (`libs/`), and Android (`jniLibs/`) native outputs.

### Changed
- **Weak Network Handling UI/UX**: Replaced the blocking interactive dialog on weak connectivity in `lib/views/chat_screen.dart` with a rate-limited background refresh (`forceNetworkRefresh()` capped to once every 2 minutes) and a discreet, non-disruptive `SnackBar` alert: `"Weak network detected... optimizing..."`.
- **File Transfer Authorization**: Hardened verification constraints on fallback seeder lookups in client and daemon network stacks.

### Fixed
- **Contact Info Layout Overflow**: Wrapped the contact settings details block in a `SingleChildScrollView` to prevent screen boundary layout errors on mobile screens.

> [!NOTE]
> Text messaging across networks is fully verified via circuit relays (RelayActive state). However, media sharing across different physical networks (e.g., direct download issues) has been improved but still requires thorough verification on multiple physical device setups.

## [0.14.0] - 2026-06-24 — STABLE v38 "Unified Drive"

### Added
- **Drive UX Redesign**: Created folder-based storage view, expandable category sections, visual thumbnail grids, and a "Download All" capability in the file explorer.
- **Hardened Reactions**: Integrated reaction delivery with SQLite `StoreInMailbox` fallback, ensuring reaction indicators persist and propagate reliably when direct transport drops.
- **Editable Themes**: Added interface configuration tools allowing customization of any system default color theme.

### Changed
- **Mailbox Delivery Fallback**: Applied mailbox store mechanics for reaction syncing.
- **File Manifest Sync**: Auto-schedules manifest sync scans on chat message updates.

## [0.13.0] - 2026-06-24 — STABLE v37 "Mesh Resurrection"

### Added
- **Group Invite Wrappers**: Hardened group invitations by wrapping Manifest keys via ECDH key exchanges.

### Fixed
- **Group Chat Restoration**: Resolved severe connectivity bugs including Noise IK session deadlocks, `GroupAction` double-encryption, Gossipsub `propagation_source` peer lookup errors, and RBN infinite loop bugs.
- **Theme Rendering**: Fixed Winter Wonderland styling anomalies.

## [0.12.0] - 2026-06-21 — STABLE v36 "Sovereign Audit (Economy)"

### Added
- **Economy Integration**: Deployed $INTR token whitepaper, dynamic staking specifications, daily activity incentives, and daily rewards formula logic.

## [0.12.0] - 2026-06-21 — STABLE v35 "Sovereign Audit"

### Added
- **$INTR Token Whitepaper**: Full economic blueprint (`Docs/INTROVERT_TOKEN_WHITEPAPER.md`) with 50/20/10/5/15 allocation matrix, 10-year emission schedule, 4-tier ownership system, gasless flow, and developer launch strategy
- **Daily Rewards System**: New `src/economy/daily_rewards.rs` module tracking 9 activity types with configurable weights, daily caps, anti-gaming measures, and dynamic pool-clearing formula. Integrated with existing RewardTracker and Solana claim flow
- **Economy Blueprint v5.0**: Updated `Docs/INTROVERT_ECONOMY_BLUEPRINT.md` with full allocation matrix, emission schedule, RBN staking parameters, and 4-tier token ownership
- **Universal Search across all tabs**: Chats tab search bar (filters by alias/peerId/handle/globalName/lastMessage), Notes/Drive tabs show "X results" indicator
- **Semantic Search tile in CLAW tab**: Combines exact substring matching with Intro-Claw `processAssistantQuery()` semantic search, deduplicates overlapping results
- **In-chat search**: Search within a conversation via 3-dots menu with 300ms debounce
- **Media, Links & Docs viewer**: 3-dots menu option showing all shared content categorized into Media/Links/Docs tabs
- **Elevated Messages**: Bookmark messages via long-press → "Elevate", view in dedicated tab, long-press to unelevate. Persists across sessions in `elevated_messages` SQLite table
- **INTR balance in header**: Live $INTR balance display with accent glow effects, updated via economy stream
- **Network status indicator**: Minimal dot + status text in header, tapping opens Network Tune/Heal bottom sheet
- **`getLastMessage` / `getLastGroupMessage` FFI**: Optimized LIMIT 1 queries for chat list previews (replaced O(N*M) pattern)
- **Avatar decode cache**: `_avatarCache` Map with LRU eviction (100 entries max)
- **Wormhole 4-word codes**: Increased from 2 to 4 words (~52 bits entropy)

### Changed
- **Sovereign P2P Architecture docs**: All project documentation updated to reflect autonomous, crowdsourced, self-healing mesh with Solana-based dynamic RBN discovery
- **Master Plan**: Vision reframed around dynamic blockchain bootstrapping, token gating, PDA escrow vault, Squads V4 governance
- **Gossipsub security**: Sender membership verification before processing messages, max_transmit_size 1MB, heartbeat 10s→30s
- **PoW difficulty**: Increased from 4 to 6 hex chars (24-bit) with ±5 minute timestamp validation
- **Request-Response codec**: Reduced from 10MB to 2MB max payload
- **Relay server limits**: Reduced from 1GB/1h/8192/4096 to 100MB/30min/256/100
- **Tunnel server**: Binds to 127.0.0.1 only (was 0.0.0.0)
- **Group secret**: Removed from GroupManifest wire format — only delivered via ECDH-wrapped GroupInvite
- **fetch_balance**: Replaced `getProgramAccounts` with lightweight `getAccountInfo` using derived ATA address
- **reqwest::Client**: Reused in SolanaIncentiveEngine instead of creating per-request
- **bootstrap_nodes**: Iterated by reference instead of cloning Vec
- **get_profile()**: Called once per event handler instead of triple
- **HashMap eviction**: Uses `indexmap::IndexMap` with `shift_remove` for true FIFO ordering on bounded buffers
- **INTROVERT_TRUST_ALL_WITNESSES**: Gated behind `#[cfg(debug_assertions)]` to prevent production use

### Fixed
- **54 audit issues resolved across 3 rounds** (24 Critical, 19 Medium, 11 Low)
- **FFI memory leaks**: 6 methods missing `_freeBinary`, `_handleFfiResult` success path, ~10 error-path leaks, pollPeerProfile/syncChatMessages Arena conversion
- **Null pointer checks**: Added to `intro_claw_process_query`, `intro_claw_heal_peer`, `intro_claw_voip_start_call`, `intro_claw_voip_record_sample`
- **`std::thread::sleep` in async**: Replaced with `tokio::time::sleep` at 3 locations
- **DuplicateSuppressor**: O(n) Vec → O(1) HashSet+VecDeque
- **GroupChatScreen**: Added `_messageController` and `_scrollController` dispose, `_displayMessages` version caching
- **`_applySearchFilter`**: Moved inside `setState` block
- **Dialog controller leaks**: `_showInChatSearch` and `_showMasterSearch` set `barrierDismissible: false`
- **`setState` after `await`**: Added `mounted` check in `_sendMessage`
- **Error swallowing**: Replaced `let _ =` with `tracing::error!` in economy module
- **GroupChatSync authorization**: Verifies sender is group member or known contact
- **FileChunkRequest authorization**: Verifies contact for Sovereign Drive fallback AND active seeder path
- **Bounded buffers**: early_chunks (100/1000/50MB), pending_messages (50/peer), incoming_transfers (50), resolved_group_codes (500), active_providers (1000), pending_claims (1000)
- **`get_storage_usage`**: Removed unreliable `fs::metadata("/")` call
- **N+1 contact query**: Pre-fetch contacts into HashMap for group messages
- **Drive file existence**: Moved to async helper to avoid UI thread blocking
- **In-chat search**: Added 300ms debounce timer
- **ClawTerminalDialog**: Cursor animation stops when final report is shown

### Security
- Gossipsub sender membership verification (non-members rejected)
- Group secret removed from plaintext wire format
- ChatSyncResponse sender authorization
- FileChunkRequest contact/group membership verification
- PoW 24-bit difficulty with timestamp staleness check
- Tunnel server localhost-only binding
- All bounded buffers with FIFO eviction (IndexMap)
- Request-Response 2MB limit
- Relay 100MB/circuit limit
- `INTROVERT_TRUST_ALL_WITNESSES` debug-only

## [0.11.0] - 2026-06-20 — STABLE v34 "Iron Claw"

### Changed
- **Master Plan overhaul:** Vision reframed around autonomous, crowdsourced, self-healing mesh with dynamic Solana-based infrastructure coordination
- **Architecture Blueprint:** Replaced hardcoded bootstrap nodes with dynamic blockchain bootstrapping via `introvert-registry` Solana program. Added Token Gating Engine, Unified Escrow PDA Vault, and Token Sink Mechanics sections. Removed detailed UI/storage/economy/automation layer sections (moved to Module Reference)
- **Economy Blueprint:** Streamlined to core token specs and self-sustaining utility loops. Removed peer lifecycle phases, prestige plane, and staking module (pending finalization). Added Squads V4 multisig address
- **Networking & Signaling:** Replaced peer discovery/signaling/file transfer sections with Dynamic Blockchain Bootstrapping and Financial Shielding Against Sybil Floods sections
- **Protocol Specification:** Replaced messaging/file/mailbox/group/media lifecycles with Decentralized RBN Infrastructure Lifecycle (on-chain init, dynamic directory, work verification, governance-gated upgrades)
- **Security & Encryption:** Replaced E2EE/storage/mailbox/FFI sections with Autonomous Infrastructure Safeguards (PDA isolation, Squads V4 governance, time-locked unstaking)
- **README.md:** Updated intro, core features, and tech stack to reflect sovereign P2P architecture with Solana-based dynamic bootstrapping, PDA vault, and token gating
- **Deployment Architecture:** RBN deployment now requires 50,000 $INTR stake in PDA escrow. Added on-chain registration and governance sections
- **Rebuild Guide:** RBN setup now requires $INTR tokens and on-chain registration instead of hardcoded IP lists
- **Configuration Reference:** Bootstrap nodes section updated to reflect dynamic Solana-based discovery with legacy fallback
- **Module Reference:** `network/config.rs` noted as legacy fallback; dynamic discovery is primary
- **Contributing:** Added architecture reading list for new contributors

### Architecture Decisions
- Bootstrap nodes are discovered dynamically from Solana on-chain registry at app startup
- Hardcoded IP arrays in `network/config.rs` serve as fallback only when Solana RPC is unreachable
- RBN operators must bond 50,000 $INTR into PDA escrow with 7-day unbonding cooldown
- Edge nodes require 500 $INTR minimum for active relay routing (Event Code 22)
- Contract upgrades controlled by Squads V4 3-of-5 Multisig — no single developer override

## [0.11.0] - 2026-06-20 — STABLE v34 "Iron Claw"

### Added
- 10 new Intro-Claw intelligence modules: offline queue, dead letter detection, peer reconnection scoring, bandwidth-aware transfer, group sync optimization, connection pre-warming, storage-aware caching, night maintenance window, VoIP call quality monitor, pre-call network check
- VoIP Intro-Claw integration: call quality tracking (RTT, loss, jitter, bitrate), activity log entries, adaptive bitrate detection, pre-call network check, call history analytics
- Real network recon implementation: live ReconContext from swarm state, peer routing table, connection analysis, storage metrics, security audit
- Real network heal implementation: multi-strategy execution (direct dial → relay → anchor routing), detailed heal reports
- Network change detection: connectivity_plus listener, auto-recon on WiFi↔Cellular↔None transitions
- Auto-recon on chat start for 1:1 and group chats
- CLAW tab live tile values: Engine shows Active/Inactive, Storage shows MB, Battery shows status, Bandwidth shows quality
- CLAW tab activity log: real-time view of all Intro-Claw operations, LOG toggle in header
- 17 MODULES info button in settings with all module descriptions and active/inactive explanations
- Anchor Mode INFO button in settings with full explanation of relay, DHT, mailbox, group storage functions
- CLAW tab result items are tappable: contacts open chat, files open, groups open group chat
- VoIP FFI functions: voip_start_call, voip_end_call, voip_record_sample, voip_get_quality
- VoIP NetworkCommand variants for all VoIP operations
- Idle mode: FCM replaces background mailbox polling and heartbeat when app is idle
- Anchor mode battery protection: auto-disable at 30% battery with activity log warning
- Anchor nodes keep 30s heartbeat (regular devices use 5 min with FCM)

### Changed
- Hybrid AI mode completely removed — Intro-Claw now 100% local, sandboxed, zero external calls
- process_assistant_query() simplified to local-only (2 parameters, was 5)
- intro_claw_get_status() returns { "is_active": bool, "mode": "local" }
- CLAW tab description updated to remove external LLM reference
- Settings Intro-Claw section simplified — removed hybrid toggle, endpoint, API key fields
- GlassmorphicContainer now uses two-layer approach: overlay + accent tint
- Network intervals optimized for FCM: heartbeat 30s→300s, republication 60s→300s, mailbox 120s→300s
- Intro-Claw tick runs idle-only maintenance when backgrounded (DB pruning, dead letters, offline queue only)
- Drive tab refresh reduced from 30s to 120s
- Background sync service disabled polling — FCM handles all wake-ups

### Removed
- llm_query() async function and all LLM integration code
- intro_claw_get_endpoint(), intro_claw_set_endpoint() FFI functions
- Hybrid mode toggle, endpoint URL field, API key field from settings UI
- _isHybridMode variable and _loadAiMode() from assistant tab
- _setIntroClawMode(), _saveIntroClawApiKey() methods from settings
- Background mailbox polling timer (FCM replaces it)

### Architecture Decisions
- DO NOT TOUCH: Direct P2P 1:1 file transfer pipeline is locked. Intro-Claw must not modify, intercept, or throttle direct 1:1 transfers.
- FCM push replaces all idle polling — devices sleep when backgrounded, wake on push
- Anchor nodes are exempt from idle mode — they maintain full mesh presence
- Battery <30% auto-disables anchor mode to protect device

## [0.10.0] - 2026-06-20 — STABLE v33 "Sovereign Palette"

### Added
- 5 new image themes: Canyon, Desert, Winter Wonderland, Morning Dew, Golden Hour, Azure Sky, Cyber City II, Cyber City III
- GlassmorphicContainer overlay layer — theme-aware (black 30% for dark, white 30% for light) for legibility
- overlayAlpha parameter for per-widget tint tuning

### Changed
- All 17 themes sorted alphabetically in picker (Introvert Dark stays as default)
- All theme images optimized: PNG→JPEG, 720px width, quality 80 (95% size reduction, 11.3MB→1.5MB)
- All Rust/RBN logging migrated from println!/eprintln! to tracing macros (info!/warn!/error!/debug!)
- Structured logging with tracing-subscriber EnvFilter initialization

### Removed
- Linen Mist, Glacier Bloom, Rose Quartz plain white themes (replaced by image themes)
- macOS Finder (1) duplicate files from project tree
- .stable, .bak, .clean, .tail backup files from project tree
- solana (Copy).rs junk files

### Fixed
- Event type 35/40 verified consistent (35=Handle Resolve Failed, 40=Message Reaction)

### Architecture Decisions
- **DO NOT TOUCH**: Direct P2P 1:1 file transfer pipeline is locked. Intro-Claw must not modify, intercept, or throttle direct 1:1 transfers. Only observation for health scoring is permitted.

## [0.9.0] - 2026-06-20 — STABLE v32 "Sovereign Glass"

### Added
- Glassmorphism UI across all tabs — reusable GlassmorphicContainer widget with BackdropFilter blur
- 5 new image themes: Beach House (light), Cyber City, Mountain Peak, Mountain Ridge, Forest
- SovereignWallpaper supports both asset paths and file paths for built-in and custom themes
- CLAW tab dual-mode: Local mode shows 3x3 query tile grid, Hybrid mode shows chat interface
- CLAW tile results shown in floating overlay dialog (not chat mode)
- Brain logo (psychology icon) above CLAW tile grid
- Network recon/heal terminal overlay with detailed milestones, timestamps, and confirmations
- Network Tune/Heal popup menu replacing NetworkOptimizationButton in top bar
- ZeroClaw attribution in Info & Legal with full MIT/Apache 2.0 license text
- Custom theme editing auto-generates custom name (custom01, custom02) — defaults never overwritten
- Delete button only shown for custom themes
- Notes tab inline header matching Drive tab style (no AppBar)
- Combined header glassmorphic boxes for Notes tab and CLAW tab
- FAB consistent positioning across all tabs (bottom: 80, endFloat)
- All tabs start below Introvert top bar (MediaQuery padding + kToolbarHeight)

### Changed
- AppBar transparent with BackdropFilter(blur: 20) at 60% opacity, extendBodyBehindAppBar: true
- Bottom navigation frosted glass pill with ClipRRect + BackdropFilter(blur: 20)
- Scaffold backgroundColor changed from Colors.transparent to AppTheme.current.bg
- Light themes show clean white bg (no dark default wallpaper)
- Light theme wallpaper opacity set to 0.45 (Linen Mist, Glacier Bloom, Rose Quartz)
- Image themes use wallpaperOpacity: 1.0 (full image, no overlay)
- SovereignEarnings Card color changed to Colors.transparent for glassmorphism
- Drive mesh capacity card wrapped with GlassmorphicContainer
- Settings merged 5 sections into "Introvert Mesh Swarm Settings"
- Settings Fano icon pushed below AppBar (SizedBox height: padding.top + kToolbarHeight + 16)
- CLAW bottom bar buttons use GlassmorphicContainer (no outer box)
- FAB padding increased from 48px to 80px for Android compatibility

### Fixed
- Android ForegroundServiceDidNotStartInTimeException — startForeground() now passes FOREGROUND_SERVICE_TYPE_SPECIAL_USE on API 34
- Light themes showing dark wallpaper bg
- FAB partially covered by navigation bar on Android
- Tab content covered by top bar on all tabs
- Notes tab title styling inconsistent with Drive tab

## [0.8.0] - 2026-06-19 — STABLE v31 "Intelligent Mesh"

### Added
- Direct P2P file transfer speed restored to 70+ Mbps (removed app-level Noise double-encryption on FileChunk)
- File transfer notification suppressed during download — image appears instantly when verified
- Early chunk buffering to fix race condition where chunks arrive before manifest
- Custom theme wallpaper support (image picker, auto-resize to 720px, JPEG quality 80)
- Wallpaper opacity slider (0-100%) in custom theme creator
- Voice memo recording (AAC-LC, temp file path with timestamp)
- Forward message option in chat and group chat context menus
- Reply Privately in group chats (only for 1:1 connected contacts)
- Reply Privately uses custom user-arrow icon and includes quoted original message
- Mesh-aware status labels ("pulling from mesh", "pushing to mesh", "tap to pull from mesh")
- File transfer bubbles use compact placeholder during download (100px)

### Changed
- Voice memo plugin uses auto-generated temp path (fixes Android MediaMuxer crash)
- Voice memos use DeviceFileSource instead of UrlSource (fixes macOS AVPlayer playback)
- File transfer timestamps fall back to current time when start_time_ms is 0 or null
- SovereignWallpaper converted to StatefulWidget (listens to AppTheme ChangeNotifier)
- ThemeConfig now includes wallpaperPath and wallpaperOpacity fields (backward compatible)
- Default is_relayed set to false for direct P2P transfers
- Initial push delay reduced from 500ms to 200ms for direct P2P
- Custom theme name auto-filled with custom01, custom02, etc.
- Custom theme name validation shows error when empty
- In-flight limit restored to 8 for direct P2P (was 16, caused relay flooding)

### Fixed
- Voice memo crash on Android (empty string passed to MediaMuxer)
- Voice memo playback failure on macOS (UrlSource to DeviceFileSource)
- Date separator showing 1 Jan 1970 for file transfers with null/zero timestamps
- Custom theme wallpaper not displaying (SovereignWallpaper not listening to theme changes)
- Edit theme flow now always applies the saved theme
- Sender "waiting for recipient" not updating after ACK (ValueKey forces rebuild)
- Receiver thumbnail not showing for verified transfers (ValueKey forces rebuild)
- File transfer notification appearing before download completes
- Group chat file transfer race condition (chunks arriving before manifest)

### Dependencies Added
- flutter_svg: ^2.0.10 (custom Reply Privately icon)

## [0.7.1] - 2026-06-19 — Intro-Claw (Phase 7)

### Added
- **Intro-Claw core automation engine** — 12 modules running on a 5-minute tick loop: battery throttling, DB pruning, media cleanup, connection optimization, message batching, predictive prefetch, sync prioritization, duplicate suppression, health scoring, storage quota, adaptive chunking, tick integration
- **Local assistant UI** — Chat-style interface in the CLAW tab with natural language queries for messages, files, contacts, notes, calls, and storage status; generic type queries (e.g., "show my photos") list all items of that type
- **Semantic intent engine** — BERT-based embedding model (all-MiniLM-L6-v2 via candle-core, 384-dim embeddings) with 12 action intents using cosine similarity matching; keyword fallback when model not loaded
- **Network recon & self-healing** — Diagnostic reports (mesh overview, peer routing, connection analysis, anchors, security audit) and 5-strategy connection recovery (direct dial, relay circuit, anchor routing, WebSocket tunnel, mailbox fallback)
- **Hybrid AI mode** — OpenAI-compatible endpoint integration with encrypted API key storage (SQLCipher) and LLM v1 API with local search context
- **FCM push notifications** — Firebase Admin SDK on RBN with direct FCM v1 API calls (no third-party bridge)
- **Intro-Claw security sandbox** — Zero access to master keys, message content, or session blobs; network isolation in Offline mode
- **FFI functions** — intro_claw_get_ai_mode, intro_claw_set_ai_mode, intro_claw_trigger_tick, intro_claw_set_active, intro_claw_get_status, intro_claw_get_endpoint, intro_claw_set_endpoint, intro_claw_process_query, intro_claw_run_network_recon, intro_claw_heal_peer
- **Embedding engine tests** — 16/16 passing

### Changed
- `economy_meta` table now stores intro_claw_active, intro_claw_ai_mode, intro_claw_api_key, intro_claw_endpoint config keys
- RBN FCM config path: `/opt/introvert/config/firebase-service-account.json`

## [0.6.0] - 2026-06-18

### Added
- Voice memo recording (AAC-LC, temp file path with timestamp)
- Custom theme wallpaper support (image picker, auto-resize to 720px, JPEG quality 80)
- Wallpaper opacity slider (0-100%) in custom theme creator
- Forward message option in chat and group chat context menus
- Reply Privately in group chats (only shown for 1:1 connected contacts)
- Reply Privately uses custom user-arrow icon from The Noun Project
- Reply Privately includes quoted original message in 1:1 chat

### Changed
- Voice memo plugin now uses auto-generated temp path (fixes Android MediaMuxer crash)
- Voice memos use DeviceFileSource instead of UrlSource (fixes macOS AVPlayer playback)
- File transfer timestamps fall back to current time when start_time_ms is 0 or null (fixes 1 Jan 1970 date separator)
- SovereignWallpaper converted from StatelessWidget to StatefulWidget (listens to AppTheme ChangeNotifier)
- ThemeConfig now includes wallpaperPath and wallpaperOpacity fields (backward compatible)

### Fixed
- Voice memo crash on Android (empty string passed to MediaMuxer)
- Voice memo playback failure on macOS (UrlSource to DeviceFileSource)
- Date separator showing 1 Jan 1970 for file transfers with null/zero timestamps
- Custom theme wallpaper not displaying (SovereignWallpaper not listening to theme changes)
- Edit theme flow now always applies the saved theme

### Dependencies Added
- flutter_svg: ^2.0.10 (for custom Reply Privately icon)

## [0.4.0] - 2026-06-16

### Added
- Typing indicator (TypingStart/TypingStop signaling, Event 39)
- Last seen status (Heartbeat every 30s, stored in contacts table)
- Message search (SQL LIKE query for 1:1 and group chats)
- Call history (DB table, 3 FFI functions, auto-logged on call end)
- Call history logging on call end in CallScreen
- BackgroundSyncService with 5-min Timer.periodic fallback

### Changed
- TypingStart, TypingStop, Heartbeat payloads added to SignalingPayload enum
- contacts table: added `last_seen INTEGER` column
- call_history table: peer_id, call_type, media_type, duration, is_incoming, timestamp
- FFI functions: introvert_send_typing_start/stop, introvert_get_last_seen, introvert_search_messages, introvert_search_group_messages, introvert_call_history_log/get/count
- Package name: chat.introvert.app
- Firebase project: introvert-p2p (Sender ID: 302706705869)

### Fixed
- WorkManager incompatibility removed (uses Flutter v1 embedding APIs)
- Fallback to Timer.periodic(5min) for background sync

## [0.3.0] - 2026-06-16

### Added
- WebSocket tunnel for NAT traversal
- Handle registry with PoW consensus
- Group muting/unmuting functionality
- Message editing and deletion
- Connection diagnostics overlay
- Video player with controls
- Location picker with map

### Changed
- Upgraded libp2p to v0.56
- Migrated to Solana SDK 4.0
- Improved direct connection auto-upgrade
- Optimized DHT replication factor (20 → 5)

### Fixed
- FFI memory leaks in file transfer
- iOS sandbox path resolution
- Relay connection overwrite issue
- Group action signature verification

## [0.1.0] - 2026-06-08

### Added
- Core engine with FFI bridge (50+ functions)
- 1-on-1 encrypted messaging
- File transfer with chunking
- Sovereign Drive encrypted storage
- Gossipsub group mesh
- WebRTC voice/video calls
- Magic Wormhole onboarding
- Solana $INTR token economy
- SQLCipher encrypted database (18 tables)
- Dark mesh isolation
- Port 443 strategy
- Multi-platform support (Android, iOS, macOS, Linux)

### Security
- Noise IK transport encryption
- HKDF-SHA256 identity derivation
- AES-256-CBC database encryption
- Zero-knowledge mailbox

## [0.0.1] - 2026-05-01

### Added
- Initial project structure
- Basic libp2p integration
- Simple messaging prototype
- FFI bridge foundation

---

## Version History

| Version | Date | Codename | Key Features |
|---------|------|----------|--------------|
| 0.11.0 | 2026-06-20 | Iron Claw | Local-only Intro-Claw, 10 intelligence modules, VoIP monitoring, real recon/heal, network change detection |
| 0.10.0 | 2026-06-20 | Sovereign Palette | 17 themes, glassmorphism overlay, tracing logging, theme optimization |
| 0.9.0 | 2026-06-20 | Sovereign Glass | Glassmorphism UI, 5 image themes, CLAW redesign, terminal overlay, Android 14 fix |
| 0.8.0 | 2026-06-19 | Intelligent Mesh | Intro-Claw AI engine, local assistant, BERT semantic search, FCM push |
| 0.7.1 | 2026-06-19 | Intro-Claw | Automation engine, local assistant, semantic AI, FCM push |
| 0.7.0 | 2026-06-18 | Sovereign Velocity | 70+ Mbps P2P, silent download, custom wallpapers |
| 0.6.0 | 2026-06-18 | Sovereign Velocity | Voice memos, themes, forward, reply privately |
| 0.1.0 | 2026-06-08 | Sovereign | Full P2P mesh with economy |
| 0.0.1 | 2026-05-01 | Genesis | Initial prototype |

## Release Process

1. Update version in `pubspec.yaml` and `Cargo.toml`
2. Update `CHANGELOG.md` with new changes
3. Create git tag: `git tag -a v0.1.0 -m "Release 0.1.0"`
4. Build platform binaries:
   ```bash
   make mac
   make android
   make ios
   ```
5. Test on all platforms
6. Push tag: `git push origin v0.1.0`
7. Create GitHub release with binaries
