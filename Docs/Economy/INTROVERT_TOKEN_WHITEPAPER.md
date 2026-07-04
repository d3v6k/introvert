# Introvert ($INTR) Token Economic Blueprint & White Paper
## A Sovereign, Highly Profitable P2P Mesh Network Economy

### 1. Executive Summary & Philosophy

Project Introvert is a privacy-first, fully decentralized communication platform that completely eliminates central servers. Operating via a crowdsourced, self-healing Peer-to-Peer (P2P) mesh network, it ensures zero-knowledge, autonomous, and censorship-resistant utility for global users.

The native $INTR token acts as the economic lifeblood of this decentralized infrastructure. It aligns incentives between daily users contributing communication activity and node operators providing network availability. Because the network cannot exist without active, dedicated infrastructure, the economic model is intentionally engineered to make running Root Bootstrap Nodes (RBNs) and Edge Nodes highly profitable, ensuring a robust backbone for over 1,000,000 active users.

By utilizing an autonomous Program-Derived Address (PDA) escrow vault on the Solana blockchain and governance via a Squads V4 multisig, the ecosystem is insulated from developer dependency, corporate censorship, or custodial risks.

### 2. Core Token Specifications

| Property | Value |
|----------|-------|
| Token Name | Introvert Token |
| Token Symbol | $INTR |
| Total Supply | 100,000,000 (Fixed / Non-inflationary) |
| Decimals | 9 (Standard SPL precision) |
| Solana Mint Address | `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` |
| Mint Authority Status | Disabled (Permanently Revoked) |
| Freeze Authority Status | None (Censorship Resistant) |
| Unified Governance Escrow (PDA) | Mathematically derived via program seeds |
| Primary Emergency Core Multisig | Squads V4 (3-of-5 Administration) |

### 3. Responsible & Fair Token Allocation Matrix

| Allocation | Percentage | Amount | Description |
|-----------|-----------|--------|-------------|
| Ecosystem Rewards Pool & Treasury | 50% | 50,000,000 $INTR | Daily activity & node emissions over 10 years |
| Community Growth & Grants | 20% | 20,000,000 $INTR | Strategic partnerships, RBN growth, security audits |
| Developer Launch Reimbursement | 10% | 10,000,000 $INTR | 100% unlocked at TGE for development cost recovery |
| Core Team Vesting | 5% | 5,000,000 $INTR | 12-month cliff + 24-month linear vesting |
| Initial Liquidity | 15% | 15,000,000 $INTR | Public distribution & AMM pools |

### 4. The 10-Year Macro-Emission Schedule

The 50% Ecosystem Rewards Pool (50,000,000 $INTR) is released over a decade via a decaying distribution curve (20% annual decay):

| Year | Annual Pool Release | Daily User+Edge Pool | Daily RBN Pool |
|------|--------------------|---------------------|----------------|
| 1 | 9,000,000 $INTR | 16,438 $INTR/day | 8,219 $INTR/day |
| 2 | 7,200,000 $INTR | 13,150 $INTR/day | 6,575 $INTR/day |
| 3 | 5,760,000 $INTR | 10,520 $INTR/day | 5,260 $INTR/day |
| 4 | 4,608,000 $INTR | 8,416 $INTR/day | 4,208 $INTR/day |
| 5 | 3,686,400 $INTR | 6,733 $INTR/day | 3,367 $INTR/day |
| 6 | 2,949,120 $INTR | 5,386 $INTR/day | 2,693 $INTR/day |
| 7 | 2,359,296 $INTR | 4,309 $INTR/day | 2,154 $INTR/day |
| 8 | 1,887,437 $INTR | 3,447 $INTR/day | 1,724 $INTR/day |
| 9 | 1,509,949 $INTR | 2,757 $INTR/day | 1,379 $INTR/day |
| 10 | 1,207,960 $INTR | 2,206 $INTR/day | 1,103 $INTR/day |
| **Total** | **40,168,162 $INTR** | *Remaining ~9.83M in Reserve Treasury for years 11+* | |

**Pool Isolation:** The User/Edge pool (67% of annual emission) and RBN pool (33%) are strictly isolated. Rewards from one pool never interfere with the other.

### 5. Pull-Based Reward Architecture

