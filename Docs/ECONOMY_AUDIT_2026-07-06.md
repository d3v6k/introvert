# Economy Pipeline Security Audit

**Date:** 2026-07-06
**Scope:** Full epoch cycle — client activity recording through INTR token payout
**Auditors:** MiMoCode (automated codebase analysis)

---

## Epoch Cycle Flow

```
Client DailyRewardEngine.record_activity()
  → shared_metrics[9] bridge (Arc<RwLock<[u64; 9]>>)
  → RewardTracker.package_telemetry()
  → TelemetryEnvelope (Ed25519-signed, 13 metrics)
  → TCP fire-and-forget to RBN at 47.89.252.80:9002

RBN RbnDailyRewardEngine.process_telemetry()
  → verify_signature() (Ed25519)
  → double-claim guard (in-memory HashMap)
  → score_activities() with ActivityWeights
  → dual-pool split (social vs infra)
  → pool-clearing formula: user_share = points / global_estimate
  → prestige multiplier (1.0x–1.5x)
  → ClaimRequest generated

Midnight UTC: close_current_epoch()
  → collect all edge scores
  → IQR outlier mitigation (Q1, Q3, IQR, upper bound)
  → clamp outliers
  → proportional distribution from 16,438 INTR daily pool
  → Vec<ClaimRequest> returned

introvert-p2p → IPC (port 9001, HMAC-SHA256 signed) → introvert-solana
  → verify HMAC
  → double-claim check (SQLite, persistent)
  → circuit breaker (SOL >= 0.01, INTR >= 1.0)
  → transfer_checked on Solana Mainnet
  → Treasury ATA → User ATA
```

---

## Security Findings

### CRITICAL

**C1: Signature message excludes mutable fields**
- File: `for_linux/src/economy/daily_rewards.rs:703-708`
- The Ed25519 signed message is `epoch_id || metrics[0..13] || timestamp`. It does NOT include `peer_id`, `solana_ata`, `is_rbn`, `is_edge_node`, or `prestige_tier`.
- Impact: An attacker with one valid signature can change the destination wallet (`solana_ata`) or node type flags while keeping the signature valid. Rewards can be redirected.
- Fix: Include ALL mutable fields in the signed message. The canonical message should be: `epoch_id || peer_id || solana_wallet || solana_ata || metrics[0..13] || is_rbn || is_edge_node || prestige_tier || timestamp`.

**C2: Lease check validates treasury, not local node**
- File: `for_linux/src/network/mod.rs:938`
- `solana_client.get_treasury_pubkey()` is used instead of the local operator's pubkey. Treasury always has >= 100K INTR, so `is_lease_valid()` always returns true.
- Impact: Unstaked nodes operate indefinitely. The 100K INTR minimum stake requirement is completely bypassed.
- Fix: Derive the local operator's Solana pubkey from the node's identity keypair and check that balance instead.

**C3: All epoch state is in-memory only**
- File: `for_linux/src/economy/daily_rewards.rs:327`
- `processed_cycles: RwLock<HashMap<String, HashMap<String, ClientCycle>>>` is never persisted to disk.
- Impact: After RBN restart, double-claim guard is void. Attacker can re-submit telemetry for any past epoch. `close_current_epoch()` cannot recover partial epoch state.
- Fix: Persist `processed_cycles` to SQLite. Add a `processed_telemetry` table with `(epoch_id, peer_id)` primary key.

**C4: On-chain staking not enforced**
- File: `solana_program/programs/introvert_registry/src/lib.rs:17`
- `stake_amount` is hardcoded to 0 in `register_rbn`. The `update_rbn` instruction allows toggling `is_active` without any bond verification.
- Impact: Anyone can register as an RBN operator without bonding 2M INTR. The escrow vault PDA described in ARCHITECTURE_BLUEPRINT.md does not exist on-chain.
- Fix: Add a `stake` instruction that transfers INTR to a PDA escrow. Gate `register_rbn` and `update_rbn(is_active=true)` on escrow balance >= 2M INTR.

**C5: IPC shared secret hardcoded in binaries**
- File: `introvert-daemon/introvert-p2p/src/main.rs:14`, `introvert-daemon/introvert-solana/src/main.rs:30`
- The HMAC-SHA256 shared secret `0757c80d60c40d5d6ac3cf337c4dda0b9c419d5b8e698d5dbb84df8991cd82f0` is identical in both binaries.
- Impact: If either binary is reverse-engineered, an attacker can forge arbitrary ClaimRequest messages and drain the treasury.
- Fix: Load the IPC secret from a file with restricted permissions (chmod 600), not from a compile-time constant. Rotate the secret periodically.

