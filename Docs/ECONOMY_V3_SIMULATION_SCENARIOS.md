# Introvert Economy v3.0: Simulation Scenarios

**Version:** 1.0
**Date:** 2026-06-22
**Reference:** ECONOMY_V3_REVISED_REWARD_MODEL.md v3.0.1
**Purpose:** Expert review — 10 growth scenarios with full assumptions and daily reward calculations

---

## 1. Simulation Parameters (Fixed)

These parameters apply to ALL scenarios:

### 1.1 Token Economics
```
Total Supply:           100,000,000 INTR (fixed, non-inflationary)
Decimals:               9 (Solana SPL — base units are nano-INTR)
10-Year Emission:       ~40,168,162 INTR
Annual Decay:           20% (multiplier: 0.8)
```

### 1.2 Pool Allocation
```
User/Edge Pool:         67% of annual emission
RBN Pool:               33% of annual emission
Pool Isolation:         RBN and User/Edge pools never interfere
```

### 1.3 Activity Weights
```
MessageSent:            10.0 per message (cap: 200/day)
MessageReceived:        5.0 per message (cap: 300/day)
GroupMessageSent:       8.0 per message (cap: 150/day)
GroupReaction:          3.0 per reaction (cap: 100/day)
FileTransferSent:       20.0 per file (cap: 20/day)
FileTransferRecv:       10.0 per file (cap: 20/day)
CallDurationSecs:       1.0 per second (cap: 3,600/day)
RelayBytes:             0.01 per KB (edge cap: 10,240 KB; RBN cap: 51,200 KB)
UptimeSeconds:          0.005 per second (edge cap: 86,400; RBN: uncapped)
Social Point Cap:       5,000 pts/day
```

### 1.4 Node Parameters
```
Edge Infra Multiplier:  38× (applied to RelayBytes + UptimeSeconds)
RBN Availability Yield: 1.5× (applied to UptimeSeconds when uptime ≥ 22 hours)
RBN Bond:               50,000 INTR (locked in PDA escrow)
Edge Minimum Stake:     500 INTR
```

### 1.5 Point Calculations Per Node Type

**Regular User** (maxed social activity, full 24h uptime):
```
Social:         5,000.0 pts (cap)
RelayBytes:     10,240 × 0.01 = 102.4 pts
UptimeSeconds:  86,400 × 0.005 = 432.0 pts
─────────────────────────────────────────
Total:          5,534.4 pts
```

**Edge Node** (maxed social, full uptime, 38× infra multiplier):
```
Social:         5,000.0 pts (cap)
RelayBytes:     10,240 × 0.01 × 38 = 3,891.2 pts
UptimeSeconds:  86,400 × 0.005 × 38 = 16,416.0 pts
─────────────────────────────────────────
Total:          25,307.2 pts
```

**RBN Operator** (light social activity, 50 MB relay, 24h uptime, 1.5× yield):
```
Social:         900.0 pts (light activity)
RelayBytes:     51,200 × 0.01 = 512.0 pts (capped at 50 MB)
UptimeSeconds:  86,400 × 0.005 × 1.5 = 648.0 pts (1.5× yield)
─────────────────────────────────────────
Total:          2,060.0 pts
```

### 1.6 Clearing Formula

**Pool-isolated proportional clearing:**
```
reward_i = (my_points / total_pool_points) × pool_size
```

**User/Edge pool:**
```
total_user_edge_points = (num_users × 5,534.4) + (num_edges × 25,307.2)
regular_reward = (5,534.4 / total_user_edge_points) × user_pool
edge_reward = (25,307.2 / total_user_edge_points) × user_pool
```

**RBN pool (equal split — all RBNs have identical points):**
```
total_rbn_points = num_rbns × 2,060
rbn_reward = (2,060 / total_rbn_points) × rbn_pool = rbn_pool / num_rbns
```

### 1.7 Key Mathematical Properties

