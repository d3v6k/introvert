# RBN Scaling Crisis — Corrected Root Cause Analysis (v2)

**Date:** 2026-07-17
**Supersedes:** PROBLEM_STATEMENT.md (v1)
**Status:** Root cause identified and code-verified. Ready for implementation planning.

---

## Changelog from v1

v1 diagnosed this as a **gossipsub mesh scaling problem** (flat mesh, O(n) heartbeat
traffic, need for topic sharding). That diagnosis does not match the code or the
stress-test logs. The actual root cause is a **relay circuit stability bug**
(a race condition plus a re-flush bug), not a pub/sub fan-out problem. This
document replaces the root-cause sections of v1; the long-term sharding/DHT
questions in v1 §8 are still valid future-scale work, just not what caused the
50-peer stress test failure.

| | v1 claim | v2 (verified) |
|---|---|---|
| Root cause | Flat gossipsub mesh, O(n) heartbeats | `idle_mode` race + full-queue re-flush on every circuit re-establishment |
| Client gossipsub | "Every client sees every other client as a gossipsub mesh peer" | Clients **do not run gossipsub at all** — `IntrovertBehaviour` on the client has no `gossipsub` field. Only RBN-class nodes do. |
| Mesh formation | Assumed libp2p gossipsub GRAFT/PRUNE churn | Group fan-out is manual unicast (`forward_to_mesh` looping over group members via request-response), unrelated to gossipsub |
| `max_connections` | Assumed unbounded | RBN default: 1,000,000 (effectively unlimited). Client default: 1,024. Neither is the proximate cause of the stress-test symptoms, but the RBN-side value should still be tightened. |

---

## 1. Executive Summary (revised)

The 50-node stress test against a single RBN caused the Android client to
become CPU-hot and sluggish. Log analysis (`android_netlog_2026-07-17.txt`)
and source review (client/RBN behaviour headers, swarm event handlers) show
this was **not** caused by gossipsub mesh overhead — clients don't participate
in gossipsub — but by two concrete bugs in the relay-circuit lifecycle:

1. **`idle_mode` race condition** — two independent writers to the same
   atomic flag with no ordering guarantee, causing 35 flaps in 9 minutes.
2. **Full-queue re-flush on every circuit re-establishment** — pending chunks
   are re-selected and re-sent in full every time a flapping circuit
   reconnects, because in-flight state resets when the circuit drops.

At 50 stress peers sharing one relay circuit, these two bugs compound: circuit
instability (caused by RBN overload under 50 concurrent reservations) triggers
repeated re-flushes, which add load back onto the same circuit, which
destabilizes it further. This is a feedback loop, not a linear O(n) cost —
which is why it presented as sudden unusability rather than gradual
degradation.

---

## 2. Verified Root Causes

### 2.1 The `idle_mode` Race (client, `mod.rs`)

Two independent write sites for `self.idle_mode`, with no coordination:

```rust
// Line 5199 — Dart/Flutter sets this when the app backgrounds
NetworkCommand::SetAppIdleState { is_idle } => {
    self.idle_mode.store(is_idle, Ordering::Relaxed);
}

// Line 4251 — any incoming payload unconditionally wakes the node
if self.idle_mode.load(Ordering::Relaxed) {
    self.idle_mode.store(false, Ordering::Relaxed);
    // "[Resilience] Wake-on-push: idle_mode reset to false"
}
```

**Race sequence, confirmed in `android_netlog_2026-07-17.txt`:**
```
04:46:40.661629  idle_mode set to false
04:46:40.662172  idle_mode set to true
04:46:40.662392  idle_mode set to true
```
Three writes in under 1ms. With 50 stress peers generating background traffic
while the app is backgrounded, every inbound payload flips `idle_mode` back to
`false`, fighting the Dart-side `true` set by the OS-level app-state callback.
35 flaps counted in the ~9-minute log window. Each flap re-triggers whatever
resilience/reconnect logic is gated on `idle_mode`, which is why the
`[Resilience] Step 1: N peers connected but no relay. Re-establishing...`
loop fires continuously as peer count climbs.

**Fix:** `idle_mode` needs single-writer semantics or hysteresis (e.g. a
debounce so a wake doesn't immediately get overridden by a stale background
signal, and the "wake on push" write should not fire if the app has
explicitly backgrounded within the last N seconds).

### 2.2 Full-Queue Re-flush on Circuit Re-establishment (`mod.rs`, lines ~2043–2084)

Every `InboundCircuitEstablished` event spawns two independent flush tasks:

```rust
// Flush 1 — RAM buffer, all pending payloads for this peer
if let Some(payloads) = self.pending_messages.remove(&src_peer_id) {
    tokio::spawn(async move { /* send all payloads */ });
}

// Flush 2 — DB-backed chunk queue, up to 100 chunks
tokio::spawn(async move {
    if let Ok(chunks) = storage.dequeue_pending_chunks(&peer_str, 100) {
        /* send all chunks */
    }
});
```

`dequeue_pending_chunks` uses an `in_flight_since` marker to avoid
double-selecting chunks *within a single call*, but that state is not
persisted across circuit drops. When the circuit flaps, the in-flight marker
resets, and the next `InboundCircuitEstablished` re-selects and re-sends the
same chunks from scratch.

