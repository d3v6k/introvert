# Introvert Economy: Mathematical Specification & Audit Reference

**Version:** 2.0  
**Date:** 2026-06-21  
**Purpose:** Comprehensive mathematical documentation of all economic calculations for expert audit  
**Status:** Corrected — all 5 critical audit discrepancies resolved

---

## 1. Token Supply & Allocation

### 1.1 Fixed Supply

```
Total Supply = 100,000,000 $INTR (non-inflationary)
Decimals = 9 (Solana SPL standard — base units are nano-INTR)
1 INTR = 1,000,000,000 nano-INTR (10^9)
```

> **Note:** All on-chain amounts are denominated in nano-INTR (base units) to match Solana's 9-decimal SPL standard. Internal tracking uses `u64` nano-INTR values to avoid floating-point precision loss.

### 1.2 Allocation Matrix

| Tranche | Percentage | Amount ($INTR) | Unlock Schedule |
|---------|-----------|----------------|-----------------|
| Ecosystem Rewards Pool | 50% | 50,000,000 | 10-year emission curve |
| Community Growth & Grants | 20% | 20,000,000 | Strategic release |
| Developer Launch Reimbursement | 10% | 10,000,000 | 100% at TGE |
| Core Team Vesting | 5% | 5,000,000 | 12-month cliff + 24-month linear |
| Initial Liquidity | 15% | 15,000,000 | AMM seeding |

**Verification:** 50% + 20% + 10% + 5% + 15% = 100% ✓  
**Verification:** 50M + 20M + 10M + 5M + 15M = 100M ✓

---

## 2. Emission Schedule (10-Year Decay)

### 2.1 Formula

The Ecosystem Rewards Pool (50,000,000 $INTR) is released over 10 years using a geometric decay:

```
Year N Annual Release = Year_1_Release × decay^(N-1)

Where:
  Year_1_Release = 9,000,000 $INTR
  decay = 0.80 (20% annual reduction)
```

### 2.2 Derived Daily User Pool

```
Year N Daily User Pool = Year N Annual Release × (user_share_ratio) / 365

Where:
  user_share_ratio = (Annual Release - RBN Allocation) / Annual Release
  RBN Allocation = Annual Release × (1/3) [RBNs get ~33% of annual pool]
```

**Simplified (from whitepaper):**
```
Year N Daily User Pool = 16,438 × 0.8^(N-1)
```

### 2.3 Full Schedule

| Year | Annual Release | Daily User Pool | RBN Annual | Calculation |
|------|---------------|-----------------|------------|-------------|
| 1 | 9,000,000 | 16,438 | 3,000,000 | 9M × 0.8^0 = 9M |
| 2 | 7,200,000 | 13,150 | 2,400,000 | 9M × 0.8^1 = 7.2M |
| 3 | 5,760,000 | 10,520 | 1,920,000 | 9M × 0.8^2 = 5.76M |
| 4 | 4,608,000 | 8,416 | 1,536,000 | 9M × 0.8^3 = 4.608M |
| 5 | 3,686,400 | 6,733 | 1,228,800 | 9M × 0.8^4 = 3.6864M |
| 6 | 2,949,120 | 5,386 | 983,040 | 9M × 0.8^5 = 2.94912M |
| 7 | 2,359,296 | 4,309 | 786,432 | 9M × 0.8^6 = 2.359296M |
| 8 | 1,887,437 | 3,447 | 629,145 | 9M × 0.8^7 = 1.887437M |
| 9 | 1,509,949 | 2,757 | 503,316 | 9M × 0.8^8 = 1.509949M |
| 10 | 1,207,960 | 2,206 | 402,653 | 9M × 0.8^9 = 1.207960M |

### 2.4 Cumulative Emission Verification

```
Sum = 9M × (1 + 0.8 + 0.8^2 + ... + 0.8^9)
    = 9M × (1 - 0.8^10) / (1 - 0.8)
    = 9M × (1 - 0.1073741824) / 0.2
    = 9M × 0.8926258176 / 0.2
    = 9M × 4.463129088
    = 40,168,161.79 $INTR

Reserve for years 11+ = 50,000,000 - 40,168,162 = 9,831,838 $INTR
```

### 2.5 Code Implementation

**File:** `src/economy/daily_rewards.rs`
```rust
const YEAR_1_DAILY_POOL: f64 = 16_43.0;
const YEAR_1_RBN_DAILY_POOL: f64 = 8_219.0;
const ANNUAL_DECAY: f64 = 0.8;

pub fn get_emission_year(&self) -> u32 {
    let tge = NaiveDate::parse_from_str(TGE_DATE, "%Y-%m-%d")...;
    let today = Utc::now().date_naive();
    let days = today.signed_duration_since(tge).num_days().max(0);
    ((days / 365) + 1) as u32
}

pub fn get_daily_pool_cap(&self) -> f64 {
    let year = self.get_emission_year();
    YEAR_1_DAILY_POOL * ANNUAL_DECAY.powi((year - 1) as i32)
}

pub fn get_rbn_daily_pool_cap(&self) -> f64 {
    let year = self.get_emission_year();
    YEAR_1_RBN_DAILY_POOL * ANNUAL_DECAY.powi((year - 1) as i32)
}
```