1. **Edge/Regular ratio is CONSTANT at 4.5724×** — derived from 25,307.2 ÷ 5,534.4. Independent of network size.
2. **RBN reward = rbn_pool ÷ num_rbns** — RBNs split the pool equally. No point-based differentiation.
3. **RBN pool is always 100% utilized** — no surplus, no burn, no rollover.
4. **RBN earnings are driven by pool scarcity** — few RBNs share a dedicated pool, yielding massive per-RBN returns.

---

## 2. Emission Schedule Reference

| Year | Annual Release | Daily User+Edge Pool | Daily RBN Pool |
|------|---------------|---------------------|----------------|
| 1 | 9,000,000 | 16,438 | 8,219 |
| 2 | 7,200,000 | 13,150 | 6,575 |
| 3 | 5,760,000 | 10,520 | 5,260 |
| 4 | 4,608,000 | 8,416 | 4,208 |
| 5 | 3,686,400 | 6,733 | 3,367 |
| 6 | 2,949,120 | 5,386 | 2,693 |
| 7 | 2,359,296 | 4,309 | 2,154 |
| 8 | 1,887,437 | 3,447 | 1,724 |
| 9 | 1,509,949 | 2,757 | 1,379 |
| 10 | 1,207,960 | 2,206 | 1,103 |

---

## 3. Simulation Scenarios

### Scenario 1: Genesis (200 Users)

**Context:** App launch. Early adopters testing the network. Minimum viable mesh.

| Assumption | Value |
|------------|-------|
| Emission Year | 1 |
| Active Users | 200 |
| Edge Nodes | 2 |
| RBN Operators | 1 |
| User+Edge Pool | 16,438 INTR/day |
| RBN Pool | 8,219 INTR/day |

**Point Totals:**
```
User+Edge Pool: (200 × 5,534.4) + (2 × 25,307.2) = 1,106,880 + 50,614 = 1,157,494 pts
RBN Pool:       1 × 2,060 = 2,060 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 1,157,494) × 16,438 =  78.58 INTR/day
Edge Node:     (25,307.2 / 1,157,494) × 16,438 = 359.27 INTR/day
RBN Operator:  (8,219 / 1) = 8,219.00 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 78.58 | 1.00× | — |
| Edge Node | 359.27 | **4.57×** | 1.00× |
| RBN Operator | 8,219.00 | **104.6×** | **22.9×** |

---

### Scenario 2: Seed Phase (500 Users)

**Context:** Word-of-mouth growth. Early community forming. First organic edge nodes.

| Assumption | Value |
|------------|-------|
| Emission Year | 1 |
| Active Users | 500 |
| Edge Nodes | 5 |
| RBN Operators | 2 |
| User+Edge Pool | 16,438 INTR/day |
| RBN Pool | 8,219 INTR/day |

**Point Totals:**
```
User+Edge Pool: (500 × 5,534.4) + (5 × 25,307.2) = 2,767,200 + 126,536 = 2,893,736 pts
RBN Pool:       2 × 2,060 = 4,120 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 2,893,736) × 16,438 =  31.44 INTR/day
Edge Node:     (25,307.2 / 2,893,736) × 16,438 = 143.74 INTR/day
RBN Operator:  (8,219 / 2) = 4,109.50 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 31.44 | 1.00× | — |
| Edge Node | 143.74 | **4.57×** | 1.00× |
| RBN Operator | 4,109.50 | **130.7×** | **28.6×** |

---

### Scenario 3: Early Traction (1,000 Users)

**Context:** First 1,000 active users. Network effects beginning. 2 RBNs providing backbone.

| Assumption | Value |
|------------|-------|
| Emission Year | 1 |
| Active Users | 1,000 |
| Edge Nodes | 10 |
| RBN Operators | 2 |
| User+Edge Pool | 16,438 INTR/day |
| RBN Pool | 8,219 INTR/day |

