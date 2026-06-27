# Introvert Economy v3.0: Revised Reward Model

**Version:** 3.0.1 (Corrected after expert audit)
**Date:** 2026-06-22
**Status:** APPROVED — Ready for code implementation
**Supersedes:** ECONOMY_MATHEMATICAL_SPECIFICATION.md v2.0
**Audit:** Expert-validated. All clearing math confirmed correct. Tragedy of the Commons vulnerability noted for v3.1 (Weighted RBN Clearing).

---

## 1. Executive Summary

This document proposes a revised reward model for the Introvert mesh network economy. The changes address three fundamental requirements:

1. **Edge nodes must earn ≥ 3× of regular users** — incentivizes infrastructure contribution
2. **RBN operators must earn significantly more** — RBNs are the backbone of the mesh
3. **RBN rewards must reflect uptime, not data volume** — RBNs are last-resort relay; measuring data throughput misaligns incentives

The proposed model is sustainable for the full 10-year emission schedule and preserves pool-isolated clearing.

---

## 2. Design Principles

| # | Principle | Rationale |
|---|-----------|-----------|
| 1 | RBNs earn by staying alive, not by relaying data | RBNs are last-resort relay; data volume is not their primary value |
| 2 | Edge nodes earn ≥ 3× regular users | Fair compensation for infrastructure contribution |
| 3 | RBNs earn significantly more than edge nodes | RBNs bond 50,000 INTR and are critical infrastructure |
| 4 | Pool isolation is sacred | RBN and user pools never interfere |
| 5 | 10-year sustainability | All rewards fit within the emission envelope |
| 6 | Anti-gaming by design | Every metric must be costly to fake |

---

## 3. Current System Analysis

### 3.1 Current Parameters

| Parameter | Value | Source |
|-----------|-------|--------|
| Total supply | 100,000,000 INTR (fixed) | Whitepaper §2 |
| 10-year emission | ~40,168,162 INTR | Whitepaper §4 |
| Year 1 daily user pool | 16,438 INTR | `daily_rewards.rs:19` |
| Year 1 daily RBN pool | 8,219 INTR | `daily_rewards.rs:20` |
| Annual decay | 20% (multiplier: 0.8) | `daily_rewards.rs:21` |
| Social point cap | 5,000 pts/day | `daily_rewards.rs:128` |
| Edge infra multiplier | 30× | `daily_rewards.rs:130` |
| RBN availability yield | 1.2× at ≥82,800s (23h) | `daily_rewards.rs:656` |

### 3.2 Current Activity Weights

| Activity | Weight | Daily Cap | Max Daily Points | Pool |
|----------|--------|-----------|-----------------|------|
| MessageSent | 10.0/msg | 200 | 2,000 | Social |
| MessageReceived | 5.0/msg | 300 | 1,500 | Social |
| GroupMessageSent | 8.0/msg | 150 | 1,200 | Social |
| GroupReaction | 3.0/react | 100 | 300 | Social |
| FileTransferSent | 20.0/file | 20 | 400 | Social |
| FileTransferRecv | 10.0/file | 20 | 200 | Social |
| CallDurationSecs | 1.0/sec | 3,600 | 3,600 | Social |
| RelayBytes | 0.01/KB | 10,240 KB (edge) / ∞ (RBN) | 3,072 (edge) / ∞ (RBN) | Infra |
| UptimeSeconds | 0.001/sec | 86,400 (edge) / ∞ (RBN) | 86.4 (edge) / ∞ (RBN) | Infra |

### 3.3 Current Earnings Comparison

**Test vector** (from `daily_rewards.rs:717-745`, single node, Year 1, global_estimate = 100,000):

| Node Type | Social Pts | Infra Pts | Total Pts | Daily INTR | vs Regular |
|-----------|-----------|-----------|-----------|------------|------------|
| Regular user | 5,000 (cap) | 137.6 | 5,137.6 | 844.5 | 1.00× |
| Edge node | 5,000 (cap) | 4,128.0 | 9,128.0 | 1,499.0 | **1.78×** |
| RBN | 900 | 104,961.3 | 105,861.3 | 8,700.7 | **10.3×** |