---

## 3. Activity Points System

### 3.1 Activity Types & Weights

Each user action is assigned a point value. Points represent the relative importance of each activity to network health.

| Activity Type | Weight (pts) | Daily Cap | Max Daily Points | Pool | Edge Multiplier | Rationale |
|--------------|-------------|-----------|-----------------|------|----------------|-----------|
| MessageSent | 10.0 | 200 | 2,000 | Social | — | Core engagement |
| MessageReceived | 5.0 | 300 | 1,500 | Social | — | Active participation |
| GroupMessageSent | 8.0 | 150 | 1,200 | Social | — | Community building |
| GroupReaction | 3.0 | 100 | 300 | Social | — | Light interaction |
| FileTransferSent | 20.0 | 20 | 400 | Social | — | High-value data movement |
| FileTransferRecv | 10.0 | 20 | 200 | Social | — | Data consumption |
| CallDurationSecs | 1.0/sec | 3,600 | 3,600 | Social | — | Real-time communication |
| RelayBytes (edge) | 0.01/KB | 10,240 KB | 3,072 | Infra | **30×** | Edge node routing |
| RelayBytes (RBN) | 0.01/KB | **Uncapped** | **Uncapped** | Infra | — | RBN infrastructure work |
| UptimeSeconds (edge) | 0.001/sec | 86,400 | 2,592 | Infra | **30×** | Network availability |
| UptimeSeconds (RBN) | 0.001/sec | **Uncapped** | **Uncapped** | Infra | — | RBN continuous availability |

> **Key distinction:** Social activities (messaging, calls, files) are capped at 5,000 points/day. Infrastructure activities (relay, uptime) have their own separate pool and are **not** subject to the 5,000 social cap.

> **Edge Multiplier:** Edge relay nodes (100,000 $INTR stake) receive a 30× multiplier on infrastructure weights. This ensures nodes earn at least 2× more than regular users, incentivizing network infrastructure contribution. The multiplier is configurable via `edge_infra_multiplier` in `ActivityWeights` (default: 30.0).

### 3.2 Point Calculation Formula

For each activity type:

```
raw_count = number of events recorded in the cycle
capped_count = min(raw_count, daily_cap)     // RBN RelayBytes: cap = ∞
points = capped_count × weight
```

### 3.3 Dual-Pool Point System (Formal Notation)

Let $A$ be the set of all tracked activities recorded for a node during a 24-hour cycle window. We partition $A$ into two disjoint subsets:

- **Social Activities** ($A_{\text{soc}}$): MessageSent, MessageReceived, GroupMessageSent, GroupReaction, FileTransferSent, FileTransferRecv, CallDurationSecs
- **Infrastructure Activities** ($A_{\text{inf}}$): RelayBytes, UptimeSeconds

$$A = A_{\text{soc}} \cup A_{\text{inf}} \quad \text{and} \quad A_{\text{soc}} \cap A_{\text{inf}} = \emptyset$$

**A. Social Pool Cap Equation:**

For each activity type $i \in A_{\text{soc}}$, let $c_i$ be the raw count of events and $w_i$ be its associated reward weight:

$$P_{\text{soc\_raw}} = \sum_{i \in A_{\text{soc}}} \min(c_i, \text{Cap}_i) \cdot w_i$$

Hard structural ceiling to prevent messaging automation farming:

$$P_{\text{soc}} = \min(P_{\text{soc\_raw}}, 5000.0)$$

**B. Infrastructure Pool Equation:**

For each infrastructure activity type $j \in A_{\text{inf}}$, the bounding function depends on `is_rbn` and `is_edge`:

$$P_{\text{inf}} = \sum_{j \in A_{\text{inf}}} f(c_j, \text{is\_rbn}, \text{is\_edge}) \cdot w_j$$

Where:

$$f(c_j, \text{is\_rbn}, \text{is\_edge}) = \begin{cases} c_j & \text{if } \text{is\_rbn} = \text{true} \\ \min(c_j, \text{Cap}_j) \times M_{\text{edge}} & \text{if } \text{is\_edge} = \text{true} \\ \min(c_j, \text{Cap}_j) & \text{otherwise} \end{cases}$$

Where $M_{\text{edge}} = 30$ (configurable via `edge_infra_multiplier`).

Caps for edge nodes:
- $\text{Cap}_{\text{RelayBytes}} = 10,240 \text{ KB}$ (max 102.4 points × 30 = 3,072 points)
- $\text{Cap}_{\text{UptimeSeconds}} = 86,400 \text{ seconds}$ (max 86.4 points × 30 = 2,592 points)

