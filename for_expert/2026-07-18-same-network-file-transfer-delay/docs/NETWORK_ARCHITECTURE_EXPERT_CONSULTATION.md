# Network Architecture Expert Consultation

**Date:** 2026-07-14
**Project:** Introvert Sovereign Messenger
**Purpose:** Expert review of networking architecture, issues encountered, and fixes applied

---

## 1. Current Network Architecture

### 1.1 System Overview

Introvert is a P2P mesh messenger with:
- **Client app:** Flutter (Dart) + Rust core (libp2p 0.56.0) on Android/iOS/macOS
- **RBN (Relay Backbone Node):** Rust daemon on Alibaba Cloud (47.89.252.80, 1GB RAM)
- **Economy daemon:** Rust Solana integration (currently disabled — treasury needs SOL funding)

### 1.2 Networking Stack

```
┌─────────────────────────────────────────────┐
│  Flutter UI (Dart)                          │
│  ┌─────────────────────────────────────────┐│
│  │  IntrovertClient (FFI bridge)           ││
│  └─────────────────────────────────────────┘│
├─────────────────────────────────────────────┤
│  Rust Core (libp2p 0.56.0)                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐│
│  │ TCP+Noise│ │   QUIC   │ │ WebRTC (WIP) ││
│  └──────────┘ └──────────┘ └──────────────┘│
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐│
│  │ Gossipsub│ │ Request  │ │  Relay       ││
│  │          │ │ Response │ │  Client      ││
│  └──────────┘ └──────────┘ └──────────────┘│
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐│
│  │ Kademlia │ │  mDNS    │ │ WebSocket    ││
│  │   DHT    │ │          │ │   Tunnel     ││
│  └──────────┘ └──────────┘ └──────────────┘│
├─────────────────────────────────────────────┤
│  Storage: SQLCipher (encrypted SQLite)      │
│  Identity: Ed25519 + X25519 + Noise IK      │
└─────────────────────────────────────────────┘
```

### 1.3 Three-Tier Networking Progression

| Tier | Method | When Used | Performance |
|------|--------|-----------|-------------|
| **1. Direct P2P** | TCP/QUIC + Noise | Same WiFi/network | 70+ Mbps, instant |
| **2. Relay via RBN** | libp2p relay circuit + gossipsub | Different networks | Variable, depends on relay |
| **3. VPN Tunnel** | WebSocket tunnel (port 80/443) | Strict firewall, VPN | Slowest, last resort |

### 1.4 Key Protocols

- **Gossipsub:** Group messages + file transfers (per-transfer topics)
- **Request-Response:** Direct P2P signaling (JSON codec v1, binary v2)
- **Relay Circuit:** libp2p relay through RBN for cross-network
- **WebSocket Tunnel:** TCP→WebSocket proxy for firewall bypass
- **RBN Mailbox:** Persistent message storage on RBN for offline delivery

---

## 2. Issues Encountered and Fixes Applied

### 2.1 Cross-Network File Transfer (CRITICAL)

**Problem:** File transfers between devices on different networks stalled at 0%. Relay circuits never established. Gossipsub messages not forwarded through RBN.

**Root Causes:**
1. RBN didn't subscribe to `file-transfer-*` gossipsub topics
2. Gossipsub handler rejected file-transfer topics (group membership check)
3. File transfer code inside `is_connected` check (false for cross-network peers)
4. No initial chunk requests sent when transfer started
5. Event loop starved by swarm events (HandleIncomingPayload never processed)

**Fixes Applied:**
- RBN auto-subscribes to `file-transfer-*` topics on first message
- Gossipsub handler bypasses group membership check for file-transfer topics
- Gossipsub publish moved before `is_connected` check
- Initial chunk requests sent immediately on transfer creation
- Command drain loop (`try_recv`) before `tokio::select!` with `biased;`

### 2.2 VPN Tunnel Instability

**Problem:** VPN detection via `connectivity_plus` had false positives, causing tunnel resets that dropped relay circuits.

**Root Causes:**
1. `ConnectivityResult.vpn` reported falsely on non-VPN connections
2. VPN detection triggered tunnel reset on every connectivity change
3. Tunnel stale detection (120s) too aggressive for VPN connections

**Fixes Applied:**
- VPN detection removed from `connectivity_listener.dart`
- VPN block removed from `SetConnectivityType` handler
- TLS→plaintext tunnel fallback (try port 443, fall back to port 80)
- Tunnel stale threshold increased: 120s → 300s
- Rate limiting on VPN tunnel activation (120s cooldown)

### 2.3 Relay Circuit Flapping

**Problem:** Relay circuits dropped briefly during file transfers, causing stalls.

**Root Cause:** `ListenerClosed` event removed `relay_reservations`, causing status to drop to "no relay".

**Fix:** Don't remove `relay_reservations` on `ListenerClosed`/`ListenerError`. Keep reservation for status stability.

