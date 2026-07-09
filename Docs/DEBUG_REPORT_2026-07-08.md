# Debug Report — 2026-07-08 (Updated 2026-07-09)

## Session Summary
Implemented Media Ingestion Safety Module, fixed RBN rewards pipeline, added Klipy stickers/memes, created Anchor Handle Registry program, and deployed RBN infrastructure updates.

## Additional Fixes (2026-07-09)

### 6. Epoch ID Off-by-One Bug
**Problem:** Midnight UTC close generated wrong epoch ID. `now - chrono::Duration::hours(0)` was a no-op, causing epoch `2026_07_09` to be closed instead of `2026_07_08`.
**Fix:** Changed to `now - chrono::Duration::days(1)` in `for_linux/src/lib.rs:440`.
**Status:** Fixed. Epoch `2026_07_08` closed successfully via startup catch-up mechanism.

### 7. IPC Secret Mismatch (Recurring)
**Problem:** Solana daemon binary on server still used hardcoded constant `0757c80d...` instead of reading from `/etc/introvert/ipc.secret`.
**Fix:** Updated introvert-solana source to load secret from file via `load_ipc_secret()`. Recompiled and deployed.
**Status:** Fixed. Both daemons confirmed reading from same file.

### 8. HMAC Timing Attack Vulnerability
**Problem:** IPC signature verification used standard string equality `expected == signature` which is vulnerable to timing attacks.
**Fix:** Added `subtle` crate dependency. Replaced with `expected.as_bytes().ct_eq(signature.as_bytes()).into()` for constant-time comparison.
**Status:** Fixed. Deployed to production.

### 9. Missed Epoch Recovery
**Problem:** If daemon restarts after midnight, the epoch close for the previous day is missed permanently.
**Fix:** Added startup catch-up mechanism that attempts to close yesterday's epoch if past 00:05 UTC.
**Status:** Fixed. Verified working — epoch `2026_07_08` recovered automatically on restart.

### Verification
- **Epoch 2026_07_08 Payout**: 3 claims, 16,438 INTR distributed successfully
  - `mPiqKQ8L...`: 4,893.67 INTR (sig: `3LP4tY7e...`)
  - `9fKYzZvg...`: 6,122.66 INTR (sig: `2TWnDuKX...`)
  - `2ZzBY7wK...`: 5,421.68 INTR (sig: `3Dotf8NG...`)
- **7/7 Unit Tests**: All passing
- **Both Daemons**: Active and reading IPC secret from file

## Issues Resolved

### 1. RBN Epoch Close Bug
**Problem:** Epoch close was using `now - 2 days` while client used `now - 0 hours`, causing the RBN to close stale epochs with no data.
**Fix:** Changed RBN epoch calculation from `now - 2 days` to `now - 0 hours` in `for_linux/src/lib.rs`.
**Status:** Fixed. Epoch `2026_07_07` closed successfully at 17:00 UTC with 3 claims.

### 2. IPC Secret Mismatch
**Problem:** Solana daemon had hardcoded secret `introvert-ipc-secret-change-in-production` while RBN read from `/etc/introvert/ipc.secret`. All claims rejected with "IPC signature verification failed".
**Fix:** Recompiled Solana daemon from updated source that reads from `/etc/introvert/ipc.secret`.
**Status:** Fixed. Both daemons now use same secret.

### 3. Legacy Telemetry Contamination
**Problem:** Old telemetry from previous test runs contaminated epoch close, causing payouts to wrong wallets.
**Fix:** Added TTL purge (48h), wallet identity cross-check, and changed PRIMARY KEY to `(epoch_id, peer_id)`.
**Status:** Fixed. Deployed and active.

### 4. Entropy False Positives
**Problem:** High-entropy images (JPEG, AVIF) blocked as "heuristicRiskBlocked".
**Fix:** Raised threshold to 7.95, changed to passive log only (no hard block). Added executable masquerading detection.
**Status:** Fixed. Deployed.

### 5. Connection Request UI
**Problem:** Connection request dialog was discreet AlertDialog.
**Fix:** Replaced with floating card overlay matching IncomingCallOverlay style.
**Status:** Fixed.

## Pending Work

### 1. Anchor Handle Registry Mainnet Deployment
**Status:** BLOCKED — deployer wallet needs 1.51 SOL
**Program ID:** `FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW`
**Deployer:** `2RhPjPgttAHZe5cEGdsZ4hLyznEQBKVXiC1T36MTZyWj`
**Guide:** `Docs/HANDLE_REGISTRY_DEPLOYMENT.md`

### 2. Post-Deployment Integration
- Wire Anchor `claim_handle` instruction in Solana daemon
- Initialize on-chain registry PDA
- Migrate existing handles from SQLite to on-chain

### 3. Client Balance Display
**Issue:** App shows 0 INTR despite on-chain balances. Likely caused by old wallet addresses in app's local storage vs new payouts to different ATAs.
**Status:** Needs investigation.

## Configuration Changes
- Epoch close time: 12:00 → 17:00 UTC
- Epoch ID calculation: `now - 13h` → `now - 0h` (RBN) matching client `CYCLE_TRANSITION_HOUR_UTC = 0`
- Entropy threshold: 7.8 → 7.95 (passive log only)
- Klipy API key: Hardcoded production key (removed settings UI)

## Verification Results
- `cargo build --release --bin introvertd` — compiles on thinkpad
- `cargo-build-sbf` — Anchor program compiles
- `flutter analyze` — zero new errors in lib/
- `solana program deploy` — verified on local validator
- Epoch close at 17:00 UTC — 3 claims, 16,438 INTR distributed
- IPC secret — both daemons reading from same file