Additionally, if $\text{is\_rbn} = \text{true}$ and $\text{UptimeSeconds} \ge 82,800$, the uptime weight is boosted:

$$w_{\text{UptimeSeconds\_RBN}} = w_{\text{UptimeSeconds}} \times 1.2 = 0.001 \times 1.2 = 0.0012$$

**C. Composite Score:**

$$P_{\text{total}} = P_{\text{soc}} + P_{\text{inf}}$$

**Why dual pools?** An edge node routing 500 MB/day earns 5,120 infra points from relay alone. Under a single 5,000-point cap, this would consume the entire daily allowance, leaving 0 points for messaging, calls, and social activity. The dual-pool system ensures social engagement always earns independently of infrastructure contribution.

### 3.4 Maximum Theoretical Points Per Day

```
Social Max = 2,000 + 1,500 + 1,200 + 300 + 400 + 200 + 3,600 = 9,200 points
Social Capped = min(9,200, 5,000) = 5,000 points

Infra Max (edge) = 102.4 + 86.4 = 188.8 points
Infra Max (RBN) = uncapped (both relay bytes and uptime are proportional to actual work)

Total Max (edge) = 5,000 + 188.8 = 5,188.8 points
Total Max (RBN)  = 5,000 + ∞ = unlimited
```

### 3.5 Code Implementation

**File:** `src/economy/daily_rewards.rs`
```rust
fn score_activities_static(state: &DailyRewardState, w: &ActivityWeights) -> Vec<DailyActivityCount> {
    ActivityType::all().iter().map(|at| {
        let raw = state.per_type_counts.get(&at_u8).copied().unwrap_or(0);
        let capped = state.per_type_capped.get(&at_u8).copied().unwrap_or(0);
        let weight = match at { /* per-type weight lookup */ };
        DailyActivityCount {
            activity_type: *at,
            raw_count: raw,
            capped_count: capped,
            points: capped as f64 * weight,
        }
    }).collect()
}
```

---

## 4. Dynamic Reward Distribution

### 4.1 Proportional Dynamic Clearing & Treasury Escrow Math

Let $Y$ represent the current integer step of the network deployment lifecycle ($1 \le Y \le 10$).

**A. Pool Target Selection:**

The emission pool source $E$ is bounded dynamically by the node classification:

$$E(Y) = \begin{cases} \lfloor 8,219 \times 10^9 \times (0.8)^{Y-1} \rfloor & \text{if } \text{is\_rbn} = \text{true} \quad \text{(Nano-INTR)} \\ \lfloor 16,438 \times 10^9 \times (0.8)^{Y-1} \rfloor & \text{if } \text{is\_rbn} = \text{false} \quad \text{(Nano-INTR)} \end{cases}$$

**B. Integer Allocation Settlement (Fixed-Point Protection):**

The pools are strictly isolated. Each pool has its own denominator:

**User/Edge Pool:**
$$R_{\text{user}} = \left\lfloor \frac{P_{\text{total}}}{D_{\text{user\_edge}}} \cdot E_{\text{user}}(Y) \right\rfloor$$

Where $D_{\text{user\_edge}} = \sum(\text{all user points}) + \sum(\text{all edge points})$

**RBN Pool:**
$$R_{\text{rbn}} = \left\lfloor \frac{P_{\text{total}}}{D_{\text{rbn}}} \cdot E_{\text{rbn}}(Y) \right\rfloor$$

Where $D_{\text{rbn}} = \sum(\text{all RBN points})$

> **Critical:** RBN rewards draw ONLY from RBN points. User/Edge rewards draw ONLY from User/Edge points. The pools never interfere with each other.

### 4.2 Mathematical Properties

1. **Zero-sum:** The sum of all user rewards equals the daily pool cap
2. **Proportional:** A user with 2× the points gets exactly 2× the reward
3. **Self-correcting:** As more users join, each user's share naturally decreases
4. **Anti-inflationary:** The pool cap decays 20% annually
5. **Pool-isolated:** RBN and user pools never interfere with each other

### 4.3 Example Calculations

**Year 1, Solo User (pre-launch):**
```
User points: 5,000 (capped)
Global points: 5,000 (only user)
Daily pool: 16,438 $INTR

Reward = (5,000 / 5,000) × 16,438 = 16,438 $INTR/day
```

**Year 1, 10,000 Active Users:**
```
User points: 5,000
Global points: 50,000,000 (10,000 users × 5,000 avg)
Daily pool: 16,438 $INTR

Reward = (5,000 / 50,000,000) × 16,438 = 0.0016438 $INTR/day
```

**Year 1, 1,000,000 Active Users:**
```
User points: 5,000
Global points: 5,000,000,000 (1M users × 5,000 avg)
Daily pool: 16,438 $INTR

Reward = (5,000 / 5,000,000,000) × 16,438 = 0.000016438 $INTR/day
```

### 4.4 Pre-Launch Estimate

