# Introvert Token Holdings, Node Tiers & Economy Specifications

This document outlines the token requirements, operational constraints, and reward multipliers for all participants in the Introvert network, including **Root Bootstrap Nodes (RBN)**, **Edge Relays**, and **Chat Clients**.

---

## 1. Node Tiers & Infrastructure Staking

The network operates on a dual-pool system: the **RBN Pool** (for critical bootstrap nodes) and the **User/Edge Pool** (for clients and background relays). Staking tiers determine routing eligibility and reward multipliers.

| Node Tier | Required $INTR Holding | Verification & Rules | Infrastructure Multiplier | Reward Ratio | Unbonding / Cooldown |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Root Bootstrap Node (RBN)** | **`2,000,000 $INTR`** | Staked in Anchor Program. PDA isolated registry. Requires public IP, Port 443 open. | **N/A** *(Isolated Pool: 8,219 $INTR/day split)* | Proportional RBN Pool Split | **`604,800 seconds`** *(7-day lock)* |
| **Edge Relay Node** | **`100,000 $INTR`** | Token Gating Engine (Event Code 22). Core disables background relay if balance drops below 100,000. | **`3.0×`** applied to `RelayBytes` & `UptimeSeconds` | **`3.0×`** vs Regular Users | **None** *(Immediate release)* |
| **Regular Client Node** | **`0 $INTR`** | Standard chat user. No routing features active (Client-only constraints). | **1.0×** *(Baseline)* | **`1.0×`** *(Baseline)* | **None** |

---

## 2. Infrastructure Reward Multipliers & Availability Yield

### A. RBN Availability Yield
*   **Uptime Criteria:** Near-continuous operation.
*   **Availability Bonus:** If an RBN node achieves **`≥ 82,800 seconds` (23 hours)** of continuous uptime in a 24-hour cycle, it receives a **`1.5×`** multiplier (50% boost) on its uptime rewards.
*   **Bandwidth Cap:** RBN operators are capped at **`51,200 KB` (50 MB)** of daily `RelayBytes` rewards to prevent traffic monopolization.

### B. Edge Relay Proof of Work
*   Edge nodes receive a **`3.0×`** multiplier boost on infrastructure points.
*   **Proof requirement:** Edge nodes must supply a valid `proof_hash` (cryptographic verification of actual data throughput) to claim rewards.

---

## 3. Client Prestige Tiers (Token Holdings & Activity)

Prestige tiers reward long-term $INTR holders and active users with visual ring badges on their chat avatars (visible to all contacts via P2P ProfileResponses) and scaling multipliers on the daily Social Rewards Pool.

### A. Balance-Based Prestige Tiers
These tiers are unlocked simply by holding $INTR in the local wallet:

| Prestige Tier | Balance Threshold | Avatar Ring Style | Avatar Scale | Social Reward Multiplier |
| :--- | :--- | :--- | :--- | :--- |
| **Citizen** | `< 100,000 $INTR` | None *(Standard)* | 1.0× | **`1.0×`** *(Baseline)* |
| **Sentinel** | `≥ 100,000 $INTR` | Cyan metallic ring | 1.15× | **`1.05×`** *(+5% boost)* |
| **Silver** | `≥ 22,000,000 $INTR` | Gunmetal grey ring | 1.30× | **`1.10×`** *(+10% boost)* |
| **Gold** | `≥ 500,000 $INTR` | Amber / Gold ring | 1.50× | **`1.20×`** *(+20% boost)* |
| **Platinum** | `≥ 1,000,000 $INTR` | Icy white-blue ring | 1.75× | **`1.50×`** *(+5% boost)* |

### B. Activity-Based Validated Tiers
These tiers are unlocked by network contributions and lapse back to the balance tier if validation requirements are not met:

| Prestige Tier | Unlock Condition | Avatar Ring Style | Avatar Scale | Social Reward Multiplier | Validation Window |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Catalyst** | Growth Champion *(Referrals)* | Purple metallic ring | 1.18× | **`1.15×`** | 30-day rolling window |
| **Pulsar** | Super Active Operator | Red metallic ring | 1.18× | **`1.15×`** | 7-day rolling window |