---

### HIGH

**H1: Double-claim guard keys on peer_id, not solana_wallet**
- File: `for_linux/src/economy/daily_rewards.rs:357-362`
- The guard checks `processed_cycles[epoch_id].contains_key(peer_id)`. But `peer_id` is a self-reported string.
- Impact: Attacker sends different `peer_id` values each time with the same `solana_wallet`/`solana_ata` to collect multiple rewards per epoch to the same wallet.
- Fix: Key the double-claim guard on `solana_wallet` (which is verified via Ed25519 signature), not `peer_id`.

**H2: No TelemetryAck**
- File: `for_linux/src/economy/mod.rs:262-275`
- Client sends telemetry via raw TCP write with no read-back. The RBN sends no confirmation.
- Impact: Silent telemetry loss. Client has no way to know if the RBN received or processed its data. Lost telemetry = lost rewards.
- Fix: Add a `TelemetryAck { epoch_id, status, estimated_reward }` response. Client should retry on timeout.

**H3: 2 of 3 trusted RBN keys are zero placeholders**
- File: `src/economy/daily_rewards.rs:27-38`
- `TRUSTED_RBN_PUBLIC_KEYS` has 3 entries. Entries 1 and 2 are `[0x00; 32]`. Effectively 1-of-1 signing.
- Impact: No key rotation capability. If the primary key is compromised, all ActivityWeights and AntiGamingConfig updates are compromised.
- Fix: Generate and populate the multisig member keys. Implement a key rotation mechanism.

**H4: Mint address used as treasury in relay registration**
- File: `src/main.rs:201,276`
- The relay/dashboard code passes the INTR mint address (`EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf`) as `treasury_pubkey` to `SolanaIncentiveEngine::new()`.
- Impact: All ATA derivations and treasury-dependent logic in the relay flow produce incorrect addresses.
- Fix: Pass the actual treasury pubkey (`9jauyKiimh6SBnpoRXcNXiLXZKSnN4h2gWKoqMcG4zHy`).

---

### MEDIUM

**M1: ATA field set to wallet address in epoch close**
- File: `for_linux/src/economy/daily_rewards.rs:511`
- `close_current_epoch()` sets `ata = wallet` (the Solana pubkey) instead of deriving the actual Associated Token Account.
- Impact: ClaimRequests sent to `introvert-solana` have the wrong ATA. The daemon may fail to find or create the correct token account.
- Fix: Derive ATA using `spl_associated_token_account::get_associated_token_address(&wallet, &mint)`.

**M2: DynamicPromoStack is dead code**
- File: `for_linux/src/economy/daily_rewards.rs:34-93`
- The struct and all its methods are defined but never instantiated or called from `process_telemetry()` or `close_current_epoch()`.
- Impact: The 20% strategic reserve (3,287.60 INTR/day Year 1) is allocated in the constants but never deducted from the daily pool. Campaign management has no runtime effect.
- Fix: Instantiate `DynamicPromoStack` in `RbnDailyRewardEngine`, call `compute_epoch_promo_distribution()` during epoch close, and deduct from the pool.

**M3: Three conflicting tier threshold systems**
- Reward multipliers (`solana.rs:90-100`): Sentinel >= 100K, Silver >= 250K, Gold >= 500K, Platinum >= 1M
- Balance gating (`balance_gating.rs:102-128`): Sentinel >= 50K, Silver >= 100K, Gold >= 250K, Platinum >= 500K
- Flutter UI (`sovereign_avatar.dart:73-77`): Uses yet another scale
- Impact: A node with 75K INTR shows Sentinel in the diagnostic UI but receives Citizen (1.0x) reward multiplier.
- Fix: Unify to a single tier definition shared across all three systems.

**M4: IPC HMAC comparison not constant-time**
- File: `introvert-daemon/introvert-solana/src/main.rs:173`
- Comment says "Constant-time comparison" but implementation uses `expected == signature` (standard Rust string equality).
- Fix: Use `subtle::ConstantTimeEq` or `hmac::verify` for the comparison.

---

## Key Constants