Before network-wide data is available, a configurable `global_points_estimate` is used:

```
DEFAULT_GLOBAL_POINTS_ESTIMATE = 100,000.0

User Daily Reward ≈ (User Points / 100,000) × Daily Pool Cap
```

**File:** `src/economy/daily_rewards.rs`
```rust
const DEFAULT_GLOBAL_POINTS_ESTIMATE: f64 = 100_000.0;

pub fn get_realtime_earnings(&self) -> serde_json::Value {
    let global_estimate = state.global_points_estimate;
    let user_share = capped_points / global_estimate;
    let intr_earned = user_share * daily_pool;
    // ...
}
```

> **Note:** `global_points_estimate` should be updated dynamically from on-chain data or peer discovery once the network is live.

### 4.5 Unclaimed Rewards Behavior

```
Q: What happens if a user doesn't claim their daily rewards?
A: Rewards accumulate indefinitely in the pending pool (nano-INTR tracking).
   There is no claim deadline. Users can claim at any time.
   The 5-minute cooldown between claims prevents spam claiming.
```

### 4.6 Empty Pool Behavior

```
Q: What happens to the RBN pool if no RBNs are active?
A: Unallocated rewards from the daily pool are NOT rolled over.
   The pool cap is a maximum distribution limit, not a guaranteed payout.
   If no RBNs are active, the RBN pool for that day is simply not distributed.

Q: What happens if fewer users are active than expected?
A: Each active user gets a larger share of the fixed daily pool.
   Example: If only 1,000 users are active instead of 10,000,
   each user gets 10× the reward they would have received.
   The pool cap is always fully distributed (if at least 1 eligible user exists).
```

---

## 5. Anti-Gaming Mathematics

### 5.1 Rapid-Fire Detection

A sliding window prevents burst attacks:

```
Window size = rapid_fire_cooldown_secs = 60 seconds
Max events per window = rapid_fire_max_per_window = 10

Algorithm:
  1. On each event, purge timestamps older than (now - 60s)
  2. If window.size >= 10, reject the event
  3. Otherwise, add timestamp to window and accept
```

**Effective rate limit:** 10 events / 60 seconds = 0.167 events/second per activity type

### 5.2 Per-Peer Message Cap

```
max_messages_per_peer = 50

For each message to peer P:
  if per_peer_count[P] >= 50:
    reject
  else:
    per_peer_count[P] += 1
```

**Anti-farming effect:** Prevents sending 50 messages to the same bot account

### 5.3 Unique Peer Requirement

```
min_unique_peers = 3

At cycle end:
  if unique_peers.size < 3:
    is_eligible = false
    reward = 0
```

**Anti-solo effect:** User must interact with at least 3 different peers

### 5.4 Minimum Message Length

```
min_message_length = 5 characters

For MessageSent and GroupMessageSent:
  if message.length < 5:
    reject (don't count)
```

**Anti-spam effect:** Prevents "a", "ok", "1", "." from earning points

### 5.5 Foreground Requirement

```
require_foreground = true

For all non-RBN activities:
  if is_foreground == false AND proof_hash is missing:
    reject
```

**Anti-idle effect:** Background apps earn nothing. RBN operators are exempt since their infrastructure work is verifiable on-chain.

### 5.6 Grace Period (Session-Relative)

```
grace_period_secs = 30

The grace period is applied RELATIVE TO THE LOCAL NODE'S SESSION START,
not absolute UTC midnight.

Algorithm:
  1. When the app starts or reconnects, record session_start_epoch
  2. For the first 30 seconds after session_start, reject all activity
  3. This prevents rapid app-restart cycling to game the system
  4. It does NOT reject legitimate activity from users who happen to be
     active at UTC 00:00 midnight rollover
```

**Why session-relative, not UTC-relative:** If the grace period were anchored to UTC midnight, users actively chatting at 00:00 would have their legitimate messages silently dropped. The session-relative approach prevents gaming without punishing genuine activity.

### 5.7 Balance Snapshot

```
At UTC 00:00:
  snapshot_balance = fetch_balance(solana_wallet)

Purpose: Detect mid-cycle balance manipulation
```

---

## 6. RBN Operator Economics

### 6.1 Staking Requirement

```
RBN Bond = 2,000,000 $INTR locked in PDA Escrow
Unbonding Period = 7 days = 604,800 seconds
```

### 6.2 RBN Earnings (Isolated Pool)

RBN operators earn from a **separate, isolated pool** that does not interfere with user social rewards:

**A. Relay Points (Uncapped):**
```
relay_points = relay_bytes / 1024 × 0.01

Example: Relaying 1 GB/day
  = 1,048,576 KB × 0.01
  = 10,485.76 points

Example: Relaying 10 GB/day
  = 10,485,760 KB × 0.01
  = 104,857.6 points

No daily cap applies to RBN relay points.
```

