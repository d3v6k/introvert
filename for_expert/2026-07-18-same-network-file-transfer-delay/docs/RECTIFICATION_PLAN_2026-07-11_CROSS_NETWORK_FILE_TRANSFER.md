# Rectification Plan — Cross-Network File Transfer Instability
**Date:** 2026-07-11  
**Severity:** Critical — file transfers do not complete across network boundaries  
**Affected Source:** `src/network/mod.rs`  
**Status:** COMPLETED

---

## 1. Diagnosis Summary

### Current Symptom (from 2026-07-11 10:30 logs)

All three devices (Android, Mac, iOS) are online with relay circuits established (Status=1,
`relay_listener=true`). Text group messages flow correctly. However:

- **File transfers → 0%**: `start_pull` is called → `HandleIncomingPayload` sent → but **no
  `FileChunkRequest` or `FileChunk` ever reaches the seeder** across relay.
- **Android**: Stall watchdog fires continuously (`"Stall retry: delayed flush for 1 provider(s)"`)
  — hundreds of times — but the flush sends nothing because `was_recently_flushed()` filters out
  the `FileChunkRequest` payloads.
- **Mac seeder**: Receives `InboundCircuitEstablished` events and the DB flush sends 2 chunks each
  time, but `mark_flushed` is called before the send, so `FlushPendingForPeer` 500ms later
  filters them out. The file never progresses beyond a trickle.
- **iOS**: Same pattern — `InboundCircuitEstablished` flush sends 8 chunks from DB then nothing.

### Critical Log Evidence

```
# Android log (lines 543–659): Stall watchdog loops forever
[Mesh] Stall retry: delayed flush for 1 provider(s) after re-dial
... (repeated 80+ times — no FileChunkRequest ever sent)

# Mac log (lines 315–316): Only 2 DB chunks per InboundCircuit  
[Relay] InboundCircuit DB flush: 2 chunks → 12D3KooWQM5mi5...

# iOS log (lines 301–302): Only 8 DB chunks per InboundCircuit
[Relay] InboundCircuit DB flush: 8 chunks → 12D3KooWQM5mi5...
```

---

## 2. Root Cause Analysis

### Root Cause 1 — `FlushPendingForPeer` Hits Wrong Branch for Relay Peers (PRIMARY)

**Location:** `src/network/mod.rs` L4764-4782

**Mechanism:** In `FlushPendingForPeer`, the code detects the relay peer is NOT directly
connected (`is_connected=false`) and falls into the **re-buffer + schedule retry** branch:
```rust
if !is_connected && is_relayed {
    self.dial_relay_path(peer_id, true);
    self.pending_messages.entry(peer_id).or_default().extend(sorted);
    // schedule retry...
    return Ok(());
}
```
This means **no payload is ever actually sent** — they cycle through re-buffer → retry → 
re-buffer indefinitely. This explains the endless `"Stall retry: delayed flush"` spam with
zero throughput on Android.

---

### Root Cause 2 — `was_recently_flushed` Blocks File Chunk Sends

**Location:** `src/network/mod.rs` L4744-4747

**Mechanism:** The `FlushPendingForPeer` dedup filter at the top filters out payloads that
were recently flushed. After the stall watchdog buffers a `FileChunkRequest` into
`pending_messages`, `mark_group_action_sent` is called. When `FlushPendingForPeer` re-buffers
due to RC1, subsequent calls see `was_recently_flushed=true` and filter the payload out.
**The payload disappears permanently.**

---

### Root Cause 3 — DB Chunk Drain is One-Shot (Not Persistent)

**Location:** `src/network/mod.rs` — `InboundCircuitEstablished` DB flush

**Mechanism:** `dequeue_pending_chunks` is a single one-shot fetch (max 20 chunks). This
explains why iOS/Android see exactly 8 and 2 chunks respectively — only the initial batch
is delivered. Remaining chunks in SQLite are never drained.

---

### Root Cause 4 — `startPull` Called for Zero-Size Manifests

**Location:** Flutter UI, `start_pull` handler in Rust

**Mechanism:** Mac log shows `startPull for gft_..._1783734902581 (size=0)` — a manifest with
`total_size=0`. This creates a zombie `IncomingTransfer` entry with no valid seeder. The entry
survives stale eviction (last_update refreshed by OutboundCircuit events) and causes the stall
watchdog to spam requests for a file that cannot be served.

---

### Root Cause 5 — Stall Watchdog Floods Command Channel

**Location:** `src/network/mod.rs` L558-622

**Mechanism:** Watchdog fires every 8s per transfer. Each cycle spawns a task that sends
`FlushPendingForPeer`. Due to RC1+RC2, the flush never succeeds. Next cycle re-spawns.
With 2+ active transfers, 10+ providers, this creates 80–200+ queued commands, degrading
all command processing.

---

## 3. Rectification Actions

### ✅ Action 1 — Fix `FlushPendingForPeer` Relay Condition (PRIMARY FIX)

**File:** `src/network/mod.rs` ~L4770

**Change:** Only enter the re-buffer branch when NEITHER directly connected NOR relay active.
When `has_active_relay=true`, fall through to the send loop even if `!is_connected`:

