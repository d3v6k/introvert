# Rectification Plan — 2026-07-18: Intelligent File Transfer Routing (v2 — Merged)

**Status:** PENDING EXPERT APPROVAL
**Supersedes:** All prior file-transfer rectification plans (2026-07-09 through 2026-07-15)
**Supersedes routing behavior only** (does not touch payout/economy/UI code paths).
**Based on:** Expert plan (CLAUDE RECTIFICATION_PLAN_2026-07-18) merged with log analysis findings.

---

## 0. Problem Summary

Three devices (Android, Mac, iOS) on the **same LAN** sharing files in a group chat experience multi-minute delays instead of near-instantaneous direct P2P transfers. Root cause analysis identified 5 cascading issues. Expert plan provides a clean architectural solution.

### Device Inventory

| Device | Peer ID | Log File |
|--------|---------|----------|
| Android | `12D3KooWQM5mi5VV23k3APgXfafBpbiiG9QJEmXfmdLtipMdxECd` | `logs/android_netlog_2026-07-18.txt` |
| Mac | `12D3KooWCSejiZ1V5UDg6tkFu7g1rHjYf1LnzMiThywrMAFNtYvf` | `logs/mac_20260717_135632.log` |
| iOS | `12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP` | `logs/ios_20260717_135635.log` |
| RBN Relay | `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a` | Server-side (not available) |

---

## 1. Root Causes (from log analysis)

| RC | Problem | Evidence from Logs | Impact |
|----|---------|-------------------|--------|
| RC1 | ALL transfers route through cloud relay even on same LAN | `Relay hint: [iOS peer] is behind RBN` — all flushes are "InboundCircuit DB flush" | Adds ~200-400ms per chunk, limits throughput to relay capacity |
| RC2 | Relay circuit flaps every 2-10 min | `Fast reconnect: transfers waiting, no relay` cycle repeats 4+ times in 26-min log | Transfers paused 10-30s every 2-10 min |
| RC3 | Same chunks re-sent 3-9x on each reconnect | `gft_04731d1e...` chunks=1-0 sent 9 times; `gft_191e35c4...` chunks=1-7 sent 5 times | Massive wasted bandwidth, floods relay capacity |
| RC4 | 5s mailbox drain cooldown blocks chunk delivery | `[Mailbox] Skipping drain — last drain was < 5s ago` appears 400+ times | Up to 5s added latency per chunk batch |
| RC5 | iOS stuck in perpetual reconnect loop | `No local push token found in DB` + continuous reconnect cycle | iOS rarely in "online" state, can't receive chunks |

### Key Finding: The Fix is Wiring, Not New Logic

The mDNS detection logic already exists and correctly discovers same-network peers. The `get_recommended_path()` function already correctly returns "direct" for mDNS peers. But neither is consulted by `forward_to_mesh()`, which is the actual routing function. The current routing order lets gossipsub-over-relay "win" before direct P2P is even attempted.

---

## 2. Requested Priority Model

```
P1 — Direct P2P               (sender <=> receiver, no relay, any network, as long as directly dialable)
P2 — Same-LAN mesh            (group members on the same network as any holder of the file pull direct P2P from them)
P3 — Seeder cascade           (first group member to finish downloading becomes a seeder for the rest —
                                a local seeder is preferred over relay even if the original sender is remote)
P4 — RBN relay                (last resort — only when P1–P3 are impossible)
```

The current codebase implements pieces of P1 and P4, has the *data* for P2 (mDNS) but never wires it into the
routing decision, and has no P3 (seeder cascade) at all. `forward_to_mesh()` today effectively runs **P4 first**
for every file payload because gossipsub-over-relay succeeds before direct delivery is even attempted — this is
root cause RC1 and it is why LAN transfers take minutes instead of being instant.

---

## 3. Root Cause to Fix Map

