# Introvert Token Economy & Infrastructure TODO List

**Date Created:** 2026-07-06  
**Last Updated:** 2026-07-09  
**Scope:** Infrastructure improvements and alignment cleanup.

---

## 🔴 Critical Priorities (On-Chain Staking & Infrastructure Gating)

### [TODO-1] Enforce On-Chain RBN Staking
*   **Target File:** `solana_program/programs/introvert_registry/src/lib.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Added staking, unbonding, and withdrawal instructions. Registration and activation now require minimum stake. Added off-chain verification.

---

## 🟡 High Priorities (Keys & Configuration)

### [TODO-2] Populate Trusted RBN Rotation Keys
*   **Target File:** `src/economy/daily_rewards.rs`
*   **Context:** Placeholder keys need replacement with valid multi-sig keys for secure remote configuration.
*   **Goal:** Generate and populate rotation member keys for secure weight configuration updates.

### [TODO-3] Correct Treasury Address in Relay Registration
*   **Target File:** `src/main.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Registration and dashboard paths now use correct treasury address and mainnet RPC.

---

## 🔵 Medium Priorities (Code Polish & Unified Scales)

### [TODO-4] Activate Dynamic Campaign Promos
*   **Target File:** `for_linux/src/economy/daily_rewards.rs`
*   **Context:** Campaign management system defined but not yet integrated.
*   **Goal:** Activate campaign management during epoch close for dynamic reward distribution.

### [TODO-5] Unify Gating & Multiplier Tier Thresholds
*   **Target Files:** 
    - `src/economy/solana.rs` (reward multiplier thresholds)
    - `src/economy/balance_gating.rs` (balance tier gating)
    - `lib/src/ui/widgets/sovereign_avatar.dart` (UI display)
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Unified tier thresholds across all systems. Flutter UI update pending.

### [TODO-6] Secure Constant-Time Authentication
*   **Target File:** `introvert-daemon/introvert-solana/src/main.rs`
*   **Status:** ✅ FIXED (2026-07-09)
*   **Changes:** Enhanced inter-process authentication with constant-time cryptographic verification and unified credential management.

### [TODO-7] Credential Management Hardening
*   **Target File:** `src/economy/solana.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Replaced embedded credentials with secure external credential loading.

### [TODO-8] Pool Separation
*   **Target File:** `src/economy/ledger_cron.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Added node type field for proper pool separation between user and infrastructure rewards.

### [TODO-9] Claim Flow Verification
*   **Target File:** `src/lib.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Claims now require on-chain verification before local state commitment.

### [TODO-10] Remove Unsafe Static Mutables
*   **Target File:** `src/lib.rs`
*   **Status:** ✅ FIXED (2026-07-07)
*   **Changes:** Replaced unsafe static mutables with proper mutable variables in economy monitoring.
