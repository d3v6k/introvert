# Introvert Economy: Pull-Based Reward Configuration Architecture

**Version:** 1.0
**Date:** 2026-06-22
**Status:** DESIGN — Pending Review
**Related:** ECONOMY_V3_REVISED_REWARD_MODEL.md, ECONOMY_V3_SIMULATION_SCENARIOS.md

---

## 1. Overview

### 1.1 Current Architecture (Push-Based)

Currently, the reward calculation runs entirely on the device:

```
Device                          RBN
  │                               │
  ├─ Track activities locally     │
  ├─ Calculate points locally     │
  ├─ Run clearing formula locally │
  ├─ Compute INTR reward locally  │
  │                               │
  └─ Submit claim ──────────────►│
```

**Problem:** The reward logic is hardcoded in the app binary. Changing conversion rates, adding bonus programs, or time-limiting promotions requires an app update.

### 1.2 Proposed Architecture (Pull-Based)

Devices pull a daily reward configuration from RBNs:

```
Device                          RBN
  │                               │
  ├─ Track activities locally     │
  ├─ Calculate points locally     │
  │                               │
  ├─ Pull daily config ─────────►│
  │◄── RewardConfig (signed) ─────┤
  │                               │
  ├─ Compute INTR reward using    │
  │   config + local points       │
  │                               │
  └─ Submit claim ──────────────►│
```

**Benefit:** RBNs control the reward logic. New programs, bonuses, and adjustments are deployed at the RBN level without app updates.

---

## 2. RewardConfig Structure

### 2.1 Core Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardConfig {
    /// Config version (monotonically increasing)
    pub version: u64,
    
    /// Date this config applies to (YYYY-MM-DD)
    pub cycle_date: String,
    
    /// Emission year (1-10)
    pub emission_year: u32,
    
    /// Base pools (from emission schedule)
    pub base_pools: BasePools,
    
    /// Activity weights (can be adjusted per-cycle)
    pub weights: ActivityWeights,
    
    /// Bonus programs (time-limited, capped)
    pub bonuses: Vec<BonusProgram>,
    
    /// Prestige tier multipliers
    pub tier_multipliers: TierMultipliers,
    
    /// Anti-gaming configuration
    pub anti_gaming: AntiGamingConfig,
    
    /// Config signature (RBN multisig)
    pub signature: Option<Vec<u8>>,
    
    /// Timestamp when config was published
    pub published_at: u64,
    
    /// Config expiry (device should re-pull after this)
    pub expires_at: u64,
}
```

### 2.2 Base Pools

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasePools {
    /// Daily user+edge pool (from emission schedule)
    pub user_edge_pool: f64,
    
    /// Daily RBN pool (from emission schedule)
    pub rbn_pool: f64,
    
    /// Bonus pool (separate from base emission)
    pub bonus_pool: f64,
}
```