**Point Totals:**
```
User+Edge Pool: (1,000 × 5,534.4) + (10 × 25,307.2) = 5,534,400 + 253,072 = 5,787,472 pts
RBN Pool:       2 × 2,060 = 4,120 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 5,787,472) × 16,438 =  15.72 INTR/day
Edge Node:     (25,307.2 / 5,787,472) × 16,438 =  71.88 INTR/day
RBN Operator:  (8,219 / 2) = 4,109.50 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 15.72 | 1.00× | — |
| Edge Node | 71.88 | **4.57×** | 1.00× |
| RBN Operator | 4,109.50 | **261.4×** | **57.2×** |

---

### Scenario 4: Community Growth (3,000 Users)

**Context:** Organic growth phase. 5 RBNs providing regional coverage. Community expanding.

| Assumption | Value |
|------------|-------|
| Emission Year | 1 |
| Active Users | 3,000 |
| Edge Nodes | 30 |
| RBN Operators | 5 |
| User+Edge Pool | 16,438 INTR/day |
| RBN Pool | 8,219 INTR/day |

**Point Totals:**
```
User+Edge Pool: (3,000 × 5,534.4) + (30 × 25,307.2) = 16,603,200 + 759,216 = 17,362,416 pts
RBN Pool:       5 × 2,060 = 10,300 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 17,362,416) × 16,438 =   5.24 INTR/day
Edge Node:     (25,307.2 / 17,362,416) × 16,438 =  23.96 INTR/day
RBN Operator:  (8,219 / 5) = 1,643.80 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 5.24 | 1.00× | — |
| Edge Node | 23.96 | **4.57×** | 1.00× |
| RBN Operator | 1,643.80 | **313.7×** | **68.6×** |

---

### Scenario 5: Network Effects (10,000 Users)

**Context:** 10K milestone. Network effects accelerating. 8 RBNs across multiple regions.

| Assumption | Value |
|------------|-------|
| Emission Year | 2 |
| Active Users | 10,000 |
| Edge Nodes | 100 |
| RBN Operators | 8 |
| User+Edge Pool | 13,150 INTR/day |
| RBN Pool | 6,575 INTR/day |

**Point Totals:**
```
User+Edge Pool: (10,000 × 5,534.4) + (100 × 25,307.2) = 55,344,000 + 2,530,720 = 57,874,720 pts
RBN Pool:       8 × 2,060 = 16,480 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 57,874,720) × 13,150 =   1.26 INTR/day
Edge Node:     (25,307.2 / 57,874,720) × 13,150 =   5.75 INTR/day
RBN Operator:  (6,575 / 8) = 821.88 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 1.26 | 1.00× | — |
| Edge Node | 5.75 | **4.57×** | 1.00× |
| RBN Operator | 821.88 | **653.6×** | **143.0×** |

---

### Scenario 6: Regional Scale (30,000 Users)

**Context:** Regional adoption. 12 RBNs providing continental coverage. Mainstream early adopters.

| Assumption | Value |
|------------|-------|
| Emission Year | 3 |
| Active Users | 30,000 |
| Edge Nodes | 300 |
| RBN Operators | 12 |
| User+Edge Pool | 10,520 INTR/day |
| RBN Pool | 5,260 INTR/day |

**Point Totals:**
```
User+Edge Pool: (30,000 × 5,534.4) + (300 × 25,307.2) = 166,032,000 + 7,592,160 = 173,624,160 pts
RBN Pool:       12 × 2,060 = 24,720 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 173,624,160) × 10,520 =   0.335 INTR/day
Edge Node:     (25,307.2 / 173,624,160) × 10,520 =   1.533 INTR/day
RBN Operator:  (5,260 / 12) = 438.33 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 0.335 | 1.00× | — |
| Edge Node | 1.533 | **4.57×** | 1.00× |
| RBN Operator | 438.33 | **1,307.2×** | **285.9×** |

---

