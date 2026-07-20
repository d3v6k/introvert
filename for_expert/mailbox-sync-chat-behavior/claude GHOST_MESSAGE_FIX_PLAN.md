# Ghost Message Fix Plan — Mailbox Sync & Chat-Open Behavior

Companion to `MAILBOX_SYNC_CHAT_BEHAVIOR.md`. This document pinpoints why old/stuck
transfers resurface as "new" messages, and gives you scoped, copy-pasteable prompts
for `mimo cli`.

---

## 1. Root Cause (confirmed in code, not speculation)

Four separate mechanisms combine to produce the symptom:

### 1.1 Pull-retry state is ephemeral and resets on every chat open
`chat_screen.dart:732-734`
```dart
final Set<String> _pullRequested = {};
static const Duration _pullRetryTimeout = Duration(seconds: 30);
```
`_pullRequested`/`_pullRequestedAt` live only on the `ChatScreen` widget instance.
Close the chat, or restart the app, and this state is gone. On the very next open,
**every incomplete file transfer in local history — including ones stuck for
weeks — looks brand new** to this logic and immediately fires `startPull` again
(`chat_screen.dart:952-964`).

### 1.2 No terminal "failed/expired" state exists anywhere
- Rust: `pending_file_chunks` has `retry_count` capped at 5 and a
  `cleanup_stale_pending_chunks` sweep (`storage.rs:1482-1512`), but this only
  deletes the raw chunk queue rows — it never writes back to the parent chat
  message.
- The chat message row itself (the `[FILE]:` content string) has no status enum
  value for "gave up" — only the generic 0/1/2/3 codes in
  `MAILBOX_SYNC_CHAT_BEHAVIOR.md §7.1`, none of which mean "failed."
- Mailbox entries on the RBN expire after 7 days (`storage.rs` mailbox TTL), but
  the **local** incomplete-file row has no matching expiry. A transfer whose
  source data is long gone from the RBN still reads as "waiting" forever and is
  retried on every chat open.

Net effect: a stuck transfer has no way to ever die. It is permanently "still
trying," and every trigger event revives it visually.

### 1.3 `_loadMessages()` does a full clear-and-rebuild, and is called constantly
`chat_screen.dart:993-995`
```dart
_messages.clear();
_messages.addAll(loaded);
```
This isn't just called on `initState`. It's also called from, among others:
- `chat_screen.dart:2278` — **any** incoming event whose content starts with
  `[FILE]:` (type 2/4 stream events), unconditionally, with no dedup check
  (contrast with the `isDuplicate` guard that *does* exist for plain text
  messages two lines later at `:2281`).
- `chat_screen.dart:2304` — sync-completion event (type 23).
- `chat_screen.dart:2307` — event type 40.
- Several UI action callbacks (`:1768, :1825, :1994, :2012, :2054, :2776, :2784, :3046, :3217`).

Each call re-scans the *entire* local message history from SQLite, including
long-resolved-as-stuck transfers, and re-runs the "is this incomplete → should I
pull" branch. Because of §1.1, that branch has no memory across the reload if the
widget was ever recreated, and even within a single session it re-fires every
30s per §1.1's `_pullRetryTimeout`.

### 1.4 Background reconciliation traffic is not distinguished from live traffic
Mailbox drain (waves of 4, every 500ms while draining — §2.5/8.2 of the companion
doc) and chat sync (`ChatSyncResponse`, batches of 100 — §3.5) both deliver
**old** messages through the **same** event path as brand-new live messages
(`ChatMessage handler → Event 2`, `MAILBOX_SYNC_CHAT_BEHAVIOR.md §5.1` step 6e).
Nothing in the payload or the event marks "this is backfill from reconciliation"
vs. "this just happened." Flutter has no way to tell the difference, so it
auto-scrolls, and applies the same "new arrival" treatment to both.

**Bottom line:** opening a chat while mailbox sync/drain is active causes the
full-history reload to run repeatedly, each time re-discovering every historically
stuck transfer as if it were unseen, re-issuing `startPull` for it, and rendering
it with the same visual treatment as a genuinely new incoming message — with no
timeout or retry ceiling to ever put it to rest.

---

## 2. Fix Plan — prompts for `mimo cli`

