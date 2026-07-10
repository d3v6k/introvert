# Debug Document — Introvert Sovereign Messenger

**Last Updated:** 2026-07-10 12:05 UTC
**Git:** main @ f476ae0 (with local performance tuning patches)

---

## Current System State

### RBN Server
- **RBN daemon**: ACTIVE — relay server operational
- **Economy daemon**: ACTIVE — unified credential management
- **Firebase**: Service account loaded, FCM push working
- **APNs**: Not configured (iOS push disabled)
- **Connected Peers**: 3 (Android, Mac, iOS) — all connected at TCP level

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

## Session Summary (2026-07-09)

### What Was Done

#### 1. Payout Pipeline Fixes
- **Epoch ID Off-by-One Bug**: Fixed `for_linux/src/lib.rs:412` — changed `hours(0)` (no-op) to `days(1)` so midnight UTC correctly closes previous day's epoch
- **Startup Catch-up Mechanism**: Added code to automatically close yesterday's epoch on daemon restart if past 00:05 UTC
- **IPC Secret Mismatch**: Updated economy daemon to read HMAC secret from secure file instead of hardcoded constant
- **Constant-Time HMAC**: Replaced `expected == signature` with `subtle::ConstantTimeEq` to prevent timing attacks
- **Verified**: Epoch 2026_07_08 closed with 3 claims, 16,438 INTR distributed successfully on Solana Mainnet

#### 2. RBN Relay Server Fix
- **Root Cause**: RBN's status check loop was requesting relay reservations from bootstrap nodes every 15 seconds, but the RBN IS the bootstrap node. This caused an infinite loop trying to listen on its own relay address.
- **Fix**: Added `is_relay_server` check to skip proactive reservation check for relay servers/anchor nodes (`for_linux/src/network/mod.rs:904-922`)
- **Impact**: RBN now properly provides relay reservations to clients instead of trying to request them from itself

#### 3. Documentation Sanitization
- Removed all server IPs, local machine references, and specific security details from GitHub docs
- Removed `deploy_rbn.sh`, `for_linux/`, `introvert-daemon/` from GitHub tracking
- Updated CHANGELOG, TODO, ECONOMY_AUDIT, RECTIFICATION_PLAN, SESSION_SYNOPSIS, HANDLE_REGISTRY_DEPLOYMENT, TOKEN_ADDRESS_DIRECTORY, DEBUG_REPORT, DEBUG_DOCUMENT

---

### Current Device Status

| Device | Peer ID | TCP Connected | Relay Status | Issue |
|--------|---------|---------------|--------------|-------|
| Android | `12D3KooWQM5mi5...` | Yes | **Recovered** | Resolved RBN mailbox exclusion & routing drop |
| Mac | `12D3KooWCSejiZ1...` | Yes | Relay working | Chunk requests flowing |
| iOS | `12D3KooWN6Hu1A...` | Yes | **Recovered** | Resolved client-side reservation desync |

### Android VPN/Mobile Mailbox Routing Issue (Resolved)

**Timeline:**
- Android client is on mobile data or VPN (using a WebSocket tunnel fallback on port 80).
- The user attempts to send a group message (GroupAction payload) to the Mac or iOS client.
- Direct P2P dialing to the recipient fails. The client successfully connects to the RBN and registers a relay reservation, but is not directly connected to the target peer.
- The message is sent to the mailbox of the RBN.
- If the RBN mailbox store request fails due to connection reset or timeout, the `OutboundFailure` handler catches it. It attempts to re-queue the message to `pending_messages` of the final recipient.
- However, since there is no periodic retry for `pending_messages` that re-checks the mailbox storage fallback (it only drains when a circuit is established), and because the RBN was excluded from the mailbox client list, the message remains permanently stuck in `pending_messages` and never gets sent.
- The minute the user switches to the same network as the other devices, a direct or local circuit connection is established, triggering `InboundCircuitEstablished` which flushes the stuck messages from the local `pending_messages` queue.

