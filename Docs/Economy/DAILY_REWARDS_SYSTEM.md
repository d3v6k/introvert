# Introvert Daily Rewards System

## Overview

The Daily Rewards System incentivizes genuine user participation in the Introvert mesh network. Unlike passive mining, rewards are earned through real communication activity — sending messages, making calls, transferring files, and relaying data for other users.

## How It Works

### Daily Cycle (UTC 00:00 — 23:59)

1. **Cycle Start (UTC 00:00):** The user's INTR balance is snapshotted on-chain. Activity tracking begins.
2. **Throughout the Day:** Every genuine action (messages, calls, file transfers, relaying) is recorded and scored against configurable weights.
3. **Cycle End (next UTC 00:00):** Total points are calculated, anti-gaming checks applied, and the reward amount is fed into the existing Solana claim pool.

### What Counts as Genuine Activity

| Activity | Points | Daily Cap | Why It Matters |
|----------|--------|-----------|----------------|
| Message sent | 10 | 200 | Core engagement — real communication |
| Message received | 5 | 300 | Being part of active conversations |
| Group message sent | 8 | 150 | Community participation |
| Group reaction | 3 | 100 | Light but meaningful interaction |
| File transfer sent | 20 | 20 | High-value data movement |
| File transfer received | 10 | 20 | Data consumption |
| Voice/video call | 1/sec | 3600s (1hr) | Real-time communication |
| Relay bytes (for others) | 0.01/KB | 10MB | Infrastructure contribution |
| Node uptime | 0.001/sec | 86400s (24hr) | Network availability (lowest weight) |

**Daily point cap:** 5,000 points
**Conversion rate:** 0.001 INTR per point (fine-tunable)
**Minimum message length:** 5 characters (anti-spam)

### What Does NOT Count

| Check | Rule |
|-------|------|
| Background state | App must be in foreground |
| Self-sent messages | Rejected entirely |
| Short messages | Under 5 characters rejected |
| Rapid-fire | Max 10 events per type per 60-second window |
| Single-peer farming | Max 50 messages to same peer per day |
| Solo usage | Minimum 3 unique peers required for eligibility |
| Idle running | Uptime weighted at 0.001 pts/sec — insufficient alone |
| Balance manipulation | INTR snapshot at cycle start detects mid-cycle tricks |

### Eligibility

To receive a daily reward, the user must:
- Have interacted with at least 3 unique peers
- Have earned at least 1 point (after all caps and filters)
- Have been active beyond the 30-second grace period

### Reward Flow

The system uses a **dynamic pool-clearing formula** (not static conversion):

```
User Daily Reward = (User Daily Points / Global Points Across Network) * Daily Pool Cap
```

This means rewards scale with network usage — more active users means each user gets a smaller share of the fixed daily pool, creating natural scarcity and incentivizing genuine participation.

**Simplified flow:**
```
User Activity -> DailyRewardEngine scores -> points calculated
    -> user_reward = (user_points / global_points) * daily_pool_cap
    -> fed into RewardTracker pending pool
    -> claimed via existing Solana treasury relay flow
```

The daily pool cap follows the 10-year emission schedule (see `Docs/INTROVERT_ECONOMY_BLUEPRINT.md` §3).

## Configuration

All parameters are stored in the `daily_reward_config` database table and can be updated at runtime via FFI. This allows fine-tuning without code changes.

## Technical Architecture

```
+-------------------------------------------------+
|                   Flutter UI                     |
|  economyStream (Event 9) + daily reward events   |
+-----------------------+-------------------------+
                        | FFI
+-----------------------v-------------------------+
|              src/economy/daily_rewards.rs        |
|  DailyRewardEngine                              |
|  +-- ActivityWeights (configurable)             |
|  +-- AntiGamingConfig                           |
|  +-- DailyCycle { snapshot, activities, score } |
|  +-- lifecycle: start -> track -> score -> submit|
+-------------------------------------------------+
|  src/economy/mod.rs (RewardTracker)             |
|  record_daily_reward(amount) -> pending pool    |
+-------------------------------------------------+
|  src/economy/solana.rs (SolanaIncentiveEngine)  |
|  submit_reward_claim -> treasury relay -> Solana|
+-------------------------------------------------+
|  src/storage.rs (StorageService)                |
|  daily_cycles, daily_activity_log, daily_config |
+-------------------------------------------------+
```

## Escrow Account

Placeholder: `PLACEHOLDER_ESCROW_ADDRESS` — will be replaced with actual PDA when on-chain program is deployed.
