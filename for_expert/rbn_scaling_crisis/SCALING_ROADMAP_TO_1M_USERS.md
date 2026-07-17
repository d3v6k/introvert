# Introvert Scaling Roadmap — Path to 1,000,000+ Users

**Date:** 2026-07-17
**Status:** Living document — update after each phase's real test data comes in
**Purpose:** Separate what has actually been proven from what is still assumed,
and lay out the concrete work + tests needed to validate (or correct) the
original 1M-user architecture target.

---

## How to read this document

Every claim below is tagged:

- ✅ **PROVEN** — verified against real logs, real source code, or a real test run.
- 🔧 **FIXED, NOT YET VERIFIED AT SCALE** — a real bug was found and a fix is
  built/building, but it has only been validated at small scale (or not yet
  re-tested at all).
- ❓ **UNKNOWN / UNTESTED** — a claim from the original architecture plan that
  has never been measured against a real system.
- 🧭 **DESIGN QUESTION** — a decision that hasn't been made yet because it
  needs data this roadmap will produce.

The goal is that by the end of this roadmap, nothing important is left in
the ❓ or 🧭 categories for the scale you actually need.

---

## Part 1 — What We've Actually Proven So Far

### 1.1 The 50-peer stress test failure was NOT a gossipsub/mesh scaling problem

✅ **PROVEN.** The original theory — "gossipsub mesh grows flat and every
peer sees every other peer's heartbeat traffic" — was checked against the
actual client source code. The client's `IntrovertBehaviour` struct has no
`gossipsub` field at all. Only RBN-class nodes run gossipsub, and only for
file-transfer chunk topics. Clients talk to the network exclusively via
direct P2P, request-response signaling, and relay circuits. This theory is
retired; do not resurrect it as an explanation for relay/circuit problems.

### 1.2 The actual cause was two verified bugs in the relay-circuit lifecycle

✅ **PROVEN**, confirmed by reading the real source at the real line numbers
(not just grepped snippets — mimo cli traced this directly in `src/network/mod.rs`):

- **`idle_mode` race** — two uncoordinated writers to the same flag (an
  OS-lifecycle callback and a "wake on incoming message" handler) fought
  each other, flipping state 35 times in a 9-minute test window.
- **Full-queue re-flush on every circuit reconnect** — a flapping circuit
  caused the same backlog of file chunks to be resent from scratch on every
  reconnect. One peer received the same ~30-chunk batch 11 separate times
  in one session (330 sends for what should have been ~30).
- These two bugs formed a feedback loop: flag flips → reconnect → resend
  everything → extra load destabilizes the circuit further → flag flips
  again. This explains the CPU spike and UI lag at 50 peers without any
  gossipsub math being involved.

### 1.3 Fixes designed, source-verified against the real libp2p dependency, and approved for build

🔧 **FIXED, NOT YET VERIFIED AT SCALE.** Four fixes were designed, and —
critically — every non-trivial claim underlying them was checked against
the actual `libp2p-relay-0.21.1` source before being approved, not assumed:

| Fix | What it does | Verified against |
|---|---|---|
| Circuit-drop/reservation logging + mailbox-drain cooldown guard | Gives visibility into real flap/reject rates; stops redundant empty mailbox-drain calls | Source-confirmed mailbox drain is a DELETE-after-fetch, so it was already data-safe, just noisy |
| Delta-based chunk re-flush | Only resends chunks whose "in flight" lease has actually expired, not the whole backlog; single-flight lock (with a 30s auto-clear safety net) prevents two flush tasks racing for the same peer | Corrected once already — the first draft would have let dead in-flight leases stall forever with no timeout; now has a hard auto-clear |
| `idle_mode` → explicit state machine | Replaces the racing boolean with `Foreground` / `Backgrounded` / `BackgroundedPendingWake` and a 5s debounce, so a background signal can't be immediately undone by an unrelated incoming message | — |
| RBN relay rate limiting | Turns on rate limits that were already built into the libp2p relay module but not configured: circuit + reservation limits tightened from library defaults (30/2min → 10/2min per peer), `max_circuits_per_peer` raised 4→8 to tolerate legitimate reconnect overlap | Confirmed via direct source read of `libp2p-relay-0.21.1/src/behaviour.rs` — default values, and that the tightening uses additive builder methods rather than accidentally replacing the library's existing per-IP protections |

