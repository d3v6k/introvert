# Deep Audit Report: Current vs stable_v31 Regression Analysis

**Date:** June 20, 2026
**Baseline:** `/Volumes/512-SSD-External/introvert back up/stable_v31/` (v0.8.0 "Intelligent Mesh")
**Current:** `/Users/dev/Development/introvert/` (v0.5.0)
**Auditor:** MiMo Code Agent

---

## Executive Summary

The current codebase has suffered **massive regressions** compared to stable_v31. A MiMo crash and reinstallation appears to have rolled the codebase back to an earlier state (v0.5.0) while some newer files were preserved or partially modified. The Intro-Claw AI engine — the headline feature of v31 — was gutted across both Rust and Flutter layers. Multiple UI features, documentation, and build configurations were also lost.

**Total findings: 42 regressions across 4 audit areas.**

| Severity | Count |
|----------|-------|
| CRITICAL | 9 |
| HIGH | 14 |
| MEDIUM | 11 |
| LOW | 8 |

---

## 🔴 CRITICAL REGRESSIONS (9)

### 1. Intro-Claw Assistant Query Engine Gutted (Rust)
**File:** `src/intro_claw.rs` — ~1391 diff lines
- Entire `AssistantQuery` parser removed (~780 lines): `parse_assistant_query()`, `execute_assistant_query()`, `parse_date_reference()`, `detect_mime_filter()`, `detect_scope()`, `format_size()`, `SearchResult`, `AssistantResponse`, `SearchScope`
- Hybrid LLM integration removed (~80 lines): `llm_query()`, `process_assistant_query()`
- Network Recon engine removed (~120 lines): `ReconPeerInfo`, `ReconContext`, `run_network_recon()` — full markdown report generation
- Network Healer removed (~130 lines): `HealAttempt`, `HealResult`, `build_heal_plan()`, `render_heal_report()` — 5-strategy recovery
- All advanced module logic gutted: `ConnectionOptimizer`, `HealthScorer`, `AdaptiveChunkSizer`, `SyncPrioritizer`, `PredictivePrefetcher`, `MessageBatcher`, `DatabasePruner`, `StorageQuotaManager`, `BatterySaverThrottler`

### 2. Five FFI Functions Removed (Rust)
**File:** `src/lib.rs`
- `intro_claw_get_endpoint()` — get LLM endpoint
- `intro_claw_set_endpoint()` — set LLM endpoint
- `intro_claw_process_query()` — **core assistant query function**
- `intro_claw_run_network_recon()` — network recon
- `intro_claw_heal_peer()` — peer healing

### 3. Five FFI Dart Bindings Removed (Flutter)
**File:** `lib/src/native/introvert_client.dart`
- `getIntroClawEndpoint()` / `setIntroClawEndpoint()`
- `processAssistantQuery()`
- `runNetworkRecon()`
- `healPeer()`

### 4. CLAW Tab Removed from Navigation (Flutter)
**File:** `lib/src/ui/main_shell.dart`
- 5th navigation tab (CLAW with `Icons.psychology_outlined`) removed — only 4 tabs remain
- Intro-Claw settings section removed (`_buildIntroClawSection()` with mode toggle, endpoint, API key, status dashboard)
- Notification sound system removed (`AudioPlayer`, `_playNotificationSound()`)
- Manifesto dialog removed (`_showManifesto()`)
- Custom theme creation UI removed

### 5. Assistant Tab Dual-Mode Architecture Lost (Flutter)
**File:** `lib/src/ui/assistant_tab.dart`
- Offline mode with 9-card quick-access grid removed
- Hybrid mode toggle and AI/LOCAL badge removed
- Result overlay system with navigation to files/contacts/chats removed
- Terminal-style recon/heal overlay removed
- Offline bottom bar (Recon/Heal/About) removed

### 6. Storage Search Functions Removed (Rust)
**File:** `src/storage.rs` (1922 → 1768 lines, -154 lines)
- `search_all_messages()` — full-text search across all 1:1 messages
- `search_all_group_messages()` — full-text search across group messages
- `search_drive_files()` — drive search with MIME filter and date range
- `search_contacts()` — contact search with generic keyword detection
- `search_notes()` — lost generic keyword detection

### 7. Network Recon/Heal Replaced with Stubs (Rust)
**File:** `src/network/mod.rs` (5473 → 4955 lines, -518 lines)
- Full recon with `ReconContext` → replaced with `format!("# Network Recon\n\nConnected peers: {}")`
- 5-strategy sequential recovery → replaced with `self.swarm.dial(peer_id)` one-liner

### 8. FCM Push Notifications Broken (RBN)
**File:** `for_linux/src/lib.rs`
- `pub mod fcm;` declaration missing — FCM module never compiles
- `for_linux/src/network/mod.rs`: FCM direct integration replaced with HTTP POST to `https://push.introvert.network/wakeup`

### 9. README.md Replaced with Flutter Boilerplate
**File:** `README.md`
- Full project description with Intro-Claw, core features, tech stack, build instructions → replaced with generic "A new Flutter project" Flutter template