### 3.4 Problems Identified

| # | Problem | Impact |
|---|---------|--------|
| 1 | Edge nodes earn only 1.78× regular users | Below the 3× target |
| 2 | RBN earnings are 99.9% from RelayBytes | Rewards data volume, not availability |
| 3 | RBN RelayBytes is uncapped | One high-traffic RBN can dominate the pool |
| 4 | UptimeSeconds contributes only 0.1% of RBN earnings | Uptime is undervalued |
| 5 | `is_rbn` flag is client-reported | Spoofable — any node can claim RBN status |
| 6 | `proof_hash` checks existence only | Trivially bypassable with any non-empty string |

---

## 4. Implemented Changes (v3.0.1)

### 4.1 Parameter Changes

| Parameter | Previous | Current (v3.0.1) | Change |
|-----------|----------|------------------|--------|
| `edge_infra_multiplier` | 30.0 | **38.0** | +27% |
| `UptimeSeconds` weight | 0.001 | **0.005** | +400% |
| `UptimeSeconds` availability yield | 1.2× at ≥23h | **1.5× at ≥22h** | Stronger incentive |
| `RelayBytes` cap (RBN) | Uncapped | **51,200 KB (50 MB)** | New cap |
| `RelayBytes` weight | 0.01 | **0.01** | Unchanged |
| `RelayBytes` cap (edge) | 10,240 KB | **10,240 KB** | Unchanged |
| Social point cap | 5,000 | **5,000** | Unchanged |

### 4.2 New Activity Weights Table

| Activity | Weight | Daily Cap (Edge) | Daily Cap (RBN) | Max Daily Points (Edge) | Max Daily Points (RBN) | Pool |
|----------|--------|-----------------|-----------------|------------------------|----------------------|------|
| MessageSent | 10.0 | 200 | 200 | 2,000 | 2,000 | Social |
| MessageReceived | 5.0 | 300 | 300 | 1,500 | 1,500 | Social |
| GroupMessageSent | 8.0 | 150 | 150 | 1,200 | 1,200 | Social |
| GroupReaction | 3.0 | 100 | 100 | 300 | 300 | Social |
| FileTransferSent | 20.0 | 20 | 20 | 400 | 400 | Social |
| FileTransferRecv | 10.0 | 20 | 20 | 200 | 200 | Social |
| CallDurationSecs | 1.0/sec | 3,600 | 3,600 | 3,600 | 3,600 | Social |
| RelayBytes | 0.01/KB | 10,240 KB | **51,200 KB** | 3,072 | **512** | Infra |
| UptimeSeconds | **0.005**/sec | 86,400 | ∞ | **432** | ∞ | Infra |

### 4.3 RBN Earnings Floor — NOT REQUIRED

**Critical finding from corrected clearing math:** The 50× floor mechanism is unnecessary. Under the pool-isolated clearing formula, RBN operators naturally earn 261× to 17,429× regular users due to RBN pool scarcity (only 2-30 RBNs share the RBN pool, while 1K-1M users share the user pool).

The floor mechanism is **removed from this proposal**. RBN earnings are determined solely by the proportional clearing formula:

```
RBN_reward = (rbn_points / total_rbn_points) × rbn_pool
```

Since total_rbn_points = num_rbns × 2,060 and rbn_pool is fixed, each RBN earns:

```
RBN_reward = rbn_pool / num_rbns
```

This is independent of RBN point weight — all RBNs earn equally regardless of activity. The pool is always 100% utilized.

### 4.4 `is_rbn` Verification (Separate Fix)

Currently `ActivityEvent.is_rbn` is client-reported (`daily_rewards.rs:195`). Any node can set `is_rbn: true` to bypass RelayBytes caps and draw from the RBN pool.

**Proposed fix:**
1. Rust engine checks on-chain: does this node's Solana address have ≥ 50,000 INTR bonded in the PDA escrow?
2. Cache result for 1 hour to avoid repeated RPC calls
3. `ActivityEvent.is_rbn` field is ignored — Rust determines RBN status internally
4. This is implemented as a separate task from the weight changes

