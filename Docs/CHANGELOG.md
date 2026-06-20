# Changelog

All notable changes to Introvert will be documented in this file.

## [0.7.0] - 2026-06-18 — MAJOR STABLE RELEASE

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
