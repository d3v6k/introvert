# Implementation Report — Ghost Message Fix & Mailbox Sync Hardening

**Date:** 2026-07-19
**Codebase version:** v0.35.0 (uncommitted)
**Git baseline:** main @ cbd5f0f
**Backup:** `20_07_26_1445` on external SSD

---

## Executive Summary

This document records all code changes made to fix the "ghost message" bug (stuck file transfers permanently resurrecting on every chat open) and harden the mailbox/sync subsystem. The work was driven by two complementary analyses:

- **Mimo's plan (P0–P5):** Protocol correctness — read receipt spam, cleared-chat race, drain efficiency, sync timeout, file message recovery.
- **Expert's plan (§2.1–2.6):** Perception bug root cause — ephemeral pull state, no terminal failure state, full-clear rebuilds, backfill/live indistinguishable.

Both plans were merged, collisions resolved (status codes 4 vs 5), and all gaps identified during three rounds of expert review were addressed.

---

## What Was Wrong (Root Causes)

### 1. Ghost Messages — Stuck transfers never die

Four mechanisms combined to make stuck file transfers permanently revive:

1. **`_pullRequested` / `_pullRequestedAt` were ephemeral** (widget-scoped, `chat_screen.dart:732-734`). Every chat close or app restart destroyed the state. On next open, every incomplete transfer in local history looked brand new and fired `startPull` again.

2. **No terminal "failed/expired" status existed.** `pending_file_chunks` capped at 5 retries and deleted chunk rows, but never marked the parent chat message. The message's status stayed at 0/1/3 forever — permanently "still trying."

3. **`_loadMessages()` did a full clear-and-rebuild** (`_messages.clear(); _messages.addAll(loaded)`) on every call. It was triggered by `[FILE]:` events, sync completion, and many UI callbacks. Each reload re-ran the pull-eligibility check for all historical transfers.

4. **Backfill traffic was indistinguishable from live traffic.** Mailbox drain and chat sync delivered old messages through the same Event 2 path as new live messages. Flutter couldn't tell the difference, so it auto-scrolled and applied "new arrival" treatment.

### 2. Read Receipt Spam

- **Bulk case:** `_markMessagesAsRead()` sent read receipts for ALL incoming messages on chat open, including very old ones. Opening a chat with 500 old messages sent 500 receipts.
- **Backfill case:** Messages arriving via mailbox drain or chat sync triggered read receipts as if they were live messages.

### 3. Drain Races and Inefficiency

- No `drain_in_progress` tracking — concurrent drain requests to the same anchor.
- `last_empty_drain` was global, not per-anchor — one anchor's empty response suppressed drains for all anchors.
- Batch size was 4 (chosen for a 1MB limit that was restored to 10MB in v37).
- `sync_in_progress` timeout was 60s — too aggressive for large history syncs.

### 4. Cleared-Chat Race

If `delete_chat` was called while `MailboxDrained` was processing, messages already past the clear guard (in the local queue) bypassed the check and were dispatched to the UI.

---

## What Was Changed

### Files Modified (7 files, ~400 lines)

| File | Changes |
|------|---------|
| `src/storage.rs` | Status 5 support, `mark_file_transfer_failed`, `sweep_expired_file_transfers`, `complete_file_transfer_recovery`, batch size 4→8 |
| `src/network/types.rs` | `is_backfill` on `ChatMessage`/`SyncMessage`, `ClearPendingMessages` command |
| `src/network/mod.rs` | Backfill flag on drain/sync paths, event format versioning, drain efficiency, clear-guard re-check, recovery status update, sync timeout, deferral comment |
| `src/network/service.rs` | `drain_in_progress: HashSet<PeerId>`, `last_empty_drain: HashMap<PeerId, Instant>` |
| `src/lib.rs` | `introvert_network_clear_pending_messages` FFI function |
| `lib/src/native/introvert_client.dart` | `clearPendingMessages` FFI binding |
| `lib/views/chat_screen.dart` | Event parser with `is_backfill`, incremental merge, persisted pull state, terminal UI, read-receipt gating |

### Status Code Scheme