### 2.3 Tier Multipliers

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierMultipliers {
    /// Citizen: base rate (1.0)
    pub citizen: f64,
    
    /// Sentinel: 1.1x avatar, base reward rate
    pub sentinel: f64,
    
    /// Silver: 1.3x avatar, slight bonus
    pub silver: f64,
    
    /// Gold: 1.6x avatar, meaningful bonus
    pub gold: f64,
    
    /// Platinum: 2.0x avatar, significant bonus
    pub platinum: f64,
    
    /// Catalyst: 1.25x avatar, growth bonus
    pub catalyst: f64,
    
    /// Pulsar: 1.25x avatar, activity bonus
    pub pulsar: f64,
}
```

**Example values:**
```rust
TierMultipliers {
    citizen: 1.0,
    sentinel: 1.05,   // 5% bonus on base rewards
    silver: 1.10,     // 10% bonus
    gold: 1.20,       // 20% bonus
    platinum: 1.50,   // 50% bonus
    catalyst: 1.15,   // 15% bonus
    pulsar: 1.15,     // 15% bonus
}
```

---

## 3. Bonus Programs

### 3.1 BonusProgram Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonusProgram {
    /// Unique program ID
    pub id: String,
    
    /// Human-readable name
    pub name: String,
    
    /// Program type
    pub bonus_type: BonusType,
    
    /// Bonus amount (INTR or points, depending on type)
    pub amount: f64,
    
    /// Maximum times this bonus can be claimed per user
    pub max_claims: u32,
    
    /// Total budget for this program (across all users)
    pub total_budget: f64,
    
    /// Budget consumed so far
    pub budget_consumed: f64,
    
    /// Start date (inclusive)
    pub starts_at: String,
    
    /// End date (inclusive, None = permanent)
    pub ends_at: Option<String>,
    
    /// Minimum prestige tier required (0 = any)
    pub min_tier: u8,
    
    /// Additional eligibility criteria
    pub criteria: BonusCriteria,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BonusType {
    /// Fixed INTR amount per claim
    FixedIntr,
    
    /// Percentage bonus on base reward
    PercentageBoost,
    
    /// Fixed points added to total
    FixedPoints,
    
    /// Multiplier on specific activity type
    ActivityMultiplier { activity_type: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonusCriteria {
    /// Minimum account age in days
    pub min_account_age_days: u32,
    
    /// Minimum unique peers interacted with
    pub min_unique_peers: u32,
    
    /// Minimum daily activity points
    pub min_daily_points: f64,
    
    /// Requires valid referral code
    pub requires_referral: bool,
    
    /// Maximum claims across all users (Sybil protection)
    pub global_max_claims: u32,
}
```

### 3.2 Example Bonus Programs

**Referral Bonus (Launch Phase):**
```rust
BonusProgram {
    id: "referral_launch_2026",
    name: "Launch Referral Bonus",
    bonus_type: BonusType::FixedIntr,
    amount: 500.0,  // 500 INTR per referral
    max_claims: 10,  // max 10 referrals per user
    total_budget: 500_000.0,  // 500K INTR total budget
    budget_consumed: 0.0,
    starts_at: "2026-07-01",
    ends_at: "2026-09-30",  // 90-day window
    min_tier: 0,  // any tier
    criteria: BonusCriteria {
        min_account_age_days: 0,
        min_unique_peers: 0,
        min_daily_points: 0.0,
        requires_referral: true,
        global_max_claims: 1000,  // first 1000 referrals network-wide
    },
}
```

**Streak Bonus (Ongoing):**
```rust
BonusProgram {
    id: "daily_streak",
    name: "Daily Streak Bonus",
    bonus_type: BonusType::PercentageBoost,
    amount: 0.01,  // +1% per consecutive day
    max_claims: 30,  // caps at 30% boost
    total_budget: f64::MAX,  // unlimited
    budget_consumed: 0.0,
    starts_at: "2026-07-01",
    ends_at: None,  // permanent
    min_tier: 0,
    criteria: BonusCriteria {
        min_account_age_days: 7,
        min_unique_peers: 3,
        min_daily_points: 100.0,
        requires_referral: false,
        global_max_claims: u32::MAX,
    },
}
```

**Prestige Tier Bonus (Ongoing):**
```rust
BonusProgram {
    id: "prestige_bonus",
    name: "Prestige Tier Bonus",
    bonus_type: BonusType::PercentageBoost,
    amount: 0.0,  // overridden by tier_multipliers
    max_claims: u32::MAX,
    total_budget: f64::MAX,
    budget_consumed: 0.0,
    starts_at: "2026-07-01",
    ends_at: None,
    min_tier: 1,  // Sentinel and above
    criteria: BonusCriteria::default(),
}
```

**Early Adopter Bonus (One-Time):**
```rust
BonusProgram {
    id: "early_adopter_2026",
    name: "Early Adopter Reward",
    bonus_type: BonusType::FixedIntr,
    amount: 1000.0,  // 1000 INTR one-time
    max_claims: 1,   // one per user
    total_budget: 1_000_000.0,  // 1M INTR total
    budget_consumed: 0.0,
    starts_at: "2026-07-01",
    ends_at: "2026-07-31",  // first month only
    min_tier: 0,
    criteria: BonusCriteria {
        min_account_age_days: 0,
        min_unique_peers: 1,
        min_daily_points: 50.0,
        requires_referral: false,
        global_max_claims: 1000,
    },
}
```

