# Rectification Plan — 2026-07-18: Intelligent File Transfer Routing

**Supersedes routing behavior only** (does not touch payout/economy/UI code paths).
**Ties together:** `PROBLEM_STATEMENT.md` (5 root causes), `INTRO_CLAW_TRANSFER_ENHANCEMENT_PLAN.md`, and the requested priority model below.

---

## 0. Requested Priority Model

```
P1 — Direct P2P               (sender ⇄ receiver, no relay, any network, as long as directly dialable)
P2 — Same-LAN mesh            (group members on the same network as *any* holder of the file pull direct P2P from them)
P3 — Seeder cascade           (first group member to finish downloading becomes a seeder for the rest —
                                a local seeder is preferred over relay even if the original sender is remote)
P4 — RBN relay                (last resort — only when P1–P3 are impossible)
```

The current codebase implements pieces of P1 and P4, has the *data* for P2 (mDNS) but never wires it into the
routing decision, and has no P3 (seeder cascade) at all. `forward_to_mesh()` today effectively runs **P4 first**
for every file payload because gossipsub-over-relay succeeds before direct delivery is even attempted — this is
root cause RC1 and it is why LAN transfers take minutes instead of being instant.

This plan closes that gap without ripping out the relay path (still needed for genuinely cross-network peers).

---

## 1. Root Cause → Fix Map

| RC | Problem | Fix Section |
|----|---------|-------------|
| RC1 | All transfers go via relay even on same LAN | §2 Routing reorder + §3 Same-network resolver |
| RC2 | Relay circuit flaps every 2–10 min | §6 Circuit health + diagnostics |
| RC3 | Same chunks resent 3–9x | §4 Pull-only chunk lifecycle + ACK |
| RC4 | 5s mailbox cooldown blocks chunk delivery | §5 Split drain queues |
| RC5 | iOS stuck in reconnect loop, no push token | §7 iOS-specific fixes |

---

## 2. Routing Decision Reorder (RC1, core fix)

### Current order in `forward_to_mesh()` (network_mod_relevant_sections.rs, Section 5)
```
1. WebRTC (if open)
2. Gossipsub (file payloads) — ALWAYS attempted for FileChunk/FileChunkRequest,
   BEFORE the is_connected() check → this is why relay "wins" on LAN
3. Direct libp2p (if connected)
4. dial_relay_path()
5. Mailbox / DB queue
```

### New order — a `TransferRouter` resolves the path *before* forward_to_mesh dispatches

```rust
/// Called once per outbound file payload, replaces the ad-hoc ordering in forward_to_mesh().
pub enum TransferPath {
    Direct(PeerId),          // dial recipient directly — same LAN or already-connected NAT-punched peer
    LocalSeeder(PeerId),     // dial a *different* peer (same LAN as us) who already holds the file
    Relay(PeerId),           // recipient behind RBN — use gossipsub/relay circuit
}

impl TransferRouter {
    pub fn resolve(
        &self,
        recipient_id: &PeerId,
        transfer_id: &str,
        mdns_peers: &HashSet<PeerId>,
        swarm_connected: impl Fn(&PeerId) -> bool,
        known_seeders: &[PeerId],          // from RegisterSeeder gossip, see §3.2
    ) -> TransferPath {
        // P1 — direct dial to the actual recipient (covers same-LAN sender→receiver,
        // and already-hole-punched NAT peers via DCUtR)
        if mdns_peers.contains(recipient_id) || swarm_connected(recipient_id) {
            return TransferPath::Direct(*recipient_id);
        }

        // P2/P3 — is there a *seeder* for this transfer that IS on our LAN?
        // (covers: original sender is remote, but a group-mate on our LAN already
        // finished downloading and can serve us instead of the relay)
        if let Some(local_seeder) = known_seeders.iter().find(|p| mdns_peers.contains(*p)) {
            return TransferPath::LocalSeeder(*local_seeder);
        }

        // P4 — fall back to relay
        TransferPath::Relay(*recipient_id)
    }
}
```

`forward_to_mesh()` calls `TransferRouter::resolve()` **first**, then:
- `Direct` / `LocalSeeder` → send via the existing binary v2 request-response codec (the same path already used
  in Section 5 "Try direct libp2p delivery if connected") — **gossipsub is skipped entirely**, so it can never
  race the relay circuit into "winning."
