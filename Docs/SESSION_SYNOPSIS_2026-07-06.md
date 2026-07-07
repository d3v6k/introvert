# Session Synopsis — Introvert Sovereign Messenger
**Date:** 2026-07-06
**Git:** `2239bd5` → `6b4398c` (1 commit pushed to `origin/main`)

---

## 1. File Chunk Drop Vulnerability Fix

**Problem:** Chunks silently dropped when relay circuit unavailable, OutboundFailure, or app restart.

**Changes (3 files, +96 lines):**
- `src/network/mod.rs` — RAM-to-DB sweep, OutboundFailure re-queue
- `for_linux/src/network/mod.rs` — same changes for RBN
- `for_linux/src/storage.rs` — added `increment_chunk_retry()` (parity gap)

**How it works:** Every FileChunk buffered in RAM is now also persisted to `pending_file_chunks` SQLite table. On OutboundFailure, chunks re-queue to DB with 5-retry budget. Periodic tick sweeps RAM to DB before draining. All paths use `INSERT OR REPLACE` for idempotency. Chunks only deleted on `FileTransferComplete` or 24h stale cleanup.

---

## 2. Economy Security Audit

**Output:** `Docs/ECONOMY_AUDIT_2026-07-06.md`

**13 findings:** 5 Critical, 4 High, 4 Medium. Key issues: signature didn't cover destination wallet, lease check validated treasury not operator, epoch state in-memory only, IPC secret hardcoded, double-claim keyed on peer_id not solana_wallet.

---

## 3. Telemetry Signature Hardening (C1 + H1)

**Problem:** Attacker could reuse signature while changing destination wallet.

**Changes:** `for_linux/src/economy/daily_rewards.rs`

**Fix:** Signing message now covers all mutable fields:
```
epoch_id || peer_id || solana_wallet || solana_ata || metrics[0..13] || is_rbn || is_edge_node || prestige_tier || timestamp
```

Double-claim guard rekeyed from `peer_id` (self-reported) to `solana_wallet` (Ed25519-verified).

---

## 4. Persistent SQLite Double-Claim (C3)

**Problem:** Double-claim guard lost on RBN restart.

**Changes:** `for_linux/src/storage.rs`, `for_linux/src/economy/daily_rewards.rs`

**Fix:** Added `processed_telemetry` table with `(epoch_id, solana_wallet)` PK. SQLite check runs before in-memory check. After processing, row persisted for restart survival.

---

## 5. ATA Derivation Fix (M1)

**Problem:** `close_current_epoch()` set `ata = wallet` instead of deriving proper PDA address.

**Fix:** Added `derive_ata()` using `Pubkey::find_program_address` with canonical INTR mint `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf`.

---

## 6. IPC Secret Decoupling (C5)

**Problem:** HMAC shared secret hardcoded in `introvert-p2p` and `introvert-solana` binaries.

**Changes:** Both `introvert-daemon/introvert-p2p/src/main.rs` and `introvert-daemon/introvert-solana/src/main.rs`

**Fix:** Removed hardcoded constant. Both daemons now read from `/etc/introvert/ipc.secret` at startup. Panics with descriptive error if file missing or not 64 hex chars.

---

## 7. Lease Check Fix (C2) + Temporary Bypass

**Problem:** `is_lease_valid()` checked treasury balance (always passes), not local operator.

**Changes:** `for_linux/src/network/mod.rs`, `for_linux/src/lib.rs`

**Fix:** Added `operator_pubkey: Pubkey` field to `NetworkService`. Derived from identity seed via `NodeIdentity::derive_solana_keypair()`. Lease check now queries operator's actual balance.

**Bypass:** Pruning disabled — logs balance but allows traffic. `// TODO: Re-enable strict lease pruning enforcement here after initial deployment phase`. `is_lease_valid()` remains functionally correct; bypass is at call site.

---

## 8. Passive Telemetry-Correlation Engine

**Problem:** Tokenless forks consuming relay bandwidth without authenticating economy telemetry.

**Changes:** `for_linux/src/network/mod.rs`, `for_linux/src/economy/daily_rewards.rs`

