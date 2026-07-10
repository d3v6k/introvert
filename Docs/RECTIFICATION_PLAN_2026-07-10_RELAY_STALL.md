# Rectification Plan — Relay File Transfer Stall (2026-07-10)

**Date:** 2026-07-10  
**Severity:** Critical — file transfers do not complete across network boundaries (relay)  
**Affected Source:** `src/network/mod.rs`  
**Git:** main @ f476ae0 + local performance tuning patches  

---

## 1. Symptom

File transfers stall at 0% indefinitely when peers are on different networks (relay path).
Observed on:
- Mac ↔ iOS cross-network  
- Android VPN ↔ iOS/Mac  
- Any peer pair where direct P2P is not possible  

**Device log evidence:**
```
11:56:46  ReservationReqAccepted + OutboundCircuitEstablished
11:56:59  Status=4 (relay_listener=false) — relay dropped after 13s
11:57:31  ReservationReqAccepted (reconnect)  ← premature flush fires HERE
11:57:44  Status=1 (relay_listener=true)      ← OutboundCircuitEstablished HERE (13s later)
           ZERO file chunk activity after this point — transfers stuck at 0%
```

---

## 2. Root Cause Analysis

### Bug A — Flush Race Condition in ReservationReqAccepted (CRITICAL)

**Location:** `src/network/mod.rs` L1898-1920

**Mechanism:**
1. Relay drops → `start_pull` called → `FileChunkRequest` payloads buffered in `pending_messages`
2. `ReservationReqAccepted` fires → code immediately flushes ALL `pending_messages` via `ForwardMeshSignaling`
3. But at this moment `OutboundCircuitEstablished` has NOT fired yet — `swarm.is_connected(peer_id) == false`
4. `forward_to_mesh` sees peer not connected → re-buffers payloads back into `pending_messages`
5. **BUT**: `pending_messages.remove()` at L1946 already consumed the entries during step 2!
6. When `OutboundCircuitEstablished` fires 13 seconds later, there is nothing left to flush
7. Transfers stay at 0% indefinitely

**Impact:** Every relay reconnect consumes and loses pending chunk requests.

---

### Bug B — Stall Watchdog Retry Has No Flush Trigger

**Location:** `src/network/mod.rs` L498-547

**Mechanism:**
1. Stall watchdog re-dials seeder + sends chunk requests into `pending_messages`
2. The requests sit in `pending_messages` until the NEXT circuit event fires
3. If no new circuit event fires (circuit already established but seeder not connected), requests stay pending forever
4. Missing: explicit delayed flush after re-dial to drain the newly added requests

---

### Bug C — Stale Transfer Eviction Races with Reconnect (60s)

**Location:** `src/network/mod.rs` L576-591

**Mechanism:**
1. Transfer created while relay is down (0 chunks received)
2. `last_update` bumped on `InboundCircuitEstablished` but NOT on `OutboundCircuitEstablished`
3. If the receiver initiates the outbound circuit and seeder doesn't establish inbound fast enough, the 60s clock keeps ticking from creation
4. Transfer evicted after 60s even though relay is now healthy

---

### Bug D — OutboundCircuitEstablished Flush Uses Stale Peer Snapshot

**Location:** `src/network/mod.rs` L1982-2000, L2005

**Mechanism:**
1. `OutboundCircuitEstablished` fires for relay (RBN), not target peer
2. `connected_peers()` snapshot at L2005 does NOT include target peers yet (circuit not registered)
3. 1500ms delay is sometimes insufficient for target peer connections to establish
4. Result: outbox flush iterates over empty or incomplete peer list

---

### Bug E — Sender Not Registered as Seeder Before Group Manifest Gossip

**Location:** `src/network/mod.rs` L7687-7698

**Mechanism:**
1. `RegisterSeeder` command sent to event loop channel
2. `SendFileChunk` (with manifest) sent immediately after — triggers Gossipsub broadcast
3. Gossipsub delivers to nearby peers near-instantly
4. Peer sends `FileChunkRequest` back before `RegisterSeeder` command is processed
5. `FileChunkRequest` handler finds no seeder — drops the request silently

---

## 3. Fixes Applied

### Fix 1 — Selective Flush at ReservationReqAccepted (Non-Chunks Only)

**File:** `src/network/mod.rs` L1898-1920

Remove `FileChunk` and `FileChunkRequest` payloads from the premature flush. Non-chunk messages (chat, gossip, acks) CAN flow once reservation is accepted. File chunk requests must wait for `OutboundCircuitEstablished`.

```
Before: flush ALL pending_messages at ReservationReqAccepted
After:  flush only NON-chunk pending_messages at ReservationReqAccepted
        FileChunk + FileChunkRequest stay in pending_messages for OutboundCircuitEstablished
```

---

### Fix 2 — Bump last_update on OutboundCircuitEstablished

**File:** `src/network/mod.rs` — OutboundCircuitEstablished handler (after L1925)

Bump `last_update` for all `incoming_transfers` immediately when `OutboundCircuitEstablished` fires. Prevents 60s stale eviction for transfers waiting for circuit flush.

---

### Fix 3 — Increase OutboundCircuitEstablished Flush Delay

**File:** `src/network/mod.rs` L1963

Increase from 1500ms to 2500ms. The relay circuit takes 1-3s to fully establish at both ends. 2500ms gives adequate margin for the seeder to appear in `connected_peers()`.

---

### Fix 4 — Stall Watchdog: Delayed Flush After Re-Dial

**File:** `src/network/mod.rs` L498-547

After re-dialing the seeder, schedule a 2s delayed flush of pending chunk requests for that peer. This ensures requests flow even if no new circuit event fires.

---

### Fix 5 — Group File: Ensure Seeder Registered Before Manifest Gossip

**File:** `src/network/mod.rs` L7687-7698

Move the `RegisterSeeder` command to a `spawn_blocking` / awaited path to ensure it is processed BEFORE the manifest gossip is delivered to peers.

---

## 4. Files Changed

| File | Lines | Fix |
|------|-------|-----|
| `src/network/mod.rs` | L1898-1920 | Fix 1: Non-chunk-only flush at ReservationReqAccepted |
| `src/network/mod.rs` | OutboundCircuit block | Fix 2+3: Bump last_update + increase delay to 2500ms |
| `src/network/mod.rs` | L498-547 | Fix 4: Delayed flush after stall watchdog re-dial |
| `src/network/mod.rs` | L7687-7698 | Fix 5: Seeder ordering for group gossip |

---

## 5. Verification Criteria

### Test A — Relay File Transfer (Primary)
1. Mac and iOS on different networks (one on 4G, one on WiFi)
2. Send group file from iOS
3. Expected: Mac receives chunks within 15s of relay circuit establishing
4. Log: `[Relay] Circuit ready — flushing N queued payloads`

### Test B — Stall Recovery
1. Drop relay mid-transfer
2. Expected: Transfer resumes within 10s of relay reconnect

### Test C — Stale Eviction Correctness
1. Transfer with 0 chunks, relay reconnects
2. Expected: last_update bumped, 60s clock reset, transfer NOT evicted

### Test D — Regression (Happy Path)
1. Both peers on same WiFi — direct P2P
2. Expected: Full speed, no regressions

---

## 6. Future Hardening

| Priority | Item |
|----------|------|
| High | RBN relay instability — why circuit drops within 13s |
| High | start_pull FFI dedup guard — prevent zombie entries on UI re-render |
| Medium | DB chunk flush retry loop |
| Low | Transfer health event to Flutter UI on eviction |