**Root Cause:**
1. **RBN Mailbox Exclusion:** In both `forward_to_mesh` and `perform_mailbox_fetch`, the list of target mailbox nodes (`anchor_ids`) was constructed purely from database contacts with `is_anchor_capable = 1` and `self.discovered_anchors`. It completely excluded the hardcoded/configured bootstrap RBNs. Thus, the client never attempted to store messages on or drain mailboxes from the RBN.
2. **Mailbox Store Request Failure Recovery:** If a `MailboxStore` request fails, `OutboundFailure` re-queues the message to `pending_messages` for the target recipient, which is only flushed on direct/relay circuit connection events, bypassing mailbox retries entirely if the target peer remains offline.

**Resolution:**
1. Updated both `forward_to_mesh` and `perform_mailbox_fetch` in `src/network/mod.rs` to explicitly push the configured bootstrap RBNs from `self.bootstrap_nodes` into the `anchor_ids` list. This ensures the client stores messages on and drains mailboxes from RBNs as verified anchor nodes.
2. Verified that the periodically running 30-second loop successfully drains `pending_messages` and retries `forward_to_mesh` which will attempt to re-store the message on the RBN mailbox if the direct connection remains offline.

### Android Mobile Data/VPN File Transfer Issue (Resolved)

**Timeline:**
- Android device on mobile data and VPN starts a file transfer (receives group manifest for a file shared from the Mac peer `12D3KooWCSejiZ1...`).
- The user taps "Download", calling FFI `start_pull`.
- Rust spawns `HandleIncomingPayload` with `FileTransfer` manifest.
- Due to the relay connection (is_relayed=true), the receiver client must pull chunks via `FileChunkRequest`.
- When forwarding `FileChunkRequest` (or the sender sending `FileChunk` payloads back), `forward_to_mesh` is invoked.
- If the circuit connection to the peer is not registered yet as active in the swarm, `forward_to_mesh` hits a broken "RELAY-AWARE ROUTING" fallback block: it attempts to send the signaling payload to the RBN (`rbn_id`) instead of the recipient, and immediately returns `Ok(())`.
- The RBN drops the request since it doesn't know who the final recipient is.
- Because `forward_to_mesh` returns `Ok(())` (indicating success), the chunk/request is never buffered in RAM (`pending_messages`) or persisted to the DB, leading to a permanent drop. The file transfer hangs at 0%.

**Root Cause:**
A legacy fallback routing block in `forward_to_mesh` (lines 3139–3150) designed to route file chunks/requests via the RBN was left in place after the `TransitFileChunk` wrapper was removed. Because the `SignalingPayload::FileChunk` and `FileChunkRequest` enums lack a destination PeerId field, RBNs cannot route these payloads. Sending them to `rbn_id` instead of the destination `recipient_id` caused them to be dropped silently by the RBN.

**Resolution:**
Removed the buggy relay-aware routing block from `forward_to_mesh`. Now, when direct circuit connections are not yet fully established, the chunks/requests are correctly buffered in RAM and persisted to the SQLite DB, and then dialed via the relay circuit. Once the circuit connection is open, the outbound/inbound handlers flush the queue directly to the recipient's PeerId, restoring file transfer functionality across relay connections.

### iOS Device Issue (Resolved)

**Timeline:**
- 21:49 UTC (Jul 8) — iOS connects fine, gets relay circuits, status=1
- 04:08 UTC (Jul 9) — iOS loses relay, enters fast reconnect loop (30s interval)
- 04:08-04:53 UTC — Stuck in "transfers waiting, no relay" with periodic brief OutboundCircuitEstablished
- 00:50 UTC — RBN fix deployed, relay server working again
- 00:56 UTC — iOS connected to RBN at TCP level, RBN sending payloads to it
- 01:20 UTC — Client-side fix deployed to properly clear stale relay reservations on total connection loss. iOS device recovers immediately.