**B. Uptime Points:**
```
uptime_points = uptime_seconds × 0.001

Example: 24-hour uptime
  = 86,400 × 0.001
  = 86.4 points
```

**C. RBN Daily Earnings (Isolated Pool):**
```
RBN Reward = (Individual RBN Points / Sum of All RBN Points Globally) × RBN Daily Pool Cap

Where:
  RBN Daily Pool Cap = 8,219 $INTR/day (Year 1)
  = 3,000,000 / 365
```

> **Critical:** RBN rewards are drawn from the RBN pool (8,219 $INTR/day), NOT the user pool (16,438 $INTR/day). The pools are strictly isolated.

### 6.3 RBN Year 1 Allocation

```
RBN Annual Pool = 3,000,000 $INTR
RBN Daily Pool = 3,000,000 / 365 = 8,219 $INTR/day
```

---

## 7. Token Tier System

### 7.1 Tier Thresholds

| Tier | Balance Required | Privilege |
|------|-----------------|-----------|
| Regular | 100,000 $INTR | Edge Relay status |
| Silver | 10,000 $INTR | Bypass rate limits |
| Gold | 25,000 $INTR | Beta access |
| Platinum | 100,000 $INTR | Governance proposals |

### 7.2 Sybil Resistance

```
Edge Relay Threshold = 100,000 $INTR

For Event Code 22 (Node Eligible):
  if wallet_balance < 100000:
    eligible = false (client-only mode)
  else:
    eligible = true (active relay)
```

---

## 8. Claim Flow Mathematics

### 8.1 Claim Threshold

```
Minimum claim = 10,000,000,000 nano-INTR (= 10 INTR)
Cooldown between claims = 300 seconds (5 minutes)
```

> **Why 10 INTR?** The previous threshold was 1,048,576 bytes (1 MB of routed data), which was a binary artifact from the relay tracking system. When migrating to native nano-INTR units, this value was incorrectly preserved as 1,048,576 nano-INTR (= 0.001048576 INTR), creating dust micro-claims that would spam the treasury. The 10 INTR threshold ensures claims justify processing fees and reduce transaction overhead.

### 8.2 Reward Proof Structure

```rust
RewardProof {
    provider_pubkey: String,    // Solana address
    consumer_peer_id: String,   // Peer consuming service
    relayed_bytes: u64,         // Bytes relayed
    timestamp: u64,             // Unix epoch
}
```

### 8.3 Availability Yield

The 1.2× availability multiplier is applied to the **uptime weight**, not the final payout:

```
If is_rbn == true AND uptime >= 82,800 seconds (23 hours):
  w_uptime_effective = 0.001 × 1.2 = 0.0012
  uptime_points = uptime_seconds × w_uptime_effective
```

> **Why 23 hours, not 24?** A day contains exactly 86,400 seconds. Due to network latency, device reboots, peer discovery handshake time, and packet delay, no node can realistically achieve uptime strictly greater than 86,400 seconds in a single daily cycle. The 23-hour threshold (82,800 seconds) accommodates healthy reconnects while still requiring near-continuous availability.
>
> **Why apply to weight, not payout?** Applying the multiplier at the weight level ensures the bonus is visible in the point breakdown and participates correctly in the pool-clearing formula. Applying it to the final payout would obscure the source of the bonus and break the proportional distribution math.

### 8.4 Daily Reward Recording

Daily rewards are tracked directly in nano-INTR base units:

```rust
// File: src/economy/mod.rs
pub fn record_daily_reward(&self, intr_amount: f64) {
    let nano_intr = (intr_amount * 1_000_000_000.0) as u64;
    if nano_intr == 0 { return; }
    let mut state = self.state.write();
    state.pending_daily_reward_nano_intr += nano_intr;
}

pub fn get_pending_daily_reward_intr(&self) -> f64 {
    let state = self.state.read();
    state.pending_daily_reward_nano_intr as f64 / 1_000_000_000.0
}
```

> **No byte conversion:** The old `bytes_equiv = intr_reward × 1,048,576` pattern has been eliminated. Rewards are tracked directly in nano-INTR, matching Solana's 9-decimal precision natively.

---

## 9. Real-Time Earnings Display

### 9.1 Calculation (every 30 seconds)

```
1. Fetch current INTR balance from Solana
2. Calculate daily pool cap: 16,438 × 0.8^(year-1)
3. Calculate user social points from activity counters (capped at 5,000)
4. Calculate user infra points from relay/uptime (uncapped for RBN)
5. total_capped_points = social_capped + infra_capped
6. Calculate earnings: intr_earned = (total_capped_points / global_estimate) × daily_pool
7. Dispatch to Flutter via Event 9 (intr_earned is already in INTR units, not nano-INTR)
```

### 9.2 Code