- `Relay` → existing gossipsub-over-`file-transfer-{id}` topic path, unchanged.

This directly reuses `mdns_peers` (already populated in `network_mod_relevant_sections.rs` Section 1) and
`get_recommended_path()` (already correct in `intro_claw_relevant_sections.rs` Section 2) — the fix is **wiring**,
not new detection logic. Today `get_recommended_path()` is computed but never consulted by `forward_to_mesh`;
after this change it becomes the actual router.

**mDNS is sufficient for same-network detection.** mDNS multicast does not cross routers/subnets by design, so
`mdns_peers.contains(peer)` is already a correct "same LAN" signal — no need for manual subnet-comparison logic
(this answers Key Question 1 in the problem statement).

---

## 3. Group Seeder Cascade (P3)

### 3.1 Sender-side: unchanged
`process_outgoing_file` still gossips the manifest to the group topic and calls `RegisterSeeder` (per
`DEBUG_REPORT_2026-07-10.md` Fix 6, already correct ordering).

### 3.2 New: `SeederAnnounce` control message
When any receiver finishes a transfer (`FileTransferComplete` fires locally), it now also:

```rust
// New lightweight gossip message (control-plane only, no chunk data)
struct SeederAnnounce {
    transfer_id: String,
    seeder_peer_id: PeerId,
    is_mdns_local: bool,   // true if the announcing peer was itself reached via mDNS by *any* group member
}
```

Published once on the group's `file-transfer-{id}` topic. Every other in-progress receiver's `TransferRouter`
appends the announcing peer to `known_seeders` for that transfer (§2). Because mDNS discovery is symmetric within
a LAN, a receiver on the same LAN as the new seeder will find it in `mdns_peers` and immediately switch from
`Relay` → `LocalSeeder` on the *next* chunk request — no protocol renegotiation needed, since chunk requests are
already peer-addressed (`FileChunkRequest { transfer_id, chunk_index }` sent to a specific `PeerId`).

**Effect:** in a 3-device group where the Mac is remote and Android+iOS are on the same LAN, only *one* copy of
the file crosses the relay (Mac → first LAN receiver). The second LAN device downloads at LAN speed from the
first, never touching Alibaba Cloud. This is the literal implementation of the requested "seeders for the rest
of the group" behavior.

### 3.3 Seeder eligibility guard
Only announce as a seeder once the full file is verified locally (hash match against manifest) — reuses the
existing `get_drive_file_by_hash` check already used for the FFI dedup guard (`DEBUG_REPORT_2026-07-10.md`,
"FFI Dedup Guard" section) so partial/corrupt downloads are never advertised.

---

## 4. Pull-Only Chunk Lifecycle (RC3)

The current design is a **hybrid** push/pull model, and that hybrid is the actual source of the duplicate-chunk
explosion:
- Relay-path receivers already *pull* via `FileChunkRequest` (confirmed in `DEBUG_DOCUMENT.md`).
- But the sender **also** independently pushes everything sitting in `pending_file_chunks` on every
  `InboundCircuitEstablished`/`OutboundCircuitEstablished` (`network_mod_relevant_sections.rs` §2–3), regardless
  of whether the receiver asked for those specific chunks.

These two mechanisms fight each other: a chunk the receiver already has (or hasn't asked for yet) gets pushed
again every time the circuit bounces (RC2), because `dequeue_pending_chunks()` has no concept of "was this chunk
already delivered" — only "was it claimed in-flight in the last 30s" (`storage_relevant_sections.rs` §3, §5).

### 4.1 Fix — unify on pull, keep push only as a LAN-direct optimization
- **Direct/LocalSeeder paths (P1–P3):** keep the current proactive push via the direct request-response channel —
  at LAN speed the "waste" of an unrequested chunk is negligible and push avoids a round trip. No DB queue
  involved at all for this path; chunks go straight from disk to the wire.
- **Relay path (P4) only:** delete the DB-flush-on-circuit-event behavior entirely. `pending_file_chunks` becomes
  **request-driven only** — a row is only dequeued in direct response to an inbound `FileChunkRequest`, never as a
  bulk drain on circuit reconnect.

```rust
// storage.rs — new, replaces the reconnect-triggered bulk dequeue
pub fn get_chunk_for_request(&self, transfer_id: &str, chunk_index: u32) -> Result<Option<Vec<u8>>> {
    // single-row fetch, no in_flight bookkeeping needed — the request IS the demand signal
}
```

### 4.2 Explicit chunk ACK (belt-and-suspenders for the relay path)
Add a small control message so the sender's queue (if it ever needs to know what's outstanding, e.g. for retry
after receiver reconnect) is authoritative on *delivery*, not on *gossipsub publish success* (publish success only
means "handed to the local libp2p gossipsub mesh," not "received by the peer" — that distinction is the deeper bug
behind RC3 even independent of the reconnect-flush behavior):

