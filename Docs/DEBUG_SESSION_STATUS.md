# Debug Session Status — Group Chat & Android Push Wakeup
**Date:** 2026-06-29  
**Session:** Android Push Notification Wakeup + Protocol Handshake Alignments + Client Rebuilds + File Transfer Optimization + NAT64 Mobile Data Resolution

---

## BUG RESOLVED: Android not receiving group messages or notifications when minimized
### Status: FIXED ✅

### Symptoms
- Android client did not ping or show notifications when minimized or when the screen was off.
- Group messages only reached the Android client with a 3-4 minute delay via the polling fallback thread when opened.
- Real-time signaling payloads (like group messages or invites) sent to offline Android devices were successfully written to the RBN mailbox, but the RBN could not trigger the wake-up push notification because the Android client's push token was never registered in the RBN registry.

### Root Causes
1. **APNS Token Override on Android**: The Flutter app initialization in `main_shell.dart` and `alert_service.dart` was checking only `apnsToken` (iOS) to determine push status. Since `apnsToken` is null on Android, it deactivated push availability, spun up the fallback polling thread, and hid local notifications.
2. **libp2p Identify Negotiation Race**: The push token registration command was executed immediately upon `ConnectionEstablished`. At this point, the underlying transport is open, but capabilities (Identify protocol names) have not finished negotiating. The payload was discarded by the transport.
3. **Stale FFI Binary**: Edits to the client-side Rust source code were not packaged during `flutter run` because Flutter does not compile C++/Rust FFI sources automatically unless `scripts/build_android.sh` is manually invoked to refresh `jniLibs`.

### Fixes Applied
1. **Cross-Platform Token Check**: Updated `alert_service.dart` and `main_shell.dart` to check `fcmToken` on Android and `apnsToken` on iOS/macOS. Android now correctly reports `Background sync initialized — push active, polling disabled`.
2. **Identify-Level Auto-Registration**: Moved the push token auto-registration loop from the `ConnectionEstablished` event to the `Identify` event handler in `src/network/mod.rs` and `for_linux/src/network/mod.rs`. This guarantees signaling capabilities are fully negotiated before transmitting sleep state payloads.
3. **Android FFI Rebuild**: Executed `./scripts/build_android.sh` to compile updated client binaries (`arm64-v8a` and `x86_64`) and placed them in `android/app/src/main/jniLibs`.
4. **RBN Server Redeployment**: Recompiled and deployed the updated `introvertd` daemon to the Alibaba Cloud RBN node.

---

## BUG RESOLVED: File transfer shown as VoIP call
### Status: FIXED ✅

### Root Cause
When `SendFile` had no existing data channel, it called `InitiateWebRtc { media_type: 3 }` which created a WebRTC connection and sent an SDP `offer`. The receiver saw `signal_type: "offer"` and dispatched Event 14 (incoming call) with no way to distinguish it from a real call.

### Fix Applied
1. Added `purpose: Option<String>` field to `WebRtcSignal` struct.
2. `InitiateWebRtc` with `media_type == 3` now sets `purpose: Some("file_transfer")`.
3. Receiver checks `purpose` — file transfer offers dispatch **Event 39** (auto-accept) instead of Event 14.

---

## BUG RESOLVED: Gossipsub membership check rejecting relayed messages
### Status: FIXED ✅

### Root Cause
Gossipsub `Event::Message` handler checked `propagation_source` (relay peer) against the group member list. When RBN relayed a group message, the `propagation_source` was the RBN's PeerId (not in the member list), causing the message to be silently rejected.

### Fix Applied
Changed to use `message.source` (original author) when available, falling back to `propagation_source` only if `message.source` is None.

---

## BUG RESOLVED: Database "file is not a database" crash
### Status: FIXED ✅

### Root Cause
When a user ran a different Introvert identity (different seed), the old SQLCipher database was encrypted with a different key. `StorageService::new` failed with "file is not a database", causing a crash.

### Fix Applied
Added retry logic: if `StorageService::new` fails with "file is not a database", delete the corrupted file and retry with a fresh database.

---

## BUG RESOLVED: Large File Transfers (>7MB) Stalling Over Relayed Connections
### Status: FIXED ✅

