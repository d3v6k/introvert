# Release Notes — Introvert v31 (0.8.0) "Intelligent Mesh"

**Date:** June 19, 2026
**Codename:** Intelligent Mesh

---

## What's New Since v30 (0.7.0 "Sovereign Velocity")

### Intro-Claw AI Engine — The Local Intelligence Layer

The headline feature of v31. Intro-Claw is Introvert's on-device automation engine, AI assistant, network healer, and semantic search system — all running locally with zero cloud dependency in Offline mode.

#### 1. Core Automation Engine
12 background maintenance modules running on a 5-minute tick loop:
- **Battery Throttling** — Scales sync frequency, heartbeat, and max connections based on battery level (Low=20%, Critical=10%)
- **Database Pruning** — Removes expired sessions (>24h), crypto sessions (>7d), mesh chunks (>7d). Hourly `PRAGMA optimize`
- **Media Cleanup** — Orphaned mesh chunk removal, storage quota management (80% warning, 90% critical)
- **Connection Optimization** — mDNS peer discovery for direct P2P upgrades
- **Message Batching** — Queues outgoing messages during poor connectivity, auto-flushes when conditions improve
- **Predictive Prefetching** — Scans top contacts' recent file references, schedules pulls for missing files
- **Sync Prioritization** — Sorts contacts by unread count, syncs top 3 first
- **Duplicate Suppression** — 10k capacity FIFO eviction, checks on every message write
- **Connection Health Scoring** — Decay-based scoring (0.9 decay, 0.1 boost) per peer
- **Storage Quota** — Auto-prune at 80%, aggressive at 90%
- **Adaptive Chunk Sizing** — Tracks throughput per peer, adjusts 64KB–512KB chunks
- **Tick Integration** — 5-minute interval via `NetworkCommand::IntroClawTick`

#### 2. Local Assistant (CLAW Tab)
Chat-style interface for querying app data using natural language:
- Search messages, files, contacts, notes, and call history
- Generic type queries ("show my photos", "show my contacts") list all items of that type
- Storage status and engine health reporting
- Works 100% offline — no LLM required
- Suggestion chips for quick access

#### 3. Semantic Intent Engine (BERT Embeddings)
Local text understanding via `candle-core`:
- Model: `all-MiniLM-L6-v2` (~23MB, downloaded on first run)
- 384-dimensional embeddings with cosine similarity matching
- 12 automation action intents with pre-computed phrase vectors
- Keyword-based fallback when model isn't loaded
- Thread priority set to `nice(10)` — never blocks UI
- 16/16 integration tests passing

#### 4. Network Recon & Healing
Diagnostic and self-healing system:
- **Recon Reports**: Mesh overview, storage usage, peer routing table, connection analysis, upgrade candidates, anchor status, security audit — formatted as monospaced markdown
- **5-Strategy Healing**: Direct dial → Relay circuit v2 → Anchor routing → WebSocket tunnel → Persistent mailbox fallback
- **Terminal-style UI**: Green-on-black milestone animation during recon/heal

#### 5. Hybrid AI Mode (Optional)
External LLM integration for advanced queries:
- OpenAI-compatible endpoint (works with OpenAI, Anthropic, Mistral, Ollama)
- API key encrypted via SQLCipher
- LLM receives local search results as context
- Graceful fallback to keyword matching when LLM unavailable

#### 6. FCM Push Notifications (RBN)
Direct Firebase Cloud Messaging on the RBN backbone:
- Firebase Admin SDK integrated in `for_linux/` RBN binary
- FCM v1 API with JWT OAuth2 authentication
- 55-minute token caching with auto-refresh
- No third-party push bridge needed
- Deployed via `deploy_rbn.sh` with service account JSON

### Bug Fixes

- **Engine toggle persistence** — `intro_claw_set_active` now persists to `economy_meta` table AND sends `NetworkCommand::IntroClawSetActive` to toggle the in-memory flag
- **File thumbnail not showing** — Sovereign Drive fallback in `_loadThumbnail` resolves stale file paths
- **Hero tag conflicts** — Unique `heroTag` on all `FloatingActionButton`s (notes, drive)
- **Date showing "01 Jan 1970"** — `startDateTime` getter guards epoch timestamps (startTimeMs <= threshold → DateTime.now())
- **File forwarding sends filename only** — Resolves actual file path via `_extractLocalPathFromProgress()` with drive fallback
- **Notification sound not playing on macOS** — Dart-side `AudioPlayer` for notification sounds (native MethodChannel has no macOS implementation)
- **Android `libc++_shared.so` not found** — Added NDK library to `jniLibs/` for candle-core BERT dependencies
- **Generic type queries returning 0 results** — "show my photos" now lists ALL image files instead of searching for the word "photos" in filenames