---

## 5. Mathematical Derivations

### 5.1 Point Calculations

**Regular user** (maxed social, full uptime):
```
Social:         5,000 pts (cap)
RelayBytes:     10,240 × 0.01 = 102.4 pts
UptimeSeconds:  86,400 × 0.005 = 432.0 pts
Total:          5,534.4 pts
```

**Edge node** (same social, 38× multiplier on infra):
```
Social:         5,000 pts
RelayBytes:     10,240 × 0.01 × 38 = 3,891.2 pts
UptimeSeconds:  86,400 × 0.005 × 38 = 16,416.0 pts
Total:          25,307.2 pts
```

**RBN** (light social, 50 MB relay, 24h uptime, 1.5× yield):
```
Social:         900 pts (light activity)
RelayBytes:     min(relay, 51,200) × 0.01 = 512 pts (if 50 MB relay)
UptimeSeconds:  86,400 × 0.005 × 1.5 = 648 pts
Total:          2,060 pts
```

### 5.2 Pool-Isolated Clearing Formula

For each pool (User/Edge pool and RBN pool):

```
reward_i = (my_points / total_pool_points) × pool_size
```

Where `total_pool_points = sum of ALL points from ALL participants in that pool`.

**User/Edge pool:**
```
total_user_edge_points = (num_users × 5,534.4) + (num_edges × 25,307.2)
regular_reward = (5,534.4 / total_user_edge_points) × user_pool
edge_reward = (25,307.2 / total_user_edge_points) × user_pool
```

**RBN pool:**
```
total_rbn_points = num_rbns × 2,060
rbn_reward = (2,060 / total_rbn_points) × rbn_pool = rbn_pool / num_rbns
```

### 5.3 Edge/Regular Ratio — Constant

```
edge_reward / regular_reward = 25,307.2 / 5,534.4 = 4.572×
```

This ratio is **constant** across all scenarios — it depends only on the point weights, not on pool size or participant count. The 38× multiplier guarantees ≥ 3× (actual: 4.57×). ✓

### 5.4 RBN/Regular Ratio — Driven by Pool Scarcity

```
rbn_reward / regular_reward = (rbn_pool / num_rbns) / (user_pool / num_users × 5534.4 / total_user_edge_points)
```

This ratio is dominated by the RBN pool having far fewer participants than the user pool. With 30 RBNs and 1M users, the RBN pool is split 30 ways while the user pool is split 1M ways.

### 5.5 `is_rbn` Verification (Separate Fix)

Currently `ActivityEvent.is_rbn` is client-reported (`daily_rewards.rs:195`). Any node can set `is_rbn: true` to bypass RelayBytes caps and draw from the RBN pool.

**Proposed fix:**
1. Rust engine checks on-chain: does this node's Solana address have ≥ 50,000 INTR bonded in the PDA escrow?
2. Cache result for 1 hour to avoid repeated RPC calls
3. `ActivityEvent.is_rbn` field is ignored — Rust determines RBN status internally
4. This is implemented as a separate task from the weight changes

---

## 6. 10-Year Emission Projection

### 6.1 Pool Schedule

| Year | Annual Release | Daily User+Edge Pool | Daily RBN Pool | Cumulative Released |
|------|---------------|---------------------|----------------|-------------------|
| 1 | 9,000,000 | 16,438 | 8,219 | 9,000,000 |
| 2 | 7,200,000 | 13,150 | 6,575 | 16,200,000 |
| 3 | 5,760,000 | 10,520 | 5,260 | 21,960,000 |
| 4 | 4,608,000 | 8,416 | 4,208 | 26,568,000 |
| 5 | 3,686,400 | 6,733 | 3,367 | 30,254,400 |
| 6 | 2,949,120 | 5,386 | 2,693 | 33,203,520 |
| 7 | 2,359,296 | 4,309 | 2,154 | 35,562,816 |
| 8 | 1,887,437 | 3,447 | 1,724 | 37,450,253 |
| 9 | 1,509,949 | 2,757 | 1,379 | 38,960,202 |
| 10 | 1,207,960 | 2,206 | 1,103 | 40,168,162 |