| Code | Meaning | How it's set |
|------|---------|--------------|
| 0 | Sending (outgoing) / default | Existing — set on message creation |
| 1 | Delivered (ACK from recipient) | Existing — set by ACK handler; also set by `complete_file_transfer_recovery` on file completion |
| 2 | Read (read receipt) | Existing — set by read receipt handler |
| 3 | In Mailbox (stored on RBN) | Existing — set by `MailboxStored` ACK |
| 5 | Failed/Expired (terminal) | **New** — set by `mark_file_transfer_failed` (chunk retry exhaustion), `sweep_expired_file_transfers` (7-day TTL) |

Status 4 ("pending recovery") was planned but removed — see "Deferred" section below.

### Detailed Changes by Subsystem

#### A. Persisted Pull-Attempt State (`chat_screen.dart`)

**Before:** `_pullRequested: Set<String>` and `_pullRequestedAt: Map<String, DateTime>` — in-memory only, destroyed on chat close.

**After:** SharedPreferences-backed, keyed by `transferId`:
- `_getPullAttemptCount(transferId)` — reads `pull_{id}_count`
- `_getPullLastAttempt(transferId)` — reads `pull_{id}_at`
- `_shouldPull(transferId)` — returns false if count >= 5 or if within backoff window
- `_recordPullAttempt(transferId)` — increments count, records timestamp
- `_resetPullAttempts(transferId)` — clears both (used on manual retry tap)

**Backoff schedule:** 30s → 2m → 10m → 30m → stop (5 attempts max, matching Rust `pending_file_chunks` retry cap).

**Where called:**
- `_loadMessages()` line ~989: `_shouldPull` check before `startPull`
- `FileTransferBubble.onTap` line ~1617: `_shouldPull` check + `_recordPullAttempt`
- Terminal state tap handler: `_resetPullAttempts` + `_recordPullAttempt` (one retry)

#### B. Incremental Merge in `_loadMessages` (`chat_screen.dart`)

**Before:** `_messages.clear(); _messages.addAll(loaded);` — full clear-and-rebuild on every call.

**After:** Diff-based merge:
1. Build `loadedById` map from freshly loaded messages.
2. Update existing entries in place (by `msgId`/`transferId`).
3. Append genuinely new entries.
4. Rebuild `_messagesById` index.

**Impact:** A mailbox-drain-triggered reload while the chat is open no longer re-renders the entire transcript and re-enters the pull-eligibility check for every historical transfer.

#### C. Targeted `[FILE]:` Event Handling (`chat_screen.dart`)

**Before:** `if (content.startsWith("[FILE]:")) { _loadMessages(); return; }` — full reload on every file event.

**After:** Parses the file metadata, finds the existing entry by `transferId`, updates in place if found. Only triggers `_loadMessages()` for genuinely new transfers not yet in the list.

#### D. Backfill Flag (`types.rs`, `mod.rs`, `chat_screen.dart`)

**Wire format:** `ChatMessage` and `SyncMessage` carry `#[serde(default)] is_backfill: bool`.

**Set by:**
- `MailboxDrained` handler: marks all drained messages as `is_backfill = true`
- `ChatSyncResponse` handler: marks all sync messages as `is_backfill = true`
- Live `ChatMessage` handler: stays `is_backfill = false` (default)

**Event format:** Versioned with `0x01` prefix byte:
```
[0x01] [8-byte timestamp] [msg_id_len] [msg_id] [rt_len] [rt] [content] [is_backfill_byte]
```
Legacy format (first byte is high timestamp byte, always > 0x01 for 2026 timestamps) is auto-detected.

**Dart behavior when `is_backfill == true`:**
- No read receipts sent
- No scroll-to-bottom
- Message still inserted into list (quietly, without animation)

#### E. Read Receipt Gating (`chat_screen.dart`)

**Two fixes:**

1. **Backfill suppression:** Read receipts only sent when `isBackfill == false` (live messages).

2. **`_markMessagesAsRead` bulk fix:** Persisted `last_read_receipt_{peerId}` timestamp via SharedPreferences. Only sends receipts for messages newer than the last receipt batch. Local state clear (`updateMessageStatusForPeer(peerId, 0)`) still runs unconditionally.

#### F. Status 5 — Failed/Expired (`storage.rs`, `mod.rs`)

**New functions:**