| Constant | Value | Location |
|----------|-------|----------|
| Year 1 Daily Pool (users) | 16,438 INTR | `daily_rewards.rs:10` (RBN), `daily_rewards.rs:54` (client) |
| Year 1 Daily Pool (RBNs) | 8,219 INTR | `daily_rewards.rs:11` (RBN), `daily_rewards.rs:55` (client) |
| Strategic Reserve | 3,287.60 INTR/day | `daily_rewards.rs:14` |
| Annual Decay | 0.8x (20%/yr) | `daily_rewards.rs:12` |
| Default Global Points | 100,000 | `daily_rewards.rs:13` |
| Daily Point Cap (standard) | 5,000 | `daily_rewards.rs:227` |
| Daily Point Cap (edge, 24h uptime) | 15,000 | `daily_rewards.rs:397-404` |
| Identity Lease Minimum | 100,000 INTR | `economy/mod.rs:428` |
| INTR Mint | `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` | consistent across all files |
| Treasury | `9jauyKiimh6SBnpoRXcNXiLXZKSnN4h2gWKoqMcG4zHy` | `lib.rs:256`, `daily_rewards.rs:43` |
| TGE Date | 2026-01-01 | `daily_rewards.rs:9` |
| Anti-Gaming: min unique peers | 3 | `daily_rewards.rs:246` |
| Anti-Gaming: max msgs per peer | 50 | `daily_rewards.rs:250` |
| Anti-Gaming: rapid fire limit | 10/window | `daily_rewards.rs:254` |

---

## Prestige Tier Reference

| Tier | Name | Threshold | Reward Multiplier |
|------|------|-----------|-------------------|
| 0 | Citizen | < 100K INTR | 1.0x |
| 1 | Sentinel | >= 100K | 1.05x |
| 2 | Silver | >= 250K | 1.10x |
| 3 | Gold | >= 500K | 1.20x |
| 4 | Platinum | >= 1M | 1.50x |
| 5 | Catalyst | activity-based | 1.15x |
| 6 | Pulsar | activity-based | 1.15x |

---

## 13 Activity Metrics

| Index | Activity | Default Weight | Cap |
|-------|----------|---------------|-----|
| 0 | MsgSent | 1.0 | 5,000 pts |
| 1 | MsgReceived | 0.5 | 5,000 pts |
| 2 | GroupMessageSent | 1.2 | 5,000 pts |
| 3 | GroupReaction | 0.3 | 5,000 pts |
| 4 | FileSend | 2.0 | 5,000 pts |
| 5 | FileRecv | 1.0 | 5,000 pts |
| 6 | CallSeconds | 0.1/sec | 5,000 pts |
| 7 | RelayBytes | 0.001/KB | 50 MB (RBN) / 10 MB (edge) |
| 8 | UptimeSeconds | 0.01/sec | 86,400s |
| 9 | WebFocusedActiveTime | 0.1/sec | 86,400s, container ≤3 |
| 10 | SandboxWebPacketData | 0.02/KB | container ≤3 |
| 11 | WebViewMediaCallHook | 0.2/sec | container ≤3 |
| 12 | UniquePeerHandshakes | 1.0/peer | 500 peers |

---

## Files Audited

Client (`src/`):
- `src/economy/daily_rewards.rs` — DailyRewardEngine, ActivityWeights, AntiGamingConfig, SignedRewardEnvelope, FFIDailyState
- `src/economy/mod.rs` — RewardTracker, package_telemetry, send_telemetry_to_rbn
- `src/economy/solana.rs` — SolanaIncentiveEngine, verify_prestige_tier
- `src/economy/balance_gating.rs` — Balance tier gating
- `src/storage.rs` — daily_reward_records, daily_activity_log, daily_reward_config tables
- `src/lib.rs` — FFI bridge (get_current_rewards_state)
- `src/main.rs` — CLI relay registration
- `src/identity.rs` — SovereignIdentity with prestige_tier

RBN (`for_linux/`):
- `for_linux/src/economy/daily_rewards.rs` — RbnDailyRewardEngine, process_telemetry, close_current_epoch, IQR
- `for_linux/src/economy/mod.rs` — send_telemetry_to_rbn, is_lease_valid
- `for_linux/src/economy/solana.rs` — SolanaIncentiveEngine (RBN-side)
- `for_linux/src/network/mod.rs` — lease_interval tick
- `for_linux/src/lib.rs` — epoch_id generation, economy monitoring loop

Daemon (`introvert-daemon/`):
- `introvert-daemon/introvert-p2p/src/main.rs` — P2P daemon, IPC signing, epoch close timer
- `introvert-daemon/introvert-p2p/src/daily_rewards.rs` — RbnDailyRewardEngine (standalone copy)
- `introvert-daemon/introvert-solana/src/main.rs` — Treasury daemon, transfer_checked, circuit breaker
- `introvert-daemon/introvert-solana/src/keygen.rs` — Treasury keypair generator

Solana (`solana_program/`):
- `solana_program/programs/introvert_registry/src/lib.rs` — register_rbn, update_rbn, RbnRegistryEntry

Flutter:
- `lib/src/ui/widgets/sovereign_avatar.dart` — PrestigeTier enum, UI tier display