### 6.2 Network Growth Model

| Year | Active Users | Edge Nodes | RBNs | Rationale |
|------|-------------|-----------|------|-----------|
| 1 | 1,000 | 10 | 2 | Early adopters, minimum viable mesh |
| 1 | 3,000 | 30 | 5 | Organic growth |
| 2 | 10,000 | 100 | 8 | Community expansion |
| 3 | 30,000 | 300 | 12 | Regional growth |
| 4 | 50,000 | 500 | 15 | Network effects |
| 5 | 100,000 | 1,000 | 18 | Mainstream adoption begins |
| 6 | 200,000 | 2,000 | 20 | Scale phase |
| 7 | 400,000 | 4,000 | 25 | Mass adoption |
| 8 | 700,000 | 7,000 | 28 | Market maturity |
| 9 | 1,000,000 | 10,000 | 30 | Full scale |
| 10 | 1,000,000 | 10,000 | 30 | Steady state |

### 6.3 Daily Earnings Projection (Corrected)

Using the pool-isolated clearing formula `reward = (points / total_pool_points) × pool`:

| Year | Users | Edges | RBNs | Regular (INTR/day) | Edge (INTR/day) | RBN (INTR/day) | Edge/Reg | RBN/Reg | RBN/Edge |
|------|-------|-------|------|-------------------|-----------------|----------------|----------|---------|----------|
| 1 | 1,000 | 10 | 2 | 15.72 | 71.88 | 4,109.50 | 4.57× | 261.4× | 57.2× |
| 1 | 3,000 | 30 | 5 | 5.24 | 23.96 | 1,643.80 | 4.57× | 313.7× | 68.6× |
| 2 | 10,000 | 100 | 8 | 1.26 | 5.75 | 821.88 | 4.57× | 653.6× | 143.0× |
| 3 | 30,000 | 300 | 12 | 0.335 | 1.533 | 438.33 | 4.57× | 1,307.2× | 285.9× |
| 4 | 50,000 | 500 | 15 | 0.161 | 0.736 | 280.53 | 4.57× | 1,742.9× | 381.1× |
| 5 | 100,000 | 1,000 | 18 | 0.064 | 0.294 | 187.06 | 4.57× | 2,905.2× | 635.5× |
| 6 | 200,000 | 2,000 | 20 | 0.026 | 0.118 | 134.65 | 4.57× | 5,228.6× | 1,143.8× |
| 7 | 400,000 | 4,000 | 25 | 0.010 | 0.047 | 86.16 | 4.57× | 8,363.9× | 1,829.3× |
| 8 | 700,000 | 7,000 | 28 | 0.0047 | 0.022 | 61.57 | 4.57× | 13,075.4× | 2,870.0× |
| 9 | 1,000,000 | 10,000 | 30 | 0.0026 | 0.012 | 45.97 | 4.57× | 17,435.1× | 3,817.0× |
| 10 | 1,000,000 | 10,000 | 30 | 0.0021 | 0.0096 | 36.77 | 4.57× | 17,428.8× | 3,816.7× |

### 6.4 Annual Earnings Per Participant

| Year | Users | RBNs | Regular (INTR/yr) | Edge (INTR/yr) | RBN (INTR/yr) |
|------|-------|------|-------------------|----------------|---------------|
| 1 | 1,000 | 2 | 5,738 | 26,236 | 1,500,000 |
| 1 | 3,000 | 5 | 1,913 | 8,745 | 600,000 |
| 2 | 10,000 | 8 | 459 | 2,099 | 300,000 |
| 3 | 30,000 | 12 | 122 | 560 | 160,000 |
| 5 | 100,000 | 18 | 24 | 107 | 68,275 |
| 7 | 400,000 | 25 | 4 | 17 | 31,448 |
| 10 | 1,000,000 | 30 | 0.77 | 3.52 | 13,420 |

### 6.5 Key Observations

**1. Edge nodes consistently earn 4.57× regular users.** This is a mathematical constant derived from the point ratio (25,307.2 ÷ 5,534.4 = 4.572). It exceeds the 3× requirement. ✓