Run these roughly in order; 2.1–2.2 are the Rust-side foundation the Flutter fixes
in 2.3–2.5 depend on. Each prompt is scoped to be independently reviewable.

### 2.1 — Add a terminal transfer status + client-side expiry (Rust)
> **Note (see §5 addendum):** use status code **5**, not 4 — mimo's plan already
> claims status 4 for a different, non-terminal meaning ("file pending
> recovery"). Codes 4 and 5 form a sequence; see §5.1 below.
```
In src/storage.rs, add a new message status code 5 = "Failed/Expired" to the
existing status scheme documented in MAILBOX_SYNC_CHAT_BEHAVIOR.md §7.1 (codes
0-3, plus 4 = "file pending recovery" from mimo's P4). Add a function
`mark_file_transfer_failed(&self, transfer_id: &str) -> Result<()>` that updates
the message row whose content is `[FILE]:{...transfer_id...}` to status 5, and
add `get_file_transfer_age_secs(&self, transfer_id: &str) -> Result<Option<i64>>`
that reads the message's stored timestamp.

In src/network/mod.rs, wherever pending_file_chunks retry_count hits the existing
5-attempt cap and the chunk row is deleted (search near storage.rs:1482-1512's
call sites), also call mark_file_transfer_failed for that transfer_id instead of
silently dropping only the chunk queue row.

Add a periodic sweep (piggyback on the existing status_check_interval, 15s, or
mailbox_fetch_interval, 120s/300s — see event loop table in the companion doc
§6.1) that finds incomplete file messages older than 7 days (matching the RBN
mailbox TTL) and calls mark_file_transfer_failed on them, since the source data
is guaranteed gone from the RBN by then.

Emit a Dart-facing event (reuse the existing FFI event stream) when a transfer
transitions to status 5 so the UI can update without a manual reload.
```

### 2.2 — Mark reconciliation/backfill traffic distinctly from live traffic (Rust)
```
In src/network/types.rs, add a `#[serde(default)] pub is_backfill: bool` field to
SyncMessage and to the ChatMessage variant of SignalingPayload (or, simpler,
tag it at the point of dispatch to Dart rather than on the wire — either works,
but wire-level is preferable since group relay also needs to preserve it).

In src/network/mod.rs:
- Where MailboxDrained messages are pushed into the processing queue
  (mod.rs:5862-5902), mark them is_backfill = true.
- Where ChatSyncResponse messages are stored via store_message_if_new
  (mod.rs:6421-6563), mark them is_backfill = true.
- Live ChatMessage arrivals (mod.rs:5988-6043) stay is_backfill = false.

When dispatching Event 2/4 to Flutter, include this flag in the serialized
payload (extend the existing binary format in a backward-compatible way — add a
trailing flag byte, since chat_screen.dart's event parser at chat_screen.dart:2253-2271
reads a fixed prefix before content).
```

### 2.3 — Stop treating reconciled/backfilled arrivals as new-message events (Dart)
```
In lib/views/chat_screen.dart, update the type 2/4 handler (around :2253-2299) to
read the new is_backfill flag from event 2.2. When is_backfill is true:
- Do not call _scrollToBottom().
- Do not trigger the "new message" insert-and-animate path — instead route it
  through a quiet incremental merge (see 2.4) rather than a UI-visible append.
- Still send read receipts only if the chat is genuinely open and the message
  postdates the last time the chat was actually viewed (avoid re-triggering read
  receipts for messages that are years old, per companion doc §8.5).

Also remove the unconditional `_loadMessages()` call currently at :2278 for
`[FILE]:` content — replace it with a targeted update to just that one
transfer_id's entry (reuse the merge logic you already have at :2340-2374 for
event type 12, which updates a single FileTransferProgress in place instead of
reloading everything).
```

### 2.4 — Replace full-list rebuild with incremental merge in `_loadMessages`
```
In lib/views/chat_screen.dart, refactor _loadMessages() (:860-1005) so it no
longer does `_messages.clear(); _messages.addAll(loaded);` on every call.
Instead:
- Keep a `_messagesById` lookup (msgId / transferId -> index) alongside
  _messages, updated incrementally.
- On reload, diff `loaded` against the current list by id: update changed
  entries in place, insert genuinely new ones (id not previously seen this
  session), and leave everything else untouched — no full clear.