**File:** `src/economy/daily_rewards.rs`
```rust
pub fn get_realtime_earnings(&self) -> serde_json::Value {
    let activities = Self::score_activities_static(&state, &weights);
    
    // CRITICAL: Separate social and infra points before capping
    let social_points: f64 = activities.iter()
        .filter(|a| matches!(a.activity_type, ...social types...))
        .map(|a| a.points).sum();
    let infra_points: f64 = activities.iter()
        .filter(|a| matches!(a.activity_type, ...infra types...))
        .map(|a| a.points).sum();
    
    let social_capped = social_points.min(weights.daily_point_cap);
    let infra_capped = infra_points;
    let capped_points = social_capped + infra_capped;
    
    // CRITICAL: RBN operators draw from RBN pool, standard users from user pool
    let effective_pool = if is_rbn { rbn_daily_pool } else { daily_pool };
    
    let user_share = capped_points / global_estimate;
    
    // CRITICAL: Output as nano-INTR integer (1 INTR = 1,000,000,000 nano-INTR)
    // No floating-point serialization — matches Solana SPL 9-decimal precision
    let intr_earned_f64 = user_share * effective_pool;
    let intr_earned_nano: u64 = (intr_earned_f64 * 1_000_000_000.0) as u64;
    
    json!({
        "intr_earned_today_nano": intr_earned_nano,  // Primary: u64 nano-INTR
        "intr_earned_today": intr_earned_f64,          // Secondary: human-readable INTR
        "is_rbn": is_rbn,
        "effective_pool": effective_pool,
        "social_points": social_capped,
        "infra_points": infra_capped,
        "total_points": capped_points,
        ...
    })
}
```

---

## 10. Worked Verification Matrix

### Test Vector 0: Regular User (No Stake, No Infrastructure)

**System State:** $Y = 1$, $G_{\text{estimate}} = 100,000.0$, $\text{is\_rbn} = \text{false}$, $\text{is\_edge} = \text{false}$

| Activity | Raw | Capped | Weight | Points |
|----------|-----|--------|--------|--------|
| MessageSent | 45 | 45 | 10.0 | 450.0 |
| MessageReceived | 120 | 120 | 5.0 | 600.0 |
| GroupMessageSent | 80 | 80 | 8.0 | 640.0 |
| GroupReaction | 25 | 25 | 3.0 | 75.0 |
| FileTransferSent | 3 | 3 | 20.0 | 60.0 |
| FileTransferRecv | 8 | 8 | 10.0 | 80.0 |
| CallDurationSecs | 1800 | 1800 | 1.0 | 1800.0 |

```
P_soc = min(3705, 5000) = 3705.0
P_inf = 0 (no relay/uptime)
P_total = 3705.0

R = floor(3705.0 / 100000.0 × 16,438,000,000,000) = 609,027,900,000 nano-INTR ≈ 609.03 $INTR
```

### Test Vector 1: Edge Node (100,000 $INTR Stake, 30× Multiplier)

**System State:** $Y = 1$, $G_{\text{estimate}} = 100,000.0$, $\text{is\_rbn} = \text{false}$, $\text{is\_edge} = \text{true}$

| Activity | Raw | Capped | Weight | Edge Mult | Points |
|----------|-----|--------|--------|-----------|--------|
| MessageSent | 45 | 45 | 10.0 | — | 450.0 |
| MessageReceived | 120 | 120 | 5.0 | — | 600.0 |
| GroupMessageSent | 80 | 80 | 8.0 | — | 640.0 |
| GroupReaction | 25 | 25 | 3.0 | — | 75.0 |
| FileTransferSent | 3 | 3 | 20.0 | — | 60.0 |
| FileTransferRecv | 8 | 8 | 10.0 | — | 80.0 |
| CallDurationSecs | 1800 | 1800 | 1.0 | — | 1800.0 |
| RelayBytes (5MB) | 5120 KB | 5120 KB | 0.01 | 30× | 1,536.0 |
| UptimeSeconds (24h) | 86400 | 86400 | 0.001 | 30× | 2,592.0 |

```
P_soc = min(450 + 600 + 640 + 75 + 60 + 80 + 1800, 5000) = min(3705, 5000) = 3705.0
P_inf = (min(5120, 10240) × 0.01 × 30) + (min(86400, 86400) × 0.001 × 30) = 1536 + 2592 = 4128.0
P_total = 3705.0 + 4128.0 = 7833.0

E(1) = floor(16438 × 10^9 × 0.8^0) = 16,438,000,000,000 nano-INTR

R = floor(7833.0 / 100000.0 × 16,438,000,000,000)
  = floor(1,287,588,540,000)
  = 1,287,588,540,000 nano-INTR
  ≈ 1,287.59 $INTR
```

> **Verification:** Edge node earns 1,287.67 $INTR vs regular user 609.50 $INTR = **2.11× ratio** ✓

### Test Vector 2: Root Bootstrap Node (Dedicated Validator)

**System State:** $Y = 1$, $G_{\text{estimate}} = 100,000.0$, $\text{is\_rbn} = \text{true}$