---

## 🟠 HIGH REGRESSIONS (14)

### 10. Message Selection/Forwarding Lost (Flutter)
**Files:** `lib/views/chat_screen.dart`, `lib/views/group_chat_screen.dart`
- Message selection toolbar (Copy, Reply, Forward, Delete) removed
- Caption dialog for file/image attachments removed
- Note bubble rendering (`[NOTE]:` parsing) removed
- Reply Privately feature removed
- Sorted chronological message insertion removed
- Debounced message loading removed

### 11. Voice Memo Recording Lost in Group Chat (Flutter)
**File:** `lib/views/group_chat_screen.dart`
- `_startRecording()`, `_stopRecordingAndSend()`, `_cancelRecording()`, `_buildRecordingOverlay()` removed

### 12. File Transfer Bubble UX Degraded (Flutter)
**File:** `lib/src/ui/widgets/file_transfer_bubble.dart`
- Sovereign Drive fallback path resolution removed
- Simplified mesh-aware status labels removed
- Clean media display (rounded corners, no frame) removed
- Hidden incoming placeholders removed

### 13. Protocol Incompatibility: Missing Caption Field (RBN)
**File:** `for_linux/src/network/mod.rs`
- `pub caption: Option<String>` field missing from message struct
- Breaks protocol compatibility with v31 peers

### 14. File Transfer Encryption Overhead (RBN)
**File:** `for_linux/src/network/mod.rs`
- v31: FileChunk never uses app-level Noise (libp2p encrypts) → `false`
- Current: FileChunk uses Noise on non-relayed connections → ~83% wire overhead

### 15. Android Manifest Non-Compliant (Android)
**File:** `android/app/src/main/AndroidManifest.xml`
- Missing permissions: CAMERA, LOCATION, VIBRATE, POST_NOTIFICATIONS, READ_MEDIA_IMAGES, FOREGROUND_SERVICE_SPECIAL_USE
- ForegroundService uses `phoneCall` type instead of `specialUse` (Android 14+ non-compliant)

### 16. Android Build Config Regressions
**File:** `android/app/build.gradle.kts`
- google-services plugin in wrong Gradle block
- Firebase hardcoded versions instead of BOM (`firebase-bom:33.7.0`)
- `firebase-analytics` included unnecessarily

### 17. Version Number Downgraded
**File:** `pubspec.yaml`
- Version: `0.5.0` (current) vs `0.7.0` (v31) — significant downgrade

### 18. Architecture Blueprint Stripped (~60%)
**File:** `Docs/ARCHITECTURE_BLUEPRINT.md`
- Lost Intro-Claw section, Sovereign Swarm, network topology diagram, security model, key file locations table

### 19. Module Reference Stripped (~70%)
**File:** `Docs/MODULE_REFERENCE.md`
- Lost file-by-file reference with line counts, function signatures, Intro-Claw engine details

### 20. Intro-Claw Storage Persistence Broken (RBN)
**File:** `for_linux/src/storage.rs`
- Missing `get_intro_claw_ai_mode()`, `set_intro_claw_ai_mode()`, `get_intro_claw_api_key()`

### 21. Reaction Event Type Mismatch
**Files:** `src/network/mod.rs`, `for_linux/src/network/mod.rs`
- v31 uses event type 40 for reactions, current uses event type 35

### 22. Custom Theme System Lost (Flutter)
**File:** `lib/theme/app_theme.dart`
- `ThemeConfig` serialization (`toJson()`, `fromJson()`, `wallpaperPath`, `wallpaperOpacity`) removed
- Custom themes system (`_customThemes`, `saveCustomTheme()`, `deleteCustomTheme()`) removed

### 23. GEMINI.md Stripped (~74%)
**File:** `GEMINI.md`
- Lost file transfer details, messaging mandate, storage schema reference, comprehensive documentation mandate

---

## 🟡 MEDIUM REGRESSIONS (11)

### 24. Custom Wallpaper Support Lost (Flutter)
**File:** `lib/blueprint_ui.dart` — `SovereignWallpaper` lost `StatefulWidget` with `AppTheme` listener, custom wallpaper via `Image.file`

### 25. flutter_svg Dependency Missing
**File:** `pubspec.yaml` — `flutter_svg: ^2.0.10` and `assets/images/reply_privately.svg` missing

### 26. Wormhole Retry Logic Removed
**File:** `src/network/wormhole.rs` — 3-attempt retry in `join_invite()` reduced to single attempt

### 27. Media Reward Tracking Removed
**File:** `src/network/media/mod.rs` — `tracker.record_relay` in data channel `on_message` removed

### 28. FileTransferProgress Fields Missing
**File:** `lib/src/native/introvert_client.dart` — `.caption` field and `.startDateTime` getter (epoch guard) removed

### 29. UI Component Manifest Stripped (~65%)
**File:** `Docs/UI_COMPONENT_MANIFEST.md` — Lost specific file paths, theme table, FFI bridge details