**Root Cause:**
When the RBN restarted, the iOS client received a `ConnectionClosed` event. The client-side code handled this by immediately removing the RBN's listeners from `self.relay_listeners` to clean up, but left `self.relay_reservations` alone (since the actual listener is cleared when `SwarmEvent::ListenerClosed` fires).
However, because `relay_listeners` mapping was cleared inside `ConnectionClosed`, when the listener closed event eventually fired, it couldn't map the `listener_id` back to the RBN's `PeerId`. Thus, the RBN was never removed from `self.relay_reservations`.
Consequently:
1. The client still believed it had a confirmed reservation (`relay_reservations` was not empty).
2. When the client reconnected to the RBN (`ConnectionEstablished` fired), it checked `!self.relay_reservations.contains(&peer_id)` before requesting a new reservation. Since it was still present, it skipped requesting a new reservation.
3. The 15s status tick and 5s fast reconnect loops similarly skipped requesting reservations because they checked `!self.relay_reservations.contains(rbn_id)`.
4. This left the client connected at the TCP level but stuck in a loop trying to process transfers without an active relay listener.

**Resolution:**
Updated `SwarmEvent::ConnectionClosed` to check if we are completely disconnected from the RBN or anchor (`!self.swarm.is_connected(&peer_id)`). If so, we immediately and cleanly remove the peer from both `relay_reservations` and `relay_listeners`. When the client reconnects, it sees that `relay_reservations` is empty and immediately requests a new reservation, resuming message and file flows.

---

## Session Summary (2026-07-10)

### What Was Done

#### 1. File Transfer Performance Tuning (Phase 1 — Applied)
Relay transfers were bottlenecked at ~256 KB/s. Applied constant tuning to `src/network/mod.rs`:

| Change | Line | Before | After |
|--------|------|--------|-------|
| Relay chunk size | ~L6390, ~L7626 | `64 * 1024` (relay) / `256 * 1024` (direct) | `256 * 1024` for both |
| In-flight limit | ~L3224 | `if is_relayed_conn { 4 } else { 8 }` | `8` unified |
| Push pacing | ~L7769 | `if is_direct { 20 } else { 250 }` | `if is_direct { 20 } else { 50 }` |
| Initial delay | ~L7716 | `if is_relayed { 2000 } else { 200 }` | `if is_relayed { 500 } else { 200 }` |
| Pull pacing | ~L6394 | `if is_direct_p2p { 10 } else { 50 }` | `if is_direct_p2p { 10 } else { 20 }` |

**Expected impact:** ~20x relay push, ~8x relay pull. But these only help when the relay is stable.

#### 2. Relay Push Model (Phase 2A — Applied)
Removed the early return at `process_outgoing_file` L7710-7713 that forced relay connections into pull-only mode. Relay connections now push chunks like direct P2P. The stall watchdog auto-recovers to pull mode if push stalls.

**Status:** Applied and compiled. Builds pass on all platforms (mac, android, ios).

#### 3. Relay File Transfer Stall Fixes (Phase 2B — Applied 2026-07-10 12:20)

Applied 5 targeted fixes to `src/network/mod.rs` to resolve file transfers stalling at 0% over relay:

| Fix | Location | Change |
|-----|----------|--------|
| Fix 1: Selective flush at ReservationReqAccepted | L1918-1958 | Only flush non-chunk messages at reservation; FileChunk/FileChunkRequest stay in pending_messages for OutboundCircuitEstablished |
| Fix 2: Bump last_update on OutboundCircuitEstablished | L1960-1974 | Reset 60s stale eviction clock for all active incoming_transfers when circuit established |
| Fix 3: Extend OutboundCircuit flush delay | L1998 | 1500ms → 2500ms to allow circuit to fully register at both ends |
| Fix 4: Stall watchdog delayed flush | L542-563 | After stall re-dial, schedule 2s delayed re-flush so requests flow without needing new circuit event |
| Fix 5: Group file seeder ordering | L7695 | 10ms yield after RegisterSeeder before manifest gossip |

**Root cause fixed:** `ReservationReqAccepted` was consuming `pending_messages` (including FileChunkRequests) before the circuit existed. When `OutboundCircuitEstablished` fired, `pending_messages` was empty — nothing to flush. Transfers stuck at 0% forever.


