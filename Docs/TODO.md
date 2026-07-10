# Introvert Token Economy & Infrastructure TODO List

**Date Created:** 2026-07-06  
**Last Updated:** 2026-07-10  
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

---

## 🔴 Critical Priorities (File Transfer Over Relay)

### [TODO-12] Fix Relay Flush Race Condition
*   **Target File:** `src/network/mod.rs` (~L1898-1920, ~L1922-1977)
*   **Status:** ✅ FIXED (2026-07-10)
*   **Changes:** Removed premature outbox/pending flush on ReservationReqAccepted. Scheduled flush to execute 2500ms after OutboundCircuitEstablished, and wired a secondary watchdog tick flush to safely drain pending file chunk requests.

### [TODO-13] Fix Stall Watchdog for Zero-Chunk Transfers
*   **Target File:** `src/network/mod.rs` (~L434-547)
*   **Status:** ✅ FIXED (2026-07-10)
*   **Changes:** Enriched ClawTickContext with active transfers, seeder states, and peer speeds. Added a delayed flush in the watchdog to trigger pull recovery immediately upon stall. Integrated IntroClaw's stall predictor to preemptively recover.

### [TODO-14] Investigate RBN Relay Circuit Instability
*   **Target:** RBN server (47.89.252.80:443)
*   **Status:** ✅ FIXED (2026-07-10)
*   **Changes:** Implemented RelayCircuitHealthScorer in IntroClaw to score relay stability based on circuit age and drop frequency. IntroClaw triggers ForceMeshRefresh to recover connection state if the relay drops.

### [TODO-15] File Transfer Performance Tuning Verification
*   **Target File:** `src/network/mod.rs`
*   **Status:** ✅ FIXED (2026-07-10)
*   **Changes:** Replaced all hardcoded transfer sizes, in-flight limits, and pacing values with the network-adaptive TransferPolicy calculated dynamically by IntroClaw. Verified compilation and successfully build-packaged on macOS, Android, and iOS.

### [TODO-16] Implement FFI start_pull Dedup Guard
*   **Target Files:** `src/lib.rs`, `src/network/mod.rs`
*   **Status:** ✅ FIXED (2026-07-10)
*   **Changes:** Declared a global ACTIVE_PULLS registry in lib.rs to deduplicate concurrent FFI start_pull calls. Wired cleanup hooks in network/mod.rs for completed, failed, or cancelled transfers. Skip pulls for already downloaded file hashes.

---

## 🔴 Critical Priorities (Sovereign P2P Outbox Architecture)

### [TODO-11] Implement Sovereign P2P Outbox and Swarm Seeding Architecture
*   **Target Files:**
    - `src/storage.rs` (Outbox schema migration, SQLite helper queries)
    - `src/network/mod.rs` (Mailbox deprecation, Presence triggers, Outbox flushing, Hybrid manifests, P2P handshake)
    - `src/network/types.rs` (Add FileTransferProposal, Accept, Verify, and CompleteAck variants)
*   **Status:** 🟡 IN PROGRESS
*   **Goal:** Replace persistent RBN mailboxes with edge-side outboxes, presence-driven delivery, two-way P2P file handshakes, and group swarm seeding.
