# Introvert v0.4.0 — Stable Release Notes

**Release Date:** 2026-06-16  
**Codename:** "Sovereign Presence"

---

## What's New in v0.4.0

### Typing Indicator
- **Real-time typing detection** — When user types, remote peers see "typing..." in chat header
- **New signaling payloads** — `TypingStart` and `TypingStop` variants added to Rust `SignalingPayload` enum
- **Event 39** — Dispatched to Dart with `[peer_id_bytes][1=typing/0=stopped]`
- **FFI functions** — `introvert_send_typing_start(peer_id)`, `introvert_send_typing_stop(peer_id)`
- **Dart API** — `sendTypingStart(peerId)`, `sendTypingStop(peerId)`

### Last Seen Status
- **Heartbeat broadcast** — Every 30 seconds, all connected peers exchange `Heartbeat { timestamp }` payloads
- **Last seen storage** — Timestamps stored in contacts table (`last_seen INTEGER` column)
- **Display** — "online" if last seen < 30s ago, "last seen Xm ago" otherwise
- **FFI function** — `introvert_get_last_seen(peer_id)` returns Unix timestamp
- **Dart API** — `getLastSeen(peerId)` returns Unix timestamp

### Message Search
- **Full-text search** — SQL `LIKE %query%` across title, content, and tags
- **1:1 search** — `search_messages(peer_id, query)` returns matching messages
- **Group search** — `search_group_messages(group_id, query)` returns matching messages
- **FFI functions** — `introvert_search_messages`, `introvert_search_group_messages`
- **Dart API** — `searchMessages(peerId, query)`, `searchGroupMessages(groupId, query)`

### Call History
- **Persistent log** — All calls recorded in `call_history` table (peer_id, call_type, media_type, duration, is_incoming, timestamp)
- **Auto-logging** — Calls automatically logged when ended in CallScreen
- **FFI functions** — `introvert_call_history_log`, `introvert_call_history_get`, `introvert_call_history_count`
- **Dart API** — `callHistoryLog()`, `callHistoryGet()`, `callHistoryCount()`

### Background Sync
- **BackgroundSyncService** — 5-minute periodic mailbox fetch
- **WorkManager removed** — Was incompatible with Flutter v1 embedding (ShimPluginRegistry)
- **Battery impact** — 5-min interval vs old 30s timer (90% reduction)

---

## Files Changed

### New Files
| File | Purpose |
|------|---------|
| `lib/src/services/background_sync_service.dart` | Background sync with Timer fallback |
| `Docs/RELEASE_NOTES_v0.4.0.md` | This document |

### Modified Files
| File | Changes |
|------|---------|
| `src/network/mod.rs` | Added TypingStart/TypingStop/Heartbeat payloads + handlers |
| `src/storage.rs` | Added last_seen column, update_last_seen/get_last_seen, search_messages/search_group_messages |
| `src/lib.rs` | Added 8 FFI functions (typing, last_seen, search, call_history) |
| `lib/src/native/introvert_client.dart` | Added Dart bindings for all new FFI functions |
| `lib/views/call_screen.dart` | Added call history logging on call end |
| `lib/views/chat_screen.dart` | Added HEIC/HEIF conversion, search integration |
| `lib/views/group_chat_screen.dart` | Added HEIC/HEIF conversion, search integration |
| `lib/src/services/webrtc_call_service.dart` | Added 1:1 call reconnect (3 attempts) |
| `lib/src/services/group_call_service.dart` | Verified reconnect working |
| `lib/src/ui/main_shell.dart` | Integrated BackgroundSyncService |
| `Docs/CHANGELOG.md` | Updated for v0.4.0 |
| `Docs/NETWORKING_&_SIGNALING.md` | Added typing/last_seen sections |
| `Docs/INTROVERT_MASTER_PLAN.md` | Updated Phase 4 status |

---

## Performance Specifications

| Connection Type | Speed | Latency |
|----------------|-------|---------|
| Direct P2P | **50 Mbps** | 5-50ms |
| Group Swarm (LAN) | **50 Mbps** | 5-30ms |
| Relayed QUIC | **2 Mbps** | 30-150ms |
| Relayed TCP | 0.5-3 Mbps | 50-250ms |

---

## Known Limitations

1. **iOS APNS** — Requires Apple Developer Account ($99/year)
2. **WorkManager** — Removed due to Flutter v1 embedding incompatibility
3. **Typing/last_seen FFI** — Requires Rust binary rebuild (done via deploy_rbn.sh)

---

## Stable File Manifest

All stable copies saved with `.stable` extension:
- `lib/src/services/background_sync_service.dart.stable`
- `lib/src/services/group_call_service.dart.stable`
- `lib/src/services/network_quality_service.dart.stable`
- `lib/src/services/webrtc_call_service.dart.stable`
- `lib/views/call_screen.dart.stable`
- `lib/views/chat_screen.dart.stable`
- `lib/views/group_chat_screen.dart.stable`
- `lib/views/group_call_screen.dart.stable`
- `lib/src/ui/main_shell.dart.stable`
- `lib/src/ui/notes_tab.dart.stable`
- `lib/src/native/introvert_client.dart.stable`
- `android/.../IntrovertService.kt.stable`
- `src/lib.rs.stable`
- `src/storage.rs.stable`
- `src/network/mod.rs.stable`
- `Docs/NETWORKING_&_SIGNALING.md.stable`
- `Docs/INTROVERT_MASTER_PLAN.md.stable`