| RC | Problem | Fix Section |
|----|---------|-------------|
| RC1 | All transfers go via relay even on same LAN | S4 Routing reorder + S5 Same-network resolver |
| RC2 | Relay circuit flaps every 2-10 min | S9 Circuit health + diagnostics |
| RC3 | Same chunks resent 3-9x | S7 Pull-only chunk lifecycle + ACK |
| RC4 | 5s mailbox cooldown blocks chunk delivery | S8 Split drain queues |
| RC5 | iOS stuck in reconnect loop, no push token | S10 iOS-specific fixes |

---

## 4. Routing Decision Reorder (RC1, core fix)

### Current order in `forward_to_mesh()` (network/mod.rs lines 3035-3284)
```
1. WebRTC (if open)
2. Gossipsub (file payloads) — ALWAYS attempted for FileChunk/FileChunkRequest,
   BEFORE the is_connected() check -> this is why relay "wins" on LAN
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
        known_seeders: &[PeerId],          // from RegisterSeeder gossip, see S5
    ) -> TransferPath {
        // P1 — direct dial to the actual recipient (covers same-LAN sender->receiver,
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
- `Direct` / `LocalSeeder` -> send via the existing binary v2 request-response codec (the same path already used
  in Section 5 "Try direct libp2p delivery if connected") — **gossipsub is skipped entirely**, so it can never
  race the relay circuit into "winning."
- `Relay` -> existing gossipsub-over-`file-transfer-{id}` topic path, unchanged.

This directly reuses `mdns_peers` (already populated in network/mod.rs line 1658) and
`get_recommended_path()` (already correct in intro_claw.rs line 1792) — the fix is **wiring**,
not new detection logic. Today `get_recommended_path()` is computed but never consulted by `forward_to_mesh`;
after this change it becomes the actual router.

**mDNS is sufficient for same-network detection.** mDNS multicast does not cross routers/subnets by design, so
`mdns_peers.contains(peer)` is already a correct "same LAN" signal — no need for manual subnet-comparison logic.

### Files to Modify
- `src/network/mod.rs`: Add `TransferRouter` struct, modify `forward_to_mesh()` to call `resolve()` first
- `src/network/service.rs`: Add `TransferRouter` field to `NetworkService`
- `src/intro_claw.rs`: Wire `get_recommended_path()` output into `TransferRouter`

---

## 5. Group Seeder Cascade (P3)

### 5.1 Sender-side: unchanged
`process_outgoing_file` still gossips the manifest to the group topic and calls `RegisterSeeder` (per
DEBUG_REPORT_2026-07-10.md Fix 6, already correct ordering).

### 5.2 New: `SeederAnnounce` control message
When any receiver finishes a transfer (`FileTransferComplete` fires locally), it now also:

```rust
// New lightweight gossip message (control-plane only, no chunk data)
struct SeederAnnounce {
    transfer_id: String,
    seeder_peer_id: PeerId,
    is_mdns_local: bool,   // true if the announcing peer was itself reached via mDNS
}
```

Published once on the group's `file-transfer-{id}` topic. Every other in-progress receiver's `TransferRouter`
appends the announcing peer to `known_seeders` for that transfer (S4). Because mDNS discovery is symmetric within
a LAN, a receiver on the same LAN as the new seeder will find it in `mdns_peers` and immediately switch from
`Relay` -> `LocalSeeder` on the *next* chunk request.

**Effect:** in a 3-device group where the Mac is remote and Android+iOS are on the same LAN, only *one* copy of
the file crosses the relay (Mac -> first LAN receiver). The second LAN device downloads at LAN speed from the
first, never touching Alibaba Cloud.

### 5.3 Seeder eligibility guard
Only announce as a seeder once the full file is verified locally (hash match against manifest) — reuses the
existing `get_drive_file_by_hash` check already used for the FFI dedup guard.

### Files to Modify
- `src/network/mod.rs`: Handle `SeederAnnounce` in `HandleIncomingPayload`, publish on `FileTransferComplete`
- `src/network/types.rs`: Add `SeederAnnounce` variant to `SignalingPayload`
- `src/network/service.rs`: Add `known_seeders: HashMap<String, Vec<PeerId>>` to `NetworkService`

---

## 6. Implementation Order (risk-ranked)

| Step | Change | Risk | Unlocks |
|------|--------|------|---------|
| 1 | `TransferRouter` + reorder `forward_to_mesh` to resolve path before dispatch (S4) | Low | Fixes RC1 — same-LAN goes direct immediately |
| 2 | Split mailbox drain cooldowns (S8) | Low | Fixes RC4 |
| 3 | `SeederAnnounce` + `known_seeders` wiring (S5) | Medium | Delivers P3 (seeder cascade) |
| 4 | Pull-only chunk lifecycle + `FileChunkAck` (S7) | Medium | Fixes RC3 — should also reduce RC2 (less circuit load) |
| 5 | Re-measure relay flap rate with RC1/RC3 fixed; pull RBN server logs if it persists (S9) | Depends on findings | Confirms/fixes RC2 |
| 6 | iOS push token + reservation-desync audit (S10) | Low-Medium | Fixes RC5 |
| 7 | Wire `TransferRouter` output into IntroClaw `TransferCircuitPrewarmer`/`get_transfer_policy` (S11) | Low | Avoids relay work for already-direct/seeded transfers |

**Steps 1-2 alone should resolve the reported same-network multi-minute-delay symptom**, since they stop every LAN
transfer from being routed through Alibaba Cloud in the first place. Steps 3-4 are what make the *group* case
efficient (one relay hop instead of N), and step 5 is diagnostic, gated on evidence not yet available.

---

## 7. Pull-Only Chunk Lifecycle (RC3)

The current design is a **hybrid** push/pull model, and that hybrid is the actual source of the duplicate-chunk
explosion:
- Relay-path receivers already *pull* via `FileChunkRequest` (confirmed in DEBUG_DOCUMENT.md).
- But the sender **also** independently pushes everything sitting in `pending_file_chunks` on every
  `InboundCircuitEstablished`/`OutboundCircuitEstablished`, regardless of whether the receiver asked for those
  specific chunks.

### 7.1 Fix — unify on pull, keep push only as a LAN-direct optimization
- **Direct/LocalSeeder paths (P1-P3):** keep the current proactive push via the direct request-response channel —
  at LAN speed the "waste" of an unrequested chunk is negligible and push avoids a round trip.
- **Relay path (P4) only:** delete the DB-flush-on-circuit-event behavior entirely. `pending_file_chunks` becomes
  **request-driven only** — a row is only dequeued in direct response to an inbound `FileChunkRequest`, never as a
  bulk drain on circuit reconnect.

```rust
// storage.rs — new, replaces the reconnect-triggered bulk dequeue
pub fn get_chunk_for_request(&self, transfer_id: &str, chunk_index: u32) -> Result<Option<Vec<u8>>> {
    // single-row fetch, no in_flight bookkeeping needed — the request IS the demand signal
}
```

### 7.2 Explicit chunk ACK
Add a small control message so the sender's queue is authoritative on *delivery*, not on *gossipsub publish success*:

```rust
struct FileChunkAck { transfer_id: String, chunk_indices: Vec<u32> } // batched, sent every ~500ms or 10 chunks
```

`remove_pending_chunk()` is called on ACK receipt, not on gossipsub-publish success. A chunk is only ever *sent*
because it was *requested*, and only ever *removed* because it was *confirmed received* — this closes the 3-9x
resend loop completely.

### 7.3 Schema change
```sql
ALTER TABLE pending_file_chunks ADD COLUMN requested_at INTEGER DEFAULT 0;
ALTER TABLE pending_file_chunks ADD COLUMN acked INTEGER NOT NULL DEFAULT 0;
-- Drop the in_flight_since bulk-flush machinery (no longer needed under pull-only model)
```

### Files to Modify
- `src/storage.rs`: Add `get_chunk_for_request()`, add `requested_at`/`acked` columns
- `src/network/mod.rs`: Remove bulk DB flush from `InboundCircuitEstablished`/`OutboundCircuitEstablished`
- `src/network/types.rs`: Add `FileChunkAck` variant to `SignalingPayload`

---

## 8. Split Mailbox Drain Queues (RC4)

Two independent cooldowns replace the single 5s global drain cooldown:

```rust
struct DrainState {
    last_mail_drain: Instant,     // chat messages / mailbox fetch — keep FCM-echo-loop protection
    last_chunk_drain: Instant,    // file chunk delivery
}

