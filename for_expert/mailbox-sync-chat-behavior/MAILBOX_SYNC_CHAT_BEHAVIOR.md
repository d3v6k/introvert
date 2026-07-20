# Mailbox, Chat Sync & Chat-Open Behavior — Expert Analysis

**Date:** 2026-07-19
**Codebase version:** v0.34.0 (main @ 07aedda)
**Purpose:** Complete documentation of the mailbox store-and-forward system, peer-to-peer chat synchronization, and what happens when a user opens a chat. For external expert review.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Mailbox System (Rust)](#2-mailbox-system-rust)
3. [Chat Sync Protocol (Rust)](#3-chat-sync-protocol-rust)
4. [Chat-Open Behavior (Flutter/Dart)](#4-chat-open-behavior-flutterdart)
5. [Message Delivery Flow](#5-message-delivery-flow)
6. [Drain & Refresh Mechanics](#6-drain--refresh-mechanics)
7. [Status Transitions](#7-status-transitions)
8. [Known Issues & Edge Cases](#8-known-issues--edge-cases)
9. [File Index](#9-file-index)

---

## 1. Architecture Overview

```
┌──────────────┐     MailboxStore      ┌──────────────────┐     MailboxDrain     ┌──────────────┐
│  Sender App  │ ─────────────────────> │  RBN Anchor Node │ <─────────────────── │ Receiver App │
│  (Flutter +  │                        │  (introvertd)    │ ───────────────────> │  (Flutter +  │
│   Rust FFI)  │ <── MailboxStored ACK  │  mailbox_messages│  MailboxDrained     │   Rust FFI)  │
└──────────────┘                        │  (SQLCipher)     │  (batch of 4)       └──────────────┘
                                        └──────────────────┘
```

**Two layers:**
- **Rust core** (`src/network/mod.rs`, `src/storage.rs`): handles all P2P networking, mailbox store/drain, sync protocol, message persistence.
- **Flutter/Dart UI** (`lib/views/chat_screen.dart`, `lib/src/ui/main_shell.dart`): triggers sync on chat open, loads messages from local DB, manages read receipts.

**Communication:** Dart calls Rust via FFI (`lib/src/native/introvert_client.dart`). Rust dispatches events back to Dart via event streams.

---

## 2. Mailbox System (Rust)

### 2.1 Purpose

The mailbox provides **store-and-forward** delivery for offline recipients. When a sender has a message for a peer that isn't directly connected, the message is stored on a connected RBN anchor node. When the recipient comes online, it drains the mailbox.

### 2.2 Data Types

**File:** `src/network/types.rs` (lines 85-100)

| Type | Fields | Purpose |
|------|--------|---------|
| `MailboxMessage` | `sender_id: String`, `payload: Vec<u8>` | Single stored message |
| `MailboxStore` | `recipient_id`, `payload`, `original_msg_id` | Request to store on RBN |
| `MailboxDrain` | _(empty)_ | Request to retrieve all pending messages |
| `MailboxDrained` | `Vec<MailboxMessage>` | Response with all drained messages |
| `MailboxStored` | `recipient_id`, `original_msg_id` | ACK to sender confirming storage |

### 2.3 Storage Schema

**File:** `src/storage.rs` (lines 176-185)

```sql
CREATE TABLE IF NOT EXISTS mailbox_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recipient_hash BLOB NOT NULL,      -- SHA-256 of PeerId, truncated to 16 bytes (zero-knowledge)
    sender_peer_id TEXT NOT NULL,
    encrypted_payload BLOB NOT NULL,
    ttl_expiry INTEGER NOT NULL,        -- unix timestamp, 7 days from storage
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_mailbox_recipient ON mailbox_messages(recipient_hash);
CREATE INDEX IF NOT EXISTS idx_mailbox_expiry ON mailbox_messages(ttl_expiry);
```

### 2.4 Storing a Message (RBN Side)

**File:** `src/network/mod.rs` (lines 5802-5860) — `MailboxStore` handler

When the RBN receives a `MailboxStore` payload:

1. **Anchor check:** Only anchor nodes process mailbox requests. Non-anchors ignore.
2. **Loopback protection** (line 5810-5815): If recipient == self, unwrap inner payload and process locally.
3. **Persist:** `storage.store_mailbox_payload(recipient, sender, payload)` — computes SHA-256 hash of recipient PeerId (first 16 bytes) for zero-knowledge indexing. Sets TTL = now + 7 days.
4. **ACK:** Sends `MailboxStored { recipient_id, original_msg_id }` back to sender.
5. **Push notification** (lines 5830-5848): Looks up recipient's FCM push token. If found, sends wake-up POST to `https://push.introvert.network/wakeup`.
6. **Proactive delivery** (lines 5852-5858): If recipient is already connected to this RBN, immediately drains and delivers via `MailboxDrained`.

### 2.5 Draining Messages (RBN Side)

**File:** `src/network/mod.rs` (lines 7527-7535) — `MailboxDrain` handler

When the RBN receives a `MailboxDrain` request:
1. Calls `storage.drain_mailbox(peer)` — atomic transaction: SELECT up to 4 messages, DELETE them, return.
2. Sends `MailboxDrained(messages)` back to the requesting peer.

**Why LIMIT 4?** To stay under the 1MB libp2p request-response limit. If more messages exist, the client re-drains after 500ms (recursive drain, line 5895-5901).

### 2.6 Receiving Drained Messages (Client Side)

**File:** `src/network/mod.rs` (lines 5862-5902) — `MailboxDrained` handler

For each message in the drained batch:
1. **Dedup** (line 5870): Skip if `msg_id` already exists in local storage (`message_exists`).
2. **File filter** (line 5877): Skip `[FILE]:` metadata messages (file transfers use their own delivery path).
3. **Clear guard** (line 5883): Skip if `storage.should_skip_mailbox_message(sender, timestamp)` returns true (message predates chat clear).
4. Push surviving messages into the normal processing queue.
5. **Recursive drain** (lines 5895-5901): If received > 0 messages, schedule another `FetchMailbox` after 500ms.

### 2.7 Drain Cooldowns

| Cooldown | Duration | Location | Purpose |
|----------|----------|----------|---------|
| Fast mailbox drain | 5 seconds | `mod.rs:465-471` (fast_poll_interval, 1s tick) | Rapid drain when relay connected |
| General mailbox drain | 30 seconds | `mod.rs:3512-3536` (perform_mailbox_fetch) | Prevent spam on anchors |
| Chunk drain | 250ms | `mod.rs:2009-2042` (OutboundCircuitEstablished) | Flush pending file chunks |
| Periodic drain | 120s (anchor) / 300s (regular) | `mod.rs:386` (mailbox_fetch_interval) | Background maintenance |

### 2.8 Cleared Chats Protection

**File:** `src/storage.rs` (lines 357-363, 972-1008)

```sql
CREATE TABLE cleared_chats (
    peer_id TEXT PRIMARY KEY,
    cleared_at DATETIME
);
```

When a user clears a chat (`delete_chat`, line 972):
1. All messages for the peer are deleted.
2. Local mailbox entries for the peer are deleted.
3. `cleared_chats` is updated with the current timestamp.

When a mailbox-drained message arrives, `should_skip_mailbox_message(sender, timestamp)` checks if the message timestamp is older than `cleared_at`. If so, it's silently dropped — preventing old messages from reappearing after a clear.

Cleanup: entries older than 7 days are pruned (`cleanup_cleared_chats`, line 1011).

### 2.9 Pending File Chunks

**File:** `src/storage.rs` (lines 329-355)

```sql
CREATE TABLE pending_file_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    transfer_id TEXT NOT NULL,
    peer_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    chunk_data BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    retry_count INTEGER DEFAULT 0,
    in_flight_since INTEGER DEFAULT 0,
    connection_id TEXT,
    UNIQUE(transfer_id, chunk_index)
);
```

Chunks are enqueued when a relay circuit isn't available, and dequeued atomically when one becomes available. Stale in-flight chunks (older than threshold) are released for retry. Auto-deleted after 5 retries.

---

## 3. Chat Sync Protocol (Rust)

### 3.1 Purpose

Chat sync allows two peers to reconcile their message histories when they come back online. Each peer sends the IDs of messages it already has; the other peer responds with what's missing.

### 3.2 Types

**File:** `src/network/types.rs` (lines 184-197, 268-275)

| Type | Fields | Purpose |
|------|--------|---------|
| `ChatSyncRequest` | `chat_id`, `is_group`, `known_msg_ids: Vec<String>`, `limit: u32` | "Here's what I have" |
| `ChatSyncResponse` | `chat_id`, `is_group`, `messages: Vec<SyncMessage>`, `missing_ids: Vec<String>`, `is_relay` | "Here's what you're missing" |
| `SyncMessage` | `msg_id`, `sender_id`, `content`, `timestamp`, `reply_to` | Single message in sync response |

### 3.3 Initiating Sync

**File:** `src/network/mod.rs` (lines 5034-5078) — `SyncChatMessages` command

1. **Lock check** (line 5036): If `sync_in_progress` contains this `chat_id`, skip (prevent duplicate syncs).
2. Insert `chat_id → Instant::now()` into `sync_in_progress`.
3. Collect known message IDs from local DB (last 100 for normal sync, empty for full sync).
4. Send `ChatSyncRequest { known_msg_ids, limit: 100 }` via `forward_to_mesh`.

### 3.4 Handling Sync Request

**File:** `src/network/mod.rs` (lines 6342-6419)

1. Load all messages from local DB for the requested `chat_id`.
2. Compute `missing_on_peer` = our messages NOT in their `known_msg_ids`.
3. Compute `missing_on_us` = their `known_msg_ids` NOT in our DB.
4. Filter out `[FILE]:` messages (file transfers have separate delivery).
5. Send `ChatSyncResponse { messages: missing_on_peer, missing_ids: missing_on_us }`.

### 3.5 Handling Sync Response

**File:** `src/network/mod.rs` (lines 6421-6563)

1. **Authorization** (lines 6425-6444): Verify sender is a group member or direct chat peer.
2. **Store** received messages using `store_message_if_new` (INSERT OR IGNORE — never overwrites existing).
3. **Recursive sync** (lines 6474-6490): If received 100 messages (full batch), schedule another `SyncChatMessages` after 500ms.
4. **Group relay** (lines 6492-6503): For group chats, relay received messages to other connected group members.
5. If peer sent `missing_ids`, send back those messages too.
6. Remove `chat_id` from `sync_in_progress`.

### 3.6 sync_in_progress Lock

**File:** `src/network/service.rs` (line 192)

```rust
sync_in_progress: HashMap<String, Instant>
```

Maps `chat_id` → when the sync started. Entries older than 60 seconds are automatically evicted (`mod.rs:1190-1191`) to prevent permanent locks from crashed syncs.

### 3.7 When Sync Is Triggered

| Trigger | Location | Type |
|---------|----------|------|
| Chat opened (Flutter) | `chat_screen.dart:781-793` | Normal sync (100 msg IDs) |
| App foreground | `main_shell.dart:318-344` | Triggers IntroClaw tick (may trigger sync) |
| Relay reservation accepted | `mod.rs:1926-1967` | Immediate mailbox fetch |
| Identify with anchor | `mod.rs:1880-1921` | Immediate mailbox drain |
| Manual refresh | `introvert_client.dart:1835` | FFI FetchMailbox command |

---

## 4. Chat-Open Behavior (Flutter/Dart)

### 4.1 initState Sequence

**File:** `lib/views/chat_screen.dart` (lines 739-794)

When a user taps on a chat, `ChatScreen.initState()` executes this sequence:

```
1. setActiveChat(peerId)           → tells IntroClaw this chat is active
2. _loadProfile()                  → loads own avatar
3. _loadPeerTier()                 → loads peer's prestige tier
4. _loadMessages()                 → loads messages from local SQLite (paginated, 100/page)
5. _markMessagesAsRead()           → clears unread count + sends read receipts
6. _startNetworkDiscovery()        → establishes secure session + IntroClaw recon
7. _startEconomyMonitor()          → subscribes to economy stream
8. Start 30s stall retry timer     → retries stalled file pulls
9. Start auto-sync                 → syncs with peer, waits 5s, reloads, scrolls to bottom
```

### 4.2 setActiveChat

**File:** `lib/src/native/introvert_client.dart` (lines 1938-1949)

FFI call to Rust's `NetworkCommand::IntroClawSetActiveChat { chat_id, peer_id, is_group }`. This tells the IntroClaw engine to:
- Bypass cooldowns for this peer
- Aggressively attempt DCUtR (hole-punch) for relayed connections
- Proactively heal offline targets on every tick

### 4.3 _loadMessages

**File:** `lib/views/chat_screen.dart` (lines 860-1005)

1. Calls `_client.getMessagesPaginated(peerId, offset: 0, limit: 100)`.
2. For each raw message:
   - Parses content — detects `[FILE]:` metadata and creates `FileTransferProgress`.
   - Checks if file exists locally (Sovereign Drive fallback).
   - For incoming incomplete files, triggers `startPull` if not already requested.
3. Pre-fetches reactions for all messages into `_reactionsCache`.
4. Updates `_messages` list and increments `_messagesVersion` (triggers rebuild).

### 4.4 _markMessagesAsRead

**File:** `lib/views/chat_screen.dart` (lines 829-847)

1. `_client.updateMessageStatusForPeer(peerId, 0)` — sets all incoming messages for this peer to status=0 (read) in the local DB.
2. Iterates all messages: for each incoming message with status != 0, sends a read receipt:
   - `_client.sendAcknowledgement(peerId, msgId, 2)` — status 2 = read.

### 4.5 _startNetworkDiscovery

**File:** `lib/views/chat_screen.dart` (lines 2204-2342)

1. `_client.startNetwork()` — idempotent network start.
2. `_client.establishSecureSession(peerId)` — Noise IK handshake.
3. `_runIntroClawRecon()` — triggers full IntroClaw tick + network recon.
4. Subscribes to network event stream for real-time updates:
   - **Event 8:** Peer connection status → updates UI ("Direct P2P" / "Relay Active").
   - **Event 10:** Network quality → on weak signal, triggers `forceNetworkRefresh()`.
   - **Event 2/4:** Incoming messages → adds to UI, sends read receipts.
   - **Event 12:** File transfer progress.
   - **Event 23:** Chat sync completion → reloads messages.
   - **Event 25:** Peer profile update.

### 4.6 Chat Dispose

**File:** `lib/views/chat_screen.dart` (lines 797-810)

1. `_client.clearActiveChat()` — tells IntroClaw the chat is no longer active.
2. Cancels all subscriptions and timers.

---

## 5. Message Delivery Flow

### 5.1 Complete Path: Send → Display

```
┌─ SENDER ──────────────────────────────────────────────────────────────────────┐
│ 1. User types message, taps send                                             │
│ 2. chat_screen._sendMessage() → FFI sendMessage(peerId, payload, replyToId)  │
│ 3. Rust: store_message_with_id(is_me=true, status=0)                         │
│ 4. Rust: forward_to_mesh(peer_id, ChatMessage{content, msg_id, timestamp})   │
│                                                                               │
│    ┌─ forward_to_mesh routing decision ──────────────────────────────┐       │
│    │ IF direct connection exists → send via request-response codec   │       │
│    │ ELSE IF relay connection → send via relay circuit               │       │
│    │ ELSE IF verified RBN connected → MailboxStore on RBN            │       │
│    │ ELSE → queue in pending_messages RAM buffer + try dial          │       │
│    └─────────────────────────────────────────────────────────────────┘       │
│ 5. On MailboxStored ACK: status → 3 ("In Mailbox")                           │
└───────────────────────────────────────────────────────────────────────────────┘
         │
         ▼ (P2P mesh network)
┌─ RECEIVER ────────────────────────────────────────────────────────────────────┐
│ 6. ChatMessage handler (mod.rs:5988-6043):                                   │
│    a. Privacy gate: verify sender is a verified contact                       │
│    b. store_message(is_me=false, status=1)                                   │
│    c. Send Acknowledgement{msg_id, status:1} back to sender                  │
│    d. Record daily reward activity                                            │
│    e. Dispatch Event 2 to Flutter UI                                         │
│ 7. Flutter: Event 2 handler → add message to _messages list → UI rebuilds    │
│ 8. If chat is open: _markMessagesAsRead() sends read receipt (status:2)      │
└───────────────────────────────────────────────────────────────────────────────┘
         │
         ▼ (ACK path)
┌─ SENDER (continued) ──────────────────────────────────────────────────────────┐
│ 9.  On ACK status=1: message status → 1 ("Delivered")                        │
│ 10. On read receipt status=2: message status → 2 ("Read")                    │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Undelivered Message Retry

**File:** `src/network/mod.rs` (lines 943-962, 1193-1211)

Every 15 seconds (status_check_interval) and every 300 seconds (mailbox_fetch_interval):
1. `storage.fetch_undelivered_messages(60)` — finds messages with `status=0` older than 60 seconds.
2. For each, if recipient is currently connected, re-sends via `forward_to_mesh`.

**File:** `src/storage.rs` (lines 1528-1551):
```sql
SELECT * FROM messages
WHERE is_me = 1 AND status = 0 AND msg_id IS NOT NULL
AND timestamp < (now - age_secs)
```

---

## 6. Drain & Refresh Mechanics

### 6.1 Main Event Loop Priorities

**File:** `src/network/mod.rs` (lines 402-1335)

The `tokio::select!` loop uses `biased;` priority:

| Priority | Timer | Interval | Purpose |
|----------|-------|----------|---------|
| 1 | Commands | immediate | Process all pending FFI commands first |
| 2 | Heartbeat | 30s (anchor) / 300s (regular) | Broadcast heartbeat |
| 3 | Fast poll | 1s | Flush pending messages, 5s fast mailbox drain |
| 4 | Pull retry | 1s | Stall detection, chunk re-request |
| 5 | Status check | 15s | Stale cleanup, reconnect ladder, undelivered retry |
| 6 | Fast reconnect | 5s | Aggressive reconnect when transfers waiting |
| 7 | Telemetry | 30min | Daily rewards telemetry |
| 8 | Mailbox fetch | 120s (anchor) / 300s (regular) | Full drain + pending flush |
| 9 | Lease | 1hr | Token lease renewal |
| 10 | IntroClaw | 5min | Connection optimization |
| 11 | Republication | 60s (anchor) / 300s (regular) | Kademlia DHT republish |
| 12 | Anchor discovery | 5min | Discover new anchors |
| 13 | Swarm events | lowest | Process libp2p events last |

### 6.2 Triggers That Cause Immediate Mailbox Drain

| Trigger | Location | What Happens |
|---------|----------|--------------|
| Relay reservation accepted | `mod.rs:1926-1967` | Immediate `perform_mailbox_fetch()` + flush pending queues |
| Identify from anchor | `mod.rs:1880-1921` | Immediate `MailboxDrain` + flush non-chunk pending |
| App foreground (Dart) | `main_shell.dart:318-344` | `setAppIdleState(false)` + `triggerIntroClawTick()` + reload contacts |
| Engine start | `main_shell.dart:705-714` | `fetchMailbox()` after 2-second delay |
| Fast poll (1s tick) | `mod.rs:465-471` | If relay connected and 5s elapsed since last fast drain |
| Connectivity change | `connectivity_listener.dart:1-44` | `setConnectivityType()` → triggers re-dial + reconnect |
| Manual pull-to-refresh | `introvert_client.dart:1835` | FFI `FetchMailbox` command |

### 6.3 Background Sync

**File:** `lib/src/services/background_sync_service.dart` (lines 1-92)

- If FCM push is available: no polling (push handles wake-ups).
- If push unavailable: 2-minute fallback polling via `fetchMailbox()`.
- `enterIdleMode()`: cancels fallback timer (app is backgrounded).
- `exitIdleMode()`: restarts fallback polling if push unavailable.

---

## 7. Status Transitions

### 7.1 Message Status Codes

| Code | Meaning | Direction |
|------|---------|-----------|
| 0 | Sending (outgoing) / Read (incoming, after read receipt) | Both |
| 1 | Delivered (ACK from recipient) | Outgoing |
| 2 | Read (read receipt from recipient) | Outgoing |
| 3 | In Mailbox (stored on RBN) | Outgoing |

### 7.2 Allowed Transitions

**File:** `src/storage.rs` (lines 1377-1398) — `update_message_status_if_higher`

```
0 → 3  (sent → in mailbox)
0 → 1  (sent → delivered, direct path)
0 → 2  (sent → read, direct path)
3 → 1  (in mailbox → delivered)
3 → 2  (in mailbox → read)
1 → 2  (delivered → read)
```

Monotonic: new_status must be > current_status. Prevents status regression.

---

## 8. Known Issues & Edge Cases

### 8.1 File Messages Excluded from Sync

`[FILE]:` metadata messages are filtered out of both mailbox drain and chat sync responses. File transfers use their own delivery pipeline (gossipsub topics or request-response). If a file transfer message is synced but the actual file data wasn't delivered, the message appears but the file is missing.

### 8.2 Recursive Drain Batch Size

Mailbox drain returns max 4 messages per call (1MB libp2p limit). If more exist, the client re-drains after 500ms. This means a user with 20+ pending messages will see them arrive in waves.

### 8.3 sync_in_progress Lock Timeout

The 60-second timeout on `sync_in_progress` entries prevents permanent locks from crashed syncs, but if a sync genuinely takes > 60 seconds (e.g., very large chat history), a duplicate sync could start.

### 8.4 Cleared Chat Race Condition

If a user clears a chat while mailbox drain is in flight, messages that were already fetched but not yet processed will bypass the clear guard (they're in RAM, not re-fetched from DB). The clear guard only checks messages coming from the mailbox drain path.

### 8.5 Read Receipts on Chat Open

When a chat is opened, `_markMessagesAsRead()` sends read receipts for ALL incoming messages, even very old ones. This could cause unnecessary network traffic for chats with large histories.

### 8.6 Proactive Delivery Window

The RBN's proactive delivery (deliver immediately if recipient is connected) creates a window where messages arrive before the client has called `MailboxDrain`. The client handles this correctly via dedup, but it means the drain response may be empty if all messages were already proactively delivered.

---

## 9. File Index

### Source Files (Copied)

| File | Lines | Purpose |
|------|-------|---------|
| `src/network/mod.rs` | ~8270 | Core network loop, all mailbox/sync handlers, forward_to_mesh, event loop |
| `src/network/service.rs` | ~257 | NetworkService struct (sync_in_progress, TransferRouter, state) |
| `src/network/types.rs` | ~422 | All network types (MailboxStore, ChatSyncRequest, etc.) |
| `src/storage.rs` | ~3324 | SQLite/SQLCipher operations (mailbox_messages, pending_file_chunks, cleared_chats) |
| `lib/views/chat_screen.dart` | ~2500+ | Chat UI, initState sequence, message loading, read receipts |
| `lib/src/native/introvert_client.dart` | ~2000+ | FFI bridge (setActiveChat, fetchMailbox, syncChatMessages) |
| `lib/src/services/background_sync_service.dart` | ~92 | Background polling when push unavailable |
| `lib/src/ui/main_shell.dart` | ~3500+ | App lifecycle (foreground/background), connectivity handling |
| `lib/connectivity_listener.dart` | ~44 | Network change detection |

### Key Line References

| Topic | File | Lines |
|-------|------|-------|
| MailboxStore handler (RBN) | `src/network/mod.rs` | 5802-5860 |
| MailboxDrained handler (client) | `src/network/mod.rs` | 5862-5902 |
| MailboxDrain handler (RBN) | `src/network/mod.rs` | 7527-7535 |
| MailboxStored ACK | `src/network/mod.rs` | 7545-7553 |
| perform_mailbox_fetch | `src/network/mod.rs` | 3512-3536 |
| 5s fast drain | `src/network/mod.rs` | 465-471 |
| 250ms chunk drain | `src/network/mod.rs` | 2009-2042 |
| mailbox_messages schema | `src/storage.rs` | 176-185 |
| store_mailbox_payload | `src/storage.rs` | 1136-1149 |
| fetch_mailbox_payloads | `src/storage.rs` | 1164-1191 |
| pending_file_chunks schema | `src/storage.rs` | 329-355 |
| cleared_chats schema | `src/storage.rs` | 357-363 |
| delete_chat + clear guard | `src/storage.rs` | 972-1008 |
| ChatSyncRequest handler | `src/network/mod.rs` | 6342-6419 |
| ChatSyncResponse handler | `src/network/mod.rs` | 6421-6563 |
| SyncChatMessages command | `src/network/mod.rs` | 5034-5078 |
| sync_in_progress lock | `src/network/service.rs` | 192 |
| ChatScreen initState | `lib/views/chat_screen.dart` | 739-794 |
| setActiveChat FFI | `lib/src/native/introvert_client.dart` | 1938-1949 |
| _loadMessages | `lib/views/chat_screen.dart` | 860-1005 |
| _markMessagesAsRead | `lib/views/chat_screen.dart` | 829-847 |
| _startNetworkDiscovery | `lib/views/chat_screen.dart` | 2204-2342 |
| ChatMessage handler (receive) | `src/network/mod.rs` | 5988-6043 |
| Status transitions | `src/storage.rs` | 1377-1398 |
| Undelivered retry | `src/network/mod.rs` | 943-962, 1193-1211 |
| Main event loop | `src/network/mod.rs` | 402-1335 |
| App foreground | `lib/src/ui/main_shell.dart` | 318-344 |
| BackgroundSyncService | `lib/src/services/background_sync_service.dart` | 1-92 |
| ConnectivityListener | `lib/connectivity_listener.dart` | 1-44 |
| forward_to_mesh | `src/network/mod.rs` | 3390-3509 |
| ReservationReqAccepted drain | `src/network/mod.rs` | 1926-1967 |
| Identify anchor drain | `src/network/mod.rs` | 1880-1921 |
