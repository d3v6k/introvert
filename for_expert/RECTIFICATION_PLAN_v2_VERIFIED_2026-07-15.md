# Rectification Plan v2 (Code-Verified): Cross-Network File Transfer Duplication & Background Unresponsiveness

**Date:** 2026-07-15
**Status:** Ready for implementation — root causes confirmed against `storage.rs` / `mod.rs` source
**Supersedes:** `RECTIFICATION_PLAN_2026-07-15_CROSS_NETWORK_AND_UNRESPONSIVENESS.md` (draft), with corrections and exact line references

---

## 0. What changed from the draft

The original draft's diagnosis is directionally correct. Reading the actual source confirms it and surfaces two things the draft understated:

1. **BUG-1 is worse than "DB re-select on reconnect."** `InboundCircuitEstablished` (mod.rs L1997-2062) fires **two independent flush paths for the same peer, back to back**: one drains the in-RAM `pending_messages` buffer (L2020-2032), the other separately dequeues the SQLite `pending_file_chunks` table (L2036-2061). Because `forward_to_mesh` (L3218-3223) pushes a chunk into **both** `pending_messages` *and* the DB when the path isn't ready, a single "path not ready" chunk gets sent **twice** the moment the circuit re-establishes — once from each flush task, running concurrently as two separate `tokio::spawn`s with no shared state. `dequeue_pending_chunks` is a plain `SELECT` (storage.rs L1399-1412) with no in-flight marking, so if the circuit flaps again mid-flush, the next `InboundCircuitEstablished` re-selects the identical rows and sends them a third time. This is the exponential-looking growth in the logs.
2. **BUG-2 has no partial mitigation to build on.** There is currently **no `idle_mode`/background-state concept anywhere in `mod.rs`**. The `status_check_interval` resilience loop (L568-953) and its RBN-dialing block (L860-919) run unconditionally regardless of app lifecycle state. `introvert_client.dart` already has a working FFI pattern for exactly this kind of native-layer signal — `setConnectivityType()` (L1230-1252) — which the Dart lifecycle bridge can be extended to mirror.

Everything below is scoped to be a minimal, mechanically verifiable diff against the current code, not a rewrite.

---

## 1. BUG-1: Duplicate chunk delivery — fix in three coordinated layers

### 1a. `storage.rs` — make dequeue atomic and stateful

**Schema change** (extend `pending_file_chunks`, `storage.rs` L331-342):

```sql
ALTER TABLE pending_file_chunks ADD COLUMN in_flight_since INTEGER NOT NULL DEFAULT 0;
```
(SQLite requires `ALTER TABLE ... ADD COLUMN`; wrap in `IF NOT EXISTS`-style existence check via `PRAGMA table_info` since SQLite has no native `ADD COLUMN IF NOT EXISTS`.)

**Replace `dequeue_pending_chunks`** (storage.rs L1399-1412) with an atomic claim that excludes rows already in flight, inside a transaction:

```rust
pub fn dequeue_pending_chunks(&self, peer_id: &str, limit: usize) -> Result<Vec<(String, u32, Vec<u8>)>> {
    let mut conn = self.conn.lock();
    let now = chrono::Utc::now().timestamp();
    let stale_cutoff = now - 30; // in-flight claims older than 30s are considered dead

    let tx = conn.transaction()?;
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM pending_file_chunks
             WHERE peer_id = ?1 AND (in_flight_since = 0 OR in_flight_since < ?2)
             ORDER BY transfer_id ASC, chunk_index ASC LIMIT ?3"
        )?;
        stmt.query_map(params![peer_id, stale_cutoff, limit as i32], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect()
    };
    if ids.is_empty() {
        tx.commit()?;
        return Ok(Vec::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    tx.execute(
        &format!("UPDATE pending_file_chunks SET in_flight_since = ?1 WHERE id IN ({})", placeholders),
        params_from_iter(std::iter::once(&now as &dyn rusqlite::ToSql).chain(ids.iter().map(|i| i as &dyn rusqlite::ToSql))),
    )?;
    let mut result = Vec::new();
    {
        let mut stmt = tx.prepare(&format!(
            "SELECT transfer_id, chunk_index, chunk_data FROM pending_file_chunks WHERE id IN ({})", placeholders
        ))?;
        let rows = stmt.query_map(params_from_iter(ids.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)? as u32, row.get::<_, Vec<u8>>(2)?))
        })?;
        for row in rows { result.push(row?); }
    }
    tx.commit()?;
    Ok(result)
}
```

