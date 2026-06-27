# Release Notes — Introvert v32 (0.9.0) "Sovereign Glass"

**Date:** June 20, 2026
**Codename:** Sovereign Glass
**Baseline:** stable_v31 (0.8.0 "Intelligent Mesh")

---

## What's New Since v31

### Glassmorphism UI Overhaul

The entire app now features a consistent glassmorphism aesthetic with blurred, translucent surfaces.

- **AppBar** — Transparent with `BackdropFilter(blur: 20)` at 60% opacity, extends behind body
- **Bottom Navigation** — Frosted glass pill with `ClipRRect` + `BackdropFilter(blur: 20)` at 50% opacity, subtle border
- **All Tab Cards** — Chats (contact/group cards), Drive (file cards), Notes (note cards), CLAW (query tiles), Settings (all sections) wrapped with reusable `GlassmorphicContainer` widget
- **Chat Bubbles** — `GlassmorphicBubble` with 10px blur, accent-tinted background, border
- **Sovereign Earnings & Wallet** — Glassmorphic containers replacing solid Card backgrounds

### 5 New Image Themes

| Theme | Style | Wallpaper | Accent |
|-------|-------|-----------|--------|
| **Beach House** | Light (Linen Mist) | `theme_beach.jpg` | Teal `#009494` |
| **Cyber City** | Dark | `theme_cybercity.png` | Cyan `#00E5FF` |
| **Mountain Peak** | Dark | `theme_mountain1.jpg` | Blue `#58A6FF` |
| **Mountain Ridge** | Dark | `theme_mountain2.jpg` | Purple `#8B5CF6` |
| **Forest** | Dark | `theme_forest.jpg` | Emerald `#34D399` |

- Image themes use `wallpaperOpacity: 1.0` (full image, no overlay)
- `SovereignWallpaper` now supports both asset paths (`assets/...`) and file paths
- Light themes without custom wallpaper show clean white bg (no dark default wallpaper)

### CLAW Tab Redesign

- **Dual-mode architecture** — Local mode shows 3×3 query tile grid; Hybrid mode shows chat interface
- **Combined header box** — Icon, "INTRO-CLAW" title, LOCAL/AI badge, and description text in single glassmorphic container
- **Brain logo** — `Icons.psychology_rounded` displayed above tile grid
- **Tile query results** — Results shown in floating overlay dialog (not chat mode), returns to tiles on close
- **Bottom bar** — RECON/HEAL/ABOUT buttons with glassmorphism, no outer container box
- **Query tiles** — 9 cards (Photos, Videos, Files, Contacts, Notes, Messages, Calls, Storage, Engine) with per-tile tint colors

### Network Recon & Heal — Terminal Overlay

- **Floating terminal dialog** — Green-on-black monospace aesthetic with animated cursor
- **Detailed milestones** — Timestamped readouts with ✓/✗ confirmations (e.g., `[00:01] ✓ Mesh interface online (libp2p v0.56)`)
- **Recon** — 8 milestones covering mesh interface, DHT, peers, relay circuits, latency, mDNS, tunnels, report
- **Heal** — 8 milestones covering peer scan, unreachable identification, direct dial, relay, anchors, tunnels, mailbox, report
- **Network Tune/Heal buttons** — Replace old NetworkOptimizationButton in top bar (popup menu)

### Settings Tab Reorganization

- **Merged 5 sections into "Introvert Mesh Swarm Settings"** — Identity, Status, Contribution, Connectivity, Swarm Status, Destructive Actions all in one section
- **Compact layout** — `dense: true` tiles, consistent 13px font
- **ZeroClaw Attribution** — New Info & Legal entry with full MIT/Apache 2.0 license text

### Theme System Improvements

- **Custom theme editing** — Editing a default theme auto-generates custom name (`custom01`, `custom02`, etc.) — defaults are never overwritten
- **Delete button** — Only shown for custom themes, hidden for defaults
- **Light theme wallpaper** — `wallpaperOpacity: 0.45` for Linen Mist, Glacier Bloom, Rose Quartz

### Android Fixes

- **ForegroundService crash fixed** — `startForeground()` now passes `FOREGROUND_SERVICE_TYPE_SPECIAL_USE` on Android 14+ (API 34)
- **AndroidManifest** — Full permissions restored, `specialUse` ForegroundService type

### FAB Positioning

- All tabs (Chats, Drive, Notes) use consistent `padding: bottom: 80` + `floatingActionButtonLocation: endFloat`
- FAB always renders above navigation bar on all tabs