- Only call _scrollToBottom() when the diff actually added something at the
  tail, not on every reload.

This ensures a mailbox-drain-triggered reload while the chat is open doesn't
cause the whole transcript (including long-resolved stuck transfers) to
re-render and re-enter the pull-eligibility check every time.
```

### 2.5 — Persist pull-attempt state and give stuck transfers a retry ceiling (Dart)
```
In lib/views/chat_screen.dart, replace the in-memory-only `_pullRequested` /
`_pullRequestedAt` (:732-734) with state that survives chat close/reopen and app
restart — either a small local table via introvert_client.dart's existing FFI
storage, or a simple SharedPreferences-backed map keyed by transfer_id, storing
{attempt_count, last_attempt_at}.

Enforce a ceiling: after N attempts (start with 5, matching the Rust
pending_file_chunks retry cap in storage.rs) or after the transfer's message
status becomes 5 ("Failed/Expired" from 2.1), stop calling startPull entirely for
that transfer_id and render it in a distinct terminal UI state (see 2.6) instead
of "waiting."

If mimo's P4 (below, §5.1) lands, its recovery-triggered `startPull` calls from
the `ChatSyncResponse` handler must increment this same shared attempt counter —
not a separate one — or a transfer can retry indefinitely by alternating between
the Dart-triggered and Rust-triggered pull paths, each resetting the other's
cooldown.

Increase _pullRetryTimeout backoff instead of a flat 30s — e.g. 30s, 2m, 10m,
30m, then stop — so an open chat doesn't hammer startPull indefinitely for a
transfer that isn't going to complete.
```

### 2.6 — Add a "Failed / Expired" visual state for file bubbles (Dart)
```
In lib/views/chat_screen.dart's FileTransferProgress rendering path, add a
branch for status 5 / retry-ceiling-reached transfers: render as a greyed-out
bubble with a "Failed to download — tap to retry" (incoming) or "Delivery
failed" (outgoing) label instead of the active progress/spinner treatment. Tapping
resets the attempt counter from 2.5 and re-issues one manual startPull.