---

## 4. Device-Side Calculation Flow

### 4.1 Daily Reward Calculation

```rust
fn calculate_daily_reward(
    config: &RewardConfig,
    local_points: &LocalPoints,
    node_type: NodeType,
    prestige_tier: u8,
    streak_days: u32,
    referral_count: u32,
    claimed_bonuses: &HashSet<String>,
) -> DailyRewardResult {
    
    // 1. Select pool based on node type
    let pool = match node_type {
        NodeType::Rbn => config.base_pools.rbn_pool,
        NodeType::Edge | NodeType::User => config.base_pools.user_edge_pool,
    };
    
    // 2. Calculate base reward from points
    let total_pool_points = estimate_total_pool_points(config, node_type);
    let base_reward = (local_points.total / total_pool_points) * pool;
    
    // 3. Apply prestige tier multiplier
    let tier_mult = get_tier_multiplier(config, prestige_tier);
    let tiered_reward = base_reward * tier_mult;
    
    // 4. Apply eligible bonus programs
    let mut bonus_total = 0.0;
    for bonus in &config.bonuses {
        if !is_bonus_eligible(bonus, prestige_tier, streak_days, referral_count, claimed_bonuses) {
            continue;
        }
        if bonus.budget_consumed >= bonus.total_budget {
            continue;  // budget exhausted
        }
        
        let bonus_amount = match bonus.bonus_type {
            BonusType::FixedIntr => bonus.amount,
            BonusType::PercentageBoost => tiered_reward * bonus.amount,
            BonusType::FixedPoints => {
                // Convert bonus points to INTR
                let bonus_points = bonus.amount;
                (bonus_points / total_pool_points) * pool
            },
            BonusType::ActivityMultiplier { .. } => {
                // Applied at activity scoring level, not here
                0.0
            },
        };
        
        bonus_total += bonus_amount;
    }
    
    // 5. Total reward
    let total_reward = tiered_reward + bonus_total;
    
    DailyRewardResult {
        base_reward,
        tier_multiplier: tier_mult,
        tiered_reward,
        bonus_total,
        total_reward,
        applied_bonuses: vec![], // list of bonus IDs applied
    }
}
```

### 4.2 Config Pull Flow

```rust
async fn pull_reward_config(client: &IntrovertClient) -> Result<RewardConfig> {
    // 1. Try to pull from connected RBNs
    let rbns = client.get_connected_rbns();
    
    for rbn in rbns {
        match client.request_reward_config(rbn.peer_id).await {
            Ok(config) => {
                // 2. Verify signature
                if verify_config_signature(&config) {
                    // 3. Cache locally
                    client.cache_reward_config(&config);
                    return Ok(config);
                }
            },
            Err(_) => continue,
        }
    }
    
    // 4. Fallback: use cached config
    if let Some(cached) = client.load_cached_reward_config() {
        warn!("[RewardConfig] Using cached config from {}", cached.cycle_date);
        return Ok(cached);
    }
    
    // 5. Last resort: use hardcoded defaults
    warn!("[RewardConfig] No config available, using defaults");
    Ok(RewardConfig::default())
}
```

---

## 5. RBN-Side Config Management

### 5.1 Config Publishing

RBNs maintain a `RewardConfig` that is:
1. **Derived from the emission schedule** (base pools)
2. **Extended with bonus programs** (time-limited, capped)
3. **Signed by the RBN multisig** (prevents tampering)
4. **Published every 24 hours** (at UTC 00:00)

### 5.2 Config Distribution

```
RBN Cluster                    Individual RBNs
     │                               │
     ├─ Multisig signs config        │
     ├─ Publish to registry ────────►│
     │                               ├─ Serve config to devices
     │                               ├─ Track bonus claims
     │                               ├─ Report consumption
     │                               │
     │◄── Aggregate reports ─────────┤
     ├─ Adjust next day's config     │
```

### 5.3 Bonus Budget Management

Each bonus program has a `total_budget` and `budget_consumed`. RBNs track:
- How many claims have been made globally
- How much INTR has been distributed
- Whether the budget is exhausted