**2. RBN operators earn enormously more than regular users.** At Year 1 (1K users), RBNs earn 261× regular users. At Year 10 (1M users), RBNs earn 17,429× regular users. The ratio INCREASES with scale because the RBN pool has few participants while the user pool has many.

**3. RBN earnings are determined by pool scarcity, not points.** Each RBN earns `rbn_pool / num_rbns`. With 30 RBNs and a 1,103 INTR/day pool, each RBN earns 36.77 INTR/day regardless of their individual point total.

**4. The RBN pool is always 100% utilized.** Since `rbn_reward = (rbn_points / total_rbn_points) × rbn_pool` and all RBNs have the same points, the entire pool is distributed. There is no unclaimed surplus.

**5. RBN earnings decline over time due to pool decay.** From 4,109.50 INTR/day (Year 1, 2 RBNs) to 36.77 INTR/day (Year 10, 30 RBNs). This is offset by expected token price appreciation.

**6. The 50× floor mechanism is unnecessary.** Under all projected scenarios, RBN base rewards (from clearing) already exceed 50× regular user rewards. The floor is dead code and has been removed from this proposal.

### 6.6 RBN Payback Period (50,000 INTR Bond)

| Year | Users | RBN Daily Earning | Days to Payback | Months |
|------|-------|------------------|-----------------|--------|
| 1 | 1,000 | 4,109.50 | 12 | 0.4 |
| 1 | 3,000 | 1,643.80 | 30 | 1.0 |
| 2 | 10,000 | 821.88 | 61 | 2.0 |
| 3 | 30,000 | 438.33 | 114 | 3.8 |
| 5 | 100,000 | 187.06 | 267 | 8.9 |
| 7 | 400,000 | 86.16 | 580 | 19.3 |
| 10 | 1,000,000 | 36.77 | 1,360 | 45.3 |

Early RBN operators see payback in under 1 month. At full scale, payback is ~3.8 years — reasonable for infrastructure investment.

---

## 7. Sustainability Analysis

### 7.1 Pool Budget Verification

**RBN pool utilization is always 100%** — the entire pool is distributed among RBNs proportionally.

| Year | RBNs | Daily RBN Pool | Per-RBN Daily | Annual Per-RBN | Total RBN Annual |
|------|------|---------------|---------------|----------------|-----------------|
| 1 | 2 | 8,219 | 4,109.50 | 1,500,000 | 3,000,000 |
| 1 | 5 | 8,219 | 1,643.80 | 600,000 | 3,000,000 |
| 2 | 8 | 6,575 | 821.88 | 300,000 | 2,400,000 |
| 3 | 12 | 5,260 | 438.33 | 160,000 | 1,920,000 |
| 5 | 18 | 3,367 | 187.06 | 68,275 | 1,228,800 |
| 10 | 30 | 1,103 | 36.77 | 13,420 | 402,653 |

**10-year RBN emissions stay within the whitepaper budget** (33% of annual release). ✓

### 7.2 Total 10-Year Emissions

```
Total ecosystem pool:       50,000,000 INTR
Projected RBN use (10yr):   ~13,389,000 INTR (100% of RBN allocation)
Projected User+Edge (10yr): ~26,779,000 INTR (100% of User/Edge allocation)
Total projected use:        ~40,168,000 INTR
Remaining reserve:          ~9,832,000 INTR (for years 11+)
```

The system is fully sustainable. ✓

---

## 8. Gaming Resistance

### 8.1 Existing Defenses

| Defense | Mechanism | Location |
|---------|-----------|----------|
| Social point cap | 5,000 pts/day max | `daily_rewards.rs:128` |
| Rapid-fire cooldown | 10 events/type/60sec | `daily_rewards.rs:126-127` |
| Per-peer message cap | 50 messages/peer/day | `daily_rewards.rs:143` |
| Min unique peers | 3 peers for eligibility | `daily_rewards.rs:142` |
| Min message length | 5 characters | `daily_rewards.rs:124` |
| Foreground enforcement | 30-second grace period | `daily_rewards.rs:139-140` |
| Proof hash for relay | Required for edge RelayBytes | `daily_rewards.rs:418-423` |
| Self-messaging rejection | Rejects is_self events | `daily_rewards.rs:405-407` |