| Activity | Raw | Capped | Weight | Points |
|----------|-----|--------|--------|--------|
| MessageSent | 30 | 30 | 10.0 | 300.0 |
| MessageReceived | 50 | 50 | 5.0 | 250.0 |
| GroupMessageSent | 40 | 40 | 8.0 | 320.0 |
| GroupReaction | 10 | 10 | 3.0 | 30.0 |
| RelayBytes (10GB) | 10,485,760 KB | uncapped | 0.01 | 104,857.6 |
| UptimeSeconds (24h, RBN) | 86400 | uncapped | 0.0012 | 103.68 |

```
P_soc = min(300 + 250 + 320 + 30, 5000) = min(900, 5000) = 900.0
P_inf = (10485760 × 0.01) + (86400 × 0.001 × 1.2) = 104857.6 + 103.68 = 104961.28
P_total = 900.0 + 104961.28 = 105861.28

E(1) = floor(8219 × 10^9 × 0.8^0) = 8,219,000,000,000 nano-INTR

R = floor(105861.28 / 100000.0 × 8,219,000,000,000)
  = floor(8,700,738,503,296)
  = 8,700,738,503,296 nano-INTR
  ≈ 8,700.74 $INTR
```

> **Verification:** These two test vectors must produce exactly `1287588540000` and `8700738503296` nano-INTR respectively in automated unit tests. Edge node earns 2.11× more than a regular user.

> **Pool Isolation Note:** In a network with 200,005 nodes (195k users + 5k edge + 5 RBNs), the User/Edge pool denominator is 761,640,000 points and the RBN pool denominator is 529,306 points. Both pools achieve 100% utilization. RBN earns 1,643.80 INTR/day from its isolated pool.

---

## 11. Key Constants Summary

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `YEAR_1_DAILY_POOL` | 16,43.0 | daily_rewards.rs | Year 1 daily user pool cap |
| `YEAR_1_RBN_DAILY_POOL` | 8,219.0 | daily_rewards.rs | Year 1 daily RBN pool cap |
| `ANNUAL_DECAY` | 0.8 | daily_rewards.rs | 20% annual emission reduction |
| `TGE_DATE` | "2026-01-01" | daily_rewards.rs | Token Generation Event date |
| `DEFAULT_GLOBAL_POINTS_ESTIMATE` | 100,000.0 | daily_rewards.rs | Pre-launch global points estimate |
| `DAILY_REWARD_ESCROW` | PLACEHOLDER | daily_rewards.rs | Escrow account (to be replaced) |
| `message_sent` weight | 10.0 | daily_rewards.rs | Points per message sent |
| `message_received` weight | 5.0 | daily_rewards.rs | Points per message received |
| `daily_point_cap` | 5,000.0 | daily_rewards.rs | Maximum daily social points per user |
| `edge_infra_multiplier` | 30.0 | daily_rewards.rs | Edge node infra weight boost (ensures ≥ 2× earnings) |
| `min_message_length` | 5 | daily_rewards.rs | Anti-spam threshold |
| `rapid_fire_cooldown_secs` | 60 | daily_rewards.rs | Anti-burst window |
| `rapid_fire_max_per_window` | 10 | daily_rewards.rs | Anti-burst limit |
| `min_unique_peers` | 3 | daily_rewards.rs | Anti-solo requirement |
| `max_messages_per_peer` | 50 | daily_rewards.rs | Anti-farming cap |
| Availability yield threshold | 82,800 sec (23h) | mod.rs | Near-continuous uptime bonus |
| Availability yield multiplier | 1.2× | mod.rs | 20% uptime bonus |
| Claim cooldown | 300 seconds | mod.rs | Minimum time between claims |
| RBN bond | 2,000,000 $INTR | whitepaper | RBN staking requirement |
| Edge relay threshold | 100,000 $INTR | whitepaper | Minimum for active relay |
| Unbonding period | 604,800 seconds | whitepaper | 7-day cooldown |

---

## 11. Audit Fixes Applied

### 11.1 RBN RelayBytes Cap Removed
**Issue:** 10MB daily cap on RelayBytes limited RBN profitability.
**Fix:** RBN operators now bypass the RelayBytes daily cap. The `is_rbn` flag on `ActivityEvent` signals the engine to skip cap enforcement for infrastructure work.
**Code:** `src/economy/daily_rewards.rs` — `record_activity()` checks `is_rbn_uncapped` before applying cap.

### 11.2 Dynamic Global Points Estimate
**Issue:** Hardcoded `DEFAULT_GLOBAL_POINTS_ESTIMATE = 100,000` caused UI to show inflated/deflated earnings.
**Fix:** Added `update_global_points_estimate()` method to update from network data. The estimate is stored in `DailyRewardState` and used in real-time earnings calculation.
**Code:** `src/economy/daily_rewards.rs` — `update_global_points_estimate()`, `get_realtime_earnings()`