| Function | Purpose | SQL |
|----------|---------|-----|
| `mark_file_transfer_failed(transfer_id)` | Sets status 5 for a specific transfer | `UPDATE messages SET status = 5 WHERE content LIKE ?1 AND status IN (0, 1, 3, 4)` |
| `sweep_expired_file_transfers(max_age_secs)` | Bulk-marks old incomplete transfers as failed | `UPDATE messages SET status = 5 WHERE content LIKE '%[[]FILE]:%' AND content NOT LIKE '%is_complete":true%' AND status IN (0, 3) AND ...` |
| `complete_file_transfer_recovery(msg_id)` | Transitions 5→1 on successful file completion | `UPDATE messages SET status = 1 WHERE msg_id = ?1 AND status = 5` |

**Key invariants:**
- `mark_file_transfer_failed` preserves status 2 (read) only — a delivered ACK (status 1) doesn't guarantee file bytes completed, so 1 can still transition to 5 (failed).
- `sweep_expired_file_transfers` excludes completed transfers (`content NOT LIKE '%is_complete":true%'`).
- `complete_file_transfer_recovery` only transitions from 5→1, preserves 2.

**Triggered by:**
- Chunk retry exhaustion: `increment_chunk_retry` at 5 attempts → `mark_file_transfer_failed`
- Periodic sweep: `sweep_expired_file_transfers(604800)` in `status_check_interval` (7-day TTL matching RBN mailbox)
- File completion: `FileTransferComplete` handler → `complete_file_transfer_recovery`

#### G. Terminal UI State (`chat_screen.dart`)

When `_getPullAttemptCount(transferId) >= 5` and the file is incomplete and incoming:
- Renders as greyed-out container with "Failed to download — tap to retry" label
- Tap resets attempt counter and issues one manual `startPull`
- Uses `Icons.error_outline` + muted colors to distinguish from active transfers

#### H. Drain Efficiency (`service.rs`, `mod.rs`)

| Field | Type | Purpose |
|-------|------|---------|
| `drain_in_progress` | `HashSet<PeerId>` | Prevents concurrent drain requests to same anchor |
| `last_empty_drain` | `HashMap<PeerId, Instant>` | Per-anchor empty-drain backoff |

**Fast-poll skip:** If ALL connected anchors had empty drains in the last 30s, skip the fast-poll 5s drain.

**Cleanup:** `drain_in_progress` entries are cleared on:
- `MailboxDrained` response
- `OutboundFailure` (timeout, connection closed)

**Batch size:** `fetch_mailbox_payloads` LIMIT increased from 4 to 8. Confirmed safe: libp2p `request_response` config at `behaviour.rs:61-62` sets 10MB limit for both requests and responses.

#### I. Cleared-Chat Race Fix (`mod.rs`, `lib.rs`, `chat_screen.dart`)

**Two fixes:**

1. **Clear-guard re-check on queue pop:** In `handle_signaling_payload`, the `while let Some((p, pl, is_wtc)) = queue.pop()` loop now re-checks `should_skip_mailbox_message` for `ChatMessage` payloads before dispatching. Catches `delete_chat` calls that arrive between the initial guard check in `MailboxDrained` and the actual dispatch.

2. **`ClearPendingMessages` command:** New FFI function `introvert_network_clear_pending_messages` clears the outbound `pending_messages` buffer for a peer. Called from `chat_screen.dart` after `_client.deleteChat(peerId)`.

#### J. Sync Timeout (`mod.rs`)

- `sync_in_progress` eviction: 60s → 120s (extended for large history syncs)
- Recursive sync guard: time-based cap (120s max) prevents infinite recursive `SyncChatMessages` loops

---

## What Was NOT Changed (Deferred)

### P4: Auto-Recovery of Missing File Data on Sync

**Status:** Infrastructure removed, deliberately deferred.

**What was planned:** When `ChatSyncResponse` receives a `[FILE]:` message whose file data is missing, mark it as status 4 ("pending recovery") and trigger `startPull` from the original sender.

**Why it was deferred:**
1. `ChatSyncResponse` currently drops `[FILE]:` messages at line 6522 (unchanged from before this effort).
2. The status-4 infrastructure (`mark_file_transfer_pending_recovery`) was written but had zero callers.
3. Wiring it up requires solving the shared-retry-ceiling problem (§5.2): the Rust-triggered `startPull` must go through Dart's persisted attempt counter, not have its own independent counter. Otherwise a transfer can ping-pong between the two paths and never reach the retry ceiling.
4. This is new scope, not a loose end from this fix.

