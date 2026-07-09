# Debug Document — Introvert Sovereign Messenger

**Last Updated:** 2026-07-09 00:30 UTC
**Git:** main @ a569a76

---

## Current System State

### RBN Server
- **introvertd**: ACTIVE — libp2p, WebSocket tunnel, dashboard
- **introvert-solana**: ACTIVE — treasury/IPC daemon with unified credential management
- **Firebase**: Service account loaded, FCM push working
- **APNs**: Not configured (iOS push disabled)

### Client Build
- **macOS**: `make mac` — builds `libintrovert.dylib`
- **Android**: `make android` — builds `libintrovert.so` for arm64-v8a + x86_64
- **iOS**: `make ios` — builds static `.a` libraries
- **Flutter**: `flutter run` — launches app

### FFI Consistency
- **Rust exports**: 134 symbols
- **Dart lookups**: 129 symbols
- **Mismatches**: 0 (all Dart lookups resolve)

---

## Known Issues

### None Critical
- All daily rewards scoring, outlier mitigation, and E2EE unit tests are passing successfully.
- Epoch close pipeline verified working with successful payouts on 2026-07-09.

### Recently Fixed (2026-07-09)
- **Epoch ID Calculation**: Midnight UTC epoch close now correctly identifies the previous day's epoch.
- **Inter-Process Authentication**: Unified credential management across all daemon processes.
- **Cryptographic Verification**: Enhanced constant-time comparison for inter-process authentication.
- **Epoch Recovery**: Added startup catch-up mechanism for missed midnight closes.

### Android Build Notes
- Requires NDK 28.2.13676358 at `$ANDROID_SDK_ROOT/ndk/28.2.13676358`
- `libc++_shared.so` must be bundled alongside `libintrovert.so`
- `google-services.json` must be at `android/app/google-services.json`

### macOS Build Notes
- `libintrovert.dylib` must be copied to `macos/Flutter/ephemeral/`
- Secure storage entitlement `-34018` warning is non-blocking (falls back to SharedPreferences)

---

## Architecture Summary

### Three Daemons
1. **Client** (`libintrovert.dylib` / `.so`) — Flutter+Rust P2P mesh client
2. **RBN** (`introvertd`) — Relay Backbone Node
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
    → Distribute proportionally from daily pool
```

### Networking Relay Path
```
forward_to_mesh(recipient, payload)
    → WebRTC DataChannel (if open)
    → Direct libp2p (if connected)
    → dial_relay_path():
        Strategy 1: Direct P2P dial
        Strategy 2: Via RBN (latency-sorted)
        Strategy 3: Via connected anchor nodes
        Strategy 4: WebSocket tunnel fallback
    → StoreInMailbox (if all fail)
```

### VPN Tunnel Strategy
```
VPN detected:
    → ActivateTunnel command
    → Use plaintext WebSocket (bypasses VPN TLS blocking)
    → Isolate bootstrap to tunnel loopback only
    → Stale tunnel detection and force-reset
    → Re-activate if 0 peers after threshold

Non-VPN:
    → Use secure WebSocket (TLS)
    → Append tunnel alongside existing bootstrap nodes
```