### Scenario 7: Mainstream Adoption (100,000 Users)

**Context:** 100K milestone. Mainstream adoption beginning. 18 RBNs providing global coverage.

| Assumption | Value |
|------------|-------|
| Emission Year | 5 |
| Active Users | 100,000 |
| Edge Nodes | 1,000 |
| RBN Operators | 18 |
| User+Edge Pool | 6,733 INTR/day |
| RBN Pool | 3,367 INTR/day |

**Point Totals:**
```
User+Edge Pool: (100,000 × 5,534.4) + (1,000 × 25,307.2) = 553,440,000 + 25,307,200 = 578,747,200 pts
RBN Pool:       18 × 2,060 = 37,080 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 578,747,200) × 6,733 =   0.0644 INTR/day
Edge Node:     (25,307.2 / 578,747,200) × 6,733 =   0.2944 INTR/day
RBN Operator:  (3,367 / 18) = 187.06 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 0.0644 | 1.00× | — |
| Edge Node | 0.2944 | **4.57×** | 1.00× |
| RBN Operator | 187.06 | **2,905.2×** | **635.5×** |

---

### Scenario 8: Scale Phase (300,000 Users)

**Context:** 300K users. Rapid growth. 22 RBNs handling increased infrastructure demand.

| Assumption | Value |
|------------|-------|
| Emission Year | 6 |
| Active Users | 300,000 |
| Edge Nodes | 3,000 |
| RBN Operators | 22 |
| User+Edge Pool | 5,386 INTR/day |
| RBN Pool | 2,693 INTR/day |

**Point Totals:**
```
User+Edge Pool: (300,000 × 5,534.4) + (3,000 × 25,307.2) = 1,660,320,000 + 75,921,600 = 1,736,241,600 pts
RBN Pool:       22 × 2,060 = 45,320 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 1,736,241,600) × 5,386 =   0.0172 INTR/day
Edge Node:     (25,307.2 / 1,736,241,600) × 5,386 =   0.0785 INTR/day
RBN Operator:  (2,693 / 22) = 122.41 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 0.0172 | 1.00× | — |
| Edge Node | 0.0785 | **4.57×** | 1.00× |
| RBN Operator | 122.41 | **7,123.8×** | **1,559.6×** |

---

### Scenario 9: Mass Adoption (1,000,000 Users)

**Context:** 1M users. Mass adoption achieved. 30 RBNs at maximum planned capacity.

| Assumption | Value |
|------------|-------|
| Emission Year | 9 |
| Active Users | 1,000,000 |
| Edge Nodes | 10,000 |
| RBN Operators | 30 |
| User+Edge Pool | 2,757 INTR/day |
| RBN Pool | 1,379 INTR/day |

