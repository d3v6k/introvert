# Introvert Group Chat — Deep Audit Report & Rectification Plan

**Date:** 2026-06-26  
**Audit Type:** Full correctness, security, and stability review of Introvert group chats  
**Scope:** Cryptography, database storage, network protocol event loop, message sending path, and Flutter UI layers.  
**Auditor:** Antigravity AI (Pair Programming Auditor)

---

## 1. Executive Summary

A comprehensive deep audit of the group chat implementation in Introvert has been completed. The audit identified crucial defects causing group chats to stop functioning reliably (or at all in certain scenarios, such as fresh app installations) and exposed security vulnerabilities in invite handling, message deduplication, and state synchronization.

We categorized the findings into four main domains:
1. **Database & Storage Core**
2. **Network Protocol & Event Loop**
3. **Message Send Path & FFI**
4. **Flutter UI Layer**

A total of **16 issues** have been uncovered. Below is a summary of findings by severity, followed by a detailed breakdown of each issue and our concrete **Rectification Plan**.

### Finding Severity Summary
*   **CRITICAL (4)**: Table `pending_group_invites` is missing from database schema bootstrap (causing silent sqlite errors on fresh installs); Pending invite poisoning via secret swap; Dynamic gossipsub subscription missing for new group creators; Unbounded in-memory `seen_group_messages` / Deduplication completely unwired.
*   **HIGH (6)**: `get_group` non-atomic double-lock race (TOCTOU); No timestamp or replay checks on group actions; Message ownership (delete/edit) permission checks unimplemented; Group invite auto-accept ignores group deletion status (tombstones); UI group invite/create list reload race condition; UI missing `mounted` guards after async operations.
*   **MEDIUM (4)**: `pending_requester_static_keys` memory leak; gossipsub subscription skipped in early-return auto-accept path; Group secret FFI send-path missing all-zeros validation; `heal_rate_limiter` in-memory leak.
*   **LOW (2)**: `my_peer_id` variable shadowing; Group chat has no empty-state UI placeholder.

---

## 2. Detailed Audit Findings

### Domain A: Database & Storage Core

#### 1. Table `pending_group_invites` Missing from DDL Bootstrap (CRITICAL)
*   **File:** `src/storage.rs` (in `bootstrap()`)
*   **Description:** The table `pending_group_invites` is never created in `bootstrap()` or migrations. Any query to store or load pending invites fails with a database SQLite error. Because errors are ignored at callsites with `let _ = ...`, this failure is completely silent. Fresh installations can never successfully process group invites.
*   **Rectification Plan:** Add `CREATE TABLE IF NOT EXISTS pending_group_invites` DDL to `bootstrap()` and add a corresponding schema upgrade check.

#### 2. Pending Invite Poisoning / TOCTOU Secret Swap (CRITICAL)
*   **File:** `src/storage.rs` (`store_pending_invite`), `src/network/mod.rs` (`GroupInvite` handler)
*   **Description:** `store_pending_invite` uses `ON CONFLICT(group_id) DO UPDATE` which silently replaces the wrapped secret. A malicious node knowing a target group ID can send a spoofed group invite wrapping a custom/malicious group secret. When the user accepts the invite, they unwrap and store the attacker-controlled secret instead of the real group secret.
*   **Rectification Plan:** Verify that the sender of the group invite is a trusted contact (and optionally signature-checked). Change the SQL update constraint to ignore duplicate incoming invites if already pending, or reject new invite values if they conflict with an existing pending invite from a different peer.

#### 3. `get_group` Dual-Lock Race (HIGH)
*   **File:** `src/storage.rs` (`get_group`)
*   **Description:** `get_group` queries the `groups` table, releases the SQLite connection mutex, and then queries the `group_secrets` table. A concurrent thread can delete or update the group secret between these two operations, leading to an inconsistent state (returning a group structure with an all-zeros secret `[0u8; 32]`).
*   **Rectification Plan:** Consolidate the two queries into a single SQL query using a `LEFT JOIN` on `groups` and `group_secrets`, executed under a single SQLite mutex lock hold.

#### 4. `save_group_secret` Lacks Slice Length Validation (MEDIUM)
*   **File:** `src/storage.rs` (`save_group_secret`)
*   **Description:** `save_group_secret` accepts a raw slice `&[u8]`, but the system requires exactly a 32-byte secret. If a bad length is saved, the loader silently falls back to zeros.
*   **Rectification Plan:** Enforce `&[u8; 32]` or validate input length strictly, returning a database error if the length is incorrect.

---

### Domain B: Network Protocol & Event Loop

#### 5. Gossipsub Topic Subscription Missing for Group Creator (CRITICAL)
*   **File:** `src/lib.rs` (in `introvert_group_create`)
*   **Description:** When a group is created, the creator sends invites and saves the group, but fails to subscribe to the group's gossipsub topic in the active session. The creator remains blind to gossipsub messages (like file transfers) until the application is restarted and the startup handler resubscribes.
*   **Rectification Plan:** Send a subscription command (`NetworkCommand::SubscribeGossipsub`) to the network service immediately after group creation.

#### 6. Auto-Accept Path Ignores Tombstone / Group Deleted Status (HIGH)
*   **File:** `src/network/mod.rs` (`GroupInvite` handler)
*   **Description:** The auto-accept invite path checks if the group exists, but does not check if `is_group_deleted` is true. If a user previously left/deleted a group (which writes an all-zeros secret and marks it tombstoned), they will automatically and silently rejoin the group if any other member sends another invite.
*   **Rectification Plan:** Verify `!self.storage.is_group_deleted(&group_id)` before triggers in the auto-accept block.

