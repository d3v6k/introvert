# Changelog

All notable changes to Introvert will be documented in this file.

## [0.31.0] - 2026-07-09 — Payout Pipeline Hardening & Automated Epoch Recovery

### Milestone
**Full audit and hardening of the RBN reward distribution pipeline. Fixed epoch ID calculation, inter-process authentication, and cryptographic verification. Added startup recovery mechanism for automated epoch processing. First successful automated INTR distribution verified on Solana Mainnet.**

### Added
- **Startup Epoch Recovery**: On daemon restart, automatically attempts to close previous day's epoch as safety net for missed midnight closes.
- **Enhanced Authentication**: Improved inter-process communication security with constant-time cryptographic verification.
- **Credential Management**: Unified credential loading across all daemon processes.

### Fixed
- **Epoch ID Calculation**: Fixed bug where midnight UTC close generated incorrect epoch identifier.
- **Inter-Process Authentication**: Unified authentication credentials across daemon processes.
- **Telemetry Persistence**: Verified that client telemetry survives daemon restarts for proper epoch processing.

### Verified
- **Epoch 2026_07_08 Payout**: 3 claims, 16,438 INTR distributed successfully on Solana Mainnet.
- **7/7 Unit Tests Passing**: Basic scoring, double-claim rejection, IQR outlier mitigation, codec tests, wire size comparison.
- **Automated Pipeline**: No manual intervention required for new clients joining the network.

---

## [0.30.0] - 2026-07-06 — Signed Telemetry Pipeline, SQLite Persistence & Snappy Reconnection Ladder

### Milestone
**Fully aligned, signed, and validated client-to-RBN telemetry pipeline, database persistence for client telemetry, midnight UTC epoch close scheduler with automatic Solana treasury claims, and 15-second snappy connection recovery cycler integration.**

### Added
- **Cryptographic Telemetry Signing**: Added `package_signed_telemetry()` on client to sign telemetry metrics using Ed25519 with client's derived Solana keypair.
- **RBN Telemetry Validation**: Added signature validation on the RBN server node to authenticate declarations before scoring.
- **SQLite Telemetry Persistence**: Expanded RBN database schema with telemetry table to securely store signed envelopes, surviving daemon restarts.
- **13-Metrics Schema Alignment**: Expanded client shared metrics from `[u64; 9]` to `[u64; 13]` to include web activity and peer handshake metrics.
- **Midnight UTC Scheduler**: Implemented background task to check for midnight UTC, run epoch close, and generate claim payouts.
- **Solana Treasury Claims**: Implemented secure claim dispatch to treasury daemon with cryptographic authentication.

### Fixed
- **Snappy Peer Reconnection**: Integrated connection state cycler into the 15-second status loop. Disconnected clients now rotate connection strategies immediately, resolving the 5-minute stuck-in-connecting bug.
- **Unit Test Outlier Preimages**: Fixed daily rewards and dual-pool separation unit tests by generating correct cryptographic proof hashes.

---

## [0.29.0] - 2026-07-06 — Recovery, Telemetry Pipeline, Anti-Gaming IQR, Drive Rebuild, VPN Fix & Notification Hardening

### Milestone
**Full codebase recovery from GitHub source, telemetry pipeline for RBN reward tracking, IQR outlier mitigation for anti-gaming, 9-field shared metrics bridge, networking relay fixes for cross-network/VPN messaging, comprehensive backup system, drive folder manager rebuild, VPN tunnel fix, and notification hardening.**

### Added
- **Telemetry Pipeline**: 30-minute interval packages activity metrics and pushes to connected RBNs with cooldown guard.
- **9-Field Shared Metrics Bridge**: Activity counts flow in real-time from the reward engine to the telemetry pipeline.
- **TelemetryEnvelope & TelemetryAck**: New signaling variants for client-RBN telemetry exchange.
- **IQR Anti-Gaming Filter**: Interquartile Range outlier mitigation with proportional reward distribution.
- **IQR Unit Test**: Validates the filter with mock epoch data.
- **Anchor Relay Strategy**: Added connected anchor node relay as fallback strategy.
- **Undelivered Message Retry**: Periodic check re-sends messages stuck at status=0 for >60s to connected recipients.
- **Economy Daemon Restored**: Treasury and IPC daemon recovered from backup.
- **Comprehensive Backup System**: Automated backup with naming convention and completeness verification.
- **Drive Folder Manager Rebuild**: Complete rebuild with folder grouping, minimized view, list/grid toggle, multi-select, batch operations, breadcrumb navigation, and storage usage bar.
- **Drive Folder Storage Layer**: Added folder management to database schema.
- **Drive Folder FFI**: Added FFI functions for folder management.
- **VPN Tunnel Fallback**: Added plaintext WebSocket fallback for VPN connections.
- **VPN Bootstrap Isolation**: Bootstrap list isolated to tunnel loopback on VPN.