**How it works:**
- `last_telemetry_seen: HashMap<String, Instant>` tracks when each wallet sent authenticated telemetry
- `peer_solana_wallets: HashMap<PeerId, String>` maps PeerId→wallet (populated from DirectInvite)
- `peer_relay_activity: HashMap<PeerId, Instant>` tracks when peers use relay bandwidth
- Background check every 6 hours cross-references relay-active wallets against telemetry timestamps
- 72-hour threshold before flagging
- `ENFORCE_FORK_GUARD = false` (audit-only mode, log but don't block)

---

## 9. Flutter UI Changes

**Settings screen (`lib/src/ui/main_shell.dart`):**
- Introvert logo below Fano icon (96px, theme-aware: `logo_white.png` for dark, `logo_black.png` for light)
- Build number with hyperlink to GitHub releases page

**Sovereign Wallet (`lib/src/ui/widgets/rewards_hud.dart`):**
- Points display uses real-time `daily_earnings` from `DailyRewardEngine` (updates every 30s)
- Shows: Social Points, Infrastructure Points, Total Points
- Removed "INTR Earned Today" (can't predict until epoch close)

**Declare Points to Mesh button:**
- Sends `NetworkCommand::SendManualTelemetry` → `TelemetryEnvelope` to RBN
- RBN receives, processes, sends back `TelemetryAck` (Event 40)
- Client listens for ack, displays: "RBN confirmed receipt for epoch YYYY_MM_DD. INTR distributed at epoch close."
- 15-second timeout if no ack received

**Points persistence:**
- `persist_current_activities()` saves to `daily_activity_log` SQLite table every 5 minutes
- On restart, `load_daily_activities()` restores `per_type_counts` and `per_type_capped`
- Points survive app crash/restart (after first 5-minute save)

---

## 10. Economy Logic Audit

**Verified in live `for_linux/` codebase:**
- User pool (16,438 INTR) vs RBN pool (8,219 INTR) separation: active
- Standard cap (5,000 pts) vs edge node cap (15,000 pts with 24h uptime): active
- Prestige multipliers (Sentinel 1.05x, Silver 1.10x, Gold 1.20x, Platinum 1.50x): active in both real-time and batch paths
- IQR outlier clamping: active
- ATA derivation: active

**Gap fixed:** `close_current_epoch()` had dormant `_tier` variable. Now uses `cycle.prestige_tier` and applies multiplier to `share * daily_pool * prestige_mult`.

---

## 11. Deployment Status

**RBN (47.89.252.80):** `introvertd` deployed with all changes. `introvert-solana` active on IPC port 9001.

**Deprecated:** `introvert-daemon/introvert-p2p/` marked as legacy. All economy logic consolidated in `for_linux/`.

---

## 12. Build Commands

```bash
# Client native library
make mac          # macOS dylib
make android      # Android .so (arm64 + x86_64)
make ios          # iOS static .a

# RBN daemon
cd for_linux && cargo build --release --bin introvertd

# Deploy RBN
./deploy_rbn.sh   # Syncs to thinkpad, cross-compiles, deploys to 47.89.252.80

# Backup
make bk           # Comprehensive backup with dd_mm_yy_time naming
```

---

## 13. Known Pre-Existing Issues

- APNs not configured (iOS push disabled)
- Single RBN (47.89.252.80) — single point of failure
- Solana registry bypassed — hardcoded to Alibaba RBN

---

## 14. Files Modified This Session

| File | Changes |
|------|---------|
| `src/network/mod.rs` | File chunk resilience, SendManualTelemetry command + handler, TelemetryAck handler (Event 40), connection state cycler status loop evaluation |
| `src/network/types.rs` | Added `SendManualTelemetry` variant |
| `src/lib.rs` | Added `introvert_send_manual_telemetry()` FFI, operator pubkey derivation, points persistence call |
| `src/economy/daily_rewards.rs` | `persist_current_activities()`, restore from DB on startup, test fixes with correct SHA-256 preimages |
| `lib/src/ui/main_shell.dart` | Logo, build number |
| `lib/src/ui/widgets/rewards_hud.dart` | Declare Points button, real-time points display, TelemetryAck listener |
| `lib/src/native/introvert_client.dart` | `telemetryAckStream`, Event 40 handler, `sendManualTelemetry()` |
| `for_linux/src/network/mod.rs` | TelemetryEnvelope/Ack variants + handlers, fork detection engine, relay activity tracking, SQLite database telemetry logging |
| `for_linux/src/economy/daily_rewards.rs` | Signature hardening, double-claim rekey, persistent SQLite, ATA derivation, prestige tier field, last_telemetry_seen |
| `for_linux/src/storage.rs` | `processed_telemetry` table, `increment_chunk_retry()`, telemetry persistence functions |
| `for_linux/src/lib.rs` | Engine struct with reward_engine, operator pubkey, midnight UTC cron scheduler, send_claim_to_treasury IPC |
| `for_linux/src/bin/stress_tester.rs` | Updated for new constructor args |
| `introvert-daemon/introvert-p2p/src/main.rs` | IPC secret from filesystem |
| `introvert-daemon/introvert-solana/src/main.rs` | IPC secret from filesystem |
| `Docs/ECONOMY_AUDIT_2026-07-06.md` | New — full economy security audit |

---

## 15. Session 2 Accomplishments (Rewards Integration & Connection Recovery)

**Milestone:** Fully resolved the Rewards Telemetry defect and the stuck-in-connecting cycler lag, deploying the updated binary to production RBN.

**Key Updates:**
- **Snappy Reconnection**: Integrated the connection state cycler (`ConnectionStateCycler`) evaluation into the 15-second status loop. Connected devices now recover snappily within 15–30 seconds.
- **13-Metrics Schema Parity**: Aligned shared metrics array layout to 13-metrics on both client and server, resolving wire deserialization drop.
- **SQLite raw telemetry persistence**: Implemented SQLite storage and loading routines for raw signed telemetry envelopes to survive RBN restarts.
- **Cryptographic Verification**: Wired Ed25519 signature checks into RBN telemetry signaling endpoints.
- **Midnight UTC Scheduler**: Implemented background epoch closing cron and claim request HMAC-SHA256 signature generator to dispatch claims to `introvert-solana` on port 9001.
- **Unit Test Fixes**: Fixed daily rewards and dual-pool separation unit tests by generating correct cryptographic proof hashes from preimage formats instead of using dummy strings.
- **Parity compile and Deploy**: Compiled macOS runner FFI library (`make mac`), iOS, and Android; successfully ran `deploy_rbn.sh` to update production RBN (`47.89.252.80`). Verified receipt of RBN confirmations on all devices.
