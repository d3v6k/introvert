# Implementation Prompt for MIMO CLI

Implement the fixes described below for three confirmed bugs in the introvert Rust core and Dart bridge: (1) duplicate file-chunk delivery on reconnect, (2) chunks never removed from the DB queue after a successful send, (3) background dial flooding while the app is idle. This supersedes `IMPLEMENTATION_PLAN_2026-07-15.md` — it keeps that plan's structure but closes gaps found during code review. Read this whole prompt before touching any file; several steps depend on earlier ones landing first.

**Before starting:** line numbers below were verified against the current `src/storage.rs` and `src/network/mod.rs`. Re-grep for the referenced symbols/comments before editing in case the file has drifted since this was written — don't trust line numbers blindly, trust the surrounding code/comment text quoted alongside them.

---

## Context you need before writing code

There are **three independent call sites** that dequeue from `pending_file_chunks` and forward the results, not one:

1. `mailbox_fetch_interval.tick()` — fires every 120s (anchor nodes) / 300s (regular nodes). Calls `dequeue_pending_chunks(&peer_str, 50)` for every connected peer, immediately, no delay. Comment directly above it says `// Chunks are NOT removed after forwarding — they stay in DB until FileTransferComplete arrives`.
2. `OutboundCircuitEstablished` handler — spawns two independent `tokio::spawn` tasks: one flushes `pending_messages` (RAM) after 1500ms, a separate one dequeues DB chunks for all connected peers after 600ms. Comment: `// Don't remove from DB here — wait for FileTransferComplete`.
3. `InboundCircuitEstablished` handler — same two-independent-spawn pattern: RAM flush after 150ms, separate DB dequeue after 400ms.

All three read the same `pending_file_chunks` table via the same `dequeue_pending_chunks()` function, and all three can fire close together around a single reconnect event (inbound and outbound circuits commonly establish together). The current `dequeue_pending_chunks` is a bare `SELECT` with no claiming mechanism, so any of these firing concurrently or in quick succession re-sends the same rows. This is the actual mechanism behind the duplicate-chunk-explosion bug — not just "InboundCircuitEstablished re-selects on reconnect" in isolation.

The fix strategy: make the DB layer the single point of truth and concurrency control (atomic claim-then-return), so it doesn't matter how many call sites hit it concurrently — only one can win a given row. Do **not** try to eliminate the redundant call sites by adding cross-site coordination flags; that's more surface area for new bugs. Let the DB-level atomicity handle it, and simplify the callers only where they've now become logically dead code (see Step 4).

---

## Step 1 — `src/storage.rs`: atomic chunk claiming

### 1a. Schema

Locate the `pending_file_chunks` CREATE TABLE block. Immediately after it, add an idempotent column addition:

```rust
let _ = conn.execute(
    "ALTER TABLE pending_file_chunks ADD COLUMN in_flight_since INTEGER NOT NULL DEFAULT 0",
    [],
);
```
This errors harmlessly (and is discarded via `let _ =`) on every run after the first — that's the existing idiom used elsewhere in this file for optional schema evolution, keep it consistent.

### 1b. Replace `dequeue_pending_chunks`

Requirements:
- Must run as a single SQLite transaction so the SELECT-claim-SELECT sequence is atomic relative to any other concurrent caller holding the same `Mutex<Connection>`.
- Only claims rows where `in_flight_since = 0 OR in_flight_since < (now - 30)` — the 30s figure is a stale-claim timeout so a crashed/dropped flush task doesn't permanently strand rows. If real-world testing shows chunks legitimately take longer than 30s to flush end-to-end (large relay hops, degraded VPN), raise this — don't silently shrink the retry window below observed p99 flush latency.
- Sets `in_flight_since = <now>` on the claimed rows before returning them.
- Add `use rusqlite::params_from_iter;` to this file's imports (currently only `params, Connection` are imported) — needed for the dynamic `IN (...)` claim query.
- Keep the function signature identical: `pub fn dequeue_pending_chunks(&self, peer_id: &str, limit: usize) -> Result<Vec<(String, u32, Vec<u8>)>>` — every existing call site depends on this exact shape.

### 1c. New function: `release_in_flight_chunk`

```rust
pub fn release_in_flight_chunk(&self, transfer_id: &str, chunk_index: u32) -> Result<()>
```
Resets `in_flight_since` to 0 for the given row so it becomes claimable again. Call this from the gossipsub `Err` path in Step 3.

---

## Step 2 — Add idle-mode plumbing

