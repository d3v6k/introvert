# Rectification v2 — On-Device Test Checklist

Run these on a real device with VPN active. Have a second peer (Mac or another phone) as the transfer partner.

---

## Pre-test Setup

1. Build: `make all && flutter run`
2. Enable debug logging: ensure `dispatch_debug_log` output is visible (logcat / Xcode console)
3. Open a SQLite shell to the app's DB (e.g. `adb shell` + `sqlite3` on the app's `introvert.db` path)
4. Confirm schema: `PRAGMA table_info(pending_file_chunks);` — should show `in_flight_since INTEGER` column

---

## Test 1: Flapping Test (Bug-1 — duplicate chunk delivery)

**Goal:** Confirm no chunk index is forwarded twice during network instability.

### Steps
1. Start a 50MB+ file transfer from Peer A → Peer B (over VPN)
2. While transfer is in progress (10-30% done), toggle the VPN off for 3 seconds, then back on
3. Repeat 5 times at ~10-second intervals
4. After transfer completes, check logs

### Pass Criteria
- Log line `[Mesh] Published FileChunk via gossipsub topic=file-transfer-<id>` — count these per transfer_id
- The count must equal `total_chunks` from the FileChunk payload (not more)
- Query: `SELECT transfer_id, COUNT(*) FROM pending_file_chunks GROUP BY transfer_id;` — row count should be 0 for completed transfers (all chunks removed)
- No log lines like `[Relay] InboundCircuit DB flush: N chunks` where N keeps growing across flaps

### Fail Indicators
- Chunk count in logs exceeds `total_chunks` for any transfer
- `pending_file_chunks` rows grow across flaps instead of shrinking
- Duplicate `[Mesh] Published FileChunk` log lines for the same `(transfer_id, chunk_index)`

---

## Test 2: Concurrent Reconnect Test (Bug-1 — race condition)

**Goal:** Confirm `InboundCircuitEstablished` + `OutboundCircuitEstablished` firing close together don't double-send.

### Steps
1. Start a file transfer from Peer A → Peer B
2. Force a relay reconnection: toggle airplane mode on for 2 seconds, then off
3. This should trigger both `InboundCircuitEstablished` and `OutboundCircuitEstablished` within ~100ms
4. Check logs immediately after reconnect

### Pass Criteria
- Exactly ONE `[Relay] InboundCircuit DB flush: N chunks` log line per reconnect event
- No duplicate `(transfer_id, chunk_index)` pairs in the flush output
- The `dequeue_pending_chunks` transaction log (if you add one) shows atomic claim — no two concurrent calls selecting the same row

### Fail Indicators
- Two `InboundCircuit DB flush` log lines for the same peer within 500ms
- Same chunk index appears in two different flush outputs

---

## Test 3: Idle Mode Test (Bug-2 — background dial flooding)

**Goal:** Confirm backgrounding suppresses proactive dials.

### Steps
1. Open the app, connect to the mesh (status=1)
2. Background the app (press home button)
3. Wait 10 minutes
4. Check logs

### Pass Criteria
- After the first 15-second tick following backgrounding, log shows `[Resilience] Idle — suppressing background dials` (every 5 minutes)
- No `[Resilience] Step 1:` or `[Resilience] Step 2:` log lines while backgrounded
- No `[Resilience] Fast reconnect:` log lines while backgrounded
- `adb shell dumpsys batterystats` (Android) or Xcode Instruments (iOS) shows near-zero network wakeups

### Fail Indicators
- RBN dial attempts continue after backgrounding
- `[Resilience] Step 1:` log lines appear while app is backgrounded
- Battery stats show significant network activity while backgrounded

---

## Test 4: Idle-with-Pending-Transfer Test (Bug-2 — transfer survival)

**Goal:** Confirm transfers survive backgrounding and complete on foreground.

### Steps
1. Start a file transfer from Peer A → Peer B
2. At ~50% progress, background the app
3. Wait 2 minutes
4. Foreground the app
5. Check if transfer completes

### Pass Criteria
- Transfer resumes and completes after foregrounding
- Log shows `[Resilience] idle_mode set to true` on background, `idle_mode set to false` on foreground
- DB-queued chunks (if any) flush on `InboundCircuitEstablished` after foreground
- No chunks lost during the idle period

### Fail Indicators
- Transfer stalls permanently after foregrounding
- Chunks are missing (transfer completes but file is corrupt)
- `pending_file_chunks` has rows for a completed transfer

---

