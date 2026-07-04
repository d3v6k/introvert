# Cross-Network Messaging & File Transfer — Failure Analysis & Remediation Plan

**Created:** 2026-07-01  
**Scope:** All three failure scenarios: different networks, VPN, mobile data  
**Priority:** P0 — Blocking core product functionality  
**Status:** RESOLVED — Cross-network file transfers verified working on VPN and mobile data (v51)

---

## Executive Summary

Three scenarios cause messaging and file transfer to fail:

1. **Devices on different networks** — relay circuit dial never completes; file chunks pile up in RAM and are dropped
2. **Android on VPN** — VPN interface addresses corrupt relay reservation; stale relay reservation detected too slowly (2 min); QUIC UDP blocked by VPN  
3. **Android on mobile network** — No network-type signal from Flutter to Rust; QUIC/UDP often blocked by carrier; exponential backoff delays relay retry for up to 5 minutes

The root causes are **layered** — each scenario has 2–4 distinct failure modes that stack.

---

## Confirmed Root Causes (Prioritized)

### ROOT CAUSE 1 — `InboundCircuitEstablished` Does NOT Flush Pending Chunks [CRITICAL]

**File:** `src/network/mod.rs`, Lines 1442–1448

When the receiver establishes an inbound circuit to the sender through an RBN relay, the
`InboundCircuitEstablished` event fires on the SENDER's side. This is the critical moment
when file chunks should be flushed to the receiver. Currently:

```rust
// CURRENT (incomplete):
libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
    debug!("[Relay] InboundCircuitEstablished from {}", src_peer_id);
    let _ = self.swarm.dial(src_peer_id); // DCUtR only
}
```

No pending messages or file chunks are flushed! The `OutboundCircuitEstablished` handler
(lines 1412–1440) flushes chunks, but targets `connected_peers` — not the specific receiver
who just connected. This means chunks pile up and are never delivered.

**The fix:** When `InboundCircuitEstablished` fires, flush all pending messages AND
pending DB file chunks for `src_peer_id`.

---

### ROOT CAUSE 2 — VPN Corrupts Relay Reservation Address [HIGH]

**File:** `src/network/mod.rs`, Lines 1251–1267

The relay reservation address selection in the Identify handler:
```rust
// CURRENT (broken — accepts VPN addresses):
let base_addr = info.listen_addrs.iter()
    .find(|a| !a.to_string().contains("127.0.0.1") && !a.to_string().contains("192.168"))
    .or_else(|| info.listen_addrs.first())
    .cloned();
```

This filter skips loopback (127.0.0.1) and LAN (192.168.x.x) addresses but does NOT
skip VPN tunnel interface addresses (10.x.x.x or 172.x.x.x). When Android has a VPN:
- The VPN tun interface gets a 10.x.x.x address
- The filter may select a VPN-tunneled address for the relay reservation
- The reservation appears accepted but is functionally dead
- All circuit dials fail silently

**The fix:** Use positive matching for public IPs, or add 10.x and 172.x to the exclusion filter.

---

### ROOT CAUSE 3 — QUIC/UDP Blocked by VPN or Mobile Carrier [HIGH]

**File:** `src/network/config.rs`

Bootstrap addresses likely only include QUIC (UDP port 443). When:
- A VPN blocks UDP traffic (very common on corporate/privacy VPNs)
- A mobile carrier blocks UDP port 443 (common on restricted data plans)

The libp2p QUIC transport silently fails. There is no automatic TCP fallback for the
same RBN address.

**The fix:** Add TCP addresses (port 443 AND port 80) alongside QUIC in bootstrap list.
Both src and for_linux config.rs need updating.

---

### ROOT CAUSE 4 — Flutter Does NOT Pass Network Type to Rust [HIGH]

**File:** `lib/src/native/introvert_client.dart:1461`, `lib/src/ui/main_shell.dart:163`

`startNetwork()` accepts no connectivity type. Rust uses identical QUIC-first connection
strategy regardless of whether device is on WiFi, LTE/5G, or VPN. The Flutter layer
DOES read `ConnectivityResult` (wifi/mobile/vpn) from the connectivity package, but
never passes it to the native layer.

**The fix:** Add `network_set_connectivity_type(u8)` FFI function. Call from Flutter on
connectivity change with the actual network type. Rust adjusts strategy (prefer TCP on
mobile/VPN).

---

### ROOT CAUSE 5 — VPN Stale Reservation Detected Too Slowly [MEDIUM]

**File:** `src/network/mod.rs`, Line 364

`status_check_interval = 120s` (2 minutes). VPN stale reservation detection only runs
every 2 minutes. This means up to 2 minutes of broken messaging after a VPN connects/disconnects.

**The fix:** Reduce to 30s. Also trigger immediate re-reservation on AutoNAT StatusChanged.

---

### ROOT CAUSE 6 — Exponential Backoff Delays Relay Retry for Text Messages [MEDIUM]

