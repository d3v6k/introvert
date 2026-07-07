# Introvert Token Economy & Infrastructure TODO List

**Date Created:** 2026-07-06  
**Last Updated:** 2026-07-07  
**Scope:** Security Audit outstanding findings and alignment cleanup.

---

## 🔴 Critical Priorities (On-Chain Staking & Infrastructure Gating)

### [TODO-1] Enforce On-Chain RBN Staking (Audit Finding C4)
*   **Target File:** `solana_program/programs/introvert_registry/src/lib.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Added `stake`, `initiate_unbond`, `withdraw` instructions. `register_rbn` and `update_rbn(is_active=true)` now require `EscrowState.staked_amount >= 2,000,000 INTR`. Added off-chain `verify_operator_stake()` to `solana.rs`.

---

## 🟡 High Priorities (Keys & Configuration Gaps)

### [TODO-2] Populate Trusted RBN Rotation Keys (Audit Finding H3)
*   **Target File:** `src/economy/daily_rewards.rs`
*   **Context:** `TRUSTED_RBN_PUBLIC_KEYS` currently contains placeholder zero keys (`[0x00; 32]`) for rotation member entries 1 & 2.
*   **Goal:** Generate and populate two valid multi-sig keys to secure remote weight configurations.

### [TODO-3] Correct Treasury Pubkey in Relay Registration (Audit Finding H4)
*   **Target File:** `src/main.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Both RBN registration and dashboard paths now use correct treasury address (`9jauyKiimh6SBnpoRXcNXiLXZKSnN4h2gWKoqMcG4zHy`), mainnet RPC, and correct treasury API URL.

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
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** `balance_gating.rs` unified to 100K/250K/500K/1M thresholds matching `solana.rs`. Flutter UI still needs update.

### [TODO-6] Secure Constant-Time IPC HMAC Verification (Audit Finding M4)
*   **Target File:** `introvert-daemon/introvert-solana/src/main.rs`
*   **Context:** Uses standard string equality `expected == signature` which is vulnerable to timing attacks.
*   **Goal:** Update to a constant-time comparison helper utilizing `subtle::ConstantTimeEq` or `hmac::verify`.

### [TODO-7] HMAC Shared Secret Hardening (Audit Finding C2)
*   **Target File:** `src/economy/solana.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Replaced `treasury_pubkey.to_bytes()` with a 32-byte secret loaded from `/etc/introvert/ipc.secret` (64-char hex). Panics if file is missing or malformed.

### [TODO-8] LedgerCron Pool Separation (Audit Finding H3-new)
*   **Target File:** `src/economy/ledger_cron.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Added `is_rbn` field to `NodeTelemetryClaim`. `compute_daily_allocations()` now splits claims into user/RBN pools and allocates from the correct daily emission (16,438 user / 8,219 RBN).

### [TODO-9] Claim Flow On-Chain Verification (Audit Finding H4-new)
*   **Target File:** `src/lib.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** `introvert_claim_rewards_async` now calls `submit_and_verify_reward_claim()` instead of `submit_reward_claim()`, ensuring on-chain confirmation before committing local state.

### [TODO-10] Remove `unsafe` Static Mutables (Audit Finding M1)
*   **Target File:** `src/lib.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Replaced `static mut` with local mutable variables in the economy monitoring async task.