**Point Totals:**
```
User+Edge Pool: (1,000,000 × 5,534.4) + (10,000 × 25,307.2) = 5,534,400,000 + 253,072,000 = 5,787,472,000 pts
RBN Pool:       30 × 2,060 = 61,800 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 5,787,472,000) × 2,757 =   0.00264 INTR/day
Edge Node:     (25,307.2 / 5,787,472,000) × 2,757 =   0.01206 INTR/day
RBN Operator:  (1,379 / 30) = 45.97 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 0.00264 | 1.00× | — |
| Edge Node | 0.01206 | **4.57×** | 1.00× |
| RBN Operator | 45.97 | **17,435.1×** | **3,817.0×** |

---

### Scenario 10: Full Scale (2,000,000 Users)

**Context:** 2M users. Maximum projected scale. 35 RBNs supporting global mesh.

| Assumption | Value |
|------------|-------|
| Emission Year | 10 |
| Active Users | 2,000,000 |
| Edge Nodes | 20,000 |
| RBN Operators | 35 |
| User+Edge Pool | 2,206 INTR/day |
| RBN Pool | 1,103 INTR/day |

**Point Totals:**
```
User+Edge Pool: (2,000,000 × 5,534.4) + (20,000 × 25,307.2) = 11,068,800,000 + 506,144,000 = 11,574,944,000 pts
RBN Pool:       35 × 2,060 = 72,100 pts
```

**Daily Rewards:**
```
Regular User:  (5,534.4 / 11,574,944,000) × 2,206 =   0.001055 INTR/day
Edge Node:     (25,307.2 / 11,574,944,000) × 2,206 =   0.004825 INTR/day
RBN Operator:  (1,103 / 35) = 31.51 INTR/day
```

| Node Type | Daily INTR | vs Regular | vs Edge |
|-----------|-----------|------------|---------|
| Regular User | 0.001055 | 1.00× | — |
| Edge Node | 0.004825 | **4.57×** | 1.00× |
| RBN Operator | 31.51 | **29,867.3×** | **6,530.6×** |

---

## 4. Consolidated Results

| # | Scenario | Users | Edges | RBNs | Year | Regular INTR/day | Edge INTR/day | RBN INTR/day | Edge/Reg | RBN/Reg | RBN/Edge |
|---|----------|-------|-------|------|------|-----------------|---------------|--------------|----------|---------|---------|
| 1 | Genesis | 200 | 2 | 1 | 1 | 78.58 | 359.27 | 8,219.00 | 4.57× | 104.6× | 22.9× |
| 2 | Seed | 500 | 5 | 2 | 1 | 31.44 | 143.74 | 4,109.50 | 4.57× | 130.7× | 28.6× |
| 3 | Early Traction | 1,000 | 10 | 2 | 1 | 15.72 | 71.88 | 4,109.50 | 4.57× | 261.4× | 57.2× |
| 4 | Community | 3,000 | 30 | 5 | 1 | 5.24 | 23.96 | 1,643.80 | 4.57× | 313.7× | 68.6× |
| 5 | Network Effects | 10,000 | 100 | 8 | 2 | 1.26 | 5.75 | 821.88 | 4.57× | 653.6× | 143.0× |
| 6 | Regional | 30,000 | 300 | 12 | 3 | 0.335 | 1.533 | 438.33 | 4.57× | 1,307.2× | 285.9× |
| 7 | Mainstream | 100,000 | 1,000 | 18 | 5 | 0.0644 | 0.2944 | 187.06 | 4.57× | 2,905.2× | 635.5× |
| 8 | Scale | 300,000 | 3,000 | 22 | 6 | 0.0172 | 0.0785 | 122.41 | 4.57× | 7,123.8× | 1,559.6× |
| 9 | Mass Adoption | 1,000,000 | 10,000 | 30 | 9 | 0.00264 | 0.01206 | 45.97 | 4.57× | 17,435.1× | 3,817.0× |
| 10 | Full Scale | 2,000,000 | 20,000 | 35 | 10 | 0.001055 | 0.004825 | 31.51 | 4.57× | 29,867.3× | 6,530.6× |

---

## 5. Annualized Earnings

| # | Scenario | Users | RBNs | Regular INTR/year | Edge INTR/year | RBN INTR/year |
|---|----------|-------|------|-------------------|----------------|---------------|
| 1 | Genesis | 200 | 1 | 28,682 | 131,134 | 2,999,935 |
| 2 | Seed | 500 | 2 | 11,475 | 52,466 | 1,499,968 |
| 3 | Early Traction | 1,000 | 2 | 5,738 | 26,236 | 1,499,968 |
| 4 | Community | 3,000 | 5 | 1,913 | 8,745 | 599,987 |
| 5 | Network Effects | 10,000 | 8 | 459 | 2,099 | 299,984 |
| 6 | Regional | 30,000 | 12 | 122 | 559 | 159,992 |
| 7 | Mainstream | 100,000 | 18 | 24 | 107 | 68,275 |
| 8 | Scale | 300,000 | 22 | 6 | 29 | 44,680 |
| 9 | Mass Adoption | 1,000,000 | 30 | 1 | 4 | 16,779 |
| 10 | Full Scale | 2,000,000 | 35 | 0 | 2 | 11,493 |

---

## 6. RBN Payback Analysis

| # | Scenario | Users | RBN Daily | Bond (50,000 INTR) | Days to Payback | Months |
|---|----------|-------|-----------|--------------------|--------------------|--------|
| 1 | Genesis | 200 | 8,219.00 | 50,000 | 6 | 0.2 |
| 2 | Seed | 500 | 4,109.50 | 50,000 | 12 | 0.4 |
| 3 | Early Traction | 1,000 | 4,109.50 | 50,000 | 12 | 0.4 |
| 4 | Community | 3,000 | 1,643.80 | 50,000 | 30 | 1.0 |
| 5 | Network Effects | 10,000 | 821.88 | 50,000 | 61 | 2.0 |
| 6 | Regional | 30,000 | 438.33 | 50,000 | 114 | 3.8 |
| 7 | Mainstream | 100,000 | 187.06 | 50,000 | 267 | 8.9 |
| 8 | Scale | 300,000 | 122.41 | 50,000 | 408 | 13.6 |
| 9 | Mass Adoption | 1,000,000 | 45.97 | 50,000 | 1,088 | 36.3 |
| 10 | Full Scale | 2,000,000 | 31.51 | 50,000 | 1,587 | 52.9 |

---

## 7. Key Insights

### 7.1 Edge Nodes: Consistent 4.57× Guarantee

The Edge/Regular ratio is a **mathematical constant**: 25,307.2 ÷ 5,534.4 = 4.5724×. This holds regardless of:
- Network size (200 to 2M users)
- Emission year (1 to 10)
- Number of participants
- Pool size

**The 3× requirement is exceeded by 52%.** No tuning needed.

### 7.2 RBN Operators: Pool Scarcity Drives Massive Returns

RBN earnings are determined by a single factor: **how many RBNs share the pool**.

```
RBN reward = rbn_pool / num_rbns
```

With only 1-35 RBNs sharing a dedicated pool, each RBN earns 100× to 30,000× what a regular user earns. This is a structural property of pool isolation — no artificial floor or multiplier needed.

### 7.3 RBN Earnings Decline Over Time (Two Factors)

1. **Pool decay** — 20% annual reduction in emission
2. **RBN count growth** — more RBNs sharing the same pool

Both are expected and healthy. Token price appreciation offsets the INTR reduction.

### 7.4 Regular User Earnings Are Tiny at Scale

At 1M users, a regular user earns 0.00264 INTR/day (~0.96 INTR/year). This is by design:
- The value of holding INTR comes from token appreciation, not daily emissions
- Daily rewards are participation incentives, not income
- The 5,000-point social cap prevents gaming

### 7.5 RBN Pool Is Always 100% Utilized

Since `rbn_reward = rbn_pool / num_rbns`, the entire pool is distributed every day. No surplus, no burn, no rollover. Clean and simple.

---

## 8. Glossary

| Term | Definition |
|------|-----------|
| **INTR** | Introvert Token (SPL, 9 decimals, 1 INTR = 10⁹ nano-INTR) |
| **RBN** | Root Bootstrap Node (50,000 INTR bond, dedicated server) |
| **Edge node** | Relay-capable node (≥500 INTR stake) |
| **Pool-isolated clearing** | RBN and user rewards drawn from separate pools |
| **Availability yield** | 1.5× uptime multiplier for RBNs with ≥22h daily uptime |
| **Social points** | Messaging, calls, files (capped at 5,000/day) |
| **Infra points** | Relay + uptime (multiplied by 38× for edge nodes) |
| **Emission year** | Year since TGE (1-based), drives pool size via 20% decay |
| **Pool scarcity** | RBN pool has few participants (1-35), yielding high per-RBN rewards |