**File:** `src/network/mod.rs`, Lines 2128–2140

For text messages (non-chunk), `dial_relay_path` applies exponential backoff:
5s, 10s, 20s... up to 300s (5 minutes). On a different network where direct P2P always
fails, this means the relay circuit retry is delayed by minutes.

While the mailbox provides a fallback, ACKs and delivery confirmations still need the
relay circuit. Without it, message status stays at "in mailbox" indefinitely.

**The fix:** For peers known to be relay-only (in `is_relayed_map`), skip exponential
backoff and retry relay dial every 30s via the status_check_interval.

---

## Implementation Plan

### Phase 0: Immediate Fixes (30 min) — P0 Critical

#### Fix 0.1 — InboundCircuitEstablished: Flush Pending Chunks to Receiver

**File:** `src/network/mod.rs`, Lines 1442–1448

Replace the current minimal handler with chunk-flushing logic. This is the single most
impactful fix — it's why file chunks never arrive on different networks.

#### Fix 0.2 — VPN Address Filter in Relay Reservation

**File:** `src/network/mod.rs`, Lines 1251–1267

Change from negative exclusion to positive public-IP check. Also apply to the
status_check_interval resilience Step 1 relay re-request.

#### Fix 0.3 — Reduce status_check_interval to 30s

**File:** `src/network/mod.rs`, Line 364

Change from 120s to 30s for faster VPN stale reservation recovery.

---

### Phase 1: Transport Redundancy (45 min) — High Impact

#### Fix 1.1 — Add TCP Bootstrap Addresses

**File:** `src/network/config.rs`

Add TCP addresses (port 443 AND port 80) to the bootstrap list alongside QUIC addresses.
The RBN must be verified to listen on these ports. This ensures dial_relay_path will
automatically try TCP when it iterates bootstrap_nodes and QUIC fails.

#### Fix 1.2 — Verify RBN TCP Listener

```bash
ssh root@47.89.252.80 "ss -tlnp | grep introvertd"
```

If RBN only listens on UDP/QUIC 443, update for_linux config and redeploy.

---

### Phase 2: Network-Type Awareness (1 hr) — Medium Impact

#### Fix 2.1 — Add network_set_connectivity_type FFI

**File:** `src/lib.rs` (new FFI export)
**File:** `src/network/types.rs` (new NetworkCommand variant)
**File:** `src/network/mod.rs` (command handler)

When network type is mobile (2) or VPN (3):
- Clear relay_dial_limiter to remove backoff penalties
- Force-clear stale relay reservations
- Prioritize TCP bootstrap addresses for immediate redial

#### Fix 2.2 — Flutter Calls setConnectivityType on Change

**File:** `lib/src/native/introvert_client.dart` (add FFI binding + method)
**File:** `lib/src/ui/main_shell.dart` (call on connectivity change)

---

### Phase 3: Verify and Test (45 min)

Test scenarios:
1. Mac WiFi ↔ Android Mobile Data: send text + 5MB file
2. Mac WiFi ↔ Android with VPN active: send text + file
3. Toggle VPN mid-transfer: verify recovery within 30s
4. Enable airplane mode on Android, re-enable: verify messages drain from mailbox

---

## Build & Deploy Requirements

After Rust changes to `src/`:
```bash
make android  # rebuilds libintrovert.so
flutter run -d <device>  # installs updated APK
```

After `for_linux/` changes:
```bash
./deploy_rbn.sh  # recompiles and redeploys introvertd to root@47.89.252.80
```

After Dart-only changes in `lib/`:
```bash
flutter run -d <device>  # hot restart is sufficient
```

---

## Files to Modify

| File | Phase | Changes |
|------|-------|---------|
| `src/network/mod.rs` | 0 | InboundCircuit flush, VPN addr filter, status_check_interval |
| `src/network/config.rs` | 1 | TCP + QUIC bootstrap addresses |
| `src/network/types.rs` | 2 | SetConnectivityType command |
| `src/lib.rs` | 2 | network_set_connectivity_type() FFI export |
| `lib/src/native/introvert_client.dart` | 2 | setConnectivityType() FFI wrapper |
| `lib/src/ui/main_shell.dart` | 2 | Pass ConnectivityResult to native on change |
| `for_linux/src/network/config.rs` | 1/3 | TCP listener addresses for RBN |

---

## Implementation Status

- [x] Fix 0.1 — InboundCircuitEstablished flush pending chunks/messages
- [x] Fix 0.2 — VPN-safe relay reservation address filter (three-tier fallback, deployed to RBN)
- [x] Fix 0.3 — status_check_interval 120s → 30s
- [ ] Fix 1.1 — TCP bootstrap addresses in config.rs
- [ ] Fix 1.2 — Verify/fix RBN TCP listener
- [x] Fix 2.1 — network_set_connectivity_type FFI
- [ ] Fix 2.2 — Flutter setConnectivityType call
- [x] Phase 3 — Device testing (cross-network file transfers verified working on VPN and mobile data)