Introvert uses a **pull-based reward configuration** system where devices calculate points locally but pull the daily conversion parameters from RBNs.

**Flow:**
```
Device: tracks activity → calculates points locally
Device: pulls RewardConfig from RBN every 24 hours
Device: applies config (conversion rate + bonuses + tier multiplier)
Device: computes final INTR reward → submits claim
```

**RewardConfig contains:**
- Base pools (user/edge + RBN, from emission schedule)
- Activity weights (adjustable per-cycle by RBN governance)
- Bonus programs (time-limited, capped, budget-tracked)
- Prestige tier multipliers (reward scaling by tier)
- Anti-gaming configuration
- RBN multisig signature (cryptographic verification)

**Benefits:**
- New reward programs deployed at RBN level without app updates
- Time-limited promotions (referral bonuses, early adopter rewards)
- Per-user caps and global budgets prevent abuse
- Prestige tier bonuses incentivize long-term holding
- Signed configs prevent tampering

### 6. Daily Participation Reward System

**Activity Points & Weights (v3.0.1):**

| Activity | Weight | Daily Cap (Edge) | Daily Cap (RBN) | Pool |
|----------|--------|-----------------|-----------------|------|
| Message Sent (min 5 chars) | 10.0 | 200 | 200 | Social |
| Message Received | 5.0 | 300 | 300 | Social |
| Group Message Sent | 8.0 | 150 | 150 | Social |
| Group Reaction | 3.0 | 100 | 100 | Social |
| File Transfer Sent | 20.0 | 20 | 20 | Social |
| File Transfer Received | 10.0 | 20 | 20 | Social |
| Voice/Video Call | 1.0/sec | 3,600s | 3,600s | Social |
| Relay Bytes | 0.01/KB | 10,240 KB | 51,200 KB | Infra |
| Node Uptime | 0.005/sec | 86,400s | Uncapped | Infra |

**Pool-Isolated Clearing Formula:**
```
reward_i = (my_points / total_pool_points) × pool_size
```

Social activities are capped at 5,000 points/day. Infrastructure activities draw from a separate pool and are not subject to the social cap.

**Anti-Gaming Guardrails:**
- On-chain balance snapshot at UTC 00:00
- Minimum 3 unique peers per cycle
- Max 10 events per type per 60-second window
- Max 50 messages to same peer per day
- Foreground enforcement with 30-second grace period

### 7. Node Economics

#### 7.1 Edge Nodes (≥100,000 $INTR)

Edge nodes receive a **3× infrastructure multiplier** on RelayBytes and UptimeSeconds weights. This guarantees edge nodes earn **≥4.57× of regular users** — a mathematical constant derived from the point ratio (25,307.2 ÷ 5,534.4).

| Metric | Value |
|--------|-------|
| Minimum Stake | 100,000 $INTR |
| Infra Multiplier | 3× |
| Edge/Regular Ratio | 4.57× (constant) |
| RelayBytes Cap | 10,240 KB/day |
| UptimeSeconds Cap | 86,400s/day |

#### 7.2 Root Bootstrap Nodes (RBNs) (≥2,000,000 $INTR)

RBNs are the backbone of the mesh network. They are evaluated primarily by **uptime**, not data volume — because RBNs are last-resort relay and their value is keeping the mesh alive.

| Metric | Value |
|--------|-------|
| Baseline Bond | 2,000,000 $INTR locked in PDA Escrow |
| Unbonding Cooldown | 7 days (604,800 seconds) |
| UptimeSeconds Weight | 0.005/sec (primary metric) |
| Availability Yield | 1.5× at ≥22 hours uptime |
| RelayBytes Cap | 51,200 KB/day (50 MB) |
| Pool | Dedicated RBN pool (33% of emission) |

**RBN Earnings:** Due to pool scarcity (only 2-30 RBNs share the dedicated RBN pool), RBN operators earn **261× to 17,429× of regular users**. The RBN pool is always 100% utilized — split equally among all active RBNs.

### 8. Prestige Tier System

The prestige tier system rewards long-term $INTR holders with visual upgrades and reward multipliers.

#### 8.1 Balance Tiers