### Tab Layout

- All tabs now start below the Introvert top bar (`MediaQuery.of(context).padding.top + kToolbarHeight`)
- Drive tab: Mesh capacity card wrapped with `GlassmorphicContainer`
- Notes tab: Title, help, menu, search bar combined in single glassmorphic box
- Notes tab: Inline header (no AppBar) matching Drive tab style

---

## Key Features at v32

| Feature | Status | Description |
|---------|--------|-------------|
| P2P Mesh | Stable | libp2p v0.56, Port 443, QUIC/UDP |
| E2EE Messaging | Stable | Noise IK, SQLCipher, zero-knowledge mailbox |
| File Transfer | Stable | 70+ Mbps direct P2P, relay fallback |
| Group Mesh | Stable | Gossipsub, full-mesh calls |
| Sovereign Drive | Stable | Content-addressed storage, auto-organization |
| Voice/Video Calls | Stable | WebRTC, adaptive quality |
| Economy | Stable | $INTR Solana token |
| Intro-Claw Engine | Stable | 12 automation modules, 5-min tick |
| Local Assistant | Stable | Query tile grid, floating results, suggestion chips |
| Semantic Search | Stable | BERT embeddings, cosine similarity |
| Network Recon/Heal | Stable | Terminal overlay with detailed milestones |
| Hybrid AI Mode | Stable | OpenAI-compatible endpoint |
| FCM Push (RBN) | Stable | Direct Firebase integration |
| **Glassmorphism UI** | **NEW** | Frosted glass across all tabs and components |
| **Image Themes** | **NEW** | 5 themes with full-screen wallpapers |
| **Theme Editor** | **IMPROVED** | Default-safe editing, custom naming |

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
```

### RBN Deployment
```bash
./deploy_rbn.sh
# Requires: Firebase service account at /opt/introvert/config/firebase-service-account.json
```

---

## Architecture

```
/introvert
├── lib/                          # Flutter UI (Dart)
│   ├── main.dart                 # App entry point
│   ├── blueprint_ui.dart         # Reusable widgets (GlassmorphicBubble, GlassmorphicContainer, SovereignWallpaper)
│   ├── theme/app_theme.dart      # Theme system (12 themes, custom themes)
│   ├── views/                    # Chat, group chat, profile, call, media gallery
│   └── src/
│       ├── native/               # FFI Bridge (introvert_client.dart, identity_manager.dart)
│       ├── ui/                   # Main shell, tabs, overlays, update service
│       ├── services/             # WebRTC, group calls, background sync, network quality
│       └── repository/           # Sync repository
├── src/                          # Rust Core Engine
│   ├── lib.rs                    # FFI C-bindings (50+ exported functions)
│   ├── main.rs                   # Headless daemon entry point
│   ├── identity.rs               # Deterministic HKDF identity derivation
│   ├── storage.rs                # SQLCipher persistence (18 tables)
│   ├── intro_claw.rs             # Intro-Claw automation engine (1500 lines)
│   ├── embedding.rs              # BERT semantic search
│   ├── network/                  # libp2p swarm, signaling, groups, registry, tunnel, E2EE
│   ├── media/                    # WebRTC implementation
│   └── economy/                  # Reward tracker + Solana incentive engine
├── for_linux/                    # RBN Daemon source tree
│   ├── src/lib.rs                # RBN main library (includes fcm module)
│   ├── src/fcm.rs                # Firebase Cloud Messaging
│   ├── src/storage.rs            # RBN storage (includes Intro-Claw persistence)
│   ├── src/network/mod.rs        # RBN network (FCM push integration)
│   └── Cargo.toml                # RBN dependencies
├── android/                      # Android build config
│   ├── app/src/main/AndroidManifest.xml
│   ├── app/build.gradle.kts      # Firebase BOM, specialUse ForegroundService
│   └── build.gradle.kts          # google-services classpath
├── scripts/                      # Build automation
├── firebase/                     # Firebase config
├── assets/                       # Images, audio, theme wallpapers
├── tests/                        # Rust test files
├── Docs/                         # Technical documentation
├── pubspec.yaml                  # Flutter dependencies (v0.9.0)
├── Cargo.toml                    # Rust dependencies
└── Makefile                      # Build orchestration
```

---

## Known Issues

- iOS release blocked by Apple Developer Account
- Android build warning about Kotlin Gradle Plugin migration
- `flutter_webrtc` plugin doesn't support Swift Package Manager yet

---

**Own your words. Own your network. Own your future.**