```rust
// BEFORE:
if !is_connected && is_relayed {
    self.dial_relay_path(peer_id, true);
    self.pending_messages.entry(peer_id).or_default().extend(sorted);
    // retry scheduled...
    return Ok(());
}

// AFTER: Only re-buffer when relay is actually down
if !is_connected && is_relayed && !has_active_relay {
    self.dial_relay_path(peer_id, true);
    self.pending_messages.entry(peer_id).or_default().extend(sorted);
    if retry_count < MAX_FLUSH_RETRIES {
        let delay_secs = 3 * (retry_count as u64 + 1);
        let tx = self.command_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            let _ = tx.send(NetworkCommand::FlushPendingForPeer {
                peer_id, retry_count: retry_count + 1,
            }).await;
        });
    }
    return Ok(());
}
// Otherwise fall through to direct send (connected OR relay circuit is active)
```

---

### ✅ Action 2 — Skip `was_recently_flushed` for File Chunks

**File:** `src/network/mod.rs` ~L4745

**Change:** Bypass dedup for `FileChunk` and `FileChunkRequest`:

```rust
// BEFORE:
let filtered: Vec<_> = payloads.into_iter()
    .filter(|p| !self.was_recently_flushed(&peer_id, p))
    .collect();

// AFTER:
let filtered: Vec<_> = payloads.into_iter()
    .filter(|p| {
        if matches!(p, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
            return true; // Never dedup file chunks
        }
        !self.was_recently_flushed(&peer_id, p)
    })
    .collect();
```

---

### ✅ Action 3 — Reject Zero-Size `start_pull` Manifests

**File:** `src/network/mod.rs` — `HandleIncomingPayload` with `FileTransfer` manifest

**Change:** Add early return for `total_size == 0`:

```rust
if manifest.total_size == 0 {
    warn!("[FFI] start_pull: ignoring manifest {} with total_size=0", manifest.transfer_id);
    return;
}
```

---

### ✅ Action 4 — DB Chunk Drain Loop (persistent, not one-shot)

**File:** `src/network/mod.rs` — `InboundCircuitEstablished` DB flush section

**Change:** Replace one-shot call with a drain loop that re-polls every 2s:

```rust
// Persistent drain loop — keep flushing until queue empty or circuit drops
let tx_db = tx.clone();
let storage_db = self.storage.clone();
let peer_str_db = src_peer_id.to_string();
tokio::spawn(async move {
    loop {
        let chunks = match storage_db.dequeue_pending_chunks(&peer_str_db, 20) {
            Ok(c) => c,
            Err(_) => break,
        };
        if chunks.is_empty() { break; }
        let count = chunks.len();
        crate::dispatch_debug_log(&format!("[Relay] DB chunk drain: {} chunks → {}", count, peer_str_db));
        for (tid, idx, data) in chunks {
            let payload = SignalingPayload::FileChunk {
                transfer_id: tid,
                chunk_index: idx,
                data_base64: base64::encode(&data),
                total_size: 0,   // receiver reconstructs from IncomingTransfer
                relay_hint: None,
            };
            let _ = tx_db.send(NetworkCommand::ForwardMeshSignaling {
                peer_id: src_peer_id,
                payload,
            }).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
});
```

---

## 4. Files Changed

| File | Location | Action |
|------|----------|--------|
| `src/network/mod.rs` | ~L4770 | Action 1: Fix relay condition in `FlushPendingForPeer` |
| `src/network/mod.rs` | ~L4745 | Action 2: Skip `was_recently_flushed` for file chunks |
| `src/network/mod.rs` | `HandleIncomingPayload` | Action 3: Reject zero-size manifests |
| `src/network/mod.rs` | InboundCircuit DB flush | Action 4: Persistent drain loop |

---

## 5. Verification Criteria

After `make all`:

1. **Primary test:** Android/iOS on different networks. Send file.
   - Expected: `[Relay] FlushPendingForPeer: direct-sent N payloads (connected=false, relay=true, relay_active=true)`
   - Transfer completes to 100%.

2. **Stall watchdog:** Let watchdog fire.
   - Expected: Logs show `"direct-sent"` not `"re-buffering"`.

3. **No zombie transfers:** Restart app.
   - Expected: No `startPull called ... (size=0)`.

4. **Regression:** Same-network transfer.
   - Expected: Completes at full speed, no regressions.

---

## 6. Deployment & Fine-Tuning (2026-07-12)

All verification criteria have been met. On **2026-07-12**, the following additional deployments were executed to finalize the rectification plan:

1. **Select Loop Starvation Solved**: Integrated `biased;` prioritization to the main network loop (`command_rx` checked first) and implemented early group action drop in `handle_single_payload`.
2. **IntroClaw Policy Cap**: Tuned the dynamic transfer policy engine in `src/intro_claw.rs` to override high-speed WiFi defaults for relayed connections, enforcing the stable `64KB` chunk size and `50ms` pacing cap.
3. **Corrected Relay Routing Query**: Modified `get_current_transfer_policy` to pass peer-specific relayed state from `is_relayed_map` instead of a global relay listener query.
4. **Native Compilation Complete**: Ran `make all` to re-compile native core libraries for macOS (`libintrovert.dylib`), Android (`libintrovert.so`), and iOS (`libintrovert.a`) containing all rectification and starvation fixes.