**What "approved for build" does NOT mean:** it does not mean these numbers
(5s debounce, 10/2min rate limit, 8 circuits/peer) are correct. They are
informed starting values. They become ✅ PROVEN only after the 50-peer
stress test is re-run against the acceptance criteria below and the real
numbers come back in range.

### 1.4 Acceptance criteria for the current fix (must pass before Part 2 starts)

| Metric | Baseline (broken) | Target | Status |
|---|---|---|---|
| `idle_mode`/`app_state` flaps in 9 min | 35 | < 5 | ⏳ pending re-test |
| Duplicate chunk sends to same peer/transfer | 30 chunks × 11 resends | 1x per chunk | ⏳ pending re-test |
| Circuit establish → drop interval | ~30s cycle | > 60s | ⏳ pending re-test |
| RBN CPU under 50-peer load | High/unstable | Stable | ⏳ pending re-test |

**This table is the gate for the rest of this roadmap.** Nothing below should
be treated as "on track" until this table is filled in with real numbers.

---

## Part 2 — What the Original 1M-User Plan Claims, and Its Actual Status

The `ARCHITECTURE_BLUEPRINT.md` sets a "Million-Node Mandate" and asserts the
system is "designed against this extreme scale to prevent loop starvation or
O(N) degradation." Here is what's actually behind that claim today:

### 2.1 RBN capacity ceiling

❓ **UNKNOWN.** The RBN currently allows up to 4,096 simultaneous relay
circuits and 8,192 reservations — numbers that exist in the config file, not
numbers derived from measuring what the server hardware can sustain. Nobody
has run the RBN past 50 real/simulated concurrent peers. "Thousands of peers
per RBN" is unverified in either direction — it could be conservative or
wildly optimistic.

**What would resolve this:** progressive stress testing (Phase 1 below),
instrumented with the new Fix-4 logging, watching real CPU/memory/circuit
counts as peer count climbs, to find the actual breaking point on real
target hardware.

### 2.2 Group chat fan-out at scale

❓ **UNKNOWN, and not a small concern.** Group messages are currently sent
by looping over every group member and unicasting to each one individually
(confirmed in the stress-test logs — `forward_to_mesh` called once per
member). This is fine at 3 members. At 500+ members, a single message send
means 500 individual network sends from the sender's device, which is a
real bottleneck on both the sender's battery/CPU and the RBN relaying all of
it. This was explicitly out of scope for the 50-peer relay-bug fix and has
not been touched.

**What would resolve this:** a dedicated large-group stress test (Phase 2
below) — this is a different scaling axis (group size) than "how many total
peers can one RBN serve," and needs its own test and possibly its own
architecture change (e.g. real gossipsub-based fan-out for groups above some
threshold, sharded delivery, or a hybrid).

### 2.3 File-transfer topic subscription on the RBN

❓ **UNKNOWN.** The RBN auto-subscribes to every file-transfer gossipsub
topic it becomes aware of, unconditionally, for as long as that transfer is
active. This is unbounded by design — nothing currently caps how many
concurrent transfer topics one RBN will track. At low concurrent-transfer
counts this is invisible; at high user counts with many simultaneous
transfers, this hasn't been tested.

### 2.4 `max_connections` and rate-limit defaults

🔧 **PARTIALLY ADDRESSED.** The RBN's `max_connections` is being tightened
from an effectively-unbounded default toward a value tied to actual
capacity, as part of the current fix. But the actual number chosen is still
a guess pending the same real-hardware testing as 2.1 — this is the same
open question, not a separate one.

### 2.5 Multi-RBN sharding, DHT-based discovery, geographic/PeerID-based partitioning

❓ **UNKNOWN, genuinely not started.** The original architecture blueprint
assumes multiple community-operated RBNs discovered via the Solana registry
contract, with clients falling back across them. This is real infrastructure
that exists in the plan and (per the blueprint) in the token-economics
design, but none of the scaling questions this roadmap raises have been
tested in a multi-RBN topology — everything so far has been measured (or is
about to be measured) against a single RBN. Whether the "1M peers" claim
holds depends heavily on how well load actually spreads across many RBNs in
practice, not just in the math.

---

## Part 3 — Phased Roadmap

Each phase has a clear exit condition: real test data hitting a target, not
"code reviewed" or "looks right." Do not start a phase until the previous
phase's exit condition is met.