This closes the exact race: two concurrent callers can no longer select the same row, and a claim that never got acknowledged (crash, drop) self-heals after 30s instead of staying stuck.

**Add a companion release function** for the failure path:

```rust
pub fn release_in_flight_chunk(&self, transfer_id: &str, chunk_index: u32) -> Result<()> {
    let conn = self.conn.lock();
    conn.execute(
        "UPDATE pending_file_chunks SET in_flight_since = 0 WHERE transfer_id = ?1 AND chunk_index = ?2",
        params![transfer_id, chunk_index as i32],
    )?;
    Ok(())
}
```

Call this from wherever `ForwardMeshSignaling` send failures are currently swallowed with `let _ =` (e.g. mod.rs L2055, L2028) so a failed send doesn't leave the chunk permanently claimed.

### 1b. `mod.rs` — collapse the two flush paths into one

The core fix: `InboundCircuitEstablished` (L1997-2062) must not run the RAM flush and the DB flush as independent, uncoordinated tasks for the same peer. Merge them into a single sequential task so the RAM buffer is authoritative and the DB is only consulted for what RAM didn't have (chunks that were persisted *because* RAM/relay wasn't available in the first place — they're mutually exclusive in practice, but concurrent execution today makes them race).

```rust
libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
    // ... existing relay_hints / dial / rate-limiter logic unchanged (L2001-2014) ...

    let ram_payloads = self.pending_messages.remove(&src_peer_id).unwrap_or_default();
    let storage = Arc::clone(&self.storage);
    let tx = self.command_tx.clone();
    let peer_str = src_peer_id.to_string();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        for payload in ram_payloads {
            let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: src_peer_id, payload }).await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        // Only now touch the DB queue, sequentially after RAM, on the SAME task —
        // no second concurrent spawn racing dequeue_pending_chunks for this peer.
        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Ok(chunks) = storage.dequeue_pending_chunks(&peer_str, 100) {
            if !chunks.is_empty() {
                crate::dispatch_debug_log(&format!("[Relay] InboundCircuit DB flush: {} chunks -> {}", chunks.len(), peer_str));
                for (transfer_id, chunk_index, chunk_data) in chunks {
                    use base64::Engine;
                    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&chunk_data);
                    let payload = SignalingPayload::FileChunk { transfer_id: transfer_id.clone(), chunk_index, total_chunks: 0, data_base64 };
                    match tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: src_peer_id, payload }).await {
                        Ok(_) => { let _ = storage.remove_pending_chunk(&transfer_id, chunk_index); } // ack-on-send, see 1c
                        Err(_) => { let _ = storage.release_in_flight_chunk(&transfer_id, chunk_index); }
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });
}
```

This alone removes the concurrent double-send: one task per `InboundCircuitEstablished`, RAM first, DB second, nothing else touches this peer's DB rows while the claim (1a) is held.

### 1c. Stop double-writing on the send side (`forward_to_mesh`, L3193-3225)

Today, when the path isn't ready, a `FileChunk` is pushed into `pending_messages` (RAM) **and** persisted to `pending_file_chunks` (DB) unconditionally (L3218-3223). That's what makes two independent flush sources necessary in the first place. Make it either/or:

```rust
if matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
    let has_rbn = self.bootstrap_nodes.iter().any(|(id, _)| self.swarm.is_connected(id));

    if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, ref data_base64, .. } = payload {
        if let Ok(chunk_data) = base64::decode(data_base64) {
            // Single source of truth: DB queue. RAM buffer is no longer used for FileChunk.
            let _ = self.storage.enqueue_pending_chunk(transfer_id, &recipient_str, chunk_index, &chunk_data);
        }
        return Ok(()); // rely exclusively on InboundCircuitEstablished's DB flush (1b)
    }

    // FileChunkRequest: keep existing RAM-only redundancy-filtered path (L3212-3217), unchanged —
    // these are small and regenerable, no need to persist them.
    if let SignalingPayload::FileChunkRequest { ref transfer_id, chunk_index, .. } = payload {
        if let Some(pending) = self.pending_messages.get_mut(&recipient_id) {
            pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: ref tid, chunk_index: ref idx, .. } if tid == transfer_id && idx == &chunk_index));
        }
        self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
        return Ok(());
    }
}
```

`FileChunk` payloads now have exactly one home (the DB queue) and exactly one drain point (the sequential task in 1b), regardless of whether an RBN was connected at enqueue time. This removes the `has_rbn` branch's asymmetry that the draft flagged as a contributing factor — both branches previously wrote to slightly different places; now they don't.

### 1d. Delete-on-send instead of delete-on-`FileTransferComplete`

Already partially implemented above (1b calls `remove_pending_chunk` right after a successful `tx.send`). Note this is "sent," not "acknowledged by the peer" — a true ACK would require a new `ChunkAck` signaling payload and round trip, which is a larger protocol change. For this pass, sent-and-removed is a large improvement over the current wait-for-`FileTransferComplete` behavior (mod.rs L7143-7153, which is a transfer-level ACK, not per-chunk) and matches what the draft asked for in spirit. Flag the true per-chunk ACK as a **follow-up**, not part of this rectification pass, since it touches the wire protocol and both peers' code.

### 1e. Verification for BUG-1

- Add a debug counter (`total_chunks_sent_per_transfer`) incremented in the `tx.send` success branch in 1b/1c, dispatched via `dispatch_debug_log` when a transfer completes. Compare against `total_chunks` in the `FileChunk` payload — mismatch = duplicate or drop.
- Flapping test: toggle the network 5x during a 50MB transfer over VPN; assert `pending_file_chunks` row count for that `transfer_id` never exceeds the chunk count that hasn't been sent yet, and never grows across a flap (only shrinks).

---

## 2. BUG-2: Background dial flooding — introduce an idle signal end-to-end

There is no existing native-side idle concept to "wire up" — it has to be added at all three layers: Dart lifecycle → FFI → Rust loop gating.

### 2a. `introvert_client.dart` — new FFI setter, mirroring `setConnectivityType`

```dart
// Inform native layer of app lifecycle state (0=foreground/active, 1=background/idle)
void setAppIdleState(bool isIdle) {
  _setAppIdleState(isIdle ? 1 : 0);
}
```
Bind `_setAppIdleState` in `_bindFunctions()` (near L1029-1212) following the exact `safeLookup` pattern used for `_setConnectivityType`, pointing at a new native export `introvert_set_app_idle_state`.

### 2b. `network_service.dart` (Flutter UI bridge — not in this upload, but this is the integration point)

In the `AppLifecycleState` listener (`didChangeAppLifecycleState`), call:
```dart
case AppLifecycleState.paused:
case AppLifecycleState.inactive:
  _client.setAppIdleState(true);
  break;
case AppLifecycleState.resumed:
  _client.setAppIdleState(false);
  break;
```
This is the piece the draft named as a target file but didn't specify concretely — the above is the minimal addition needed once 2a/2c exist.

### 2c. `mod.rs` — gate the resilience loop on idle state

Add a shared flag (e.g. `Arc<AtomicBool>` on the engine struct, set via the new FFI command) and check it at the top of the RBN-dialing block:

```rust
_ = status_check_interval.tick() => {
    // ... existing stale-transfer cleanup (L569-584) and status dispatch (L586-628) stay as-is ...

    let is_idle = self.idle_mode.load(Ordering::Relaxed);

    if !is_idle {
        // existing L860-919 RBN dial / relay reservation block, unchanged
    } else {
        // Idle: don't dial, don't request reservations. Let existing connections
        // lapse naturally; only log at a much lower frequency to avoid its own noise.
        if self.last_idle_log.elapsed() > Duration::from_secs(300) {
            self.last_idle_log = Instant::now();
            crate::dispatch_debug_log("[Resilience] Idle — suppressing background dials");
        }
    }
    // ... rest of tick (undelivered message retry, status broadcast) unchanged ...
}
```

Also gate `fast_reconnect_interval.tick()` (L954-970) the same way — it independently re-triggers relay reservation attempts and would otherwise undo the suppression above.

### 2d. Dynamic tick interval instead of (or in addition to) a boolean gate

The draft's suggestion to scale `status_check_interval` from 15s → 5min in the background is complementary, not a replacement — a coarse interval alone doesn't stop the *first* tick after backgrounding from dialing everything, which is what actually produces the frame-skip burst in the logs. Recreating a `tokio::time::interval` mid-loop requires re-entering the `select!` with a new interval object; simplest is to keep the 15s tick always and use the boolean from 2c to skip work on each tick while idle — cheaper than juggling interval reconstruction, and avoids missing the FCM-wake case where idle mode needs to be exited without waiting up to 5 minutes for the next tick.

### 2e. Wake-on-push exception

Per the draft's point 2.2: when a high-priority FCM message arrives, the existing push-handling code path (wherever FCM payload is processed on the native side — not present in this upload) should call the equivalent of `self.idle_mode.store(false, Ordering::Relaxed)` before processing, so a real incoming transfer can still establish a relay reservation even while the OS considers the app backgrounded.

### 2f. Verification for BUG-2

- Background the app for 10 minutes with no pending transfers; `adb shell dumpsys batterystats` / Xcode Instruments should show near-zero network wakeups after the first tick.
- Background the app mid-transfer; confirm the transfer still completes once foregrounded (RAM `pending_messages` may be lost on process death, but DB-queued chunks from 1c survive and flush on `InboundCircuitEstablished` per 1b).

---

## 3. BUG-3: Leak of pending chunks on cancel/eviction — confirmed, straightforward fix

`storage.rs` already has `remove_pending_chunks_for_transfer` (L1445-1452); it's just not called from the stale-transfer watchdog. Add it to the cleanup block that already exists at mod.rs L573-584:

```rust
for id in &stale_ids {
    warn!("[Resilience] Cleaning up stale transfer: {} (no update in >5min)", id);
    self.incoming_transfers.remove(id);
    let ft_topic = libp2p::gossipsub::IdentTopic::new(format!("file-transfer-{}", id));
    let _ = self.swarm.behaviour_mut().gossipsub.unsubscribe(&ft_topic);
    let _ = self.storage.remove_pending_chunks_for_transfer(id); // NEW
}
```
And equivalently wherever manual user-initiated cancel is handled (not visible in this excerpt — search for the transfer-cancel command handler and add the same call).

---

## 4. Rollout order

1. **Storage layer first (1a)** — additive schema change and new functions, no behavior change until callers use them. Safe to ship alone.
2. **BUG-3 fix** — one-line addition, no dependencies, ship immediately.
3. **1b + 1c together** — these two must land in the same change; 1c's removal of RAM buffering for `FileChunk` requires 1b's sequential flush to be in place first, or chunks enqueued mid-deploy could be dropped.
4. **BUG-2 (2a-2e)** — independent of the above, can ship in parallel, but needs coordinated Dart + Rust changes in the same release since the FFI symbol is new.
5. Run the verification steps in §1e and §2f before closing this out.

## 5. Explicit non-goals for this pass

- True per-chunk peer ACK protocol (flagged in 1d) — larger wire-protocol change, separate plan.
- Any change to `intro_claw.rs` pacing/chunk-sizing logic — the draft named it as an audit file but the confirmed root causes are entirely in the DB/RAM double-buffering and the missing idle gate, not in transfer pacing. Recommend leaving it out of this specific fix to keep the diff reviewable, and revisit only if duplication persists after 1a-1c land.
