# Release Notes — Introvert v39 (0.15.0) "Relay Resiliency"

**Date:** June 25, 2026  
**Codename:** Relay Resiliency  
**Baseline:** stable_v38 (0.14.0 "Unified Drive")  

---

## What's New Since v38

### Core Swarm & Relay Improvements

- **ListenerClosed Relay Auto-Recovery** — Handled `SwarmEvent::ListenerClosed` in the client's network loop (`src/network/mod.rs`). If a circuit relay reservation listener closes due to a transient connection drop, the node automatically removes the invalid listener/reservation record. If still connected to the RBN relay peer, it immediately attempts to listen on the relay circuit (`/p2p/{peer_id}/p2p-circuit`) again. This fixes the issue where a client node would become permanently unreachable over the relay backbone without a full restart.
- **File Transfer Authorization Hardening** — Upgraded security constraints on fallback seeder lookups across the mesh to verify proper peer authorizations on file chunk queries.
- **RBN Code Sync** — Synchronized RBN daemon source files in `for_linux/src/network/mod.rs` to support the v2.0.0 protocol binary codec, facilitating direct binary payload negotiation mirroring the client swarm interface.

### UI & UX Optimizations

- **Weak Network Non-Blocking Auto-Optimization** — Replaced the intrusive `AlertDialog` on weak connection triggers with a rate-limited background optimizer. The client now calls `forceNetworkRefresh()` asynchronously in the background (rate-limited to a 2-minute window) and presents a sleek, non-intrusive SnackBar: `"Weak network detected... optimizing..."`.
- **Contact Settings Layout Overflow Fix** — Solved a layout constraint break inside `_ContactInfoDialog` (`lib/views/chat_screen.dart:L545-682`). Wrapped the details column in a `SingleChildScrollView` to prevent screen boundary crashes on compact mobile displays.

---

## Key Features at v39

| Feature | Status | Description |
|---------|--------|-------------|
| P2P Mesh | Stable | libp2p v0.56, Port 443, QUIC/UDP |
| E2EE Messaging | Stable | Noise IK, SQLCipher, zero-knowledge mailbox |
| File Transfer | Stable | 70+ Mbps direct P2P, relay fallback |
| Group Mesh | Stable | Gossipsub, direct P2P, reliable reactions |
| Voice/Video Calls | Stable | WebRTC, adaptive quality |
| Economy | Stable | $INTR Solana token, daily rewards v3.0.1 |
| Intro-Claw Engine | Stable | 27 modules, local-only, sandboxed |
| Sovereign Drive | Stable | Folder-based, thumbnail grid, file explorer |
| Emoji Reactions | Stable | Reliable propagation via mailbox, counts, details |
| RBN Infrastructure | **Hardened** | Alibaba Cloud + thinkpad.local, auto-recovery active |
| 17 Themes | Stable | Editable, custom theme system |

---

## Build Instructions

### Prerequisites
- Rust 1.75+, Flutter 3.24+, Android NDK 28.x, Xcode 15+, cargo-zigbuild

### macOS / Android / iOS
```bash
make mac && flutter run       # macOS
make android && flutter run   # Android
make ios && flutter run       # iOS
make all                      # Compile all native binaries
```

### RBN Deployment
```bash
cd for_linux
ulimit -n 65536
cargo zigbuild --target x86_64-unknown-linux-gnu --release --bin introvertd
ssh root@47.89.252.80 "systemctl stop introvertd"
scp target/x86_64-unknown-linux-gnu/release/introvertd root@47.89.252.80:/opt/introvert/bin/introvertd
ssh root@47.89.252.80 "systemctl start introvertd"
```

---

## Known Issues & Verification Required

> [!WARNING]
> While text message relay and peer profile discovery are verified as fully operational across distinct network address spaces via circuit relays, sharing and downloading media (images/videos) across different networks over relay fallback circuits still needs to be thoroughly checked and verified on physical hardware.