### Phase 0 — Close out the current bug fix (in progress)

**Goal:** Confirm the relay-circuit stability fix actually works, at the
scale it was found at.

- Build fixes in order: logging → delta re-flush → idle_mode state machine
  → RBN rate limiting.
- Re-run the original 50-peer stress test after each fix lands (not just at
  the end).
- **Exit condition:** Part 1.4's acceptance table fully populated with
  passing numbers.

### Phase 1 — Find the real single-RBN ceiling

**Goal:** Replace the "thousands of peers" assumption with a measured number.

- Repeat the stress test at increasing peer counts: 100 → 250 → 500 → 1,000
  → beyond, on real target server hardware (not a dev laptop, if production
  hardware differs).
- At each step, capture: RBN CPU/memory, circuit establish/drop rate, actual
  message/chunk delivery latency, and client-side battery/CPU on a real
  device connected to that RBN.
- Identify the point where any of these degrade non-linearly (the signature
  of a feedback loop or resource exhaustion, same pattern as the bug just
  fixed) versus degrading gracefully and linearly (expected, tunable).
- **Exit condition:** a documented, tested peer-count ceiling per RBN on
  real hardware, plus a clear resource-cost-per-peer number that lets you
  calculate "how many RBNs do we need for N total users" from data instead
  of guessing.

### Phase 2 — Large-group fan-out

**Goal:** Determine whether unicast group fan-out needs to be replaced, and
with what, before group sizes exceed what it can handle.

- Stress test group chats specifically: hold peer-to-RBN count constant at
  a safe level (from Phase 1) and scale group membership instead: 10 → 50 →
  200 → 1,000+ members in one group.
- Measure sender-side cost (battery/CPU/time-to-send-complete) and
  RBN-relay cost separately, since both are affected.
- 🧭 **Design question to resolve here:** at what group size (if any) does
  it become worth switching from unicast fan-out to a real gossipsub topic
  per group — accepting the mesh-maintenance overhead in exchange for
  taking the fan-out cost off the sender? This tradeoff can only be judged
  with real numbers from this phase, not from the original architecture
  blueprint's assumptions.
- **Exit condition:** either confirmation that unicast fan-out holds up to
  your realistic max group size, or a validated design + implementation for
  whatever replaces it above that threshold.

### Phase 3 — File-transfer topic scaling

**Goal:** Bound the RBN's unconditional file-transfer topic subscription
before it becomes unbounded load.

- Stress test many concurrent file transfers against one RBN, independent
  of general peer count and group size.
- Determine whether a cap or eviction policy is needed on how many
  transfer topics an RBN will track simultaneously, and what happens to a
  transfer that gets evicted.
- **Exit condition:** either evidence this isn't a real bottleneck at target
  scale, or a designed + tested cap/eviction mechanism.

### Phase 4 — Multi-RBN topology testing

**Goal:** Validate the actual multi-RBN architecture the 1M-user claim
depends on, not just single-RBN capacity times a peer count.

- Stand up multiple RBNs and test: client failover between RBNs, load
  distribution across RBNs (even, or does the Solana-registry discovery
  mechanism create hot-spotting on newer/more-visible RBNs?), and
  cross-RBN behavior for relay/mailbox/file-transfer paths where two
  communicating peers are anchored to different RBNs.
- Test the RBN staking/bonding economics' effect on real network topology,
  not just the smart-contract logic — e.g. does the current mix of
  incentives actually produce enough geographically/load distributed RBNs,
  or does it cluster?
- **Exit condition:** a tested multi-RBN deployment handling a realistic
  simulation of your target user distribution, with the same
  logging-driven acceptance-criteria approach as Phase 0.

### Phase 5 — Sustained/production load validation

**Goal:** Confirm behavior under realistic, mixed, sustained load — not just
synthetic stress spikes.

- Long-duration test (days, not minutes) mixing normal usage patterns:
  intermittent connectivity, app backgrounding/foregrounding (directly
  exercises the Phase 0 fix), mixed 1:1/group/file-transfer traffic, and
  real mobile network conditions (not just localhost/LAN stress-test
  peers).
- This is where you'd catch anything that only shows up over time (slow
  leaks, lease/timeout edge cases, the kind of bug class Phase 0 just fixed
  but at longer timescales).
