# Instruction Prompt for Mimo CLI to Implement Rectification Plan v2

Copy and paste the entire block below directly into Mimo CLI to execute the verified code changes.

```markdown
Role: Expert Rust & Dart FFI Software Engineer
Task: Implement Rectification Plan v2 for Introvert Messenger to resolve cross-network file transfer duplication, database queues, and background dial flooding.

Follow the instructions below exactly. Do not overwrite files entirely; perform targeted replacements.

---

### Step 1: Update SQLite Storage Schema & Functions in `src/storage.rs`

1. Open `src/storage.rs`.
2. Locate the table creation block (near `create_tables` / `pending_file_chunks`). Add the `in_flight_since` column dynamically if it does not exist:
```rust
// In storage.rs table creation, execute:
let _ = conn.execute(
    "ALTER TABLE pending_file_chunks ADD COLUMN in_flight_since INTEGER NOT NULL DEFAULT 0",
    [],
);
```
3. Replace the `dequeue_pending_chunks` implementation (approx L1399-1412) with the following atomic, transaction-looped version:
```rust
pub fn dequeue_pending_chunks(&self, peer_id: &str, limit: usize) -> Result<Vec<(String, u32, Vec<u8>)>> {
    let mut conn = self.conn.lock();
    let now = chrono::Utc::now().timestamp();
    let stale_cutoff = now - 30; // 30s timeout

    let tx = conn.transaction()?;
    let mut result = Vec::new();
    
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

    if !ids.is_empty() {
        let mut update_stmt = tx.prepare("UPDATE pending_file_chunks SET in_flight_since = ?1 WHERE id = ?2")?;
        let mut select_stmt = tx.prepare("SELECT transfer_id, chunk_index, chunk_data FROM pending_file_chunks WHERE id = ?1")?;

        for id in &ids {
            update_stmt.execute(params![now, id])?;
            let mut rows = select_stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                result.push((row.get::<_, String>(0)?, row.get::<_, i32>(1)? as u32, row.get::<_, Vec<u8>>(2)?));
            }
        }
    }

    tx.commit()?;
    Ok(result)
}
```
4. Add the companion function `release_in_flight_chunk` in `src/storage.rs`:
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

---

### Step 2: Implement Idle Mode & Coordinated Flush in `src/network/mod.rs`

1. Open `src/network/mod.rs`.
2. Add `idle_mode: Arc<AtomicBool>` and `last_idle_log: Instant` to the `NetworkService` struct, and initialize them in the constructor (`idle_mode: Arc::new(AtomicBool::new(false))` and `last_idle_log: Instant::now()`).
3. Add a new FFI/Engine command to set the idle state:
```rust
// Inside handle_command, add:
NetworkCommand::SetAppIdleState { is_idle } => {
    self.idle_mode.store(is_idle, Ordering::Relaxed);
    info!("[Resilience] Idle mode set to: {}", is_idle);
}
```
4. In `InboundCircuitEstablished` event handler (approx L1997-2062), merge flushes sequentially on a single task, and handle delete-on-publish:
```rust
libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
    // Keep existing relay_hints / dial / rate-limiter logic unchanged (L2001-2014)
    if let Some(&rbn_id) = self.relay_reservations.iter().next() {
        self.relay_hints.insert(src_peer_id, rbn_id);
    }
    self.relay_dial_limiter.remove(&src_peer_id);
    let _ = self.swarm.dial(src_peer_id);

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

        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Ok(chunks) = storage.dequeue_pending_chunks(&peer_str, 100) {
            if !chunks.is_empty() {
                crate::dispatch_debug_log(&format!("[Relay] InboundCircuit DB flush: {} chunks -> {}", chunks.len(), peer_str));
                for (transfer_id, chunk_index, chunk_data) in chunks {
                    use base64::Engine;
                    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&chunk_data);
                    let payload = SignalingPayload::FileChunk { transfer_id: transfer_id.clone(), chunk_index, total_chunks: 0, data_base64 };
                    let _ = tx.send(NetworkCommand::ForwardMeshSignaling { peer_id: src_peer_id, payload }).await;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    });
}
```
5. Update `forward_to_mesh` (approx L3193-3225) to act as the single point of truth for deleting/releasing database chunks after transmission:
    * In Gossipsub success path for file chunk: call `self.storage.remove_pending_chunk(&transfer_id, chunk_index)`.
    * In Gossipsub error path for file chunk: call `self.storage.release_in_flight_chunk(&transfer_id, chunk_index)`.
    * Update the fallback block so `FileChunk` payloads are ONLY saved to the DB, never to RAM `pending_messages`:
```rust
if matches!(payload, SignalingPayload::FileChunk { .. } | SignalingPayload::FileChunkRequest { .. }) {
    let has_rbn = self.bootstrap_nodes.iter().any(|(id, _)| self.swarm.is_connected(id));

    if let SignalingPayload::FileChunk { ref transfer_id, chunk_index, ref data_base64, .. } = payload {
        if let Ok(chunk_data) = base64::decode(data_base64) {
            let _ = self.storage.enqueue_pending_chunk(transfer_id, &recipient_str, chunk_index, &chunk_data);
        }
        return Ok(()); 
    }

    if let SignalingPayload::FileChunkRequest { ref transfer_id, chunk_index, .. } = payload {
        if let Some(pending) = self.pending_messages.get_mut(&recipient_id) {
            pending.retain(|p| !matches!(p, SignalingPayload::FileChunkRequest { transfer_id: ref tid, chunk_index: ref idx, .. } if tid == transfer_id && idx == &chunk_index));
        }
        self.pending_messages.entry(recipient_id).or_default().push(payload.clone());
        return Ok(());
    }
}
```
6. Gate the `status_check_interval` tick (approx L568) and the `fast_reconnect_interval` tick (approx L954) on `idle_mode`:
```rust
// In status_check_interval tick:
let is_idle = self.idle_mode.load(Ordering::Relaxed);
if !is_idle {
    // ... run RBN dial / reservation requests ...
} else {
    if self.last_idle_log.elapsed() > Duration::from_secs(300) {
        self.last_idle_log = Instant::now();
        crate::dispatch_debug_log("[Resilience] Idle — suppressing background dials");
    }
}
```
7. Fix Bug-3: Call database cleanup on stale transfer eviction:
```rust
// Inside status_check_interval stale transfer cleanup block:
for id in &stale_ids {
    warn!("[Resilience] Cleaning up stale transfer: {} (no update in >5min)", id);
    self.incoming_transfers.remove(id);
    let ft_topic = libp2p::gossipsub::IdentTopic::new(format!("file-transfer-{}", id));
    let _ = self.swarm.behaviour_mut().gossipsub.unsubscribe(&ft_topic);
    let _ = self.storage.remove_pending_chunks_for_transfer(id); // NEW
}
```

---

### Step 3: Wire up Dart Lifecycle & FFI Bridge

1. Open `lib/src/native/introvert_client.dart`.
2. Bind the new FFI setter:
```dart
typedef IntrovertSetAppIdleStateC = FfiResult Function(Int32 isIdle);
typedef IntrovertSetAppIdleStateDart = FfiResult Function(int isIdle);

// Bind:
late IntrovertSetAppIdleStateDart _setAppIdleState;
_setAppIdleState = safeLookup('set_app_idle_state', () => _dylib.lookupFunction<IntrovertSetAppIdleStateC, IntrovertSetAppIdleStateDart>('introvert_set_app_idle_state'), (state) => FfiResult.dummy);

void setAppIdleState(bool isIdle) {
  _setAppIdleState(isIdle ? 1 : 0);
}
```
3. Expose `introvert_set_app_idle_state` in the Rust FFI boundary (typically in `src/lib.rs` / `src/api.rs`).
4. Update the Flutter app lifecycle listener (typically in your `AppLifecycleState` observer inside `lib/src/services/network_service.dart` or similar) to call `setAppIdleState(true)` when paused/inactive, and `setAppIdleState(false)` when resumed.

---

### Step 4: Verification and Rebuild

1. Run `cargo clean && make all` to rebuild the native libraries.
2. Verify with compilation. Test connection resilience and file chunk de-duplication over VPN.
```