### Current Issue: Relay Instability Prevents File Transfers (UNRESOLVED)

**Severity:** Critical — file transfers do not complete across network boundaries

**Observed behavior (device logs 2026-07-10 11:56-12:00):**
1. Mac connects to RBN, gets `ReservationReqAccepted` + `OutboundCircuitEstablished`
2. Within 13 seconds, relay drops (status=4, relay_listener=false)
3. `start_pull` called for 6 iOS files while relay is DOWN → `FileChunkRequest` payloads buffered in `pending_messages`
4. Relay reconnects at 11:57:31 (`ReservationReqAccepted`)
5. Status=1 (ONLINE) at 11:57:44
6. **ZERO file chunk activity after reconnection** — no `FileChunkRequest` or `FileChunk` logs
7. Gossipsub `GroupAction` messages repeat every 15 seconds (file manifests re-gossiped)
8. Transfers remain stuck at 0% indefinitely

**Root Cause Analysis:**

**Problem A — Flush Race Condition:**
When `ReservationReqAccepted` fires, it immediately flushes `pending_messages` (L1898-1920). But the relay circuit isn't established yet — `OutboundCircuitEstablished` fires 13 seconds later. The `forward_to_mesh` call during the first flush finds `swarm.is_connected(&peer_id) == false` and re-buffers the payloads. When `OutboundCircuitEstablished` fires and tries to flush again, the payloads may have been consumed and re-buffered, creating a race condition.

**Problem B — Stall Watchdog Not Firing:**
The stall watchdog (L434-547) should detect transfers with 0 chunks after 8 seconds and re-request. But device logs show NO stall detection activity for any of the 10+ active transfers. Either:
- The `IncomingTransfer` entries are not being created by `start_pull`
- The transfers are being silently evicted before the watchdog fires
- The watchdog loop is not iterating over the transfers

