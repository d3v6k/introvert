# Rectification Plan: Cross-Network File Transfer & App Unresponsiveness Debugging

**Date:** 2026-07-15  
**Author:** Antigravity  
**Status:** DRAFT (Pending Expert Audit & Approval)  
**Target Files:**  
- Rust Core: `src/network/mod.rs` (Network Loop & Relay Handlers)
- Rust Core: `src/storage.rs` (SQLite SQLCipher DB Operations)
- Rust Core: `src/intro_claw.rs` (Transfer Policies & Optimization)
- Flutter/Dart: `lib/src/services/network_service.dart` (UI Bridge & Lifecycle)

---

## 1. Executive Summary

introvert's recent logs reveal two critical issues causing severe app unresponsiveness, battery drain, and cross-network file transfer instabilities:
1. **Exponential Duplicate File Chunk Explosion**: During connection drops/reconnects over RBN relays, a positive feedback loop repeatedly sends, stores, and duplicates the same file chunks. This triggers a memory and CPU bottleneck (processing vectors of base64 data) and floods the single-threaded Tokio loop.
2. **Resilience Starvation / Background Dial Flooding**: When the app is in the background or `IdleMode`, the 15-second status tick continues dialing RBN bootstrap nodes and requesting relay reservations. This causes battery drain and triggers OS-level frame skips and lag.

No changes will be applied until these findings are verified and approved by a network audit expert.

---

## 2. Root Cause Analysis & Rectification Strategy

### [BUG-1] Exponential Duplicate File Chunk Explosion (Memory & Channel Congestion)
*   **Location:** `src/network/mod.rs` (lines 2034-2062) & `src/storage.rs`
*   **The Issue:**
    *   When `InboundCircuitEstablished` is fired, the engine queries the SQLCipher database for up to 100 pending chunks using `storage.dequeue_pending_chunks()`.
    *   `dequeue_pending_chunks()` performs a `SELECT` query but **does not delete or mark the chunks as sent** from the `pending_file_chunks` table. They are only removed when the entire transfer finishes and `FileTransferComplete` is received.
    *   If the relay connection drops or flaps during transmission, `InboundCircuitEstablished` fires again on reconnect. It queries the database and sends the **exact same chunks again**.
    *   If the path is currently "not ready", `forward_to_mesh()` buffers the chunks in `pending_messages` AND writes them to the DB *again* (effectively duplicating them in the database).
    *   This positive feedback loop results in massive duplicate chunk propagation (we see logs showing `InboundCircuit flush: 562 payloads` and multiple flushes of the same 26/100 chunks).
*   **Rectification Plan:**
    1.  **Introduce an In-Flight State in DB**: Modify the `pending_file_chunks` table schema to include an `in_flight` boolean flag or `sent_at` timestamp.
    2.  **Atomically Mark/Dequeue Chunks**: Update `dequeue_pending_chunks` in `src/storage.rs` to select chunks, mark them as `in_flight = 1`, and return them atomically.
    3.  **Handle Send Failure / Timeout**: If a chunk fails to send, or if its `in_flight` state times out (e.g., >30s without acknowledgement), reset its `in_flight` state to `0` so it can be retried.
    4.  **Delete Sent Chunks Immediately on ACK**: Instead of waiting for the full `FileTransferComplete` packet (which may never arrive if a transfer is interrupted), delete chunks from `pending_file_chunks` as soon as they are successfully published/acknowledged.
    5.  **De-duplicate `pending_messages`**: Add a redundancy filter in `pending_messages` to ensure duplicate `FileChunk` payloads for the same `transfer_id` and `chunk_index` are rejected before insertion.

---

### [BUG-2] Background Dial Flooding during Idle Mode (Resource Exhaustion)
*   **Location:** `src/network/mod.rs` (lines 850-920)
*   **The Issue:**
    *   `status_check_interval` ticks every 15 seconds to monitor peer connections and RBN status.
    *   When the app enters `IdleMode` (in the background or minimized), polling is disabled. However, the status loop continues to run.
    *   If the relay reservation is dropped (which is expected in background/sleep mode), the resilience loop immediately tries to dial all RBN bootstrap nodes and request reservations again.
    *   This triggers `No RBNs reachable — will retry in 30s` every 15 seconds.
    *   This background activity keeps the CPU awake, blocks Tokio execution threads, and results in Android/iOS frame skips (`Choreographer: Skipped 44 frames!`).
*   **Rectification Plan:**
    1.  **Respect Idle State in Resilience Loop**: Check the local node's `idle_mode` or background state inside the status check loop.
    2.  **Back-Off / Suppress background dials**: If the app is in `IdleMode`, suppress aggressive dials and relay reservation requests. Only attempt reconnects when a high-priority FCM wake-up is received or when the app lifecycle transitions back to `resumed`.
    3.  **Increase Tick Interval in Background**: Dynamically adjust the status check interval (e.g., scale from 15s when active to 5 minutes when backgrounded).

---

### [BUG-3] Leak of Pending Chunks on Cancel / Eviction
*   **Location:** `src/network/mod.rs` (lines 568-584 & 7260-7280)
*   **The Issue:**
    *   If a file transfer times out and is cleaned up by the stale transfer watchdog (`status_check_interval`), the node unsubscribes from the Gossipsub topic.
    *   However, the pending chunks are **never** removed from `pending_file_chunks` SQLite table because `remove_pending_chunks_for_transfer` is only called inside the `FileTransferComplete` handler.
    *   Stale chunks are left in the DB permanently, cluttering database storage and getting sent on subsequent reconnects.
*   **Rectification Plan:**
    1.  **Call DB Cleanup on Eviction/Cancel**: Ensure `remove_pending_chunks_for_transfer` is explicitly called when a transfer is evicted by the stale watchdog or manually cancelled by the user.

---

## 3. Recommended Audit Files for Networking Experts

The following files contain the core networking loop, peer resilience strategies, and database-backed chunk queues. These are the primary files to share with a systems/networking expert for audit and fine-tuning:

1.  **`src/network/mod.rs`** (Rust Core)
    *   *Why:* Contains the core `tokio::select!` event loop, `InboundCircuitEstablished` event handler, and the `forward_to_mesh()` logic. This is the heart of the networking layer.
2.  **`src/storage.rs`** (Rust Core)
    *   *Why:* Contains the SQLite schema and function implementations for `enqueue_pending_chunk` and `dequeue_pending_chunks` which control the persistent queue.
3.  **`src/intro_claw.rs`** (Rust Core)
    *   *Why:* Houses `IntroClawService` which monitors peer statistics, connection health, and determines the transfer pacing, chunk sizing, and network-adaptive policies.
4.  **`lib/src/services/network_service.dart`** (Flutter UI Bridge)
    *   *Why:* Manages lifecycle events (active/inactive/paused/resumed) and passes connectivity types and active chat contexts to the Rust core.

---

## 4. Verification Plan (Post-Implementation)

Once changes are approved and implemented:
1.  **Flapping Simulation**: Artificially toggle network interfaces on/off during a 50MB file transfer over VPN. Verify that `pending_file_chunks` database counts remain bounded and do not duplicate.
2.  **Background Resource Profile**: Put the app in the background for 10 minutes. Verify via `adb shell dumpsys batterystats` or Xcode Instruments that CPU wakeups and network dials drop to zero/near-zero.
3.  **Cancel/Failure Sweep**: Start a transfer, cancel it halfway, and inspect `pending_file_chunks`. Verify that the database is completely cleaned of the aborted transfer's chunks.