### 30. Build Script Robustness Lost
**File:** `scripts/build_android.sh` — Lost openssl-sys pre-build step, dynamic NDK path detection simplified

### 31. CHANGELOG Missing v0.8.0 Entry
**File:** `Docs/CHANGELOG.md` — Version history table missing 0.8.0 Intelligent Mesh row (now restored)

### 32. Reaction Event Type in Flutter
**File:** `lib/views/chat_screen.dart`, `lib/views/group_chat_screen.dart` — Event type 35 vs v31's 40

### 33. Non-Relayed Push Delay
**File:** `src/network/mod.rs` — 500ms delay vs v31's 200ms

### 34. Onboarding Intro-Claw Config Lost
**File:** `lib/src/ui/onboarding_screen.dart` — Intro-Claw mode selection (Offline/Hybrid) removed from onboarding flow

---

## 🟢 LOW REGRESSIONS (8)

### 35-42. Minor Issues
- FAB hero tags missing (`notes_fab`, `drive_fab`)
- Themed `fillColor` on FABs lost
- Media gallery save-to-downloads feature lost
- Image path support in note sharing lost
- Test handle claim/query tests removed from `ffi_integration.rs`
- Stale backup files in `src/network/` (`mod.rs.clean`, `mod.rs.tail`)
- `intro_claw_set_active()` FFI behavior changed (sends wrong command)
- `intro_claw_get_status()` lost `is_active` field

---

## ✅ WHAT'S NEW IN CURRENT (Not in v31)

| File | Description | Assessment |
|------|-------------|------------|
| `src/network/e2ee.rs` | TreeKEM group E2EE | New feature |
| `src/network/service.rs` | Extracted NetworkService struct | Refactor |
| `src/network/types.rs` | Extracted types/signaling | Refactor |
| `src/network/group.rs` | TreeKEM E2EE group integration | New feature |
| `src/bin/audit_verify.rs` | Audit verification binary | New tool |
| `src/bin/stress_tester.rs` | Stress testing binary | New tool |
| `lib/src/ui/custom_theme_creator.dart` | Custom theme creator dialog | New UI |
| `lib/views/chat_features.dart` | Chat features helper | New UI |
| `lib/views/location_picker_screen.dart` | Location picker | New UI |
| `lib/src/ui/widgets/security_shield.dart` | Security shield widget | New UI |
| `for_linux/src/media/mod.rs` | Media module for RBN | New RBN |
| Structured logging | `println!`/`eprintln!` → `tracing::*` throughout | Improvement |
| Test `NetworkConfig` struct | Tests refactored from positional args | API evolution |

---

## 🔧 RESTORATION PRIORITY

### P0 — Restore Immediately (Core Functionality)
1. Restore `src/intro_claw.rs` from v31 (assistant query engine, recon, healer, all modules)
2. Restore 5 FFI functions in `src/lib.rs`
3. Restore 5 storage search functions in `src/storage.rs`
4. Restore `for_linux/src/lib.rs` `pub mod fcm;` declaration
5. Restore `for_linux/src/network/mod.rs` FCM integration + caption field
6. Restore `lib/src/native/introvert_client.dart` FFI bindings + caption/startDateTime
7. Restore `lib/src/ui/main_shell.dart` CLAW tab + settings + notification sounds
8. Restore `lib/src/ui/assistant_tab.dart` dual-mode architecture

### P1 — Restore Soon (Important Features)
9. Restore `lib/views/chat_screen.dart` message selection, forwarding, captions, note bubbles
10. Restore `lib/views/group_chat_screen.dart` voice memos, selection, reply privately
11. Restore `lib/src/ui/widgets/file_transfer_bubble.dart` mesh-aware UX
12. Restore `lib/theme/app_theme.dart` custom themes
13. Restore `README.md` from v31
14. Restore `GEMINI.md` from v31
15. Restore `Docs/ARCHITECTURE_BLUEPRINT.md` from v31
16. Restore `Docs/MODULE_REFERENCE.md` from v31
17. Restore `Docs/UI_COMPONENT_MANIFEST.md` from v31

### P2 — Restore When Possible
18. Fix Android manifest permissions and ForegroundService type
19. Fix Android build.gradle.kts Firebase BOM
20. Restore `pubspec.yaml` version to 0.7.0+
21. Restore `scripts/build_android.sh` robustness
22. Restore wormhole retry logic
23. Restore `flutter_svg` dependency
24. Fix reaction event type (35 → 40)
25. Restore onboarding Intro-Claw config

---

## Methodology

4 parallel audit agents compared:
1. **Rust Core** (`src/`): All 16 source files diffed line-by-line
2. **Flutter/Dart** (`lib/`): All 35 shared files diffed
3. **RBN + Build Configs** (`for_linux/`, `android/`, root configs): All build and deployment files compared
4. **Tests + Docs** (`tests/`, `Docs/`): All 18 test files and 20+ doc files compared

Each agent ran recursive file listings, then `diff` on every shared file, reporting missing files, removed functions, changed signatures, and dependency shifts.

---

**Own your words. Own your network. Own your future.**
