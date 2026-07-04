# Introvert v0.7.0 — MAJOR STABLE RELEASE

**Release Date:** 2026-06-18  
**Codename:** "Sovereign Velocity"

---

## 1. Release Summary

This is a **major stable release** focused on three critical areas:
1. **Direct P2P file transfer performance** — restored to 70+ Mbps
2. **File transfer UX** — silent download with instant reveal
3. **Custom theme wallpapers** — full wallpaper support with opacity control

---

## 2. Key Changes

### Direct P2P File Transfer (70+ Mbps Restored)

| Change | Before | After |
|--------|--------|-------|
| App-level Noise on FileChunk | Enabled (~83% wire overhead) | Disabled (libp2p transport encrypts) |
| Default is_relayed for sender | true (forced relay) | false (direct P2P) |
| Initial push delay | 500ms | 200ms |
| In-flight limit (direct) | 16 (caused flooding) | 8 (balanced) |
| Chunk arrival race condition | Chunks discarded before manifest | Early chunk buffering |

### File Transfer UX (Silent Download → Instant Reveal)

| State | Before | After |
|-------|--------|-------|
| Manifest arrival | Event 12 dispatched + stored in DB | Suppressed (silent) |
| During download | "pulling from mesh" + progress bar | Nothing shown |
| After verification | Image appears | Image appears instantly |
| Sender status | "waiting for recipient" persists | Updates to "verified" |
| Placeholder | 100px box with file extension | Compact 40px loading bar |

### Custom Theme Wallpapers

| Feature | Details |
|---------|---------|
| Image picker | Gallery selection with auto-resize |
| Resize | 720px wide, JPEG quality 80 |
| Opacity | 0-100% slider |
| Display | BoxFit.cover, Alignment.topCenter |
| Storage | App documents directory |
| Name auto-fill | custom01, custom02, etc. |

### Other Features
- Voice memo recording (AAC-LC, 44.1kHz, 128kbps)
- Forward message in chat/group chat context menus
- Reply Privately in group chats (1:1 connected contacts only)
- Custom user-arrow icon for Reply Privately
- Quoted original message in Reply Privately

---

## 6. Intro-Claw (Phase 7)

Intro-Claw is the core automation and local assistant engine for Introvert.

### Core Automation Engine
12 modules running on a 5-minute tick loop:

| Module | Purpose |
|--------|---------|
| Battery throttling | Reduces activity when battery is low |
| DB pruning | Cleans expired data from SQLCipher |
| Media cleanup | Removes orphaned media files |
| Connection optimization | Evaluates and upgrades peer connections |
| Message batching | Batches outgoing messages for efficiency |
| Predictive prefetch | Pre-fetches likely-needed data |
| Sync prioritization | Orders sync operations by urgency |
| Duplicate suppression | Prevents duplicate message/file processing |
| Health scoring | Computes mesh and peer health metrics |
| Storage quota | Enforces storage limits |
| Adaptive chunking | Adjusts file chunk sizes dynamically |
| Tick integration | Coordinates all modules on the timer |

### Local Assistant (CLAW Tab)
- Chat-style UI for natural language queries
- Queries: messages, files, contacts, notes, calls, storage status
- Generic type queries ("show my photos") list all items of that type

### Semantic Intent Engine
- Model: `all-MiniLM-L6-v2` (BERT-based, via candle-core)
- Embedding dimension: 384
- 12 action intents with cosine similarity matching
- Keyword fallback when model is not loaded

### Network Recon & Self-Healing
- Diagnostic reports: mesh overview, peer routing, connection analysis, anchors, security audit
- 5-strategy connection recovery: direct dial, relay circuit, anchor routing, WebSocket tunnel, mailbox fallback

### Hybrid AI Mode
- OpenAI-compatible endpoint integration
- API key encrypted via SQLCipher
- LLM v1 API with local search context

### FCM Push Notifications
- Firebase Admin SDK on RBN
- Direct FCM v1 API calls (no third-party bridge)
- Config path: `/opt/introvert/config/firebase-service-account.json`

### Key Files
- `src/intro_claw.rs` — 1500+ lines, core automation engine
- `src/embedding.rs` — 300+ lines, semantic intent engine
- `lib/src/ui/assistant_tab.dart` — 800+ lines, CLAW tab UI
- `for_linux/src/fcm.rs` — 230+ lines, RBN FCM integration

---

## 3. File Manifest

### New Files
- `assets/images/reply_privately.svg` — Custom Reply Privately icon

### Modified Files
- `src/network/mod.rs` — Noise removal, default is_relayed, early chunk buffer, push delay, Event 12 suppression
- `lib/src/ui/widgets/file_transfer_bubble.dart` — Silent download UX, compact placeholder, status labels
- `lib/views/chat_screen.dart` — Voice memo, Forward, ValueKey for rebuilds
- `lib/views/group_chat_screen.dart` — Reply Privately, Forward, ValueKey for rebuilds
- `lib/views/chat_features.dart` — DeviceFileSource for voice memo playback
- `lib/src/ui/custom_theme_creator.dart` — Wallpaper picker, resize, opacity, name auto-fill
- `lib/src/ui/main_shell.dart` — Edit theme always applies
- `lib/src/native/introvert_client.dart` — Timestamp guard
- `lib/blueprint_ui.dart` — SovereignWallpaper as StatefulWidget
- `lib/theme/app_theme.dart` — wallpaperPath, wallpaperOpacity fields
- `pubspec.yaml` — Version 0.7.0, flutter_svg dependency
- `Cargo.toml` — Version 0.7.0

---

## 4. Build from Scratch

### Prerequisites
- Flutter SDK >= 3.3.0
- Rust toolchain (stable)
- Android SDK (API 33+)
- Xcode (for iOS/macOS)
- cargo-zigbuild (for cross-compilation)

### Android Build
```bash
flutter build apk --release
```

### macOS Build
```bash
flutter build macos --release
```

### RBN Binary
```bash
./deploy_local_rbn.sh
```

---

## 5. Architecture Reference

### File Transfer Protocol
- **Direct P2P**: Sender pushes 256KB chunks at 20ms pacing over libp2p request-response
- **Relay**: Receiver-driven pipelined pull with 64KB chunks
- **Chunk sizes**: 256KB direct, 64KB relayed
- **Verification**: SHA-256 hash comparison after reassembly

### Event System
- Event 12: File transfer progress
- Event 25: Group sync payloads
- Event 39: Typing indicators

### Database
- SQLite via Rust FFI
- Tables: messages, contacts, groups, group_members, call_history, drive_files

### Network Stack
- libp2p with Kademlia DHT, GossipSub mesh, Noise encryption
- QUIC/TCP transports, mDNS local discovery
- WebSocket tunneling, relay circuit v2
