# Debug Document — Introvert Sovereign Messenger

**Last Updated:** 2026-07-06 08:30 UTC
**Git:** main @ f62bc59
**Backup:** 06_07_26_0830

---

## Current System State

### RBN Server (47.89.252.80)
- **introvertd**: ACTIVE on port 443 (libp2p), port 80 (WSS tunnel), port 8080 (dashboard)
- **introvert-solana**: ACTIVE on localhost:9001 (treasury/IPC)
- **PeerId**: `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a`
- **Firebase**: Service account loaded, FCM push working
- **APNs**: Not configured (iOS push disabled)

### Client Build
- **macOS**: `make mac` — builds `libintrovert.dylib` (41MB)
- **Android**: `make android` — builds `libintrovert.so` for arm64-v8a + x86_64 (with `libc++_shared.so`)
- **iOS**: `make ios` — builds static `.a` libraries
- **Flutter**: `flutter run` — launches app

### FFI Consistency
- **Rust exports**: 134 symbols
- **Dart lookups**: 129 symbols
- **Mismatches**: 0 (all Dart lookups resolve)

---

## Known Issues

### None
- All daily rewards scoring, outlier mitigation, and E2EE unit tests are passing successfully.

### Android Build Notes
- Requires NDK 28.2.13676358 at `$ANDROID_SDK_ROOT/ndk/28.2.13676358`
- `libc++_shared.so` must be bundled alongside `libintrovert.so` (handled by `build_android.sh`)
- `google-services.json` must be at `android/app/google-services.json`

### macOS Build Notes
- `libintrovert.dylib` must be copied to `macos/Flutter/ephemeral/` (handled by `make mac`)
- Secure storage entitlement `-34018` warning is non-blocking (falls back to SharedPreferences)

---

## Architecture Summary

### Three Daemons
1. **Client** (`libintrovert.dylib` / `.so`) — Flutter+Rust P2P mesh client
2. **RBN** (`introvertd`) — Relay Backbone Node on Alibaba Cloud
3. **Economy** (`introvert-solana`) — Treasury/IPC daemon

### Telemetry Pipeline
```
DailyRewardEngine.record_activity()
    → shared_metrics[idx] = capped_count
    → RewardTracker.package_telemetry()
    → SignalingPayload::TelemetryEnvelope
    → forward_to_mesh() to RBN (30-min interval)
    → RBN processes via RbnDailyRewardEngine
    → TelemetryAck returned
```

### IQR Anti-Gaming Filter
```
close_current_epoch(epoch_id)
    → Collect all edge scores
    → Sort → Q1, Q3, IQR
    → Upper Bound = Q3 + 1.5 * IQR
    → Clamp outliers to Upper Bound
    → Distribute: (Capped Points / Total Capped) * 16,438 INTR
```

### Networking Relay Path
```
forward_to_mesh(recipient, payload)
    → WebRTC DataChannel (if open)
    → Direct libp2p (if connected)
    → dial_relay_path():
        Strategy 1: Direct P2P dial
        Strategy 2: Via RBN (latency-sorted, relay_hint prioritized)
        Strategy 3: Via connected anchor nodes
        Strategy 4: WebSocket tunnel fallback
    → StoreInMailbox (if all fail)
```

### VPN Tunnel Strategy
```
VPN detected (connectivity_type == 5):
    → ActivateTunnel command
    → Use ws:// port 80 (plaintext, bypasses VPN TLS blocking)
    → Isolate bootstrap to tunnel loopback only
    → Stale tunnel detection: 15s (mobile) / 30s (WiFi/VPN)
    → Force-reset and re-activate if 0 peers after threshold

Non-VPN:
    → Use wss:// port 443 (TLS)
    → Append tunnel alongside existing bootstrap nodes
```

### Drive Folder Manager
```
Storage: drive_files table with folder column
         drive_folders table for metadata
         upsert_drive_file_with_folder() — assigns folder on receive

Auto-organize:
    Group chat files → folder named after group
    1:1 chat files → folder named after contact alias
    Introvert Explained → pinned at top (4 guide images)

UI: Minimized folder view (ExpansionTile)
    List/grid toggle
    Multi-select → move, delete, share
    Breadcrumb navigation
    Storage usage bar
```

### Notification Rules
```
Native Android (IntrovertFirebaseMessagingService.kt):
    Foreground → skip native notification (Dart handles sound)
    Background → 3-minute cooldown between notifications
    Sound + vibration enabled on notification channel

Dart (AlertService):
    Foreground → showAlert() returns immediately (no native notification)
    Background → 3-minute cooldown, posts native notification
    Sound plays for: messages, group invites, calls (always, even foreground)
```

---

## Recovery Procedure

### From backup to working app:
1. Copy backup to `/Users/dev/Development/introvert/`
2. `flutter pub get`
3. `make mac` (or `make android` / `make ios`)
4. `flutter run`

### Deploy RBN:
1. `./deploy_rbn.sh` — syncs to thinkpad, compiles, deploys to 47.89.252.80

### Backup naming:
`dd_mm_yy_time` format (e.g., `06_07_26_0830`)

---

## Recent Changes (v0.29.0)

### Networking Fixes
- Removed `relay_reservations.clear()` on VPN transition
- Fixed `ListenerClosed` recovery to use full multiaddr
- Reverted in-flight limits to relay=4, direct=8
- Added anchor relay strategy to `dial_relay_path`
- Added undelivered message retry (>60s)

### Telemetry Pipeline
- `TelemetryEnvelope` and `TelemetryAck` signaling variants
- 30-minute telemetry interval with 5-minute cooldown
- `shared_metrics` bridge: `Arc<RwLock<[u64; 9]>>`

### Anti-Gaming
- IQR outlier mitigation in `close_current_epoch()`
- Unit test `test_iqr_outlier_mitigation_and_batch_distribution` passes

### Build Fixes
- Android: `libc++_shared.so` bundled from NDK
- FFI: `introvert_storage_update_group_message_status_by_id` added
- Backup: `dd_mm_yy_time` naming, completeness verification

---
## Backup Status (2026-07-06 08:34)
- Git: main @ f62bc59
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 08:52)
- Git: main @ f62bc59
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 09:50)
- Git: main @ f62bc59
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 10:19)
- Git: main @ 2d44868
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 11:09)
- Git: main @ 2239bd5
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 14:35)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 17:25)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 19:25)
- Git: main @ 6b4398c (dirty - working copy compiled and deployed)
- RBN: introvertd on 47.89.252.80:443 (ACTIVE, updated to 13-metrics schema + signature verification + SQLCipher database persistence + midnight payout schedule)
- Economy: introvert-solana on localhost:9001 (ACTIVE)

---
## Backup Status (2026-07-06 19:39)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 19:47)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-06 19:54)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-07 06:11)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-07 06:36)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-07 06:52)
- Git: main @ 6b4398c
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-07 12:43)
- Git: main @ 1ed4e60
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-08 18:30)
- Git: main @ 72a5880
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-08 19:46)
- Git: main @ 9802c2a
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-08 21:54)
- Git: main @ 9802c2a
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001
