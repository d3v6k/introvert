# Release Notes — Introvert v33 (0.10.0) "Sovereign Palette"

**Date:** June 20, 2026
**Codename:** Sovereign Palette
**Baseline:** stable_v32 (0.9.0 "Sovereign Glass")

---

## What's New Since v32

### Glassmorphism Legibility

`GlassmorphicContainer` now uses a two-layer approach for better readability:
- **Overlay layer**: Theme-aware — `Colors.black` at 30% for dark themes, `Colors.white` at 30% for light themes
- **Tint layer**: Accent color at 8% for style
- New `overlayAlpha` parameter (default 0.3) for per-widget tuning

### 5 New Image Themes

| Theme | Style | Wallpaper | Accent |
|-------|-------|-----------|--------|
| **Canyon** | Dark | `theme_canyon.jpg` (129KB) | Amber-orange `#FF6B35` |
| **Desert** | Dark | `theme_desert.jpg` (90KB) | Golden `#FFB830` |
| **Winter Wonderland** | Dark | `theme_winter.jpg` (118KB) | Ice blue `#60CFFF` |
| **Morning Dew** | Light | `theme_light1.jpg` (31KB) | Sky blue `#2196F3` |
| **Golden Hour** | Light | `theme_light2.jpg` (33KB) | Amber `#E67E22` |
| **Azure Sky** | Light | `theme_light3.jpg` (34KB) | Ocean blue `#0077CC` |
| **Cyber City II** | Dark | `theme_cyber_city2.jpg` (101KB) | Magenta `#E040FB` |
| **Cyber City III** | Dark | `theme_cyber_city3.jpg` (119KB) | Neon green `#00FF60` |

### Theme System Changes

- **Removed**: Linen Mist, Glacier Bloom, Rose Quartz (plain white themes without wallpapers)
- **Sorted**: All 17 themes now listed alphabetically in the theme picker (Introvert Dark stays as default)
- **Optimized**: All theme images converted from PNG to JPEG at 720px width, quality 80 — total 1.5MB (was 11.3MB, 95% reduction)

### Logging Migration

- All Rust/RBN source files migrated from `println!`/`eprintln!` to `tracing` macros (`info!`/`warn!`/`error!`/`debug!`)
- Structured logging with `tracing-subscriber` and `EnvFilter` initialization in `main.rs`

### Code Quality

- All macOS Finder `(1)` duplicate files cleaned up
- `.stable`, `.bak`, `.clean`, `.tail` backup files removed from project tree
- `solana (Copy).rs` junk files removed
- Event type 35/40 verified consistent (35=Handle Resolve Failed, 40=Message Reaction)

---

## Key Features at v33

| Feature | Status | Description |
|---------|--------|-------------|
| P2P Mesh | Stable | libp2p v0.56, Port 443, QUIC/UDP |
| E2EE Messaging | Stable | Noise IK, SQLCipher, zero-knowledge mailbox |
| File Transfer | Stable | 70+ Mbps direct P2P, relay fallback |
| Group Mesh | Stable | Gossipsub, full-mesh calls, TreeKEM E2EE |
| Sovereign Drive | Stable | Content-addressed storage, auto-organization |
| Voice/Video Calls | Stable | WebRTC, adaptive quality, mock RTP for testing |
| Economy | Stable | $INTR Solana token |
| Intro-Claw Engine | Stable | 12 automation modules, 5-min tick, BERT search |
| Local Assistant | Stable | Query tile grid, floating results, suggestion chips |
| Network Recon/Heal | Stable | Terminal overlay with detailed milestones |
| Hybrid AI Mode | Stable | OpenAI-compatible endpoint |
| FCM Push (RBN) | Stable | Direct Firebase integration |
| Glassmorphism UI | Stable | Frosted glass across all tabs, theme-aware overlay |
| **17 Themes** | **Stable** | 12 dark + 3 light + 2 image themes, alphabetically sorted |
| **Tracing Logging** | **Stable** | Structured logging throughout Rust/RBN |
| **Android 14** | **Stable** | ForegroundService specialUse type |

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
flutter run
```

### Android
```bash
flutter build apk
flutter run
```

### RBN Deployment
```bash
./deploy_rbn.sh
```

---

**Own your words. Own your network. Own your future.**