This is the piece regular users will actually notice: instead of a transfer that
either silently spins forever or intermittently reappears looking new, it settles
into a clearly-terminal, tappable state — the same mental model as WhatsApp/
Signal's "Download failed."
```

---

## 3. Suggested order of operations

1. **2.1 + 2.2** (Rust) first — nothing downstream works without the status code
   and the backfill flag existing on the wire.
2. **2.4** (Dart, incremental merge) — do this before 2.3/2.5 so you're not
   debugging retry logic against a UI that's still tearing itself down every
   reload.
3. **2.3** (suppress new-message treatment for backfill).
4. **2.5** (persisted retry ceiling).
5. **2.6** (terminal UI state) — cosmetic last, but this is what actually reads
   as "fixed" to a user.

## 4. Secondary polish (optional, lower priority)

- §8.5 in the companion doc: `_markMessagesAsRead` sends read receipts for *all*
  incoming messages on chat open regardless of age — worth gating on "postdates
  last-read timestamp" once 2.2's is_backfill flag exists, since you'll have the
  data to do it cheaply.
- §8.4 cleared-chat race: messages already in RAM from an in-flight drain can
  bypass the `cleared_chats` guard. Low priority unless you're seeing cleared
  chats repopulate.

---

## 5. Addendum — reconciling with mimo cli's plan

mimo generated a separate plan (P0–P5) covering read-receipt spam, the
cleared-chat race, drain efficiency, sync timeout, and file-message recovery.
It's mostly orthogonal to this document — it attacks protocol correctness and
efficiency, this document attacks the perception bug. One real collision, two
places worth merging, and answers to the questions that bear on this plan.

### 5.1 Collision: two different meanings were assigned to status 4
mimo's P4 proposes status=4 = "file pending" (sync revealed a `[FILE]:` message
whose data never arrived; trigger a recovery `startPull`). This document
originally also proposed status=4, meaning "gave up, terminal." Same code,
incompatible meanings — this has been **renumbered above**: status 4 = mimo's
"pending recovery," status 5 = this doc's "Failed/Expired" terminal state.

These two states aren't just non-conflicting once renumbered — they're
naturally sequential and should be implemented as one status-code migration,
not two:
```
0/1/2/3 (existing) → 4 "pending recovery" (mimo P4: sync found the message,
file data is missing, auto-retry in progress) → 5 "Failed/Expired" (this doc
§2.1/2.5: retry ceiling or 7-day TTL reached, give up, terminal)
```
Implement both status values and the storage/migration work in a single pass
(merge mimo's P4 into this doc's §2.1 prompt) rather than as two separate
schema changes to the same tables.

### 5.2 P4's recovery `startPull` must share the retry ceiling from §2.5
mimo's P4 triggers `startPull` directly from the Rust `ChatSyncResponse`
handler whenever it notices missing file data. §2.5 in this doc independently
throttles `startPull` from the Dart side with a persisted attempt counter. Wire
these together — the Rust-triggered recovery attempt from P4 should increment
the *same* counter that Dart's chat-open pull path checks, otherwise a stuck
transfer can ping-pong between the two trigger sites and never reach the
retry ceiling, reintroducing the exact "never dies" problem this doc exists to
fix. Practically: have P4's recovery call go through the same FFI entry point
Dart uses (or have Dart own all `startPull` calls, with Rust just flagging
`status=4` and letting Dart's existing pull logic pick it up on next reconcile).

### 5.3 P2 / P5 reduce the trigger frequency that §2.4 has to survive
mimo's P2 (`drain_in_progress` lock, avoid redundant recursive drains on
empty responses) and P5 (skip fast-poll drain if the last one was empty and
recent) both reduce how often mailbox drain fires while a chat is open. That
directly shrinks the number of times `_loadMessages()` gets triggered in the
first place. Worth landing P2/P5 *before or alongside* §2.4 (incremental
merge) — fewer redundant reload triggers makes §2.4 easier to verify, since
you won't be fighting drain noise while testing the diff logic.

### 5.4 P0 (read-receipt cutoff) — recommend using §2.2's `is_backfill` flag instead of a wall-clock window
mimo's own open Question 1 asks whether a 5-minute wall-clock cutoff is the
right signal. It isn't the best one available once §2.2 lands: a message
sitting in mailbox for 3 days while the recipient was offline is *genuinely
new to the recipient* the moment it's delivered, even though it's not "recent"
by wall clock — a flat cutoff would wrongly suppress the receipt for exactly
the case (long offline period, then reconnect) this whole mailbox system
exists to handle. `is_backfill` is a better signal because it's set at the
point of origin (drain/sync vs. live `ChatMessage`), not inferred from
timestamp: send a receipt whenever `is_backfill == false` regardless of
message age, and skip/batch receipts when `is_backfill == true` (those are
messages the local DB already reconciled, not first-time deliveries).
Recommend building P0 on top of §2.2 rather than shipping the cutoff-based
version first and reworking it.

### 5.5 No changes needed to P1 / P3
mimo's P1 (cleared-chat race) and P3 (sync_in_progress timeout /
max-iterations) don't intersect anything in this document — safe to implement
independently, in either order.

### 5.6 On mimo's Question 5 ("is file recovery worth the complexity?")
Given §5.1 shows P4 slots directly into this document's existing status-code
work with no extra schema cost, yes — it's worth doing as the same change.
Skipping it and only showing a "file unavailable" placeholder would leave the
transfer permanently unrecoverable with no automatic retry path, which is a
regression relative to what §2.1/2.5/2.6 are trying to build (a transfer that
either recovers or reaches a clean terminal state — not one that's just
declared dead on first sight of a gap).

### 5.7 Revised master order
1. Rust status-code migration: statuses 4 + 5 together (mimo P4 + this doc's §2.1).
2. §2.2 (`is_backfill` flag) — needed by both this doc's §2.3 and mimo's P0.
3. mimo P2 + P5 (drain efficiency / empty-drain dedup) — shrinks trigger noise.
4. §2.4 (Dart incremental merge in `_loadMessages`).
5. §2.3 (suppress new-message treatment for backfill) + mimo P0 (read receipts),
   built on `is_backfill` per §5.4.
6. §2.5 (persisted retry ceiling, shared with P4 per §5.2).
7. §2.6 (terminal UI state for status 5).
8. mimo P1 (cleared-chat race) and P3 (sync timeout) — independent, any time.