### 2a. State field
Add to the network engine struct (wherever it's defined — confirm the actual file, it is not necessarily `service.rs`; grep for the `struct` that owns `last_token_registration` and add the new fields to the same struct):
```rust
pub(crate) idle_mode: Arc<AtomicBool>,
pub(crate) last_idle_log: Instant,
```
Initialize `idle_mode: Arc::new(AtomicBool::new(false))` and `last_idle_log: Instant::now()` in the constructor, alongside the existing `last_token_registration: HashMap::new()` initializer.

### 2b. New `NetworkCommand` variant
Add `SetAppIdleState(bool)` (or `{ idle: bool }`, match the enum's existing naming convention — check whether other variants use tuple or struct form before choosing) to the `NetworkCommand` enum, wherever it's defined.

### 2c. Handle it in `handle_command`
In the `match command { ... }` block inside `async fn handle_command`, add:
```rust
NetworkCommand::SetAppIdleState(idle) => {
    self.idle_mode.store(idle, Ordering::Relaxed);
    crate::dispatch_debug_log(&format!("[Resilience] idle_mode set to {}", idle));
    Ok(())
}
```

### 2d. Gate the resilience loop
In the `status_check_interval.tick()` arm, wrap the RBN-dial / relay-reservation block (the one that logs `"[Resilience] Step 1: {} peers connected but no relay..."` and `"No RBNs reachable — will retry in 30s"`) in:
```rust
if !self.idle_mode.load(Ordering::Relaxed) {
    // existing block, unchanged
} else if self.last_idle_log.elapsed() > Duration::from_secs(300) {
    self.last_idle_log = Instant::now();
    crate::dispatch_debug_log("[Resilience] Idle — suppressing background dials");
}
```
Leave the stale-transfer cleanup, status dispatch, and undelivered-message retry in the same tick arm running unconditionally — only the active dialing/reservation block should be suppressed. Backgrounded users should still see accurate status and get their stuck messages retried once a connection exists; they just shouldn't cause new dials.

Apply the same `if !idle_mode` gate to the `fast_reconnect_interval.tick()` arm — it independently re-triggers relay reservation attempts and will undo the suppression above if left ungated.

### 2e. Wake-on-push exception
Find wherever incoming FCM/push wakeups are handled on the native side (search for the push wakeup / high-priority message handling — it is referenced from the Dart side but may be entirely in a file not covered by this repo excerpt; if you can't locate it, flag this as a follow-up rather than guessing). Before processing a wakeup-triggering event, call `self.idle_mode.store(false, Ordering::Relaxed)` so a real incoming transfer can still complete a relay reservation while the OS still considers the app backgrounded.

---

## Step 3 — `forward_to_mesh`: single source of truth for `FileChunk`, ack-based removal

### 3a. Never buffer `FileChunk` in RAM
In the fallback block that currently does both `self.pending_messages.entry(recipient_id).or_default().push(payload.clone())` **and** `self.storage.enqueue_pending_chunk(...)` for the same `FileChunk` payload: remove the RAM push entirely for `FileChunk`. Persist to DB only, then `return Ok(())`.

Keep `FileChunkRequest` exactly as-is (RAM-only, with its existing same-transfer/same-index dedup via `.retain()`) — do not persist requests to the DB, they're cheap to regenerate.

### 3b. Hook removal to the actual gossipsub publish result, not to the internal channel send
Find the gossipsub `publish()` call inside `forward_to_mesh` (the block that logs `"[Mesh] Published {} via gossipsub topic={}"` on success and `"[Mesh] Gossipsub publish FAILED"` on error). This runs for every `FileChunk`/`FileChunkRequest` send attempt, from every caller, before the RAM/DB fallback is ever reached — it's the correct single choke point.

In the `Ok(_)` arm, **only when `is_chunk_data` is true** (i.e. this is a `FileChunk`, not a `FileChunkRequest` — the current code doesn't distinguish here and you must add that guard, otherwise you'll issue harmless-but-wasteful DELETEs for every request too):
```rust
if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, .. } = payload {
    let _ = self.storage.remove_pending_chunk(transfer_id, chunk_index);
}
```

In the `Err(e)` arm, same guard, call `release_in_flight_chunk` instead so the row becomes claimable again on the next flush attempt rather than being stuck in-flight for up to 30s:
```rust
if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, .. } = payload {
    let _ = self.storage.release_in_flight_chunk(transfer_id, chunk_index);
}
```

**Known limitation, do not try to fully solve it in this pass:** `gossipsub.publish()` returning `Ok` means the message was accepted into the local mesh, not that the intended recipient actually received it — this is a fire-and-forget pubsub primitive. Deleting the only copy of chunk data on that signal is a real improvement over the current "never delete until FileTransferComplete" behavior, but it is not a true end-to-end ACK, and there is a non-zero chance of chunk loss on a peer that appears mesh-reachable but hasn't actually subscribed yet. Log this clearly (e.g. a debug counter of chunks sent vs. `total_chunks` per transfer) so it's measurable, and leave a `// TODO(follow-up): per-chunk peer ACK` comment at the removal site. A real fix requires a new `ChunkAck` wire payload and touches both peers — out of scope here.

---

## Step 4 — Clean up now-redundant/stale code at the other two flush sites

Do **not** skip this step — it's not cosmetic. Leaving these as-is will mislead the next person who reads them, since they directly contradict the new behavior.

- At the `mailbox_fetch_interval` block: delete the loop that sweeps `pending_messages` `FileChunk` payloads into the DB (the one right before the "Flush pending messages periodically" comment). After Step 3a, `pending_messages` can never contain a `FileChunk`, so this loop is dead code. Update or delete the comment `// Chunks are NOT removed after forwarding — they stay in DB until FileTransferComplete arrives` — it's now false; removal happens in `forward_to_mesh` regardless of caller.
- At the `OutboundCircuitEstablished` handler: update or delete the comment `// Don't remove from DB here — wait for FileTransferComplete` for the same reason.
- Leave the actual dequeue-and-forward logic at both sites structurally as-is (don't merge them into `InboundCircuitEstablished`'s task) — the DB-level atomicity from Step 1 makes redundant calls safe, just occasionally a no-op. Merging all three into one coordination point is unnecessary complexity for this pass; only do it if profiling later shows the redundant spawns are a real cost.

---

## Step 5 — Stale transfer cleanup (independent, low-risk, do this first if you want an easy win)

In the stale-transfer watchdog inside `status_check_interval.tick()` (the block that removes transfers with no update in >5 minutes and unsubscribes the gossipsub topic), add immediately after the unsubscribe call:
```rust
let _ = self.storage.remove_pending_chunks_for_transfer(id);
```
Also find the user-initiated manual-cancel command handler (search for where a transfer is cancelled outside the watchdog) and add the same call there if it's missing.

---

## Step 6 — Dart / FFI bridge

- `src/lib.rs` (or wherever other FFI exports for this pattern live — confirm by finding `introvert_set_connectivity_type` or equivalent): add `introvert_set_app_idle_state(idle: i32)` exporting to `NetworkCommand::SetAppIdleState`.
- `lib/src/native/introvert_client.dart`: add a typedef + `safeLookup` binding for the new export (copy the exact pattern used for `_setConnectivityType`, including its placeholder-on-bind-failure behavior), then a public method:
```dart
void setAppIdleState(bool isIdle) {
  _setAppIdleState(isIdle ? 1 : 0);
}
```
- Wherever `AppLifecycleState` changes are handled in the UI layer (`main_shell.dart` or `network_service.dart` — confirm which file actually owns the `didChangeAppLifecycleState` override): call `client.setAppIdleState(true)` on `paused`/`inactive`, `client.setAppIdleState(false)` on `resumed`.

---

## Rollout order (respect this — later steps assume earlier ones are in place)

1. Step 5 (stale cleanup) — trivial, ship alone if you want to land something immediately.
2. Step 1 (storage atomic claim) — additive, no behavior change until Step 3 lands, but must exist before Step 3.
3. Step 3 (forward_to_mesh single-source + ack-based removal) — depends on Step 1.
4. Step 4 (dead-code/comment cleanup) — do right after Step 3, same PR if possible, so the codebase never sits in a state where comments contradict behavior.
5. Step 2 + Step 6 (idle mode, Rust + Dart together) — independent of 1/3/4, but the Rust and Dart halves must land in the same release since the FFI symbol is new.

## Acceptance criteria / verification

- **Flapping test:** toggle network interfaces 5x during a 50MB transfer over VPN. Query `pending_file_chunks` row count for that `transfer_id` continuously — it must be monotonically non-increasing except for legitimate new enqueues, and must never re-send a chunk index already confirmed sent (add the debug counter mentioned in Step 3b and assert `sent_count == total_chunks`, not more).
- **Concurrent-reconnect test:** force `InboundCircuitEstablished` and `OutboundCircuitEstablished` to fire within the same 100ms window (simulate if needed) with a nonempty DB queue for that peer; assert no chunk index is forwarded more than once across both handlers' spawned tasks.
- **Idle test:** background the app with zero pending transfers for 10 minutes; `adb shell dumpsys batterystats` / Xcode Instruments should show near-zero network wakeups after the first tick following backgrounding.
- **Idle-with-pending-transfer test:** background mid-transfer, confirm the transfer still completes on foregrounding (DB-queued chunks survive; Step 2's suppression must not block the flush that happens on `InboundCircuitEstablished`/`OutboundCircuitEstablished`/`mailbox_fetch_interval`, only the proactive RBN-dial loop).
- **Cancel test:** start a transfer, cancel it halfway, confirm `pending_file_chunks` has zero rows for that `transfer_id` immediately after cancellation, not after any subsequent tick.

## Explicit non-goals for this pass

- True per-chunk peer ACK protocol (flagged in Step 3b) — separate plan, touches the wire protocol on both peers.
- Any change to `intro_claw.rs` transfer pacing/chunk-sizing — the confirmed root causes are entirely in the DB/RAM double-buffering and the missing idle gate, not transfer pacing. Leave it out to keep this diff reviewable.
- Consolidating the three independent DB-flush call sites into one coordination point — the DB-level atomicity makes this a performance/log-noise concern, not a correctness one; revisit only if it shows up in profiling.

## Build/verify command
`make all && flutter run` — run the full test list above before considering this done, not just a clean build.