## Test 5: Wake-on-Push Test (Step 2e)

**Goal:** Confirm incoming FCM push immediately resets idle mode.

### Steps
1. Background the app (idle mode active, connected to mesh)
2. From another device, send a message or file to the backgrounded device
3. Check logs

### Pass Criteria
- Log shows `[Resilience] idle_mode set to false` IMMEDIATELY on FCM receipt (before fetchMailbox completes)
- `[Resilience] Wake-on-push: idle_mode reset to false` typically will NOT appear here — it's a fallback for non-FCM reconnects while idle (e.g. data arriving via the periodic `mailbox_fetch_interval` reconnect with no FCM involved). Its absence in this test is expected, not a failure.
- The relay reservation logic runs on the next status_check tick (≤15s, not 2-5min)
- The incoming message/file is received successfully

### Fail Indicators
- No `idle_mode set to false` log line when FCM arrives while backgrounded
- The incoming transfer stalls until manual foreground
- Wake latency exceeds 30 seconds (indicates waiting for mailbox_fetch_interval instead of FCM-triggered reset)

### Note
Wake latency is bounded by dial+relay establishment time (seconds), not the mailbox interval. The FCM path is: `IntrovertFirebaseMessagingService → IntrovertService → onWakeupCallback → AlertService.onWakeup → setAppIdleState(false) + fetchMailbox()`. The `setAppIdleState(false)` call resets idle mode before `fetchMailbox()` runs, so the next status_check tick (≤15s) can run the full dial ladder.

---

## Test 6: Cancel Test (Bug-3 — chunk cleanup)

**Goal:** Confirm cancelling a transfer removes all DB rows immediately.

### Steps
1. Start a file transfer
2. Cancel it at ~30% progress
3. Immediately query: `SELECT COUNT(*) FROM pending_file_chunks WHERE transfer_id = '<id>';`

### Pass Criteria
- Row count is 0 immediately after cancellation (no tick required)

### Fail Indicators
- Rows persist after cancellation
- Rows are only cleaned up after the 5-minute stale transfer watchdog

---

## Test 7: FCM Echo Loop Suppression

**Goal:** Confirm the FCM push → fetchMailbox → push echo loop is broken.

### Steps
1. Background the app (idle mode active)
2. From another device, send 5 messages in quick succession to the backgrounded device
3. Wait 2 minutes
4. Check logs

### Pass Criteria
- First push triggers: `Background Wakeup! Triggering P2P Fetch...`
- Subsequent pushes within 30s show: `Wakeup suppressed (30s cooldown — FCM echo loop breaker)`
- Total `Fetch Mailbox: Success` count is ≤ 5 (one per distinct message batch), not 100+
- After 30s cooldown expires, a new push can trigger a fresh fetch

### Fail Indicators
- `Fetch Mailbox: Success` count exceeds 20 (echo loop active)
- `idle_mode set to false` appears hundreds of times
- WakeLock is acquired/released in a tight loop

---

## Quick DB Queries

```sql
-- Check pending chunks for a specific transfer
SELECT transfer_id, chunk_index, in_flight_since FROM pending_file_chunks WHERE transfer_id = '<id>';

-- Check total pending chunks across all transfers
SELECT transfer_id, COUNT(*) as chunk_count FROM pending_file_chunks GROUP BY transfer_id;

-- Check for stale in-flight claims (should be 0 after 30s)
SELECT COUNT(*) FROM pending_file_chunks WHERE in_flight_since > 0 AND in_flight_since < strftime('%s', 'now') - 30;

-- Verify schema
PRAGMA table_info(pending_file_chunks);
```

---

## Log Patterns to Watch

| Pattern | Meaning |
|---------|---------|
| `[Resilience] idle_mode set to true` | App entered idle mode |
| `[Resilience] idle_mode set to false` | App exited idle mode (foreground or wake-on-push) |
| `[Resilience] Idle — suppressing background dials` | Idle gate working correctly |
| `[Resilience] Wake-on-push: idle_mode reset to false` | Incoming payload woke the app |
| `[Mesh] Published FileChunk via gossipsub topic=...` | Chunk sent successfully (check count vs total_chunks) |
| `[Mesh] Gossipsub publish FAILED` | Chunk send failed (should see release_in_flight_chunk) |
| `[Relay] InboundCircuit DB flush: N chunks` | DB queue flush on circuit establishment |
| `[Periodic] Flushing N pending DB chunks` | Mailbox fetch interval flush |
