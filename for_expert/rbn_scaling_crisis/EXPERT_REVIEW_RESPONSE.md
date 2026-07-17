# Expert Review Response

**Date:** 2026-07-17
**Status:** Expert feedback incorporated. Ready for implementation.

---

## Corrections to Initial Analysis

### 1. Mailbox Drain Is NOT the Same Bug

The expert correctly flagged `perform_mailbox_fetch` firing on every `ReservationReqAccepted`. However, after code review, the mailbox path is **safe**:

```rust
// storage.rs line 1180-1182: messages are DELETED after fetch
for id in row_ids {
    tx.execute("DELETE FROM mailbox_messages WHERE rowid = ?1", params![id])?;
}
```

`fetch_mailbox_payloads` is a consume operation, not a read. Repeated `MailboxDrain` calls return empty lists after the first drain. This is unnecessary network traffic but NOT a re-send bug.

### 2. Group Fan-out Is NOT a Separate Bug

The expert asked if group fan-out re-sends to all n members on circuit flap. After code review:

- `introvert_group_send_message` (lib.rs line 3121-3147) sends `GroupAction` to each member via `ForwardMeshSignaling`
- This is a one-time send — it's not re-triggered on circuit flap
- If a message was buffered in `pending_messages` because the peer wasn't directly connected, it will be re-flushed on `InboundCircuitEstablished` — but this is the same re-flush bug as Fix 2, not a separate group fan-out bug

### 3. The `pending_messages` Flush on ReservationReqAccepted

Lines 1934-1949 flush ALL pending messages for ALL peers on every `ReservationReqAccepted`:

```rust
let pending_peers: Vec<PeerId> = self.pending_messages.keys().cloned().collect();
for pid in pending_peers {
    if let Some(payloads) = self.pending_messages.remove(&pid) {
        // ... send all payloads
    }
}
```

This is broader than the `InboundCircuitEstablished` flush (which only flushes for the specific peer that just connected). But since `pending_messages.remove()` removes the messages from the buffer, they won't be re-sent on the next `ReservationReqAccepted`.

---

## Incorporating Expert Feedback

### Fix 1 (idle_mode): State Machine Instead of Debounce

**Expert feedback:** "Model this as an explicit state machine (`Foreground`, `Backgrounded`, `BackgroundedPendingWake`) rather than a bool + timestamp."

**Revised design:**

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum AppState {
    Foreground,
    Backgrounded,
    BackgroundedPendingWake,
}

// In NetworkService:
app_state: AppState,
last_state_change: Instant,

// SetAppIdleState handler:
NetworkCommand::SetAppIdleState { is_idle } => {
    if is_idle {
        self.app_state = AppState::Backgrounded;
    } else {
        self.app_state = AppState::Foreground;
    }
    self.last_state_change = Instant::now();
}

// HandleIncomingPayload handler:
if self.app_state == AppState::Backgrounded {
    // Don't wake if app explicitly backgrounded within last 5 seconds
    if self.last_state_change.elapsed() > Duration::from_secs(5) {
        self.app_state = AppState::BackgroundedPendingWake;
        // Allow reconnect ladder to run
    }
}
```

**Expert's tradeoff noted:** "A blanket 5s suppression window will also delay wake-on-push for a legitimate incoming message right after backgrounding."

**Revised approach:** The debounce should gate the *reconnect-ladder retriggering*, not message delivery itself. The `HandleIncomingPayload` handler should still process the message — it just shouldn't flip the `app_state` to `Foreground` if the app was explicitly backgrounded within 5 seconds.

### Fix 2 (delta re-flush): Use ConnectionId, Timeout as Primary

**Expert feedback:** "Reuse libp2p's existing `ConnectionId` instead of inventing a new `circuit_id` column. Make the timeout-based expiry the primary mechanism, not the drop-event reset."

**Revised design:**

```rust
// storage.rs: add connection_id column to pending_file_chunks
ALTER TABLE pending_file_chunks ADD COLUMN connection_id TEXT DEFAULT NULL;

// dequeue_pending_chunks: select chunks where:
// - in_flight_since = 0 (never sent)
// - OR (in_flight_since < now - 30s AND connection_id != current_connection_id)
// - OR (connection_id IS NULL) (legacy chunks)

// On InboundCircuitEstablished: pass connection_id to dequeue
// On ConnectionClosed: DON'T reset in_flight_since (timeout handles it)
```

**Key change:** The timeout-based expiry (`in_flight_since < now - 30s`) is the primary safety net. The explicit reset on `ConnectionClosed` is an optimization, not a correctness requirement.

### Fix 3 (backpressure): Circuit Dial-Rate Limiter

**Expert feedback:** "The thing that actually caused your cascade was concurrent circuit establishment/re-establishment churn, not reservation count. Redirect this fix toward a circuit dial-rate limiter."

**Revised design:**

```rust
// In NetworkService:
circuit_dial_limiter: TokenBucket,

// TokenBucket: allows N new circuit establishments per second
// Default: 1 new circuit/sec/peer, 10 new circuits/sec global

// On InboundCircuitEstablished:
if !self.circuit_dial_limiter.try_acquire() {
    warn!("[Relay] Circuit dial rate limit exceeded — dropping circuit");
    // Don't process this circuit establishment
    return;
}
```

**Expert's sizing advice:** "Start conservatively (e.g. 1 new circuit/sec/peer) and tune from real load-test data rather than picking a static reservation cap."

### Fix 4 (logging): Do This First

**Expert feedback:** "Do this first, unconditionally. It's low-risk, and you'll want that data to validate whether 1-3 actually worked."

**Action:** Add detailed circuit drop logging before implementing fixes 1-3.

---

## Revised Implementation Order

| Order | Fix | Risk | Dependencies |
|-------|-----|------|-------------|
| 1 | Circuit drop logging (Fix 4) | Low | None |
| 2 | Delta-based re-flush (Fix 2) | Low | None |
| 3 | idle_mode state machine (Fix 1) | Medium | Fix 4 (to observe behavior) |
| 4 | Circuit dial-rate limiter (Fix 3) | Medium | Fix 4 (to measure per-circuit cost) |

---

## Open Questions (Revised)

1. **What's the right debounce window for idle_mode?** 5 seconds? 10 seconds? Should it be configurable?

2. **What's the right timeout for in_flight_since?** 30 seconds? 60 seconds? Should it be configurable?

3. **What's the right rate limit for circuit dial?** 1/sec/peer? 5/sec/peer? Should it be configurable?

4. **Should we add a backpressure signal to the client?** When the RBN rejects a circuit, should the client back off? For how long?

5. **Are there other feedback loops we're missing?** The idle_mode race + re-flush loop is one. Are there others?

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/network/mod.rs` | idle_mode state machine, circuit dial-rate limiter, logging |
| `src/storage.rs` | Delta-based re-flush (connection_id column, timeout-based expiry) |
| `for_linux/src/network/mod.rs` | Circuit dial-rate limiter (RBN side) |
