# Rectification Plan — 1:1 File Transfer Stall on VPN / Cross-Network
**Date:** 2026-07-09  
**Severity:** High — transfers freeze permanently when one peer is on VPN or a different carrier network  
**Log Evidence:** `/Users/dev/Downloads/introvert_introvert_netlog_1783585654469.txt`  
**Affected Source:** `src/network/mod.rs`

---

## 1. Symptom

A 1:1 (and sometimes group) file transfer stalls indefinitely when the sending peer is on
VPN or a different network (e.g. Android on mobile data / VPN while the iOS receiver is on
Wi-Fi). The transfer UI shows 0 % progress and never recovers without a manual app restart.

---

## 2. Log Evidence Summary

```
[12:24:32] [Resilience] Fast reconnect: transfers waiting, no relay
           (peers=0, incoming=7, seeders=21, pending=1)

[12:26:21] [Relay] InboundCircuit flush: 27 payloads → 12D3KooWN6Hu...
[12:26:21] [Relay] InboundCircuit DB flush: 4 chunks → 12D3KooWN6Hu...

[12:27:07] [Resilience] Fast reconnect: transfers waiting, no relay
           (peers=0, incoming=7, seeders=21, pending=1)
           ← identical counts 3 minutes later — nothing moved
```

Key observations:

| Metric | Value | Meaning |
|--------|-------|---------|
| `incoming=7` | 7 | Active `incoming_transfers` entries, none completing |
| `seeders=21` | 21 | Active seeder registrations — seeder side is healthy |
| `pending=1` | 1 | Single `FileChunkRequest` stuck in `pending_messages` RAM |
| Chunks received | 0 | Zero file data delivered across all 7 transfers |

Circuit establishments and payload flushes fire repeatedly (27 payloads flushed at 12:26:21)
but 0 chunks are ever delivered. The stagnant-messages detector triggers `ForceMeshRefresh`
at 120 s but the zombie `incoming_transfers` survive the refresh and the loop continues.

---

## 3. Root Cause Analysis

### Root Cause A — Zombie `incoming_transfers` Accumulation (PRIMARY)

**Mechanism:**

1. Receiver calls `startPull` for N files on first connect → N entries in `incoming_transfers`
2. The relay circuit breaks (VPN peer changes address / NAT rebind)
3. Flutter re-renders the message list on reconnect and calls `startPull` again for the same
   files, creating **duplicate entries with new timestamp suffixes** in `incoming_transfers`
   that do not match any registered seeder TID
4. Original entries also remain because 0 chunks arrived → `is_complete` never set

**Why it persists:**

The stale-transfer cleanup threshold was **5 minutes (300 s)**:

```rust
// BEFORE (src/network/mod.rs ~L580)
let stale_threshold = Duration::from_secs(300);
let stale_ids = incoming_transfers.filter(elapsed > stale_threshold)
```

A zombie transfer with 0 chunks received keeps `has_pending_transfers = true` for the full
5 minutes, causing the fast-reconnect log to fire every 30 s. In practice the VPN peer never
delivers chunks during a session so the 5-minute timer is never hit.

**Affected lines:** `src/network/mod.rs` L577–589

---

### Root Cause B — Stall Watchdog Doesn't Re-Establish the Relay Circuit

**Mechanism:**

1. The 8 s stall watchdog (`pull_retry_interval`) detects no new chunks
2. It spawns `FileChunkRequest` messages for the first missing chunks
3. `forward_to_mesh` sees the seeder peer is disconnected and buffers requests in
   `pending_messages` (RAM queue)
4. Watchdog fires again 8 s later — same result, buffer deduplicates, no progress
5. **No relay re-dial is attempted at any point in this loop**

The watchdog retried the *request* but never tried to re-establish the *circuit*.
`dial_relay_path` is only called from connection lifecycle events, not from the transfer
stall recovery path.

**Affected lines:** `src/network/mod.rs` L518–554

---

### Root Cause C — DB Chunk Flush Does Not Retry After Circuit Drop

**Mechanism:**

On `OutboundCircuitEstablished`, the DB chunk flush (chunks the sender stored offline)
iterates `connected_peers()` after a 600 ms delay. The seeder may appear briefly connected
but drop before the task runs, or may deliver only the first batch (the log shows exactly
4 chunks flushed each time — the initial fetch limit) and not retry.

The InboundCircuit DB flush (`dequeue_pending_chunks`) is a single one-shot call; if the
circuit closes mid-send the remaining queued chunks are never re-attempted until the next
`InboundCircuitEstablished` event.

**Affected lines:** `src/network/mod.rs` L2083–2110

---

## 4. Rectification Actions

### ✅ Fix 1 — Aggressive Stale Transfer Eviction (APPLIED)

**File:** `src/network/mod.rs` — stale cleanup block ~L577

**Logic change:**

```
Old rule:  evict if last_update.elapsed() > 300s
New rule:  evict if (0 chunks received AND elapsed > 60s)
               OR (any progress AND elapsed > 90s)
```

- **Zero-chunk entries** are evicted after 60 s. These are almost certainly zombie entries
  from a UI re-render or a seeder behind hard NAT that cannot deliver anything.
- **Partially-received entries** are evicted after 90 s of no progress (previously 5 min).
- Eviction log now includes the chunk count at time of removal for post-mortem diagnostics.