### 2.4 File Transfer Pacing

**Problem:** File transfers through relay took 3-4 minutes for a single image.

**Root Cause:** Old parameters: 64KB chunks, 4 in-flight, 2000ms initial delay.

**Fix:** Optimized to 256KB chunks, 8 in-flight, 500ms initial delay (~10x throughput improvement).

### 2.5 Cross-Network Transfer Delay

**Problem:** Cross-network file transfers stalled 3-4 minutes before starting.

**Root Cause:** Relay reservation takes time to establish. No proactive re-request.

**Fix:** Relay reservation timer reduced from 30s to 10s. Device re-requests reservation every 10s until accepted.

### 2.6 Android Stability Issues

**Problems:**
- `ForegroundServiceDidNotStartInTimeException` on API 34+
- `ForegroundServiceStartNotAllowedException` on API 31+ background starts
- Battery optimization prompt violates Google Play policy
- FFI panics caused JVM crashes

**Fixes:**
- `startForegroundCompat()` with `FOREGROUND_SERVICE_TYPE_SPECIAL_USE`
- `ForegroundServiceStartNotAllowedException` catch
- Battery optimization prompt removed
- `ffi_catch!` macro wrapping all 170 FFI functions

---

## 3. Current Network Configuration

### 3.1 RBN Server

- **Host:** 47.89.252.80 (Alibaba Cloud ECS, 1GB RAM)
- **Binary:** `introvertd` — RBN daemon with mailbox + V2 token + gossipsub auto-subscribe
- **Ports:** 443 (libp2p TCP+QUIC), 80 (WebSocket tunnel)
- **Services:** `introvertd.service` (systemd), `introvert-solana.service` (disabled)

### 3.2 Key Constants

| Constant | Value |
|----------|-------|
| RBN PeerID | `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a` |
| Tunnel URL (TLS) | `wss://47.89.252.80/tunnel` |
| Tunnel URL (plaintext) | `ws://47.89.252.80:80/tunnel` |
| V2 Token Mint | `FhKJjqpsCbymrk4Ntv5jFyZihHsAkW4Fb4fuJYBniydP` |
| Treasury | `DZWeLhjPeH3q4Z45HyTh5BbWXiuXdHKK7od4yR9wGLQm` |

### 3.3 Relay Parameters

| Parameter | Value |
|-----------|-------|
| Max circuit bytes | 1 GB |
| Max circuit duration | 1 hour |
| Max reservations | 8192 |
| Max circuits | 4096 |
| Relay reservation retry | 10 seconds |
| Tunnel stale threshold | 300 seconds |
| Mobile tunnel stale threshold | 15 seconds |

### 3.4 File Transfer Parameters

| Parameter | Value |
|-----------|-------|
| Chunk size (relay) | 256 KB |
| Chunk size (direct) | 256 KB |
| In-flight limit | 8 |
| Initial relay delay | 500 ms |
| Gossipsub topic | `file-transfer-{transfer_id}` |
| Max incoming transfers | 50 |

---

## 4. Open Questions for Expert Review

1. **Relay circuit stability:** The relay circuits still flap during network transitions. Is there a way to make them more resilient without adding excessive overhead?

2. **Gossipsub scaling:** Each file transfer creates a new gossipsub topic. Does this scale to hundreds of concurrent transfers? What are the memory/bandwidth implications?

3. **RBN capacity:** The RBN has 1GB RAM. With 8192 max reservations and 4096 max circuits, is this sufficient? What happens when limits are hit?

4. **WebSocket tunnel performance:** The TCP→WebSocket proxy adds overhead. Is there a more efficient tunneling approach for firewall bypass?

5. **Mobile data optimization:** On cellular data, the tunnel is kept active permanently. Is this battery-efficient? Are there better strategies?

6. **Offline message delivery:** The RBN mailbox stores messages for offline peers. How does this interact with the gossipsub message routing? Are there consistency issues?

7. **Security:** File transfer payloads are base64-encoded JSON over gossipsub. Is this secure enough? Should we use the binary v2 codec for file transfers?

8. **IPv6 support:** The code has IPv6 listeners. Does the RBN support IPv6? Are there NAT64 considerations for mobile carriers?

---

## 5. Test Results Summary

| Scenario | Status | Notes |
|----------|--------|-------|
| Same-network file transfer | ✅ Working | Instant, direct P2P |
| Different-network file transfer | ✅ Working | 10-20s delay (relay establishment) |
| VPN file transfer | ✅ Working | Delays due to tunnel establishment |
| Group messaging | ✅ Working | Gossipsub broadcast |
| RBN mailbox | ✅ Working | Offline message delivery |
| Token V2 | ✅ Applied | All addresses updated |
| Android FGS | ✅ Fixed | API 29+/31+ handled |
| FFI safety | ✅ Fixed | 170 functions wrapped |