const MAIL_DRAIN_COOLDOWN: Duration = Duration::from_secs(30); // matches the FCM echo-loop fix already deployed
const CHUNK_DRAIN_COOLDOWN: Duration = Duration::from_millis(250); // just enough to prevent a thundering herd
```

### Files to Modify
- `src/network/mod.rs`: Replace single `last_drain` timer with split `DrainState`

---

## 9. Relay Circuit Stability Diagnostics (RC2)

The available logs cannot fully explain the 2-10 min drop cycle because **RBN server-side logs are not part of
this package**. Two candidate causes remain open pending server logs:
1. RC3's duplicate-chunk flood was pushing far more data/circuit churn through the RBN than a normal transfer
   would — once S7 removes the resend storm, re-measure before assuming a server-side limit is at fault.
2. A client-initiated teardown bug of the same shape as the already-fixed iOS reservation-desync bug
   (DEBUG_REPORT_2026-07-09.md) could exist on a different code path.

**Action:** add explicit logging that distinguishes *why* a circuit closure was observed —
`ListenerClosed` (local), `ConnectionClosed` (remote or local-initiated close), or a relay-protocol-level
`CircuitClosed` reason code — and re-run the same 3-device same-LAN scenario after S4 and S7 land. Pull actual
RBN (`introvertd`) logs for the equivalent time window before further relay-side changes.

---

## 10. iOS-Specific Fixes (RC5)

1. **Push token registration:** DEBUG_DOCUMENT.md confirms `APNs: Not configured (iOS push disabled)`. Until
   APNs is configured server-side, iOS cannot receive wake pushes and will always rely on foreground polling.
2. **Reconnect loop pattern match:** Apply the same audit as the already-fixed relay_reservations desync
   (DEBUG_REPORT_2026-07-09.md): confirm `ConnectionClosed` on iOS is correctly clearing `relay_reservations`
   when `!self.swarm.is_connected(&peer_id)`, specifically for cold-start cases.
3. Once S5 (seeder cascade) and S4 (direct-first routing) land, iOS's dependence on a working relay circuit drops
   sharply for the same-LAN group case.

---

## 11. IntroClaw Integration

This plan sits under the existing INTRO_CLAW_TRANSFER_ENHANCEMENT_PLAN.md unchanged:
- `TransferRouter` (S4) consumes `ClawTickContext.active_seeder_peers`, `mdns_discovered`, and `known_seeders`
- `TransferCircuitPrewarmer` should now **only prewarm relay circuits for peers the `TransferRouter` actually
  resolved to `Relay`** — prewarming for direct/seeded peers is wasted RBN capacity
- `get_transfer_policy()` should source `is_relayed` from `TransferRouter::resolve()` output

---

## 12. Validation Plan

Re-run the exact 3-device scenario (Android/Mac/iOS, same LAN, group file share) after each step:
- **Step 1:** `[Relay] InboundCircuit DB flush` log lines should no longer appear for this scenario — chunks
  should flow via the direct request-response codec instead.
- **Step 3:** the *second* LAN receiver's log should show a `LocalSeeder` resolution referencing the *first* LAN
  receiver's peer ID, not the original (possibly remote) sender's.
- **Step 4:** grep for repeated `chunks=X-Y` sends of the same `transfer_id`/range — should occur 0 times (down
  from 3-9x observed in PROBLEM_STATEMENT.md RC3 evidence).
- **Step 5:** with steps 1-4 in place, capture whether `OutboundCircuitEstablished` -> drop cycles still occur;
  if they do, escalate to RBN server-side log review before further client changes.
- **Full regression:** Cross-network (different LANs) file transfers should still work via relay.

---

## 13. Expert Review — Hardening Notes (Approved)

Expert reviewed v2 against source material and original plan. **Verdict: Approved for implementation, phased as written.** Four hardening items to fold into steps 3-4:

### 13.1 Seeder Fallback on Failure (fold into Step 3)
`resolve()` picks the first LAN seeder in `known_seeders` and returns `LocalSeeder`, but there's no path back to `Relay` if that seeder's app is backgrounded/killed mid-serve.

**Fix:** If a `LocalSeeder` dial or chunk request times out, mark that seeder as bad for this transfer (short cooldown) and re-resolve — falling through to the next known seeder or `Relay`. Without this, a stalled seeder silently stalls the whole downstream chain instead of degrading to relay.

```rust
// In TransferRouter::resolve(), add timeout/cooldown tracking:
pub struct TransferRouter {
    failed_seeders: HashMap<(String, PeerId), Instant>,  // (transfer_id, seeder) -> last_failure
    seeder_cooldown: Duration,  // e.g., 30s
}