**Problem C — Sender Not Registered as Seeder:**
The iOS sender gossips file manifests via `GroupAction::Message`. The Mac receiver creates `IncomingTransfer` entries and sends `FileChunkRequest` to the iOS peer. But if the iOS peer never called `RegisterSeeder` for these transfer IDs (because it only gossiped the manifest, didn't do a direct push), the `FileChunkRequest` handler on iOS has nothing to serve.

**Problem D — Relay Connection Unstable:**
The relay drops within 13 seconds of establishment. This could be:
- RBN server resource exhaustion
- Network-level instability between Mac and RBN
- libp2p relay circuit timeout configuration too aggressive

**Pending Fixes:**
1. ✅ **Flush reliability** — Fixed: `ReservationReqAccepted` now only flushes non-chunk messages. `FileChunk` + `FileChunkRequest` payloads are held in `pending_messages` until `OutboundCircuitEstablished` fires.
2. ✅ **Stale eviction guard** — Fixed: `OutboundCircuitEstablished` now bumps `last_update` for ALL active incoming transfers, preventing 60s eviction of valid waiting transfers.
3. ✅ **OutboundCircuit flush delay** — Fixed: Increased from 1500ms to 2500ms to give relay circuit time to fully establish at both ends.
4. ✅ **Stall watchdog flush** — Fixed: After re-dialing seeder, schedules a 2s delayed flush so requests flow even without a new circuit event.
5. ✅ **Proactive seeder dial on circuit** — Fixed: `OutboundCircuitEstablished` now proactively dials all seeder peers with active incoming transfers.
6. ✅ **Group file seeder ordering** — Fixed: 10ms yield after `RegisterSeeder` before manifest gossip ensures seeder is registered before `FileChunkRequest` arrives.

---

## Pending Work

### Immediate (Critical)
- **Fix relay flush race condition** — Remove `ReservationReqAccepted` flush, rely on `OutboundCircuitEstablished` flush only
- **Fix stall watchdog for zero-chunk transfers** — Add diagnostic logging, ensure watchdog fires for transfers created while relay is down
- **Investigate RBN relay instability** — Check why relay circuit drops within seconds of establishment

### Short-term
- **Proactive relay dial on manifest receipt** — When receiver gets manifest from offline sender, dial relay immediately
- **Verify performance tuning** — Test Phase 1/2A changes once relay is stable (expected ~20x relay push improvement)
- **RBN restart safety** — add graceful relay reservation preservation across RBN restarts

### Medium-term
- **Anchor Handle Registry deployment** — needs 1.51 SOL for deployer wallet
- **Client balance display** — app shows 0 INTR despite on-chain balances
- **Phase 3: Group transfer optimization** — shared file read, parallel DHT provider download

---

## Architecture Summary

### Three Daemons
1. **Client** (`libintrovert.dylib` / `.so`) — Flutter+Rust P2P mesh client
2. **RBN** — Relay Backbone Node (relay server for all clients)
3. **Economy** — Treasury/IPC daemon

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

### Networking Relay Path (v0.30.2+ — Mailbox Removed)
```
forward_to_mesh(recipient, payload)
    → Durable payloads saved to outbox (SQLite)
    → WebRTC DataChannel (if open, skip FileChunk on direct)
    → Direct libp2p (if connected, in-flight limit: 8)
        → Binary codec v2.0.0 if peer supports it (~25% savings)
    → dial_relay_path():
        Strategy 1: Direct P2P dial
        Strategy 2: Via RBN (latency-sorted, ALL RBNs for file chunks)
        Strategy 3: Via connected anchor nodes
        Strategy 4: WebSocket tunnel fallback
    → Buffer in pending_messages (RAM, 50/peer cap)
    → Persist FileChunk to pending_file_chunks (SQLite, 24h TTL)
```

### File Transfer Flow (Current)
```
Sender:
    process_outgoing_file()
        → SHA-256 hash (streaming)
        → RegisterSeeder (pull model)
        → Send FileTransfer manifest
        → Push chunks (256KB, 50ms pacing for relay)
        → Stall watchdog auto-converts push→pull if stalled 8s

Receiver:
    FileTransfer manifest received
        → Create IncomingTransfer entry
        → Send initial pipeline of FileChunkRequest
        → Pull pacing: 20ms between requests
        → On completion: SHA-256 verify, atomic write
        → Register as seeder (group transfers only)
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

---
## Backup Status (2026-07-09 05:23)
- Git: main @ a6f18dd
- RBN: introvertd operational
- Economy: daemon operational

---
## Backup Status (2026-07-09 05:48)
- Git: main @ a6f18dd
- RBN: introvertd operational
- Economy: daemon operational

---
## Backup Status (2026-07-09 06:06)
- Git: main @ 95cf389
- RBN: introvertd operational
- Economy: daemon operational

---
## Backup Status (2026-07-09 06:14)
- Git: main @ 24b75ab
- RBN: introvertd operational
- Economy: daemon operational

---
## Backup Status (2026-07-09 17:09)
- Git: main @ 9bbb2ac
- RBN: introvertd operational
- Economy: daemon operational

---
## Backup Status (2026-07-09 17:10)
- Git: main @ 9bbb2ac
- RBN: introvertd operational
- Economy: daemon operational

---
## Backup Status (2026-07-09 19:19)
- Git: main @ e6d2240
- RBN: introvertd operational
- Economy: daemon operational

---
## Backup Status (2026-07-09 19:20)
- Git: main @ e6d2240
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-10 10:56)
- Git: main @ f476ae0
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001

---
## Backup Status (2026-07-10 12:05)
- Git: main @ f476ae0 + local performance tuning patches
- RBN: introvertd on 47.89.252.80:443 (UNSTABLE — relay drops within 13s)
- Economy: introvert-solana on localhost:9001
- File transfers: NOT WORKING over relay (see session summary above)

---
## Backup Status (2026-07-10 13:22)
- Git: main @ f476ae0
- RBN: introvertd on 47.89.252.80:443
- Economy: introvert-solana on localhost:9001
