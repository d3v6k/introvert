# Release Notes: v51 — "Cross-Network Success & Sync Integrity"
**Date:** July 1, 2026
**Version:** `0.21.4`
**Predecessor:** v50 (`0.21.3`)

---

## Executive Summary

**Milestone achieved: Cross-network file transfers verified working on VPN and mobile data.**

v51 addresses sync integrity issues and completes the cross-network delivery story:

1. **Cross-Network File Transfers Working**: Verified on VPN and mobile data connections. Files transfer reliably across different networks via relay circuits.
2. **Chat List Sorting Fixed**: Changed from insertion order (`MAX(id)`) to chronological order (`MAX(timestamp)`). Old sync'd messages no longer bubble to top of chat list.
3. **Mailbox Re-Delivery Prevention**: New `cleared_chats` table tracks when chats are cleared. Mailbox drain skips messages from before the clear timestamp.
4. **Mailbox Drain Deduplication**: Three guards prevent re-processing: (1) message already exists, (2) file metadata filter, (3) cleared-chat timestamp check.
5. **File Transfer Timestamps**: `FileTransferBubble` now displays HH:MM below status text, matching sticker/voice memo style.
6. **Create/Join Group Dialogs Fixed**: Uses parent widget context instead of invalidated bottom sheet context.

---

## Networking Milestone

### Cross-Network File Transfers — VERIFIED WORKING

After months of debugging, cross-network file transfers now work reliably:

- **Different networks**: Mac WiFi ↔ Android mobile data ✅
- **VPN**: Mac WiFi ↔ Android with VPN active ✅
- **Mobile data**: Mac WiFi ↔ Android on cellular ✅

Key enablers:
- Relay reservation three-tier fallback (Gemini fix) — prevents VPC private IP leakage
- VPN stale reservation detection at 30s intervals
- DCUtR hole-punching for direct connection upgrades
- Persistent file chunk queue for app restart resilience
- ALL RBNs iterated for file chunks (no early break)

---

## Changes

### Storage
- New `cleared_chats` table — tracks `peer_id` + `cleared_at` timestamp
- `delete_chat` now clears local mailbox entries and records clear timestamp
- `message_exists()` — O(1) dedup check for mailbox-drained messages
- `should_skip_mailbox_message()` — compares message timestamp against clear timestamp
- `cleanup_cleared_chats()` — prunes entries older than 7 days
- `get_last_messages_all()` / `get_last_group_messages_all()` — sorted by `MAX(timestamp)` not `MAX(id)`

### Networking
- `ClearMailboxForPeer` command — triggers proactive mailbox drain after chat clear
- `MailboxDrained` handler — three guards: dedup, [FILE] filter, cleared-chat timestamp check
- Recursive drain delay increased from 200ms to 500ms

### UI
- `FileTransferBubble` — `timestamp` parameter added, HH:MM displayed below status text
- Chat list sorting — `_lastMessageTimestamps` map, contacts/groups sorted by timestamp
- Create/Join Group dialogs — `_showCreateGroupDialog()` / `_showJoinGroupDialog()` helper methods

---

## Files Modified

| File | Changes |
|------|---------|
| `src/storage.rs` | `cleared_chats` table, `message_exists()`, `should_skip_mailbox_message()`, `cleanup_cleared_chats()`, `delete_chat` updated, `get_last_messages_all`/`get_last_group_messages_all` sorted by timestamp |
| `src/network/mod.rs` | `MailboxDrained` handler with dedup/FILE/clear guards, `ClearMailboxForPeer` handler |
| `src/network/types.rs` | `ClearMailboxForPeer` command variant |
| `src/lib.rs` | `delete_chat` FFI sends `ClearMailboxForPeer` command |
| `for_linux/src/network/mod.rs` | `ClearMailboxForPeer` command variant + handler (RBN no-op) |
| `lib/src/ui/main_shell.dart` | `_lastMessageTimestamps`, chat list sorting, `_showCreateGroupDialog()`, `_showJoinGroupDialog()` |
| `lib/src/ui/widgets/file_transfer_bubble.dart` | `timestamp` parameter, HH:MM display |
| `lib/views/chat_screen.dart` | Pass `timestamp: msg.startDateTime` to `FileTransferBubble` |
| `lib/views/group_chat_screen.dart` | Pass `timestamp: ts` to `FileTransferBubble` |
| `pubspec.yaml` | Version bumped to 0.21.4 |

---

## Build & Deploy

```bash
cargo check             # Verify both trees compile
make mac                # Rebuild macOS client
make android            # Rebuild Android client
./deploy_rbn.sh         # Deploy RBN (if needed)
```

## How to Test

1. **Cross-network file transfer**: Mac on WiFi, Android on mobile data → send image → arrives
2. **VPN file transfer**: Mac on WiFi, Android with VPN → send image → arrives
3. **Chat list stability**: Send messages, sync, verify chat list doesn't rearrange
4. **Chat clear + mailbox**: Clear chat, wait 10+ minutes, verify old messages don't reappear
5. **File transfer timestamps**: Send image, verify HH:MM shows below status text
6. **Create/Join Group**: Tap "Create Sovereign Group" and "Join Sovereign Group" — dialogs should open