#### 7. No Timestamp or Replay Checks in `verify_action` (HIGH)
*   **File:** `src/network/group.rs` (`verify_action`)
*   **Description:** `SignedGroupAction` carries a timestamp, but `verify_action` never validates it. Group action payloads can be replayed indefinitely (e.g. re-elevating a peer to admin after revocation, or re-deleting a group).
*   **Rectification Plan:** Reject actions where the timestamp is more than 300 seconds in the past, or keep track of action sequence numbers/nonces per group member.

#### 8. Edit/Delete Message Ownership Checks Unimplemented (HIGH)
*   **File:** `src/network/group.rs` (`verify_action`)
*   **Description:** The network handler defers checking ownership of edited or deleted group messages with a comment, allowing any member to edit or delete any other member's messages.
*   **Rectification Plan:** Verify that the signer of the Edit/Delete action is the original sender of the target message, or an admin of the group.

#### 9. Gossipsub Subscription Skipped in Auto-Accept Path (MEDIUM)
*   **File:** `src/network/mod.rs` (`GroupInvite` handler)
*   **Description:** When an invite is auto-accepted, the function saves the secret and returns early, completely skipping the gossipsub subscription block.
*   **Rectification Plan:** Perform the gossipsub subscription inside the auto-accept block prior to returning.

#### 10. `pending_requester_static_keys` Memory Leak (MEDIUM)
*   **File:** `src/network/mod.rs`
*   **Description:** Peer public keys are stored in `pending_requester_static_keys` when join requests arrive, but they are never evicted if the join request is rejected, ignored, or if the peer disconnects.
*   **Rectification Plan:** Implement cleanup in `RejectGroupJoin` and connection teardown.

#### 11. `heal_rate_limiter` In-Memory Accumulation Leak (MEDIUM)
*   **File:** `src/network/mod.rs`
*   **Description:** The rate limiter uses an in-memory `HashMap` to throttle healing requests, but old entries are never pruned, causing unbounded accumulation in long-running nodes.
*   **Rectification Plan:** Periodically prune rate limiter entries older than their timeout during the liveness tick.

---

### Domain C: Message Send Path & FFI

#### 12. Unwired/Unbounded `seen_group_messages` Deduplication (CRITICAL)
*   **File:** `src/network/service.rs`, `src/network/mod.rs`
*   **Description:** `seen_group_messages` is declared as a `HashSet<String>` but is never populated or checked in the message reception path, meaning group message deduplication is completely non-functional.
*   **Rectification Plan:** Implement a size-bounded message cache or LRU set to store and filter duplicate message IDs in the network event loop.

#### 13. Send Path Lacks Non-Zero Secret Validation (MEDIUM)
*   **File:** `src/lib.rs` (`introvert_group_send_message`)
*   **Description:** The FFI message encryption step does not check if the retrieved group secret is all-zeros, which could cause a node to encrypt messages with a null key.
*   **Rectification Plan:** Add a check to return an error if the secret is all-zeros.

#### 14. Non-Unique and Deterministic `msg_id` (MEDIUM)
*   **File:** `src/lib.rs` (`introvert_group_send_message`)
*   **Description:** Message IDs are built purely using the group ID and the current nanosecond timestamp. This is deterministic and susceptible to collisions if multiple messages are generated in the same nanosecond slot, which can overwrite rows in SQLite.
*   **Rectification Plan:** Incorporate secure random bytes (e.g. 4 bytes of randomness) into the message ID suffix.

---

### Domain D: Flutter UI Layer

#### 15. UI Database Reload Race Conditions (HIGH)
*   **File:** `lib/src/ui/main_shell.dart`
*   **Description:** Immediately after calling FFI commands to accept an invite or create a group, the UI executes list reloads. Because the SQLite transaction inside Rust is processed asynchronously, the reload query runs before the write commits, causing the new group to be missing from the UI list until the next tick.
*   **Rectification Plan:** Introduce a brief, non-blocking delay (e.g. 500-600ms) before invoking UI refreshes, or await the completion of the FFI operations.

#### 16. Missing `mounted` Guards in Async Flutter Blocks (HIGH)
*   **File:** `lib/views/group_chat_screen.dart`
*   **Description:** In several asynchronous picker pathways (image, video, file, and location pickers) as well as confirmation dialogs, `setState` or message-reloads are triggered without checking if the widget is still `mounted`. This can lead to framework crashes.
*   **Rectification Plan:** Add `if (!mounted) return;` statements before calling state mutators.

---

## 3. Rectification Plan

To address these vulnerabilities and functionality gaps, we will execute the following steps in sequence:

1.  **Database Core Updates (`src/storage.rs`)**:
    *   Inject the `pending_group_invites` table DDL into the `bootstrap()` function.
    *   Refactor `get_group` to use a `LEFT JOIN` on `groups` and `group_secrets` within a single lock capture block.
    *   Change `store_pending_invite` to secure against invite swaps (e.g., use `INSERT OR IGNORE` or validation).
2.  **Network Engine Updates (`src/network/mod.rs` & `src/network/service.rs`)**:
    *   Fix the `auto_accept` tombstone check.
    *   Subscribe to gossipsub on auto-accept and group creation.
    *   Wire up deduplication checking using a size-limited LRU pattern on `seen_group_messages`.
    *   Add eviction policies for `pending_requester_static_keys` and `heal_rate_limiter`.
3.  **FFI Message Layer (`src/lib.rs` & `src/network/group.rs`)**:
    *   Enforce non-zero checks on the group secret in the send pathway.
    *   Make `msg_id` unique by adding random bytes.
    *   Add `NetworkCommand::SubscribeGossipsub` event loop command and wire it up.
4.  **Flutter UI Adjustments (`lib/`)**:
    *   Add refresh delays after FFI writes.
    *   Inject `mounted` checks after every async gap.
    *   Bind `type == 23` to refresh both contacts/members and message history.