```rust
struct FileChunkAck { transfer_id: String, chunk_indices: Vec<u32> } // batched, sent every ~500ms or 10 chunks
```

`remove_pending_chunk()` is called on ACK receipt, not on gossipsub-publish success. Combined with §4.1, a chunk
is only ever *sent* because it was *requested*, and only ever *removed* because it was *confirmed received* — this
closes the 3–9x resend loop completely rather than just reducing its frequency.

### 4.3 Schema change
```sql
-- Drop the in_flight_since bulk-flush machinery (no longer needed under pull-only model)
-- Keep the table for the relay-path retry case, but it now records "requested, not yet acked"
ALTER TABLE pending_file_chunks ADD COLUMN requested_at INTEGER DEFAULT 0;
ALTER TABLE pending_file_chunks ADD COLUMN acked INTEGER NOT NULL DEFAULT 0;
```

This answers Key Question 3 directly: the right model is neither "in_flight flag with timeout" nor "delete on
some proxy for success" alone — it's **request-gated send + ACK-gated delete**, which is a stronger invariant than
either.

---

## 5. Split Mailbox Drain Queues (RC4)

Two independent cooldowns replace the single 5s global drain cooldown:

```rust
struct DrainState {
    last_mail_drain: Instant,     // chat messages / mailbox fetch — keep FCM-echo-loop protection
    last_chunk_drain: Instant,    // file chunk delivery
}

const MAIL_DRAIN_COOLDOWN: Duration = Duration::from_secs(30); // matches the FCM echo-loop fix already deployed
const CHUNK_DRAIN_COOLDOWN: Duration = Duration::from_millis(250); // just enough to prevent a thundering herd
```

Under the pull-only model (§4), "chunk drain" mostly disappears as a concept for the relay path (each request is
answered individually), so this cooldown split matters most for the RAM-buffered `pending_messages` control
traffic (`SeederAnnounce`, `FileChunkAck`, `FileChunkRequest`) which must not be held behind the 30s mail cooldown
the way it currently sits behind a single shared timer.

---

## 6. Relay Circuit Stability Diagnostics (RC2)

The available logs cannot fully explain the 2–10 min drop cycle because **RBN server-side logs are not part of
this package** (noted in `PROBLEM_STATEMENT.md` §2 — "Server-side (not available)"). Two candidate causes remain
open pending server logs:
1. RC3's duplicate-chunk flood was pushing far more data/circuit churn through the RBN than a normal transfer
   would — once §4 removes the resend storm, re-measure before assuming a server-side limit is at fault.
2. A client-initiated teardown bug of the same shape as the already-fixed iOS reservation-desync bug
   (`DEBUG_REPORT_2026-07-09.md` §1) could exist on a different code path (Android's cycle looks reconnect-driven
   rather than server-driven — "Fast reconnect: transfers waiting, no relay" firing on a timer, not on an observed
   `ConnectionClosed`).

**Action:** add explicit logging that distinguishes *why* a circuit closure was observed —
`ListenerClosed` (local), `ConnectionClosed` (remote or local-initiated close), or a relay-protocol-level
`CircuitClosed` reason code — and re-run the same 3-device same-LAN scenario after §2 and §4 land, since with
direct P2P handling all same-LAN traffic, the relay circuit will carry near-zero data and any flapping still
observed will cleanly isolate to a server-side or protocol-level cause. Pull actual RBN (`introvertd`) logs for
the equivalent time window before further relay-side changes — this is required, not optional, to close RC2 with
confidence (this answers Key Question 4: insufficient evidence in the current package to say definitively).

---

## 7. iOS-Specific Fixes (RC5)

1. **Push token registration:** `DEBUG_DOCUMENT.md` confirms `APNs: Not configured (iOS push disabled)`. Until
   APNs is configured server-side, iOS cannot receive wake pushes and will always rely on foreground polling —
   configure APNs credentials on `introvertd` and verify `No local push token found in DB to auto-register`
   resolves post-registration.
