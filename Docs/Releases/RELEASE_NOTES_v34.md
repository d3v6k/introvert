# Release Notes — Introvert v34 (0.11.0) "Iron Claw"

**Date:** June 20, 2026
**Codename:** Iron Claw
**Baseline:** stable_v33 (0.10.0 "Sovereign Palette")

---

## What's New Since v33

### Intro-Claw: Local-Only, Fully Sandboxed

The hybrid AI mode has been completely removed. Intro-Claw now operates 100% on-device with zero external API calls, eliminating any prompt injection risk.

- Removed `llm_query()` async function and all LLM integration code
- Removed `intro_claw_get_endpoint()`, `intro_claw_set_endpoint()` FFI functions
- Removed hybrid mode toggle, endpoint URL field, API key field from settings
- `process_assistant_query()` simplified to local-only (2 parameters, was 5)
- Status returns `{ "is_active": bool, "mode": "local" }`
- Settings shows "100% Local — Sandboxed" with green shield icon

### Intro-Claw Intelligence: 10 New Modules

All 8 previously proposed intelligence modules implemented, plus 2 VoIP modules:

| Module | Description |
|--------|-------------|
| **Offline Message Queue** | Buffers messages when network drops (500 max), flushes to connected peers on restore |
| **Dead Letter Detection** | Flags messages stuck >5 min, attempts alternative routes |
| **Peer Reconnection Scoring** | Tracks disconnect frequency, flags unstable peers after 3 disconnects/hour |
| **Bandwidth-Aware Transfer** | Monitors throughput per peer (10-sample window), quality tiers: Full/Medium/Low/Minimal |
| **Group Sync Optimization** | Prioritizes group members by unread count when syncing |
| **Connection Pre-warming** | Pre-dials top 3 contacts when contacts list opens (5-min cooldown) |
| **Storage-Aware Caching** | Auto-prunes orphaned mesh chunks when disk >80% |
| **Night Maintenance Window** | Heavy cleanup during 30+ min idle periods (max once/hour) |
| **VoIP Call Quality Monitor** | Tracks RTT, packet loss, jitter, bitrate per call sample |
| **Pre-Call Network Check** | Checks peer connectivity before call, returns quality estimate |

### VoIP Intro-Claw Integration

- Call quality monitoring with RTT/loss/jitter/bitrate tracking
- Activity log entries for call start, quality degradation, path switches, call end
- Adaptive bitrate detection with quality downgrade warnings
- Pre-call network check with quality estimation
- Call history analytics (last 50 calls)

### Network Recon & Heal — Real Implementations

The stubs have been replaced with real implementations:

- **Recon**: Builds live `ReconContext` from swarm state — connected peers, anchors, relay reservations, seeders, transfers, pending messages, storage usage. Generates detailed markdown report with peer routing table, connection analysis, storage metrics, security audit.
- **Heal**: Multi-strategy execution — direct dial → relay circuit → anchor routing. Returns detailed heal report with attempt results.

### Network Change Detection

- `connectivity_plus` listener monitors WiFi↔Cellular↔None transitions
- Network Lost: Red alert "All connections dropped. Messages will be queued."
- Network Restored: Auto-recon after 2s delay, green "Connections re-optimized"
- Network Type Changed: Same auto-recon with "Re-optimizing connections..."

### Auto-Recon on Chat Start

- 1:1 Chat: `_runIntroClawRecon()` called on chat open
- Group Chat: `runNetworkRecon()` called on group open
- Connection status notifications (Direct P2P, Relay Active)

### CLAW Tab Redesign

- **18 tiles** (was 9): 9 data queries + 9 Intro-Claw action tiles
- **Live tile values**: Engine shows Active/Inactive, Storage shows MB, Battery shows status, Bandwidth shows quality
- **Activity log**: Real-time view of all Intro-Claw operations from last hour, with category icons and severity colors
- **LOG toggle** in header — green when active, shows/hides activity log view
- **17 MODULES info button** in settings — shows all modules with descriptions, active/inactive explanations
- **Result items are tappable** — contacts open chat, files open via OpenFile, groups open group chat
- **Description updated** — removed external LLM reference, now reads "100% on-device in a sandboxed environment"

### Glassmorphism Enhancements

- `GlassmorphicContainer` now uses two-layer approach: overlay (dark 30%/light 30%) + accent tint (8%)
- New `overlayAlpha` parameter for per-widget tuning

### Documentation

- DO NOT TOUCH rule documented for direct P2P 1:1 file transfer pipeline
- ARCHITECTURE_BLUEPRINT.md updated with Intro-Claw Intelligence Layer section
- CHANGELOG.md updated with v0.11.0 entry

---

## Key Features at v34

| Feature | Status | Description |
|---------|--------|-------------|
| P2P Mesh | Stable | libp2p v0.56, Port 443, QUIC/UDP |
| E2EE Messaging | Stable | Noise IK, SQLCipher, zero-knowledge mailbox |
| File Transfer | Stable | 70+ Mbps direct P2P, relay fallback |
| Group Mesh | Stable | Gossipsub, full-mesh calls, TreeKEM E2EE |
| Voice/Video Calls | Stable | WebRTC, adaptive quality, Intro-Claw monitoring |
| Economy | Stable | $INTR Solana token |
| **Intro-Claw Engine** | **Stable** | 27 modules, local-only, sandboxed, activity log |
| **VoIP Intelligence** | **Stable** | Call quality monitoring, pre-call check, adaptive bitrate |
| **Network Recon/Heal** | **Stable** | Real multi-strategy implementations |
| **Network Change Detection** | **Stable** | Auto-recon on connectivity change |
| **Idle Mode** | **Stable** | FCM replaces polling/heartbeat, battery-efficient background |
| **Anchor Mode** | **Stable** | Relay server, DHT server, mailbox, auto-disable at 30% battery |
| Glassmorphism UI | Stable | Frosted glass across all tabs, theme-aware overlay |
| 17 Themes | Stable | 12 dark + 3 light + 2 image themes |

---

## Build Instructions

### Prerequisites
- Rust 1.75+
- Flutter 3.24+
- Android NDK 28.x
- Xcode 15+ (iOS/macOS)

### macOS
```bash
make mac && flutter run
```

### Android
```bash
flutter build apk && flutter run
```

### RBN Deployment
```bash
./deploy_rbn.sh
```

---

**Own your words. Own your network. Own your future.**