When `budget_consumed >= total_budget`, the bonus is marked as exhausted and devices stop claiming it.

---

## 6. Security Considerations

### 6.1 Config Signature

The `RewardConfig` is signed by the RBN multisig (Squads V4). Devices verify:
1. The signature is valid
2. The signer is a known RBN operator
3. The config date matches today's cycle

### 6.2 Anti-Gaming

| Attack | Defense |
|--------|---------|
| Fake config from non-RBN | Signature verification |
| Replay old config | Cycle date validation |
| Claim bonus multiple times | Device-side tracking + on-chain verification |
| Sybil referrals | Global max claims + min account age |
| Budget exhaustion race | RBN-side atomic counter |

### 6.3 Fallback Behavior

If a device cannot reach any RBN:
1. Use cached config (up to 48 hours old)
2. If no cache, use hardcoded defaults (base v3.0.1 weights)
3. Device still earns points — just with default conversion rates
4. When RBN connection is restored, device recalculates with live config

---

## 7. Implementation Phases

### Phase 1: Config Infrastructure (v3.1)
- Define `RewardConfig` struct in Rust
- Add config pull FFI to Dart
- RBN serves config via existing signaling protocol
- Device caches config locally

### Phase 2: Prestige Tier Multipliers (v3.1)
- Add `TierMultipliers` to config
- Apply multipliers in device-side calculation
- Update Dart UI to show tier-adjusted earnings

### Phase 3: Bonus Programs (v3.2)
- Add `BonusProgram` struct
- Implement referral bonus (first program)
- Track claims in SQLite
- RBN aggregates claim reports

### Phase 4: Dynamic Config Management (v3.3)
- RBN multisig governance for config changes
- Budget tracking across RBN cluster
- Analytics dashboard for program performance

---

## 8. Example: First 90 Days

### Day 1 Config (2026-07-01):
```json
{
  "version": 1,
  "cycle_date": "2026-07-01",
  "emission_year": 1,
  "base_pools": {
    "user_edge_pool": 16438.0,
    "rbn_pool": 8219.0,
    "bonus_pool": 5000.0
  },
  "weights": { ... },
  "bonuses": [
    {
      "id": "referral_launch_2026",
      "name": "Launch Referral Bonus",
      "bonus_type": "FixedIntr",
      "amount": 500.0,
      "max_claims": 10,
      "total_budget": 500000.0,
      "starts_at": "2026-07-01",
      "ends_at": "2026-09-30"
    },
    {
      "id": "early_adopter_2026",
      "name": "Early Adopter Reward",
      "bonus_type": "FixedIntr",
      "amount": 1000.0,
      "max_claims": 1,
      "total_budget": 1000000.0,
      "starts_at": "2026-07-01",
      "ends_at": "2026-07-31"
    }
  ],
  "tier_multipliers": {
    "citizen": 1.0,
    "sentinel": 1.05,
    "silver": 1.10,
    "gold": 1.20,
    "platinum": 1.50,
    "catalyst": 1.15,
    "pulsar": 1.15
  }
}
```

### Day 91 Config (2026-09-29):
```json
{
  "version": 91,
  "cycle_date": "2026-09-29",
  "bonuses": [
    {
      "id": "referral_launch_2026",
      "name": "Launch Referral Bonus",
      "budget_consumed": 287500.0,
      "ends_at": "2026-09-30"
    },
    {
      "id": "daily_streak",
      "name": "Daily Streak Bonus",
      "bonus_type": "PercentageBoost",
      "amount": 0.01,
      "max_claims": 30,
      "starts_at": "2026-07-01",
      "ends_at": null
    }
  ]
}
```

---

## 9. Open Questions

1. **Bonus pool sizing:** Should the bonus pool be a fixed amount per day, or a percentage of the base pool?
2. **Config frequency:** Should devices pull every 24 hours, or on-demand when opening the app?
3. **Offline grace period:** How long can a device use a cached config before rewards stop?
4. **RBN config governance:** Who decides what bonus programs to deploy? Multisig vote? Automated rules?
5. **Cross-device claim tracking:** How do we prevent the same user from claiming bonuses on multiple devices?
