# Debug Report — 2026-07-10

## Session Summary
Applied file transfer performance tuning (Phase 1 + Phase 2A). Discovered that relay instability prevents file transfers from completing despite the tuning changes. Diagnosed flush race condition and stall watchdog gaps from device logs.

## Changes Applied

### File Transfer Performance Tuning (`src/network/mod.rs`)
| Change | Line | Before | After |
|--------|------|--------|-------|
| Relay chunk size | ~L6390, ~L7626 | `64 * 1024` (relay) / `256 * 1024` (direct) | `256 * 1024` for both |
| In-flight limit | ~L3224 | `if is_relayed_conn { 4 } else { 8 }` | `8` unified |
| Push pacing | ~L7769 | `if is_direct { 20 } else { 250 }` | `if is_direct { 20 } else { 50 }` |
| Initial delay | ~L7716 | `if is_relayed { 2000 } else { 200 }` | `if is_relayed { 500 } else { 200 }` |
| Pull pacing | ~L6394 | `if is_direct_p2p { 10 } else { 50 }` | `if is_direct_p2p { 10 } else { 20 }` |
| Relay push model | ~L7710-7713 | `if is_relayed { return Ok(()) }` | Removed — relay now pushes chunks |

### Build Verification
- `cargo check --lib` — 28 warnings, 0 errors
- `make mac` — success
- `make android` — success
- `make ios` — success

## Issues Discovered

### 1. Relay Flush Race Condition (CRITICAL)
**Problem:** When `ReservationReqAccepted` fires, `pending_messages` are flushed immediately (L1898-1920). But the relay circuit isn't established yet — `OutboundCircuitEstablished` fires 13 seconds later. The `forward_to_mesh` call during the first flush finds `swarm.is_connected(&peer_id) == false` and re-buffers the payloads. When `OutboundCircuitEstablished` fires and tries to flush again, the payloads may have been consumed and re-buffered.

**Log Evidence:**
```
11:57:31 [Relay] ✅ ReservationReqAccepted via 12D3KooWJqi...
         ← pending_messages flushed here, but circuit not ready
11:57:44 [Status] Status change: 1 (peers=3, relay_listener=true)
         ← OutboundCircuitEstablished fired, but payloads already consumed
```

**Fix:** Remove flush at `ReservationReqAccepted`. Only flush at `OutboundCircuitEstablished`.

### 2. Stall Watchdog Not Firing (CRITICAL)
**Problem:** Stall watchdog (L434-547) should detect transfers with 0 chunks after 8 seconds and re-request. Device logs show NO stall detection activity for 10+ active transfers created via `start_pull` at 11:57:01.

**Log Evidence:** No `[Mesh] Transfer ... stalled` logs appear anywhere in the 4-minute log window, despite 10+ transfers being active with 0 chunks received.

**Possible Causes:**
- `IncomingTransfer` entries not created by `start_pull` (FFI → `HandleIncomingPayload` → `FileTransfer` handler)
- Transfers silently evicted by stale cleanup (60s threshold for 0-chunk transfers)
- Watchdog loop not iterating over the transfers

**Fix:** Add diagnostic logging at watchdog entry. Verify `start_pull` creates entries.

### 3. Relay Circuit Instability (CRITICAL)
**Problem:** Relay drops within 13 seconds of establishment. Pattern repeats every ~45 seconds.

**Log Evidence:**
```
11:56:46 ReservationReqAccepted + OutboundCircuitEstablished
11:56:59 Status=4 (relay_listener=false) — relay dropped after 13s
11:57:31 ReservationReqAccepted (reconnect)
11:57:44 Status=1 (relay_listener=true)
         ← relay stable for ~1 minute, then pattern may repeat
```

**Investigate:** RBN server logs, libp2p relay config (`max_circuit_duration`, `max_circuit_bytes`), circuit slot availability.