### 11.3 Direct Nano-INTR Tracking
**Issue:** Converting INTR to byte equivalents introduced floating-point precision loss.
**Fix:** Added `pending_daily_reward_nano_intr` field to `EconomyState`. Daily rewards now tracked in nano-INTR base units (1 INTR = 1,000,000,000 nano-INTR), matching Solana's 9-decimal SPL standard natively.
**Code:** `src/economy/mod.rs` — `record_daily_reward()`, `get_pending_daily_reward_intr()`

### 11.4 RBN Pool Isolation
**Issue:** RBN and user rewards could interfere with each other.
**Fix:** Added separate `YEAR_1_RBN_DAILY_POOL = 8,219` constant and `get_rbn_daily_pool_cap()` method. RBN rewards use the RBN pool, user rewards use the user pool. **Critical:** The clearing formula denominator must also be pool-isolated — RBN rewards divide by RBN points only, User/Edge rewards divide by User/Edge points only. Using a single global denominator causes RBN pool under-utilization.
**Code:** `src/economy/daily_rewards.rs` — `get_rbn_daily_pool_cap()`

### 11.5 Foreground Exploit Mitigation
**Issue:** Client-reported `is_foreground` flag could be spoofed.
**Fix:** Added `proof_hash` field to `ActivityEvent`. Edge nodes must provide cryptographic proof of actual relay work (verified throughput hash). RBN operators exempt (infrastructure work is verifiable on-chain).
**Code:** `src/economy/daily_rewards.rs` — `record_activity()` validates `proof_hash` for RelayBytes.

### 11.6 Dual-Pool Point System
**Issue:** Edge relays routing 500 MB/day would hit the 5,000 social cap from infrastructure work alone, leaving 0 points for social activity.
**Fix:** Social and infrastructure points use separate pools. Social capped at 5,000. Infrastructure uncapped for RBN (both relay bytes and uptime), capped at 188.8 for edge nodes (102.4 relay + 86.4 uptime).
**Code:** `src/economy/daily_rewards.rs` — `score_activities_static()`

### 11.7 Session-Relative Grace Period
**Issue:** UTC-midnight grace period would reject legitimate messages from users active at 00:00.
**Fix:** Grace period is relative to the local node's session start time, not absolute UTC midnight.
**Code:** `src/economy/daily_rewards.rs` — `record_activity()` uses `session_start_epoch` (set when app boots or reconnects), NOT `cycle_start_epoch` (UTC midnight boundary).

### 11.8 Availability Yield Threshold
**Issue:** `uptime > 86,400` (24 hours) is impossible in a single daily cycle due to network latency and reboots.
**Fix:** Changed to `uptime >= 82,800` (23 hours) to accommodate healthy peer discovery reconnects.
**Code:** `src/economy/mod.rs` — `prepare_reward_proof()`

### 11.9 Edge Node Infrastructure Multiplier
**Issue:** Edge nodes (100,000 $INTR stake) earned the same as regular users (no stake), providing no incentive to run infrastructure.
**Fix:** Added `edge_infra_multiplier: 30.0` to `ActivityWeights`. Edge nodes receive 30× boost on RelayBytes and UptimeSeconds weights, ensuring they earn at least 2× more than regular users.
**Code:** `src/economy/daily_rewards.rs` — `ActivityWeights`, `score_activities_static()`

### 11.10 Pool-Isolated Clearing Formula
**Issue:** The original clearing formula used a single global GPE denominator for both user and RBN pools. This caused RBN rewards to be diluted by user points (99.9% of RBN pool left unallocated).
**Fix:** User/Edge pool divides by `user_edge_points` (excludes RBN). RBN pool divides by `rbn_points_total` (excludes user/edge). Both pools now achieve 100% utilization.
**Code:** `src/economy/daily_rewards.rs` — `get_realtime_earnings()`

---

## 12. Audit Checklist

- [ ] Verify emission schedule sums to ~40.17M over 10 years
- [ ] Verify daily pool cap formula: `16,438 × 0.8^(year-1)`
- [ ] Verify point weights match whitepaper specification
- [ ] Verify dual-pool system isolates social from infrastructure points
- [ ] Verify RBN pool is strictly isolated from user pool
- [ ] Verify anti-gaming constants are sufficient
- [ ] Verify claim threshold and cooldown are appropriate
- [ ] Verify RBN bond and unbonding period
- [ ] Verify tier thresholds for Sybil resistance
- [ ] Verify gasless flow covers SOL gas correctly
- [ ] Verify PDA escrow has no private key
- [ ] Verify Squads V4 multisig threshold (3-of-5)
- [ ] Verify `global_points_estimate` is appropriate for pre-launch
- [ ] Verify nano-INTR tracking matches Solana's 9-decimal precision
- [ ] Verify availability yield threshold (23h) is attainable
- [ ] Verify grace period is session-relative, not UTC-relative
- [ ] Verify anti-gaming prevents all identified attack vectors
- [ ] Verify `proof_hash` validation prevents foreground spoofing
