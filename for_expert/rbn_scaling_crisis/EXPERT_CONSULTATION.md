# Expert Consultation: Relay Circuit Stability Under Load

**Date:** 2026-07-17
**Prepared for:** Network architecture expert review
**Severity:** CRITICAL — blocks scaling beyond ~50 concurrent peers per RBN

---

## 1. System Context

Introvert is a P2P mesh messenger with:
- **Client** (Flutter/Dart + Rust via FFI): mobile/desktop app that connects to peers
- **RBN** (Rust daemon): Relay Backbone Node that relays traffic between peers that can't connect directly
- **libp2p v0.56**: networking stack (TCP, QUIC, WebSocket, relay circuit, Kademlia DHT)

**Key architectural constraint:** Clients do NOT run gossipsub. All message routing is via:
- Direct P2P (WebRTC or libp2p request-response)
- Relay circuit through RBN (libp2p circuit relay)
- Group fan-out is sender-side unicast to each group member (not gossipsub)

---

## 2. Problem Statement

A stress test with 50 virtual nodes connected to a single RBN caused the Android client to become CPU-hot and nearly unusable. The stress test nodes were legitimate libp2p peers connecting to the same RBN as the real devices.

**Symptoms observed in `android_netlog_2026-07-17.txt`:**
- Peer count grew from 5 → 37 over ~10 minutes
- `idle_mode` flapped 35 times in 9 minutes (true→false→true in <1ms)
- Same peer received `InboundCircuit DB flush: 30 chunks` 11 times
- `[Resilience] Step 1: N peers connected but no relay` fired continuously
- Relay circuit dropped and re-established every ~30 seconds

---

## 3. Root Cause Analysis

### 3.1 The `idle_mode` Race Condition

**Location:** `src/network/mod.rs`

Two independent write sites for `self.idle_mode: Arc<AtomicBool>`:

```rust
// Write site 1: Dart/Flutter lifecycle callback (line 5199)
// Fires when app backgrounds or foregrounds
NetworkCommand::SetAppIdleState { is_idle } => {
    self.idle_mode.store(is_idle, Ordering::Relaxed);
}

// Write site 2: Incoming payload handler (line 4251)
// Fires on ANY incoming payload while idle
if self.idle_mode.load(Ordering::Relaxed) {
    self.idle_mode.store(false, Ordering::Relaxed);
    crate::dispatch_debug_log("[Resilience] Wake-on-push: idle_mode reset to false");
}
```

**The race:**
1. App backgrounds → Dart sends `SetAppIdleState(true)` → `idle_mode = true`
2. Stress test peer sends payload → `idle_mode = false` (wake-on-push)
3. Dart's lifecycle callback fires again (app still backgrounded) → `idle_mode = true`
4. Another payload arrives → `idle_mode = false`
5. Repeat 35 times in 9 minutes

**Evidence from log:**
```
04:46:40.661629  idle_mode set to false
04:46:40.662172  idle_mode set to true
04:46:40.662392  idle_mode set to true
```
Three writes in under 1ms. The Dart-side write and the wake-on-push write are racing with no coordination.

**Why this matters:** The `idle_mode` flag gates the entire reconnect ladder (lines 736-740) and fast reconnect (lines 971-977). Each flap re-triggers the reconnect logic, which dials the RBN, which establishes a new circuit, which triggers a re-flush.

### 3.2 Full-Queue Re-flush on Circuit Re-establishment

**Location:** `src/network/mod.rs` lines 2043-2084

Every `InboundCircuitEstablished` event spawns two independent flush tasks:

```rust
// Flush 1: RAM buffer (lines 2043-2055)
// Removes ALL pending payloads for this peer and sends them
if let Some(payloads) = self.pending_messages.remove(&src_peer_id) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        for payload in payloads {
            let _ = tx.send(NetworkCommand::ForwardMeshSignaling {
                peer_id: src_peer_id, payload
            }).await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
}

// Flush 2: DB-backed chunk queue (lines 2057-2084)
// Dequeues up to 100 chunks from SQLite and sends them
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(200)).await;
    if let Ok(chunks) = storage.dequeue_pending_chunks(&peer_str, 100) {
        for (transfer_id, chunk_index, chunk_data) in chunks {
            // ... encode and send each chunk
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
});
```

**The problem:** `dequeue_pending_chunks` uses an `in_flight_since` marker to prevent double-selecting within a single call. But this marker is NOT persisted across circuit drops. When the circuit flaps:
1. Circuit drops → in-flight state is lost
2. New circuit established → `InboundCircuitEstablished` fires
3. `dequeue_pending_chunks` re-selects the same chunks (in_flight_since = 0)
4. Same 30 chunks are re-sent

**Evidence from log:** Same peer (`...MAFNtYvf`) received `InboundCircuit DB flush: 30 chunks` 11 separate times in one session. That's 330 chunk sends for what should have been ~30.

**Why this matters:** Each re-flush adds load to the circuit, which makes it more likely to drop, which triggers another re-flush. This is a positive feedback loop.

### 3.3 The Feedback Loop

The two bugs compound:

```
idle_mode race → reconnect ladder fires → RBN dial → circuit established
    → re-flush 30 chunks → circuit overloaded → circuit drops
    → idle_mode race → reconnect ladder fires → ...
```