// In resolve(), skip seeders in cooldown:
if let Some(local_seeder) = known_seeders.iter()
    .find(|p| mdns_peers.contains(*p) && !self.is_seeder_in_cooldown(transfer_id, p))
{
    return TransferPath::LocalSeeder(*local_seeder);
}
```

### 13.2 `known_seeders` Cleanup (fold into Step 3)
The `HashMap<String, Vec<PeerId>>` has no eviction path. Needs to be cleared on `FileTransferComplete`/transfer eviction, same as `pending_file_chunks`.

**Fix:** Add cleanup to:
- `FileTransferComplete` handler → remove entry for that transfer_id
- Stale transfer watchdog eviction → remove entry
- Transfer cancel → remove entry

### 13.3 Control-Topic Subscription for Direct-Only Participants (verify in Step 1)
Once chunk data skips gossipsub entirely (S4/S7), confirm the `file-transfer-{id}` topic is still subscribed to purely for `SeederAnnounce`/`FileChunkAck` control traffic. The current subscribe call is gated on `is_file_payload` inside the code path being restructured.

**Fix:** In the `TransferRouter` dispatch path, ensure `gossipsub.subscribe(&file_transfer_topic)` is called for ALL `TransferPath` variants (Direct, LocalSeeder, Relay), not just Relay. The topic is needed for control messages regardless of data path.

### 13.4 Wire Compatibility for New SignalingPayload Variants (address in Step 3/4)
Old clients need to silently ignore unknown `SignalingPayload` variants rather than erroring. `SeederAnnounce` and `FileChunkAck` should ship behind additive-enum-safe deserialization.

**Fix:** Ensure serde `#[serde(deny_unknown_fields)]` is NOT used on `SignalingPayload`, and add `#[serde(other)]` fallback variant for forward compatibility. Or use a version check before publishing new variants.

---

## 14. PR Strategy

**Cut steps 1-2 as a standalone PR first.** They alone fix the reported symptom (same-network multi-minute delay) and are independently low-risk. Don't block on the seeder-cascade work.

| PR | Steps | Risk | Fixes |
|----|-------|------|-------|
| PR-1 | Steps 1-2 (TransferRouter + cooldown split) | Low | RC1, RC4 — fixes the reported delay |
| PR-2 | Steps 3-4 (seeder cascade + pull-only chunks + hardening notes 13.1-13.4) | Medium | RC3, reduces RC2 |
| PR-3 | Steps 5-7 (relay diagnostics, iOS, IntroClaw) | Low-Medium | RC2, RC5, optimization |

PR-1 can merge independently. PR-2 builds on PR-1. PR-3 is diagnostic/optimization, not blocking.