### Symptoms
Large file transfers (7MB+) over relays (pull sequence) took 10+ minutes to complete, or failed completely, even on fast networks.

### Root Cause
The client pulled 256KB chunks (base64 encoded to ~341KB) using an 8-deep in-flight window. On relayed links, this thundering herd (2.7MB of concurrent data) saturated Yamux multiplexer windows, leading to resets, lost packets, and continuous watchdog timeout loop collapse.

### Fixes Applied
1. **Adaptive Chunk Size:** Direct connection uses 256KB chunks, relayed connection automatically falls back to 64KB chunks.
2. **Constrained Pipelining:** Direct connection uses 12-deep pipeline, relayed connection uses 4-deep pipeline (max 256KB in-flight).
3. **Pacing Delay:** Added a 100ms request pacing interval on relays.
4. **Watchdog Alignment:** Dynamically scaled the watchdog retry window and `next_pull_idx` to match the target pipeline size (4 for relay, 12 for direct P2P).

---

## BUG RESOLVED: Android Mobile Data Connection and Stale Sockets on Handover
### Status: FIXED ✅

### Symptoms
Android could not send or receive messages on mobile data. Messages sent on mobile data remained stuck and were not sent/received even after switching back to working Wi-Fi networks (unless joining the same local group network).

### Root Causes
1. **IPv6-Only / NAT64 Cellular Routing:** Cellular data networks are commonly IPv6-only using NAT64 translation. Since RBN's address was configured as a raw IPv4 literal (`/ip4/47.89.252.80/tcp/443`), the carrier's DNS64 server was bypassed, making RBN completely unreachable.
2. **Network Handover Delay:** The periodic loop that re-dials RBN bootstrap nodes and flushes pending messages ticked only once every 5 minutes, leaving the client stuck on dead/stale sockets after a network transition.

### Fixes Applied
1. **Wildcard DNS NAT64 Resolution:** Integrated native OS DNS resolution (`ToSocketAddrs`) on `47.89.252.80.sslip.io` in the bootstrap resolver (`src/network/config.rs`). On IPv6-only carriers, the OS automatically resolves this to a synthesized AAAA IPv6 address using the carrier's NAT64 prefix, allowing routing to the RBN node.
2. **Proactive Handover Stack Refresh:** Updated `_triggerClawNetworkRecovery()` in Flutter's `main_shell.dart` to call `_client.forceNetworkRefresh()` immediately on connectivity changes. This aggressively teardowns stale sockets, clears noise sessions, and forces a re-dial of RBN nodes immediately.

---

## Build & Deploy Status (as of 2026-06-29)

| Component | Status | Notes |
| :--- | :--- | :--- |
| **macOS Native Library** | ✅ Built | Compiled with recent updates |
| **Android Native Library** | ✅ Built | Compiled via `./scripts/build_android.sh` for arm64-v8a and x86_64 |
| **iOS Native Library** | ✅ Built | Compiled with recent updates |
| **Alibaba RBN (47.89.252.80)** | ✅ Deployed | Recompiled and deployed via `./deploy_rbn.sh` |

---

## Key Files Reference

| File | Purpose |
| :--- | :--- |
| [src/lib.rs](file:///Users/dev/Development/introvert/src/lib.rs) | FFI client functions |
| [src/network/config.rs](file:///Users/dev/Development/introvert/src/network/config.rs) | Bootstrap node definitions, ToSocketAddrs DNS fallback |
| [src/network/mod.rs](file:///Users/dev/Development/introvert/src/network/mod.rs) | Client network service, Identify-level push registration, adaptive pull parameters |
| [for_linux/src/network/config.rs](file:///Users/dev/Development/introvert/for_linux/src/network/config.rs) | Daemon bootstrap definitions with DNS resolver |
| [for_linux/src/network/mod.rs](file:///Users/dev/Development/introvert/for_linux/src/network/mod.rs) | RBN network service, Identify-level push registration, adaptive pull parameters |
| [lib/src/native/alert_service.dart](file:///Users/dev/Development/introvert/lib/src/native/alert_service.dart) | Push notification token status |
| [lib/src/ui/main_shell.dart](file:///Users/dev/Development/introvert/lib/src/ui/main_shell.dart) | Main application shell and UI bootstrap, network recovery handover |
