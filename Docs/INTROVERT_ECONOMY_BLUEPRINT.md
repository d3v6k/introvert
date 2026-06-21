# Introvert Protocol: Sovereign Mesh Tokenization Blueprint v5.0

This document serves as the authoritative economic specification for the $INTR token integration into the Introvert communication ecosystem. It pairs cryptographic node security with economic incentives to create a self-sustaining, highly profitable decentralized infrastructure.

## 1. Core Token Specifications

| Property | Value |
|----------|-------|
| Token Name | Introvert Token |
| Symbol | $INTR |
| Mint Address | `NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn` |
| Total Supply | 100,000,000 (Fixed / Non-inflationary) |
| Decimals | 9 (Standard SPL precision) |
| Mint Authority | Disabled (Permanently Revoked) |
| Freeze Authority | None (Censorship Resistant) |
| Unified Governance Escrow (PDA) | Mathematically derived via program seeds |
| Primary Emergency Core Multisig | Squads V4: `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf` (3-of-5) |

## 2. Token Allocation Matrix

| Allocation | % | Amount | Description |
|-----------|---|--------|-------------|
| Ecosystem Rewards Pool & Treasury | 50% | 50,000,000 | Daily activity & node emissions over 10 years |
| Community Growth & Grants | 20% | 20,000,000 | Partnerships, RBN growth, audits, grants |
| Developer Launch Reimbursement | 10% | 10,000,000 | 100% unlocked at TGE |
| Core Team Vesting | 5% | 5,000,000 | 12-month cliff + 24-month linear vesting |
| Initial Liquidity | 15% | 15,000,000 | Public distribution & AMM pools |

## 3. The 10-Year Macro-Emission Schedule

The 50% Ecosystem Rewards Pool decays at 20% annually:

| Year | Annual Release | Daily User Cap | RBN Annual Pool |
|------|---------------|----------------|-----------------|
| 1 | 9,000,000 | 16,438/day | 3,000,000 |
| 2 | 7,200,000 | 13,150/day | 2,400,000 |
| 3 | 5,760,000 | 10,520/day | 1,920,000 |
| 4 | 4,608,000 | 8,416/day | 1,536,000 |
| 5 | 3,686,400 | 6,733/day | 1,228,800 |
| 6 | 2,949,120 | 5,386/day | 983,040 |
| 7 | 2,359,296 | 4,309/day | 786,432 |
| 8 | 1,887,437 | 3,447/day | 629,145 |
| 9 | 1,509,949 | 2,757/day | 503,316 |
| 10 | 1,207,960 | 2,206/day | 402,653 |
| **Total** | **40,168,162** | *~9.83M reserve for years 11+* | |

## 4. Daily Participation Rewards

**Dynamic Pool-Clearing Formula:**
```
User Daily Reward = (User Points / Global Points) * Daily Pool Cap
```

**Activity Points:**
| Activity | Points | Daily Cap |
|----------|--------|-----------|
| Message Sent (min 5 chars) | 10 | 200 |
| Message Received | 5 | 300 |
| Group Message Sent | 8 | 150 |
| Group Reaction | 3 | 100 |
| File Transfer Sent | 20 | 20 |
| File Transfer Received | 10 | 20 |
| Voice/Video Call | 1/sec | 3600s |
| Relay Bytes | 0.01/KB | 10MB |
| Node Uptime | 0.001/sec | 86400s |

**Anti-Gaming:** Balance snapshot at UTC 00:00, min 3 unique peers, rate limiting (10/type/60s), per-peer caps (50/day), foreground enforcement.

## 5. RBN Staking Infrastructure

| Parameter | Value |
|-----------|-------|
| Baseline Bond | 50,000 $INTR in PDA Escrow |
| Year 1 RBN Allocation | 3,000,000 $INTR |
| Relay Points | 0.01 per KB |
| Uptime Points | 0.001 per second |
| Unbonding Cooldown | 7 days (604,800 seconds) |

## 6. 4-Tier Token Ownership Matrix

| Tier | Balance | Privilege |
|------|---------|-----------|
| Regular | 500 $INTR | Edge Relay status; standard rewards |
| Silver | 10,000 $INTR | Bypass rate limits; priority routing |
| Gold | 25,000 $INTR | Beta access; premium transfers |
| Platinum | 100,000 $INTR | Governance proposals; infinite ZK storage |

## 7. Gasless Economic Flow

```
[User/Node] signs $INTR transaction locally
      ↓
[Treasury Fee Payer] co-signs, covers $SOL gas
      ↓
[Solana] processes SPL token movement gaslessly
```

## 8. Developer Launch Strategy

- **Dual-Sided Liquidity Pool:** Portion of 10M $INTR paired with USDC/SOL on Raydium/Orca
- **Pre-Launch Private Allocations:** OTC to strategic entities
- **Programmed TWAP Distribution:** Time-Weighted Average Price release post-launch

## 9. Governance & Security

- **Squads V4 Multisig:** 3-of-5 threshold controls registry program upgrades
- **PDA Escrow Vault:** Keyless, governed purely by immutable program logic
- **7-Day Unbonding:** Prevents exit-scams and infrastructure churn
- **Contract Upgrades:** No single developer override path
