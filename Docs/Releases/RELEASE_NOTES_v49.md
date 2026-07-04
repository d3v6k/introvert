# Release Notes: v49 — "Cross-Network Delivery & Mailbox Integrity"
**Date:** July 1, 2026
**Version:** `0.21.2`
**Predecessor:** v48 (`0.21.1`)

---

## Executive Summary

v49 addresses critical delivery confirmation and mailbox integrity issues:

1. **Relay Reservation Fix**: Changed from relative multiaddr to full multiaddr for relay reservations, fixing `MissingRelayAddr` errors after network switches.
2. **Anchor Filtering**: Only hardcoded bootstrap nodes (`verified_rbns`) receive mailbox payloads, preventing regular peers from being treated as storage nodes.
3. **Mailbox Replication**: Messages are now stored on ALL connected verified RBNs, ensuring availability regardless of which anchor the recipient drains from.
4. **Delivery Confirmation**: Added `MailboxStored` ACK from anchor to sender, with new status 3 (In Mailbox) showing a clock icon in the UI.
5. **Sync Safety**: Changed from `ON CONFLICT DO UPDATE` to `INSERT OR IGNORE` for chat sync, preventing stale data from rolling back current messages.
6. **File Transfer Fixes**: Restored 64KB relay chunks, removed TransitFileChunk routing, added stale `FileTransferComplete` guard.

### Known Issue
Cross-network file transfer still requires a live relay circuit. File chunks cannot go through the anchor mailbox. When the relay circuit can't establish (different RBNs, VPN blocking), chunks pile up in RAM and get dropped. See `DEBUG_DOCUMENT.md` for detailed analysis.

---

## Changes

### Networking
- Relay reservation uses full multiaddr (not relative) — fixes `MissingRelayAddr`
- `verified_rbns` filter for mailbox storage (only bootstrap nodes)
- Mailbox replication to ALL connected verified RBNs
- `OutboundCircuitEstablished` flush with rate limiter clear
- TransitFileChunk removed — chunks flow through normal relay circuit
- 64KB relay chunks restored (was 256KB for all paths)
- Relay dial simplified (one RBN by latency, early break)

### Delivery Confirmation
- `MailboxStored` ACK from anchor to sender
- Message status 3 (In Mailbox) with clock icon
- `store_message_if_new` (INSERT OR IGNORE) for sync
- `fetch_undelivered_messages` for retry logic (60s threshold)
- Stale `FileTransferComplete` guard — only process for active transfers
- File messages excluded from chat sync

### UI
- Caption dialog redesigned with thumbnails, Cancel/Send buttons
- Removed duplicate `_addSendingPlaceholder` (fixed double thumbnail)
- `is_verified: false` on chunk send (premature verified tick fix)

---

## Files Modified

| File | Changes |
|------|---------|
| `for_linux/src/network/mod.rs` | Relay reservation, anchor filter, mailbox replication, circuit flush, sync safety, chunk sizing |
| `src/network/mod.rs` | Same changes for main source tree |
| `for_linux/src/storage.rs` | `store_message_if_new`, `fetch_undelivered_messages` |
| `src/storage.rs` | Same |
| `for_linux/src/network/types.rs` | `MailboxStored` variant, `original_msg_id` on `MailboxStore` |
| `src/network/types.rs` | Same |
| `lib/blueprint_ui.dart` | Status 3 (clock icon) rendering |
| `lib/views/chat_screen.dart` | Caption dialog redesign, removed `_addSendingPlaceholder` |

---

## Build & Deploy

```bash
make mac               # Rebuild macOS client
make android           # Rebuild Android client
./deploy_rbn.sh        # NOT required (client-only changes)
```

## How to Test

1. Mac on WiFi, Android on mobile data
2. Send text message Mac→Android — should arrive via mailbox (clock icon on Mac)
3. Send text message Android→Mac — should arrive via mailbox
4. When both on same network — files transfer correctly (256KB chunks)
5. Open chat after sync — messages should NOT roll back (INSERT OR IGNORE)
