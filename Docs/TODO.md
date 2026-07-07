# Introvert Token Economy & Infrastructure TODO List

**Date Created:** 2026-07-06  
**Target Window:** 2026-07-07 Session  
**Scope:** Security Audit outstanding findings and alignment cleanup.

---

## 🔴 Critical Priorities (On-Chain Staking & Infrastructure Gating)

### [TODO-1] Enforce On-Chain RBN Staking (Audit Finding C4)
*   **Target File:** `solana_program/programs/introvert_registry/src/lib.rs`
*   **Context:** `stake_amount` is currently hardcoded to `0` and active status can be toggled without bond verification.
*   **Goal:** 
    1. Implement a `stake` instruction that transfers $INTR from the operator to a PDA escrow account.
    2. Gate `register_rbn` and `update_rbn` instructions on having an escrow balance of $\ge 2,000,000$ INTR.

---

## 🟡 High Priorities (Keys & Configuration Gaps)

### [TODO-2] Populate Trusted RBN Rotation Keys (Audit Finding H3)
*   **Target File:** `src/economy/daily_rewards.rs`
*   **Context:** `TRUSTED_RBN_PUBLIC_KEYS` currently contains placeholder zero keys (`[0x00; 32]`) for rotation member entries 1 & 2.
*   **Goal:** Generate and populate two valid multi-sig keys to secure remote weight configurations.

### [TODO-3] Correct Treasury Pubkey in Relay Registration (Audit Finding H4)
*   **Target File:** `src/main.rs`
*   **Context:** The relay and dashboard bootstrap code currently passes the INTR mint address (`EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf`) as the treasury pubkey parameter.
*   **Goal:** Replace it with the actual treasury wallet address (`9jauyKiimh6SBnpoRXcNXiLXZKSnN4h2gWKoqMcG4zHy`) to ensure correct ATA derivations.

---

## 🔵 Medium Priorities (Code Polish & Unified Scales)

### [TODO-4] Activate Dynamic Campaign Promos (Audit Finding M2)
*   **Target File:** `for_linux/src/economy/daily_rewards.rs`
*   **Context:** `DynamicPromoStack` is currently dead code (defined but never instantiated or evaluated).
*   **Goal:** Instantiate `DynamicPromoStack` in the RbnDailyRewardEngine, evaluate campaigns during `close_current_epoch()`, and deduct Year 1 promo distributions from the pool.

### [TODO-5] Unify Gating & Multiplier Tier Thresholds (Audit Finding M3)
*   **Target Files:** 
    - `src/economy/solana.rs` (reward multiplier thresholds)
    - `src/economy/balance_gating.rs` (balance tier gating)
    - `lib/src/ui/widgets/sovereign_avatar.dart` (UI display)
*   **Context:** Sentinel/Silver/Gold/Platinum tiers currently use conflicting balance scales across the three modules.
*   **Goal:** Unify to a single consistent mapping (e.g., Sentinel = 100K, Silver = 250K, Gold = 500K, Platinum = 1M INTR) across all modules.

### [TODO-6] Secure Constant-Time IPC HMAC Verification (Audit Finding M4)
*   **Target File:** `introvert-daemon/introvert-solana/src/main.rs`
*   **Context:** Uses standard string equality `expected == signature` which is vulnerable to timing attacks.
*   **Goal:** Update to a constant-time comparison helper utilizing `subtle::ConstantTimeEq` or `hmac::verify`.