---

## v40 vs Current Comparison

The networking issues were NOT fundamentally introduced by the Intro Codec (v2.0.0).
The Intro Codec is declared in the behaviour stack but is UNUSED (no handler for
`RequestResponseV2` events, `forward_to_mesh` always uses the v1 `request_response`).

The stability difference between v40 and current is:
- v40 testing was likely done on same-network scenarios
- The relay infrastructure and circuit establishment code is the same
- The critical missing piece is reliable chunk delivery via InboundCircuitEstablished


---

## CHANGES IMPLEMENTED (2026-07-01)

### ✅ Fix 0.1 — InboundCircuitEstablished: Flush Pending Chunks/Messages to Receiver
**File:** `src/network/mod.rs`, Lines 1465–1524  
**What changed:** The `InboundCircuitEstablished` event handler now:
1. Clears the `relay_dial_limiter` for `src_peer_id` (removes backoff penalty)
2. Attempts DCUtR hole-punch (unchanged)
3. **NEW:** Flushes all RAM-buffered pending messages to `src_peer_id` with 150ms stabilization delay
4. **NEW:** Flushes all DB-persisted pending file chunks (100 at a time, 50ms pacing, 400ms stabilization delay)

This is the primary fix for cross-network file transfer failure. The `OutboundCircuitEstablished` 
flush targets `connected_peers()` which doesn't include the receiver yet. `InboundCircuitEstablished` 
fires at exactly the right moment — when the receiver has connected through the relay.

### ✅ Fix 0.2 — VPN-Safe Relay Reservation Address Filter (Three-Tier Fallback)
**Files:** `src/network/mod.rs` (client Identify handler), `for_linux/src/network/mod.rs` (RBN daemon Identify handler)
**What changed:** The relay reservation address selection now uses a **three-tier fallback** 
to prioritize routeable public IPs over VPC/private addresses:

1. **`bootstrap_nodes` lookup** (first choice): Looks up the peer in `self.bootstrap_nodes` — 
   the hardcoded RBN addresses used to connect. These are always public IPs.
2. **`anchor_mappings` lookup** (second choice): Checks `self.anchor_mappings`, populated in 
   `ConnectionEstablished` whenever a non-relayed connection is established. Captures the 
   actual endpoint address of direct connections.
3. **Filtered `info.listen_addrs`** (last resort): Falls back to the peer's Identify listen 
   addresses, but filters out ALL private/VPN ranges:
   - `127.x.x.x` — loopback
   - `192.168.x.x` — LAN
   - `localhost` — hostname
   - `10.x.x.x` — VPN tun interface / AWS private / Alibaba private
   - `172.16.x.x–172.31.x.x` — VPN tun / Docker private bridge
   Prefers addresses containing `/ip4/` or `/ip6/` transport protocols.

4. **Fallback**: If no base_addr at all, constructs relative `/p2p/<peer_id>/p2p-circuit`.

**Root cause fixed:** RBN runs inside Alibaba Cloud VPC with private IP `172.19.0.4`. 
Previously, `info.listen_addrs` was checked first, causing relay reservation dials to 
the private IP, which failed silently until the 30s status_check_interval retry with 
the public IP. Now the public IP from `bootstrap_nodes` is always tried first.

**Note:** The legacy `stable_v12` file has no such priority — it only checks `info.listen_addrs` 
with a simpler `127.0.0.1`/`192.168` filter and no `bootstrap_nodes`/`anchor_mappings` lookup.

### ✅ Fix 0.3 — Reduced status_check_interval to 30s
**File:** `src/network/mod.rs`, Line 364  
**What changed:** VPN stale reservation detection now runs every 30s (was 120s = 2 minutes).
This means VPN-invalidated reservations are detected and fixed 4x faster.

### ✅ Fix 0.4 — Relay In-Flight Limit Restored to v40 Values
**File:** `src/network/mod.rs`, Line 2293  
**What changed:** In-flight limits restored: relay=4 (was 8), direct=8 (was 12).
The doubled values caused relay `ResourceLimitExceeded` errors on congested RBNs.
These values match the stable v40 baseline.

### ✅ Fix 0.5 — relay_dial_limiter Cleared on ForceMeshRefresh
**File:** `src/network/mod.rs`, Lines 3467–3471  
**What changed:** `ForceMeshRefresh` (called by Flutter on every network connectivity change) 
now also clears the `relay_dial_limiter`. Previously, exponential backoff penalties accumulated 
from failed dials on the old network (e.g. WiFi) would delay message delivery on the new 
network (e.g. mobile) for up to 5 minutes. This is now reset on every network change.

### Compile Status
`cargo check` passes — 31 pre-existing warnings, 0 errors. ✅