### UI Improvements

- **Clean image/video thumbnails** — Verified media shows just the image with rounded corners (no accent border frame)
- **Caption on attachment** — WhatsApp-style caption dialog after picking files (single image/video/file)
- **CLAW tab** — New 5th navigation tab with chat interface, RECON button, HEAL button, info panel
- **Settings UI** — Endpoint URL + API key fields for Hybrid mode, engine status dashboard

### Architecture

- **5th Component Layer**: Automation Layer (Intro-Claw) added to architecture alongside Core Engine, UI, Storage, and Economy
- **Rust dependencies**: `candle-core`, `candle-transformers`, `candle-nn` (0.8), `tokenizers` (0.21), `hf-hub` (0.4), `jsonwebtoken` (9)
- **New files**: `src/intro_claw.rs`, `src/embedding.rs`, `lib/src/ui/assistant_tab.dart`, `for_linux/src/fcm.rs`
- **New FFI functions**: `intro_claw_get_endpoint`, `intro_claw_set_endpoint`, `intro_claw_process_query`, `intro_claw_run_network_recon`, `intro_claw_heal_peer`

### Documentation Updates

20 documentation files updated to include Intro-Claw as a key architectural element:
- Root README, Master Plan, Architecture Blueprint, Module Reference, FFI API Reference
- Configuration Reference, Database Schema, Security, Testing, Troubleshooting
- Networking, Deployment, Build Guide, Environment Variables, UI Manifest
- Changelog, Release Notes, Push Notification Architecture, Marketing Report

---

## Key Features at v31

| Feature | Status | Description |
|---------|--------|-------------|
| P2P Mesh | ✅ Stable | libp2p v0.56, Port 443, QUIC/UDP |
| E2EE Messaging | ✅ Stable | Noise IK, SQLCipher, zero-knowledge mailbox |
| File Transfer | ✅ Stable | 70+ Mbps direct P2P, relay fallback |
| Group Mesh | ✅ Stable | Gossipsub, full-mesh calls |
| Sovereign Drive | ✅ Stable | Content-addressed storage, auto-organization |
| Voice/Video Calls | ✅ Stable | WebRTC, adaptive quality |
| Economy | ✅ Stable | $INTR Solana token |
| **Intro-Claw Engine** | ✅ NEW | 12 automation modules, 5-min tick |
| **Local Assistant** | ✅ NEW | Natural language search, suggestion chips |
| **Semantic Search** | ✅ NEW | BERT embeddings, cosine similarity |
| **Network Recon/Heal** | ✅ NEW | 5-strategy connection recovery |
| **Hybrid AI Mode** | ✅ NEW | OpenAI-compatible endpoint |
| **FCM Push (RBN)** | ✅ NEW | Direct Firebase integration |

---

## Build Instructions

### Prerequisites
- Rust 1.75+
- Flutter 3.24+
- Android NDK 28.x
- Xcode 15+ (iOS/macOS)

### macOS
```bash
make mac
flutter build macos
cp macos/Flutter/ephemeral/libintrovert.dylib build/macos/Build/Products/Release/introvert_tests.app/Contents/Frameworks/
open build/macos/Build/Products/Release/introvert_tests.app
```

### Android
```bash
flutter build apk
# Install libc++_shared.so if not bundled (candle-core dependency)
# See Docs/BUILD_&_DEPLOYMENT_GUIDE.md
```

### RBN Deployment
```bash
./deploy_rbn.sh
# Requires: Firebase service account at /opt/introvert/config/firebase-service-account.json
```

---

## Known Issues

- iOS release blocked by Apple Developer Account
- Android build warning about Kotlin Gradle Plugin migration
- `flutter_webrtc` plugin doesn't support Swift Package Manager yet

---

**Own your words. Own your network. Own your future.**