| Tier | Balance Required | Avatar Scale | Ring Color | Reward Multiplier |
|------|-----------------|-------------|------------|------------------|
| Citizen | < 100,000 $INTR | 1.0× | None | 1.0× |
| Sentinel | ≥ 100,000 $INTR | 1.15× | Cyan metallic | 1.05× |
| Silver | ≥ 22,000,000 $INTR | 1.30× | Gunmetal grey | 1.10× |
| Gold | ≥ 500,000 $INTR | 1.50× | Amber/gold | 1.20× |
| Platinum | ≥ 1,000,000 $INTR | 1.75× | Icy white-blue | 1.50× |

#### 8.2 Activity Tiers (Validated)

| Tier | Trigger | Avatar Scale | Ring Color | Reward Multiplier | Validation |
|------|---------|-------------|------------|------------------|------------|
| Catalyst | Growth champion | 1.18× | Purple metallic | 1.15× | 30-day referral window |
| Pulsar | Super active | 1.18× | Red metallic | 1.15× | 7-day activity window |

Activity tiers drop back to the balance-based tier if validation lapses.

#### 8.3 Peer Visibility

Prestige tiers are visible to ALL contacts in 1:1 and group chats. Tier data is exchanged via the P2P ProfileResponse protocol and stored in the contacts database. Every message a user sends displays their tier badge.

### 9. Bonus Programs (Pull-Based)

Bonus programs are deployed via the RewardConfig and funded from a separate bonus pool. They are time-limited, capped per-user, and budget-tracked globally.

| Program | Amount | Per-User Cap | Window | Budget |
|---------|--------|-------------|--------|--------|
| Referral Bonus | 100,000 INTR/referral | 10 referrals | 90 days | 500,000 INTR |
| Early Adopter | 1,000 INTR | 1 claim | 30 days | 1,000,000 INTR |
| Daily Streak | +1%/day | 30% max | Permanent | Unlimited |
| Prestige Bonus | 5-50% base | Tier-based | Permanent | Unlimited |

Bonus programs are signed by the RBN multisig. Devices verify the signature before applying bonuses.

### 9.1 DynamicPromoStack (Customizable Campaign Layer)

The DynamicPromoStack enables runtime promotion adjustments without code rebuilds. It operates on the 10% Strategic Reserve allocation (3,287.60 INTR/day in Year 1).

**Campaign Types:**
- CommunityThemeVote — Daily theme competitions with community voting
- EarlyAdopterBonus — Early user onboarding rewards
- DeveloperHackathonYield — Developer contribution bounties
- DynamicBonusCampaign — Custom promotional campaigns

**Math Model:**
```
[Strategic Reserve Daily Ceiling: 3,287.60 INTR]
                    │
                    ├──► [- Minus] Active Campaigns (e.g., Theme: 1,000 INTR)
                    │
                    └──► [= Equals] Referral Pool (2,287.60 INTR)
```

**Safety Features:**
- Auto-eviction — Expired campaigns automatically removed at epoch close
- Safety cap — Promo deductions cannot exceed Strategic Reserve ceiling
- Runtime adjustments — No code rebuilds required
- Referral pool compression — Core referral rewards always protected

### 10. Gasless Economic Flow

```
[End-User / Node] → Signs transaction locally in $INTR
        ↓
[Treasury Fee Payer] → Co-signs, covers $SOL gas fees
        ↓
[Solana Blockchain] → Processes SPL token movement gaslessly
```

### 11. Developer Launch Strategy

- **Dual-Sided Liquidity Pool:** Portion of 10M $INTR paired with USDC/SOL on Raydium/Orca
- **Pre-Launch Private Allocations:** OTC to strategic privacy-focused entities
- **Programmed TWAP Distribution:** Time-Weighted Average Price release into AMM post-launch

### 12. Governance & Security

- **Squads V4 Multisig:** 3-of-5 threshold controls registry program upgrades
- **PDA Escrow Vault:** Keyless, governed purely by immutable program logic
- **7-Day Unbonding:** Prevents exit-scams and infrastructure churn
- **RBN Bond Verification:** On-chain check of 2,000,000 INTR stake prevents RBN status spoofing
- **Signed RewardConfig:** RBN multisig signs daily reward configuration to prevent tampering
- **Contract Upgrades:** No single developer override path