### Fixed
- **VPN Relay Regression**: Removed destructive relay state clearing on network transition.
- **ListenerClosed Multiaddr**: Fixed relay re-reservation to use full multiaddr.
- **In-flight Limits**: Reverted to baseline limits to prevent relay circuit saturation.
- **Android libc++_shared.so**: Updated build script to bundle required native library.
- **FFI Consistency**: Added missing FFI symbol between Dart and Rust.
- **Notification Spam**: Added cooldown and foreground suppression to prevent notification spam.

### Changed
- **Makefile**: Added `bk` target for comprehensive backup.

### Infrastructure
- **RBN Deployed**: Compiled and deployed to production RBN server. Active and serving.
- **Economy Daemon**: Treasury daemon running on production server.

---

## [0.28.0] - 2026-07-04 — VPN Resilience, RBN Blacklisting & Active Session Hardening

### Milestone
**VPN connectivity hardening, intelligent RBN blacklisting with exponential cooldown, active chat session prioritization (aggressive upgrades + healing), and app launch warm-up.**

### Added
- **VPN Connection Detection & Bootstrap Isolation**: Automatic tunnel activation on VPN detection with bootstrap isolation.
- **Chat Screen Offline Sync**: Updated status display for offline state during VPN disruptions.
- **Queue Congestion Prevention**: Removed redundant dial loops to prevent queue exhaustion.
- **Intelligent RBN Blacklisting & Cooldown Reset**: Exponential cooldowns with automatic clearing on network events.
- **Active Chat Prioritization**: Aggressive connection upgrades for active chat sessions.
- **App Launch Warm-Up**: Initial connection pass on app startup.

### Removed
- **Local Development Node**: Cleaned development RBN from default configuration.

---

## [0.27.0] - 2026-07-04 — VPN Resilience, Kliphy GIFs & UI Fixes

### Milestone
**VPN connectivity hardening, Kliphy GIF integration with attribution, group chat reactions restored, and external messenger WebView layout/scroll fixes.**

### Added
- **Kliphy GIF API integration** with attribution
- **VPN tunnel stale detection** with automatic recovery
- **Tunnel lifecycle tracking** for connection monitoring
- **Messenger login detection** for setup guide optimization

### Fixed
- **Group chat reactions** — Fixed empty reaction cache population
- **Messenger WebView layout** — Fixed gaps and overflow issues
- **Messenger vertical scroll** — Added proper scroll handling
- **VPN tunnel stuck state** — Added stale tunnel detection and recovery

---

## [0.26.0] - 2026-07-03 — DynamicPromoStack & Customizable Campaign Layer

### Milestone
**DynamicPromoStack integrated into production daemon. Open-ended campaign management system allows runtime promotion adjustments without code rebuilds. Strategic Reserve ceiling enforced with automatic deduction and referral pool compression.**

### Added
- **DynamicPromoStack** — Runtime campaign registry with open/close/adjust operations
- **ActiveCampaign struct** — Campaign configuration with payout allocation and expiration
- **PromoType enum** — Multiple campaign types for different reward mechanisms
- **Auto-eviction** — Expired campaigns automatically removed at epoch close
- **Safety cap** — Promo deductions cannot exceed Strategic Reserve ceiling

### Architecture
```
[Strategic Reserve Daily Ceiling: 3,287.60 INTR]
                    │
                    ├──► [- Minus] Active Campaigns
                    │
                    └──► [= Equals] Referral Pool
```

---

## [0.25.0] - 2026-07-03 — Economy Chain Audit & TelemetryEnvelope Implementation

### Milestone
**Full economy chain audit completed. Critical blockers identified and resolved. TelemetryEnvelope with Ed25519 signing implemented across all codebases.**

### Audit Findings & Fixes
- **Token mint unified** — All references use canonical mint address
- **Balance gate removed** — Merit-based rewards only
- **Ed25519 signature verification** — Real cryptographic verification implemented
- **TelemetryEnvelope struct** — 13 metrics array, wallet addresses, proof hash, client signature
- **Double-claim guard** — Tracking pairs for replay prevention
- **Dynamic reward calculation** — Proportional pool-clearing formula

### Added
- **TelemetryEnvelope** — Signed telemetry packet with full field coverage
- **package_telemetry()** — Client-side method to package metrics into signed envelope
- **send_telemetry_to_rbn()** — Secure send to RBN daemon
- **Ed25519 verification** — Real signature verification
- **SHA-256 proof hash** — For relay bytes verification
- **All 13 record_*() methods** — Complete activity tracking