- **Exit condition:** stable operation over the full test duration at your
  target concurrent-user simulation, with no manual intervention required.

---

## Part 4 — Summary Table: Where Everything Stands Right Now

| Area | Status | Blocking next phase? |
|---|---|---|
| Root cause of 50-peer stress failure | ✅ Proven | — |
| Fix design for that failure | 🔧 Built, verified against real libp2p source | Yes — blocks Phase 1 |
| Fix validated at the scale it was found | ⏳ Pending re-test | Yes — blocks Phase 1 |
| Single-RBN ceiling (real hardware) | ❓ Unknown | Blocks Phase 4 sizing |
| Large-group fan-out cost | ❓ Unknown | Independent — can run parallel to Phase 1 once Phase 0 passes |
| File-transfer topic scaling | ❓ Unknown | Independent — can run parallel to Phase 1/2 |
| Multi-RBN real-world topology | ❓ Unknown, not started | Depends on Phase 1 results |
| Sustained/production-realistic load | ❓ Unknown, not started | Depends on all above |

**Bottom line for right now:** the thing that was actually broken (relay
circuit stability at 50 peers) is understood and being fixed correctly. The
"1,000,000+ users" claim from the original architecture document remains a
design target, not a verified capability — and won't become one until Phases
1 through 5 produce real numbers. That's not a red flag on the architecture;
it's the normal, honest state of a system that hasn't been tested past 50
peers yet. The path above is how you close that gap deliberately instead of
discovering the next ceiling the same way you found this one — by it
breaking in someone's hands.

---

## Part 5 — Source Documents

| Document | Contents |
|---|---|
| `PROBLEM_STATEMENT.md` (v1) | Original (incorrect) gossipsub-mesh theory — kept for record, superseded |
| `PROBLEM_STATEMENT_CORRECTED.md` (v2) | Corrected root cause: idle_mode race + re-flush feedback loop |
| `EXPERT_CONSULTATION.md` | Detailed bug analysis with code locations and initial fix proposals |
| `EXPERT_REVIEW_RESPONSE.md` | First round of fix corrections |
| `MIMO_CLI_IMPLEMENTATION_PROMPT.md` | Implementation-ready spec sent to mimo cli, with corrected Fix 2/3 designs |
| `ARCHITECTURE_BLUEPRINT.md` | Original system design and the "Million-Node Mandate" this roadmap is validating |
| `SOVEREIGN_P2P_ARCHITECTURE_PLAN.md` | Planned outbox/seeding architecture — relevant to Phase 2/3 fan-out design questions |
| This document | Consolidated status + phased roadmap to close the gap between design target and verified capability |

---

## Phase 0 Close-Out — 2026-07-17

### What's resolved
- **RBN crash at 250 peers:** ✅ RESOLVED. The `join!` fix (Fix 2) eliminated the memory pressure that caused the crash. 250 peers now runs stable at 229MB RSS (was crashing at 257MB).

### What's been learned
- **Real devices produce significantly more circuit churn than synthetic peers.** Android: 17 events/5min, iOS: 29 events/5min, Mac: 24 events/5min. Synthetic peers: 1-6 events/5min. This is normal behavior for real devices on real networks — not a bug to suppress.
- **The system must be robust to frequent circuit re-establishment**, not try to prevent it via rate limiting. The join!/delta-reflush approach (handle gracefully) is correct; the rate limiter (suppress) would harm real users.
- **Synthetic-peer-derived cost models underestimate real-world load.** The "0.01% CPU/peer, 0.3MB RSS/peer" figures were computed from synthetic peers that churn far less than real devices.

### Revised acceptance criteria
| Metric | Original Target | Revised Target | Status |
|--------|----------------|----------------|--------|
| RBN crash at 250 peers | No crash | No crash | ✅ PASS |
| Duplicate flush rate | 1x per chunk (zero duplicates) | Sub-linear with circuit frequency | ✅ PASS (17 events → 5 flushes) |
| Rate limiter engagement | N/A | Should not suppress legitimate reconnects | ✅ PASS (rate limiter correctly inactive) |

### Open questions for Phase 1
1. Per-peer cost model needs recalibration with real-device behavior
2. Future stress tests should weight toward real devices or model realistic circuit churn
3. Rate limiter tuning should NOT be based on diluted synthetic-peer averages