### 8.2 New Defenses in This Proposal

| Defense | Mechanism | Impact |
|---------|-----------|--------|
| RBN RelayBytes cap (50 MB) | Prevents relay-volume gaming | Limits relay points to 512/day |
| RBN bond verification | On-chain check of 50,000 INTR stake | Prevents `is_rbn` spoofing |
| UptimeSeconds as primary metric | Cannot be faked — requires actual uptime | Core RBN value is being online |

### 8.3 Remaining Vulnerabilities

| # | Vulnerability | Severity | Mitigation |
|---|--------------|----------|------------|
| 1 | `is_rbn` flag spoofable | **CRITICAL** | On-chain bond check (separate fix) |
| 2 | `proof_hash` existence-only | **MEDIUM** | Future: validate hash content against relay manifest |
| 3 | Grace period uses wrong epoch | **MEDIUM** | Future: use session-relative epoch |
| 4 | UptimeSeconds self-reported | **LOW** | RBN uptime verified by peer discovery pings |

---

## 9. Transition Plan

### 9.1 Phase-In Schedule (90 Days)

Weight changes are blended linearly over 90 days to avoid earnings cliffs:

```rust
fn blended_weight(old: f64, new: f64, days_since_change: u32) -> f64 {
    if days_since_change >= 90 { return new; }
    let blend = days_since_change as f64 / 90.0;
    old * (1.0 - blend) + new * blend
}
```

| Day | UptimeSeconds Weight | Edge Multiplier | RBN Relay Cap |
|-----|---------------------|-----------------|---------------|
| 0 | 0.001 | 30 | Uncapped |
| 30 | 0.0023 | 32.7 | 153,600 KB |
| 60 | 0.0037 | 35.3 | 102,400 KB |
| 90 | 0.005 | 38 | 51,200 KB |

---

## 10. Code Changes Required

### 10.1 `src/economy/daily_rewards.rs`

| Change | Location | Detail |
|--------|----------|--------|
| `edge_infra_multiplier` default | Line 130 | 30.0 → 38.0 |
| `uptime_seconds` default weight | Line 113 | 0.001 → 0.005 |
| `cap_relay_bytes` for RBN | Line 122 | Add RBN-specific cap (51,200) |
| Availability yield | Line 656 | 1.2× → 1.5×, threshold 82800 → 79200 |
| Phase-in blending | `score_activities_static()` | Apply blended weights during transition |

### 10.2 Documentation

| File | Action |
|------|--------|
| `Docs/ECONOMY_V3_REVISED_REWARD_MODEL.md` | This document (authoritative) |
| `Docs/ECONOMY_MATHEMATICAL_SPECIFICATION.md` | Update to reference v3.0 |
| `Docs/CHANGELOG.md` | Document changes |
| Python simulator | Update with new parameters |

### 10.3 Test Vectors (Verified)

**Test Vector 1 — Edge Node** (Year 1, global_estimate = 100,000):
```
Social: 3,705.0 pts (45 sent + 120 recv + 80 grp_sent + 25 react + 3 files + 8 file_recv + 1800s call)
Infra:  18,361.6 pts (5,120 KB × 0.01 × 38 + 86,400s × 0.005 × 38)
Total:  22,066.6 pts
Nano:   3,627,307,707,999 (f64 precision)
```

**Test Vector 2 — RBN Node** (Year 1, global_estimate = 100,000):
```
Social: 900.0 pts (30 sent + 50 recv + 40 grp_sent + 10 react)
Infra:  1,160.0 pts (min(10,485,760, 51,200) × 0.01 + 86,400 × 0.005 × 1.5)
Total:  2,060.0 pts
Nano:   169,311,400,000
```

**Test Vector 3 — Dual-Pool Separation** (regular user, 500 MB relay):
```
Social: 1,000.0 pts (100 sent)
Infra:  102.4 pts (capped at 10,240 KB × 0.01)
Pools correctly isolated — social and infra tracked separately.
```