### 4. Sender Not Registered as Seeder (HIGH)
**Problem:** iOS sender gossips file manifests via `GroupAction::Message`. Mac receiver creates `IncomingTransfer` entries and sends `FileChunkRequest` to iOS peer. But if iOS never called `RegisterSeeder` for these transfer IDs (because it only gossiped the manifest, didn't do a direct push), the `FileChunkRequest` handler on iOS has nothing to serve.

**Fix:** Ensure `process_outgoing_file` always calls `RegisterSeeder` before gossiping the manifest.

## Fixes Applied (Session 2)

### 1. Selective Flush at ReservationReqAccepted (Fix 1 — APPLIED)
**Change:** `ReservationReqAccepted` handler now only flushes non-chunk messages. `FileChunk` and `FileChunkRequest` payloads stay in `pending_messages` to be flushed by `OutboundCircuitEstablished`.
**Location:** `src/network/mod.rs` L1918-1958

### 2. Bump last_update on OutboundCircuitEstablished (Fix 2 — APPLIED)
**Change:** `OutboundCircuitEstablished` handler now bumps `last_update` for ALL active `incoming_transfers`. Prevents 60s stale eviction of valid transfers waiting for relay circuit.
**Location:** `src/network/mod.rs` L1960-1974

### 3. Extend OutboundCircuit Flush Delay (Fix 3 — APPLIED)
**Change:** Flush delay in `OutboundCircuitEstablished` increased from 1500ms to 2500ms.
**Location:** `src/network/mod.rs` ~L1998

### 4. Proactive Seeder Dial on Circuit (Fix 4 — APPLIED)
**Change:** `OutboundCircuitEstablished` now also proactively dials seeder peers for ALL active incoming_transfers (not just those in pending_messages).
**Location:** `src/network/mod.rs` L1975-1988

### 5. Stall Watchdog Delayed Flush (Fix 5 — APPLIED)
**Change:** After stall watchdog re-dials seeder, schedules a 2s delayed flush of pending chunk requests. Requests flow even if no new circuit event fires.
**Location:** `src/network/mod.rs` L542-563

### 6. Group File Seeder Ordering (Fix 6 — APPLIED)
**Change:** 10ms yield after `RegisterSeeder` before manifest gossip ensures seeder is registered before `FileChunkRequest` arrives from peers.
**Location:** `src/network/mod.rs` ~L7695

## Verification Results
- `cargo check --lib` — 31 warnings, 0 errors ✅
- `make mac` — success ✅
- `make android` — success ✅
- `make ios` — success ✅
- **File transfers over relay** — FIXES APPLIED + INTROCLAW STABILITY ENHANCEMENTS INTEGRATED

## Remaining Work
1. Real-device test: Mac ↔ iOS cross-network file transfer under VPN boundary
2. DB chunk flush retry loop

---

## FFI Dedup Guard & Local Drive Verification (Implemented)
- Added `ACTIVE_PULLS` global thread-safe `HashSet` in `src/lib.rs` to track active pull sequences and return early on duplicate `introvert_network_start_pull` calls.
- Integrated a check against `self.storage.get_drive_file_by_hash(&file_hash)` in the `FileTransfer` manifest handler in `src/network/mod.rs` to skip downloading and immediately notify the UI of completion if the file already exists locally.
- Checked and removed from `ACTIVE_PULLS` on success/integrity verification, staleness eviction, and user cancellation.

---

## Session 3 Fixes (Implemented)

### 7. Decoupled Watchdog Retry from Stale Eviction (Fix 7 — APPLIED)
*   **Change:** Added `last_retry: Instant` to the `IncomingTransfer` struct. The watchdog retry check now queries `t.last_retry.elapsed() > watchdog_timeout` and updates `t.last_retry = Instant::now()`, leaving `t.last_update` untouched.
*   **Result:** Restores functionality to the stale eviction timer. Stalled transfers whose seeders are permanently offline can now successfully age out (after 60s/90s of no chunks) instead of being kept alive indefinitely by watchdog resets.
*   **Location:** `src/network/service.rs` L108, `src/network/mod.rs` L469-495, L6732

### 8. Group Info Validation Guard (Fix 8 — APPLIED)
*   **Change:** Added database lookup check for `group_id` on manifest (`FileTransfer`) receipt. If the group info is not found or the group secret is all-zeros (healing pending), the manifest is rejected and ignored early.
*   **Result:** Prevents the client from entering stall loops for files in groups it is not a member of or doesn't have decryption keys for.
*   **Location:** `src/network/mod.rs` L6610-6628

---

## Phase 3: IntroClaw File Transfer Intelligence (Implemented)

Designed and implemented 6 enhancements in `src/intro_claw.rs` and `src/network/mod.rs` to make IntroClaw actively manage and optimize file transfers:

1. **Transfer-Aware ClawTickContext** — Context now includes `active_seeder_peers`, `active_receiver_peers`, `stalled_transfers`, `peer_throughput_bps`, and `has_relay_circuit`.
2. **Transfer-Aware Pre-Warming** — `TransferCircuitPrewarmer` actively dials seeders via relay and maintains the circuit, preventing transfer stalls proactively.
3. **Relay Circuit Health Scorer** — `RelayCircuitHealthScorer` tracks relay drops to identify unstable RBN relay circuits, triggering a ForceMeshRefresh when drops thrash.
4. **Network-Adaptive Transfer Policy** — `get_transfer_policy` selects chunk size, pipeline depth, pacing, and in-flight limits based on network type (WiFi vs cellular vs VPN), battery, and active VoIP calls.
5. **Stall Prediction & Healing** — Timing tracking via `record_chunk_timing` computes EMA inter-chunk intervals, predicting stalls in 3s (down from 8s) to trigger preemptive healing.
6. **Skip rate limit on seeder heal** — `execute_claw_actions` recognizes if a target peer is an active seeder, bypassing the relay rate limiter to dial immediately on heal/prewarm.