**Confirmed in the log:** the same peer (`...MAFNtYvf`) received
`InboundCircuit DB flush: 30 chunks` **11 separate times** in one session —
not because 11 different batches of 30 chunks were queued, but because the
same ~30-chunk backlog was repeatedly re-flushed after each circuit flap.

**Fix:** flush should be delta-based — mark chunks `in_flight` durably
(DB-persisted, not just in-memory) with a timeout/lease, and only re-select
chunks whose lease has expired, not the entire pending set.

### 2.3 Corrected Understanding of Gossipsub's Role

`client_behaviour_header.rs` has no `gossipsub` field; `rbn_behaviour_header.rs`
does. Clients talk to the RBN via `request_response` (signaling) and
`relay_client`/circuit relay — never via libp2p's gossipsub protocol directly.
Group-chat fan-out (`introvert_group_send_message`) is a manual loop over
group members using unicast request-response sends, confirmed in the netlog:

```
introvert_group_send_message: Group ... has 3 members
introvert_group_send_message: Forwarding GroupAction to member A
introvert_group_send_message: Forwarding GroupAction to member B
```

This is O(n) per sender for group size, which is a real scaling concern for
large groups, but it is **sender-side unicast fan-out, not gossipsub mesh
maintenance**, and it was not what the 50-peer stress test exercised (that
test connected 50 unrelated peers to the RBN, not 50 members of one group).

RBN-side gossipsub is used only for file-transfer chunk topics
(`file-transfer-{transfer_id}`), which the RBN auto-subscribes to whenever it
sees a manifest reference one. This is a legitimate future scaling question
(the RBN currently subscribes to every transfer topic unconditionally) but is
separate from the chat/relay path that failed in this stress test.

### 2.4 `max_connections` Defaults

- RBN: 1,000,000 (effectively unbounded — the RBN will accept connections
  from an unlimited number of stress peers with no backpressure).
- Client: 1,024.

Neither value directly caused the flapping (the RBN happily accepted 50
connections; the failure was circuit-level, not connection-count-level). But
the RBN's unbounded default means there is currently no backpressure
mechanism protecting a single RBN from an unbounded number of simultaneous
relay reservations — which is what let 50 stress peers all compete for the
same relay circuit capacity in the first place. This should be tightened
independent of the two bugs above.

---

## 3. Why It Looked Like Gossipsub Overload

The symptoms described in the original stress test (CPU spike, hot phone,
sluggish UI, "relay circuit flapping every ~30 seconds," "idle_mode
oscillation," "excessive chunk flushes") are all real and are all explained
by §2.1 and §2.2 — a feedback loop where circuit instability triggers
re-flushes, and re-flush traffic (30 chunks × 11 re-sends = 330 chunk sends
for what should have been ~30) adds load that makes the circuit more likely
to drop again. At 50 peers this loop had enough concurrent triggers to
saturate a mobile CPU. It scales badly with peer count not because of
gossipsub math, but because each additional peer is another potential
trigger for a re-flush storm on the shared RBN relay circuit.

---

## 4. Immediate Fixes (this bug, not the 1M-user roadmap)

1. **Fix the `idle_mode` race** — single-writer pattern or a debounce/lease so
   wake-on-push can't fight an explicit app-backgrounded signal within a
   short window.
2. **Make chunk re-flush delta-based** — persist `in_flight` state (with a
   lease/timeout) so a circuit flap doesn't cause the full pending queue to
   be re-selected and re-sent.
3. **Add backpressure to RBN relay reservations** — cap concurrent
   reservations/circuits per RBN (independent of `max_connections`) so a
   stress test (or real load spike) degrades gracefully instead of
   destabilizing the shared circuit for every connected peer.
4. **Tighten `max_connections` on the RBN** from 1,000,000 to a value tied to
   actual relay/circuit capacity, once (3) exists to enforce it gracefully.

These four are independent of, and should land before, any gossipsub
topic-sharding or multi-RBN work — that work addresses a different (real, but
not-yet-hit) scaling limit at much higher peer counts.

---

## 5. Deferred: Genuine 1M-User Scaling Questions (from v1 §8)

These remain valid for the eventual 1,000+ peers/RBN and 1M total user
targets, but are **not** blockers for fixing the 50-peer stress test result:

- Gossipsub topic partitioning for file-transfer chunk distribution, since
  the RBN currently auto-subscribes to every transfer topic unconditionally.
- Multi-RBN sharding for relay/mailbox capacity.
- Group message fan-out cost at large group sizes (currently O(n) unicast
  per sender — this *will* need addressing for groups much larger than the
  3-member case seen in these logs, independent of the RBN relay bug).
- DHT-based peer discovery to reduce reliance on a single relay circuit per
  RBN.

---

## 6. Files Referenced

| File | Relevance |
|------|-----------|
| `android_netlog_2026-07-17.txt` | Source of the idle_mode flap count and 11x re-flush evidence |
| `client_behaviour_header.rs` / `rbn_behaviour_header.rs` | Confirms gossipsub is RBN-only |
| `client_relay_references.txt` / `rbn_relay_references.txt` | Relay reservation/circuit code paths |
| `rbn_gossipsub_config.txt` / `rbn_gossipsub_references.txt` | Confirms gossipsub scope = file-transfer topics only |
| Client `mod.rs` lines 5199–5201, 4251–4253 | `idle_mode` race |
| Client `mod.rs` lines 2043–2084 | Full-queue re-flush bug |