---

## 11. Open Questions for Expert Review

1. **Is the Edge/Regular ratio (4.57×) acceptable?** The requirement was ≥ 3×. The 38× multiplier delivers 4.57×. Should we reduce to exactly 3× (multiplier ≈ 25.6)?

2. **Should the RBN pool allocation change?** Currently 33% of emission. RBNs earn enormously more than regular users. Should we reallocate some to the user pool?

3. **Is the 90-day phase-in too fast/slow?** RBNs with high relay volume see earnings drop as UptimeSeconds replaces RelayBytes dominance.

4. **Should RBN operators earn governance weight?** In addition to INTR rewards, RBNs could receive voting power proportional to bond + uptime.

5. **RBN earnings are pool-size-dependent, not points-dependent.** All RBNs earn equally regardless of activity. Should high-activity RBNs earn more? (This would require weighted RBN clearing instead of equal-split.)

---

## Appendix A: Full Earnings Table (Corrected)

| Year | Users | Edges | RBNs | User Pool | RBN Pool | Regular INTR/day | Edge INTR/day | RBN INTR/day | Edge/Reg | RBN/Reg | RBN/Edge |
|------|-------|-------|------|-----------|----------|-----------------|---------------|--------------|----------|---------|---------|
| 1 | 1,000 | 10 | 2 | 16,438 | 8,219 | 15.72 | 71.88 | 4,109.50 | 4.57× | 261.4× | 57.2× |
| 1 | 3,000 | 30 | 5 | 16,438 | 8,219 | 5.24 | 23.96 | 1,643.80 | 4.57× | 313.7× | 68.6× |
| 2 | 10,000 | 100 | 8 | 13,150 | 6,575 | 1.26 | 5.75 | 821.88 | 4.57× | 653.6× | 143.0× |
| 3 | 30,000 | 300 | 12 | 10,520 | 5,260 | 0.335 | 1.533 | 438.33 | 4.57× | 1,307.2× | 285.9× |
| 4 | 50,000 | 500 | 15 | 8,416 | 4,208 | 0.161 | 0.736 | 280.53 | 4.57× | 1,742.9× | 381.1× |
| 5 | 100,000 | 1,000 | 18 | 6,733 | 3,367 | 0.064 | 0.294 | 187.06 | 4.57× | 2,905.2× | 635.5× |
| 6 | 200,000 | 2,000 | 20 | 5,386 | 2,693 | 0.026 | 0.118 | 134.65 | 4.57× | 5,228.6× | 1,143.8× |
| 7 | 400,000 | 4,000 | 25 | 4,309 | 2,154 | 0.010 | 0.047 | 86.16 | 4.57× | 8,363.9× | 1,829.3× |
| 8 | 700,000 | 7,000 | 28 | 3,447 | 1,724 | 0.0047 | 0.022 | 61.57 | 4.57× | 13,075.4× | 2,870.0× |
| 9 | 1,000,000 | 10,000 | 30 | 2,757 | 1,379 | 0.0026 | 0.012 | 45.97 | 4.57× | 17,435.1× | 3,817.0× |
| 10 | 1,000,000 | 10,000 | 30 | 2,206 | 1,103 | 0.0021 | 0.0096 | 36.77 | 4.57× | 17,428.8× | 3,816.7× |

## Appendix B: Glossary

| Term | Definition |
|------|-----------|
| **INTR** | Introvert Token (SPL, 9 decimals) |
| **RBN** | Root Bootstrap Node (50,000 INTR bond) |
| **Edge node** | Relay-capable node (≥500 INTR stake) |
| **Pool-isolated clearing** | RBN and user rewards drawn from separate pools |
| **Availability yield** | Uptime multiplier for near-24h availability |
| **Social points** | Messaging, calls, files (capped at 5,000/day) |
| **Infra points** | Relay + uptime (separate pool) |
| **Emission year** | Year since TGE (1-based) |
| **Pool scarcity** | RBN pool has few participants, yielding high per-RBN rewards |