**What was removed:** `mark_file_transfer_pending_recovery` function, status-4 entry transitions in `update_message_status_if_higher`.

**What was kept:** Status-5 infrastructure (actively used by sweep and chunk-retry).

**Where to find the deferral:** Comment at `src/network/mod.rs` line ~6522 in the `ChatSyncResponse` handler, documenting the three requirements for re-introducing this feature.

---

## Expert Review Gaps (All Resolved)

| Gap | Issue | Resolution |
|-----|-------|------------|
| 1 (P1) | `ClearPendingMessages` only fixed the outbound queue; the inbound `MailboxDrained` race (messages in queue bypassing clear guard) wasn't addressed | Added clear-guard re-check at top of `while let` queue-pop loop in `handle_signaling_payload` |
| 2 (Status) | Raw-SQL functions could overwrite status 2 (read) with status 4/5; `FileTransferComplete` handler's `update_message_status_if_higher(1)` fails when status is 5 | `mark_file_transfer_failed` guards: `status IN (0, 1, 3, 4)`; `complete_file_transfer_recovery` transitions 5→1 only; sweep excludes completed transfers |
| 3 (Read receipts) | `_markMessagesAsRead` still fired for ALL old messages on chat open | Persisted `last_read_receipt_{peerId}` timestamp; only sends receipts for messages newer than last batch |
| 4 (Drain) | `drain_in_progress` had no timeout fallback; `last_empty_drain` was global not per-anchor | `last_empty_drain` changed to `HashMap<PeerId, Instant>`; `drain_in_progress` cleared on `OutboundFailure`; 20s libp2p timeout confirmed at `behaviour.rs:57-58` |
| 5 (Batch) | Batch size 4→8 assumed 10MB limit without verification | Confirmed: `set_request_size_maximum(10 * 1024 * 1024)` and `set_response_size_maximum(10 * 1024 * 1024)` at `behaviour.rs:61-62` |
| 6 (Recovery) | `mark_file_transfer_pending_recovery` had zero callers; dead code | Removed function and status-4 entry transitions; deferral documented at sync drop point |
| 7 (Status regression) | Raw-SQL `mark_file_transfer_failed` could overwrite status 2 (read) with 5 (failed) | Guard changed to `status IN (0, 1, 3, 4)` — preserves read status |
| 8 (Recovery completion) | `FileTransferComplete` handler's `update_message_status_if_higher(1)` fails when status is 5 (1 > 5 is false) | New `complete_file_transfer_recovery` function: `UPDATE status = 1 WHERE status = 5` |

---

## Compilation Status

- **Rust:** `cargo check` passes with 37 warnings (all pre-existing — unused variables, dead code). Zero errors.
- **Dart:** `flutter analyze` passes with zero errors in app code (`lib/`). Pre-existing warnings in `plugins/` and `for_expert/` (stale copies) are unrelated to this change set.

---

## Testing Notes

### What to verify on the real mesh (Mac, Android, iOS):

1. **Ghost message reproduction:** Open chat with old stuck transfers → close chat → reopen → verify they no longer re-fire `startPull` or appear as new.

2. **Backfill silence:** Send messages from device A to device B (B offline). Bring B online. Open chat. Verify drain messages appear quietly (no scroll-to-bottom, no "new message" animation).

3. **Terminal state:** Let a transfer fail 5 times. Verify it shows "Failed to download — tap to retry". Tap it. Verify it retries once.

4. **Read receipts:** Send 10 messages from A to B (B offline). Bring B online. Open chat. Verify B sends read receipts only for genuinely new messages, not for old ones already in DB.

5. **Cleared-chat race:** Clear chat on device A exactly while drain is in flight from RBN. Verify cleared messages don't reappear.

---

## Source Documents

- Expert analysis: `for_expert/mailbox-sync-chat-behavior/MAILBOX_SYNC_CHAT_BEHAVIOR.md`
- Ghost message fix plan: `for_expert/mailbox-sync-chat-behavior/claude GHOST_MESSAGE_FIX_PLAN.md`
- Full source copies: `for_expert/mailbox-sync-chat-behavior/src/` and `for_expert/mailbox-sync-chat-behavior/lib/`
- Merged plan: `.mimocode/plans/1784479454276-hidden-rocket.md`