2. **Reconnect loop pattern match:** iOS's `Fast reconnect: transfers waiting, no relay (peers=15, incoming=0,
   seeders=0, pending=1)` → `No RBNs reachable — will retry in 30s` → repeat is structurally similar to the
   already-fixed `relay_reservations` desync (`DEBUG_REPORT_2026-07-09.md` §1). Apply the same audit: confirm
   `ConnectionClosed` on iOS is correctly clearing `relay_reservations` when `!self.swarm.is_connected(&peer_id)`,
   specifically for the case where the *first* reservation attempt after a cold start never succeeds (as opposed
   to the already-covered "RBN restarted mid-session" case).
3. Once §3 (seeder cascade) and §2 (direct-first routing) land, iOS's dependence on a working relay circuit drops
   sharply for the same-LAN group case — it will pull from Android/Mac directly — reducing the practical impact of
   this bug even before the root cause is fully isolated.

---

## 8. IntroClaw Integration

This plan is designed to sit under the existing `INTRO_CLAW_TRANSFER_ENHANCEMENT_PLAN.md` unchanged:
- `TransferRouter` (§2) is a new, small, pure-logic module — it consumes `ClawTickContext.active_seeder_peers`,
  `mdns_discovered`, and the new `known_seeders` list (populated from `SeederAnnounce`, §3.2), all of which are
  already planned additions in that enhancement doc.
- `TransferCircuitPrewarmer` (Enhancement 2) should now **only prewarm relay circuits for peers the
  `TransferRouter` actually resolved to `Relay`** — prewarming a relay circuit for a peer that direct/seeder
  routing will serve is wasted RBN capacity and re-introduces relay churn.
- `get_transfer_policy()` (Enhancement 3) gets a cheap win: `is_relayed` in that function should be sourced from
  `TransferRouter::resolve()`'s output rather than re-derived, so policy and routing never disagree about which
  path is active.

---

## 9. Implementation Order (risk-ranked)

| Step | Change | Risk | Unlocks |
|------|--------|------|---------|
| 1 | `TransferRouter` + reorder `forward_to_mesh` to resolve path before dispatch (§2) | Low | Fixes RC1 — same-LAN goes direct immediately |
| 2 | Split mailbox drain cooldowns (§5) | Low | Fixes RC4 |
| 3 | `SeederAnnounce` + `known_seeders` wiring (§3) | Medium | Delivers P3 (seeder cascade) |
| 4 | Pull-only chunk lifecycle + `FileChunkAck` (§4) | Medium | Fixes RC3 — should also reduce RC2 (less circuit load) |
| 5 | Re-measure relay flap rate with RC1/RC3 fixed; pull RBN server logs if it persists (§6) | Depends on findings | Confirms/fixes RC2 |
| 6 | iOS push token + reservation-desync audit (§7) | Low–Medium | Fixes RC5 |
| 7 | Wire `TransferRouter` output into IntroClaw `TransferCircuitPrewarmer`/`get_transfer_policy` (§8) | Low | Avoids relay work for already-direct/seeded transfers |

Steps 1–2 alone should resolve the reported same-network multi-minute-delay symptom, since they stop every LAN
transfer from being routed through Alibaba Cloud in the first place. Steps 3–4 are what make the *group* case
efficient (one relay hop instead of N), and step 5 is diagnostic, gated on evidence not yet available in this
package.

---

## 10. Validation Plan

Re-run the exact 3-device scenario (Android/Mac/iOS, same LAN, group file share) after each step and check:
- Step 1: `[Relay] InboundCircuit DB flush` log lines should no longer appear for this scenario at all — chunks
  should flow via the direct request-response codec instead.
- Step 3: the *second* LAN receiver's log should show a `LocalSeeder` resolution referencing the *first* LAN
  receiver's peer ID, not the original (possibly remote) sender's.
- Step 4: grep for repeated `chunks=X-Y` sends of the same `transfer_id`/range — should occur 0 times (down from
  3–9x observed in `PROBLEM_STATEMENT.md` RC3 evidence).
- Step 5: with steps 1–4 in place, capture whether `OutboundCircuitEstablished` → drop cycles still occur; if they
  do, escalate to RBN server-side log review before further client changes.