At 50 stress peers, each peer is a potential trigger for this loop. The combined load saturates the mobile CPU.

---

## 4. Proposed Fixes

### Fix 1: Debounce `idle_mode` Wake-on-Push

**Concept:** Add a debounce so wake-on-push can't fight an explicit app-backgrounded signal within a short window.

```rust
// New field in NetworkService:
last_idle_transition: Instant,

// Modified wake-on-push (line 4251):
if self.idle_mode.load(Ordering::Relaxed) {
    // Don't wake if app explicitly backgrounded within last 5 seconds
    if self.last_idle_transition.elapsed() > Duration::from_secs(5) {
        self.idle_mode.store(false, Ordering::Relaxed);
        crate::dispatch_debug_log("[Resilience] Wake-on-push: idle_mode reset to false");
    }
}

// Modified SetAppIdleState (line 5199):
NetworkCommand::SetAppIdleState { is_idle } => {
    self.idle_mode.store(is_idle, Ordering::Relaxed);
    self.last_idle_transition = Instant::now();
}
```

**Effect:** Wake-on-push is suppressed for 5 seconds after an explicit background signal, preventing the race.

### Fix 2: Delta-Based Chunk Re-flush

**Concept:** Persist `in_flight` state durably (DB-persisted, not just in-memory) with a timeout/lease. Only re-select chunks whose lease has expired.

**Current `dequeue_pending_chunks` (storage.rs):**
```rust
// Selects chunks where in_flight_since = 0 OR in_flight_since < (now - 30s)
// Sets in_flight_since = now for selected chunks
// Returns selected chunks
```

**Problem:** `in_flight_since` is reset when the circuit drops because the chunks are never "acknowledged" — they're just re-selected.

**Proposed fix:**
1. Add `circuit_id` column to `pending_file_chunks` table
2. When dequeuing, set `circuit_id` to the current circuit's identifier
3. When circuit drops, mark all chunks with that `circuit_id` as `in_flight_since = 0` (available for re-selection)
4. When re-selecting, only select chunks where `in_flight_since = 0 OR (in_flight_since < now - 30s AND circuit_id != current_circuit)`

This ensures chunks are only re-sent when:
- They've never been sent (in_flight_since = 0)
- Their lease expired AND they were sent on a different (dead) circuit

### Fix 3: RBN Relay Reservation Backpressure

**Concept:** Cap concurrent relay reservations per RBN so a stress test (or real load spike) degrades gracefully.

**Current state:** RBN `max_connections` = 1,000,000 (effectively unlimited). RBN `max_reservations` = 8,192. But there's no per-peer fairness — 50 stress peers can consume all reservations.

**Proposed fix:**
1. Add a per-peer reservation limit (e.g., max 1 reservation per peer)
2. Add a global reservation limit tied to actual capacity (e.g., 100 concurrent reservations)
3. When limit is reached, reject new reservations with a backpressure signal
4. Tighten `max_connections` from 1,000,000 to a value tied to relay capacity (e.g., 10,000)

### Fix 4: Circuit Stability Logging

**Concept:** Add detailed logging when circuits drop to diagnose future flapping issues.

```rust
// In ConnectionClosed handler for RBN/anchor peers:
if is_rbn_or_anchor && !self.swarm.is_connected(&peer_id) {
    let duration = connection_duration.as_secs_f64();
    warn!("[Relay] Circuit dropped for {} after {:.1}s — reason: {:?}",
          peer_id, duration, close_reason);
}
```

---

## 5. Questions for Expert Review

1. **Is the debounce approach for `idle_mode` correct?** Should we use a different mechanism (e.g., single-writer pattern, explicit lock, or channel-based coordination)?

2. **Is the `circuit_id` approach for delta-based re-flush correct?** Should we use a different identifier (e.g., connection ID, timestamp, or sequence number)?

3. **What should the per-peer reservation limit be?** 1? 5? 10? What's the tradeoff between fairness and throughput?

4. **What should the global reservation limit be?** 100? 500? 1000? How does this relate to the RBN's CPU/memory capacity?

5. **Should we add a backpressure signal to the client?** When the RBN rejects a reservation, should the client back off? For how long?

6. **Are there other feedback loops we're missing?** The idle_mode race + re-flush loop is one. Are there others in the relay circuit lifecycle?

7. **What's the right order to implement these fixes?** Should we do them all at once, or in sequence? Which has the highest impact/lowest risk?

---

## 6. Files for Review

| File | Lines | What to review |
|------|-------|----------------|
| `src/network/mod.rs` | 4251-4253 | Wake-on-push idle_mode reset |
| `src/network/mod.rs` | 5199-5201 | SetAppIdleState command handler |
| `src/network/mod.rs` | 2043-2084 | InboundCircuitEstablished flush logic |
| `src/network/mod.rs` | 736-740 | idle_mode gate on reconnect ladder |
| `src/network/mod.rs` | 971-977 | idle_mode gate on fast reconnect |
| `src/storage.rs` | dequeue_pending_chunks | in_flight_since logic |
| `for_linux/src/main.rs` | 31 | max_connections default (1,000,000) |
| `for_linux/src/network/behaviour.rs` | relay_config | max_reservations, max_circuits |
| `android_netlog_2026-07-17.txt` | full | Stress test evidence |
