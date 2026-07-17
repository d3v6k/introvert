# Rectification Plan v2 — Cross-Network File Transfer Dedup, DB Queue, Idle Mode

**Goal:** Fix 3 bugs: (1) file chunk duplication in DB queue, (2) chunks stuck in DB after successful send, (3) background dial flooding when app is idle. Implement per `for_expert/MIMO_CLI_RECTIFICATION_PROMPT.md`.

---

## Step 1: Storage — `src/storage.rs`

### 1a. Add `in_flight_since` column (near L343, after CREATE TABLE)

After the `pending_file_chunks` CREATE TABLE block (L331-342), add:
```rust
let _ = conn.execute(
    "ALTER TABLE pending_file_chunks ADD COLUMN in_flight_since INTEGER NOT NULL DEFAULT 0",
    [],
);
```
Safe no-op if column already exists.

### 1b. Replace `dequeue_pending_chunks` (L1399-1412)

Replace with atomic transaction-looped version:
- Uses transaction to SELECT chunk IDs → UPDATE `in_flight_since` → SELECT data
- Only dequeues where `in_flight_since = 0 OR in_flight_since < (now - 30)` (30s stale timeout)
- Prevents duplicate delivery of in-flight chunks

### 1c. Add `release_in_flight_chunk` function (after L1452)

New function: resets `in_flight_since = 0` when gossipsub publish fails.

---

## Step 2: Network — `src/network/`

### 2a. Add fields to `NetworkService` struct (`service.rs` L92)

Add after `last_token_registration`:
```rust
pub(crate) idle_mode: Arc<AtomicBool>,
pub(crate) last_idle_log: Instant,
```
Import `std::sync::atomic::{AtomicBool, Ordering}`.

### 2b. Add `SetAppIdleState` variant to `NetworkCommand` (`types.rs` L394)

### 2c. Handle `SetAppIdleState` in `handle_command` (`mod.rs` L3373+)

### 2d. Merge `InboundCircuitEstablished` flush (`mod.rs` L2016-2062)

Replace two separate `tokio::spawn` blocks with single merged task:
1. 150ms delay → flush RAM payloads (20ms spacing)
2. 250ms more → dequeue DB chunks (100 limit, 50ms spacing)

### 2e. Update `forward_to_mesh` fallback (`mod.rs` L3193-3225)

`FileChunk` → DB only (never RAM `pending_messages`). `FileChunkRequest` → existing dedup+RAM.

### 2f. Gossipsub success/error hooks (`mod.rs` L3036-3065)

- `Ok(_)` for FileChunk → `remove_pending_chunk()`
- `Err(_)` for FileChunk → `release_in_flight_chunk()`

### 2g. Gate `status_check_interval` on `idle_mode` (`mod.rs` L568+)

### 2h. Gate `fast_reconnect_interval` on `idle_mode` (`mod.rs` L954+)

### 2i. Stale transfer DB cleanup (`mod.rs` L577-584)

Add `storage.remove_pending_chunks_for_transfer(id)` after unsubscribe.

---

## Step 3: Dart FFI Bridge

### 3a. `lib/src/native/introvert_client.dart` — typedef, binding, public method

### 3b. `src/lib.rs` — FFI export `introvert_set_app_idle_state`

### 3c. `lib/src/ui/main_shell.dart` L323-331 — wire lifecycle to `setAppIdleState`

---

## Files to modify (7 files)

| File | Changes |
|------|---------|
| `src/storage.rs` | ALTER TABLE, replace dequeue, add release_in_flight_chunk |
| `src/network/service.rs` | Add idle_mode + last_idle_log fields |
| `src/network/types.rs` | Add SetAppIdleState variant |
| `src/network/mod.rs` | Constructor, handle_command, InboundCircuit merge, forward_to_mesh, gossipsub hooks, idle gating, stale cleanup |
| `src/lib.rs` | FFI export |
| `lib/src/native/introvert_client.dart` | typedef, binding, method |
| `lib/src/ui/main_shell.dart` | Lifecycle wiring |

## Build: `make all && flutter run`
