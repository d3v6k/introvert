# Debug Report — 2026-07-08

## Session Summary
Implemented Media Ingestion Safety Module, fixed RBN rewards pipeline, added Klipy stickers/memes, created Anchor Handle Registry program, and deployed RBN infrastructure updates.

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