**Expected outcome:** The fast-reconnect loop self-heals within 60–90 s instead of running
indefinitely for 5+ minutes.

---

### ✅ Fix 2 — Re-Dial Seeder on Stall Watchdog Retry (APPLIED)

**File:** `src/network/mod.rs` — stalled transfers retry block ~L518

**Logic change:**

```rust
// BEFORE: just spawned requests (buffered in pending_messages if peer offline)
tokio::spawn(async move {
    for idx in first_missing_idx..limit {
        send FileChunkRequest ...
    }
});

// AFTER: re-dial first, then send with 300 ms head-start
for &provider in &providers {
    if !self.swarm.is_connected(&provider) {
        self.relay_dial_limiter.remove(&provider);     // clear exponential backoff
        self.dial_relay_path(provider, true);           // true = file_chunk priority
    }
}
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(300)).await; // let dial establish
    for idx in first_missing_idx..limit {
        send FileChunkRequest ...
    }
});
```

The `for_file_chunk = true` flag on `dial_relay_path` bypasses the exponential backoff
rate-limiter and tries **all** RBNs (not just the first one), which is the right behaviour
for a file chunk re-request that has no mailbox fallback.

**Expected outcome:** Every stall-retry cycle actively re-establishes the relay circuit
before queuing requests, so requests flow immediately once the circuit is up instead of
accumulating in `pending_messages`.

---

### 🔲 Fix 3 — DB Chunk Flush Retry Loop (PLANNED)

**File:** `src/network/mod.rs` — `InboundCircuitEstablished` handler ~L2083

Replace the single one-shot `dequeue_pending_chunks` call with a retry loop that re-polls
every 2 s until the queue is empty or the circuit closes:

```rust
// Planned implementation sketch
tokio::spawn(async move {
    loop {
        let chunks = storage.dequeue_pending_chunks(&peer_str, 20)?;
        if chunks.is_empty() { break; }
        for (tid, idx, data) in chunks {
            send FileChunk ...
            sleep(50ms).await;
        }
        // Re-poll to catch any chunks that arrived during the send
        sleep(2s).await;
        if !swarm.is_connected(&peer_id) { break; }
    }
});
```

**Status:** Not yet applied — requires access to the swarm handle inside the spawn, which
needs a connected-peers watch channel. Deferred to follow-up.

---

### 🔲 Fix 4 — Flutter `startPull` Deduplication Guard (PLANNED)

**File:** `lib/views/group_chat_screen.dart` — `startPull` call site ~L627

Add an FFI query (`has_active_transfer(tid)`) so the Dart layer can skip `startPull` if
Rust already has an active `incoming_transfers` entry for that exact transfer ID. This
prevents the UI re-render on reconnect from creating duplicate zombie entries.

```dart
// Planned guard
if (shouldPull && !_client.hasActiveTransfer(tid)) {
    _client.startPull(...);
}
```

**Status:** Requires a new FFI function. Planned for follow-up.

---

## 5. Files Changed in This Session

| File | Section | Change |
|------|---------|--------|
| `src/network/mod.rs` | L518–554 | Fix 2: re-dial + 300 ms delay on stall-watchdog retry |
| `src/network/mod.rs` | L577–600 | Fix 1: 60 s/90 s stale eviction with chunk-count logging |

---

## 6. Verification Criteria

After rebuilding (`make all`) and deploying to test devices:

### Test A — Zombie Cleanup (Fix 1)
1. Android on VPN, iOS on Wi-Fi. Send a group file from Android.
2. Immediately kill Android VPN connection.
3. **Expected log within 60 s:**
   ```
   [Resilience] Stale transfer evicted: gft_xxxx (0 chunks received)
   ```
   Fast-reconnect messages should stop appearing after eviction.

### Test B — Stall Recovery (Fix 2)
1. Android on VPN, iOS on Wi-Fi. Initiate a 1:1 file send.
2. Let it stall for 15 s.
3. Re-enable Android VPN.
4. **Expected within 10 s of VPN re-enable:**
   ```
   [Mesh] Transfer gft_xxxx stalled. Retrying PULL for chunks 0..N
   [Relay] 🔌 OutboundCircuitEstablished via 12D3KooWJq...
   [DEBUG] Found transfer in incoming_transfers. Decoded chunks so far: 1
   ```
   Transfer resumes.

### Test C — Regression (happy path)
1. Same Wi-Fi, both peers. Large file transfer.
2. Confirm completes at full speed with no regressions to chunk serving.

---

## 7. Future Hardening Backlog

| Priority | Item | Description |
|----------|------|-------------|
| High | Chunk ACK receipts | Receiver sends `FileChunkAck` per chunk. Seeder knows which chunks were dropped vs received, enabling targeted re-sends. |
| High | Adaptive chunk size | Reduce chunk size from 64 KB → 16 KB for relayed/VPN peers (detected via `is_relayed_map`) to stay within relay frame limits. |
| Medium | DB flush retry loop | Fix 3 above — persistent retry until queue drained. |
| Medium | Flutter pull dedup | Fix 4 above — FFI guard to prevent zombie `startPull` on reconnect. |
| Low | Transfer health event | Emit a SwarmEvent (Type=X) to Flutter when a transfer is evicted as stale, so the UI can show a "Transfer failed — tap to retry" prompt. |
