# Introvert v0.3.0 — Stable Release Notes

**Release Date:** 2026-06-16  
**Codename:** "Sovereign Notes"

---

## What's New in v0.3.0

### Sovereign Notes (New Feature)
- **Full notes system** with title, content, tags, and image attachments
- **SQLite-backed storage** with `notes` and `note_versions` tables
- **8 FFI functions**: create, update, delete, get, get_all, search, save_version, get_versions
- **Search** — filters by title, content, or tags (partial text matching)
- **Version history** — save and restore previous versions of notes
- **Share to chat** — send note content to contacts or group chats
- **Export/Import** — JSON archive with optional XOR encryption, merge or replace modes
- **Image attachments** — pick from gallery, attach to notes
- **WhatsApp-style date separators** — TODAY, YESTERDAY, day names for this week, DD MMM YY for older

### Chat Improvements
- **Last message preview** in chat list — shows last message instead of peer ID (like WhatsApp)
- **Friendly file labels** — "📷 Photo", "🎬 Video", "🎵 Audio" instead of raw `[FILE]:` manifest
- **Location sharing** — opens map picker before sending (WhatsApp-style flow)
- **Location rendering** — receiver sees map thumbnail, taps to open native maps (Apple Maps/Google Maps)
- **Date separators** — TODAY, YESTERDAY, MONDAY-SUNDAY for this week, DD MMM YY for older
- **Timestamp fix** — timestamps now correctly convert UTC to local timezone

### Peer Info Dialog
- Shows avatar, display name, introvert handle (i@), and peer ID
- Handle displayed only if registered

### Code Quality
- **0 errors, 0 warnings** across entire codebase (Dart + Rust)
- **0 deprecated API usage** — all `.withOpacity()` → `.withValues()` migrated
- **0 unused imports/fields/variables** — all cleaned up
- **Context-after-await guards** — all `mounted` checks added for async operations

### Bug Fixes
- **Read receipts** — now actually sent to remote peer when chat is opened
- **Call screen navigation** — fixed hang-up button getting stuck (double-pop race)
- **Samsung foreground service crash** — `startForeground()` now called in `onCreate()`
- **Date display** — "01 Jan 70" fixed by guarding against epoch timestamps
- **Group chat LocationBubble** — was missing, now renders map preview
- **Location parsing** — robust parsing with `tryParse` instead of fragile `try/catch`

---

## Files Changed Since v0.2.0

### New Files
| File | Purpose |
|------|---------|
| `lib/src/ui/notes_tab.dart` | Sovereign Notes tab with CRUD, search, export/import, help |
| `Docs/RELEASE_NOTES_v0.3.0.md` | This document |

### Modified Files
| File | Changes |
|------|---------|
| `lib/src/native/introvert_client.dart` | Added notes FFI bindings (8 functions) |
| `lib/views/chat_screen.dart` | Location picker, date separators, peer info dialog, read receipts fix |
| `lib/views/group_chat_screen.dart` | Location picker, LocationBubble handler, friendly file labels |
| `lib/src/ui/main_shell.dart` | Last message preview, friendly message labels, Notes tab integration |
| `src/storage.rs` | Added `notes` and `note_versions` tables + CRUD operations |
| `src/lib.rs` | Added 8 notes FFI functions |
| `src/network/mod.rs` | Field cleanup (unused fields prefixed with _) |

---

## Performance Specifications (Unchanged from v0.2.0)

| Connection Type | Speed | Latency |
|----------------|-------|---------|
| Direct P2P | **50 Mbps** | 5-50ms |
| Group Swarm (LAN) | **50 Mbps** | 5-30ms |
| Relayed QUIC | **2 Mbps** | 30-150ms |
| Relayed TCP | 0.5-3 Mbps | 50-250ms |

---

## Known Limitations

1. **WebRTC signaling** — Not stored in mailbox, requires both peers online for call signaling
2. **File transfer cancel** — FFI function not yet implemented in Rust native library
3. **Swift Package Manager** — flutter_webrtc and other plugins don't support SPM yet

---

## Stable File Manifest

All stable copies saved with `.stable` extension:

```
lib/src/ui/notes_tab.dart.stable
lib/src/native/introvert_client.dart.stable
lib/views/chat_screen.dart.stable
lib/views/group_chat_screen.dart.stable
lib/src/ui/main_shell.dart.stable
src/storage.rs.stable
src/lib.rs.stable
src/network/mod.rs.stable
android/.../IntrovertService.kt.stable
Docs/NETWORKING_&_SIGNALING.md.stable
```
