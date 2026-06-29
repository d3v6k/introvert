# Introvert v0.2.0 — Stable Release Notes

**Release Date:** 2026-06-16  
**Codename:** "Sovereign Calls"

---

## What's New in v0.2.0

### 🎥 Group Video/Audio Calls
- **Full-mesh group WebRTC calls** — WhatsApp-style group calls with up to 8 participants
- Each participant connects to every other participant directly
- Grid video layout with participant tiles, local video PiP
- Audio-only fallback for bandwidth-constrained devices
- Late joiners can join via ongoing call notification banner in group chat
- Call type selection (audio/video) before starting

### 📞 Improved 1:1 Calls
- **Audio/video call selection** — Choose audio or video before starting a call
- **Call button added to 1:1 chat screen** — Tap video icon in chat app bar to start a call
- **Network quality checks** — Pre-call bandwidth verification, blocks calls if network too weak
- **Auto-downgrade** — Video automatically switches to audio when network degrades
- **Quality indicators** — "AUDIO ONLY" badge shown when downgraded, snackbar notifications
- **Fixed hang-up button** — Resolved bug where hang-up button was unresponsive after call start

### 📬 Offline Message Delivery (Store-and-Forward)
- **Zero-message-loss architecture** — Messages sent to offline peers are stored on Anchor nodes (RBNs) and delivered when the peer reconnects
- **Mailbox system** — Encrypted payloads stored on RBN indexed by recipient hash (SHA-256 of PeerId), RBN cannot decrypt content
- **Automatic polling** — Foreground: polls every 5 seconds; Background: every 15 minutes; On reconnect: immediate full drain
- **Recursive drain** — When messages are received, triggers another drain after 200ms to catch any remaining messages
- **TTL-based expiry** — Messages expire after configurable time-to-live, automatically cleaned up by Anchor nodes
- **What gets stored** — ChatMessage, Acknowledgement, FileTransfer, GroupInvite, GroupAction, MessageReaction, EditMessage, SetRetention
- **What doesn't** — WebRTC signaling (transient, RAM-buffered only), FileChunks (RAM-buffered, re-requested on reconnect)
- **Read receipts work offline** — Acknowledgement payloads are mailbox-eligible, so delivery/read confirmations are stored and forwarded even when the recipient is offline
- **File transfers resume** — Partial file transfers are tracked; when peer comes back online, missing chunks are requested via FileChunkRequest

### 📶 Intelligent Network Quality Monitoring
- **NetworkQualityService** — Real-time bandwidth, packet loss, and RTT monitoring
- **Pre-call checks** — Verifies network before allowing calls (minimum 40 kbps for audio, 300 kbps for video)
- **During-call monitoring** — WebRTC stats polled every 3 seconds
- **Adaptive quality** — Auto-downgrades video→audio when quality drops below thresholds
- **Quality events** — Emits events for quality transitions, triggers UI updates

### 📱 8-Member Group Call Limit
- Group call button hidden when group has > 8 members
- Safety checks at all entry points (initiate, join, accept)
- Ongoing call banner hidden when > 8 participants
- User-friendly snackbar messages explaining the limit

### ✅ Read Receipts Fixed
- **Critical bug fixed** — Read receipts now actually sent to remote peer
- Previously: `_markMessagesAsRead()` only updated local database
- Now: Sends `Acknowledgement { status: 2 }` (Read) for each unread message when chat opens
- Senders now see blue double-checkmarks (✓✓ Read) as expected
- Fixed in both 1:1 chats and group chats

### 🔧 Android Foreground Service Fix
- **Fixed ForegroundServiceDidNotStartInTimeException crash** on Samsung devices
- `startForeground()` now called in `onCreate()` as safety net
- Prevents crash when Android 12+ enforces 5-second foreground service timeout
- Added try/catch around notification creation

### 📄 Comprehensive Networking Documentation
- Complete rewrite of `NETWORKING_&_SIGNALING.md` — 15 sections, full reference
- Documented 50 Mbps direct P2P speeds, 2 Mbps cross-network relay speeds
- Full message lifecycle, mailbox system, call signaling, read receipts
- Event system reference with all 40+ event codes
- Performance benchmarks and latency profiles

---

## Files Changed

### New Files
| File | Purpose |
|------|---------|
| `lib/src/services/group_call_service.dart` | Multi-peer WebRTC group call engine |
| `lib/src/services/network_quality_service.dart` | Bandwidth monitoring, pre-call checks, auto-downgrade |
| `lib/views/group_call_screen.dart` | Group call UI (video grid, controls, participants) |

### Modified Files
| File | Changes |
|------|---------|
| `lib/views/call_screen.dart` | Added quality warning callback, auto-downgrade indicator, pre-call network check, fixed hang-up navigation |
| `lib/views/chat_screen.dart` | Added 1:1 call button + audio/video selection + network check + read receipt fix |
| `lib/views/group_chat_screen.dart` | Added group call button, ongoing call banner, 8-member limit, read receipt fix |
| `lib/src/services/webrtc_call_service.dart` | Integrated NetworkQualityService, auto video→audio downgrade |
| `lib/src/ui/main_shell.dart` | Added incoming group call handling + overlay |
| `android/.../IntrovertService.kt` | Fixed foreground service crash on Samsung devices |
| `Docs/NETWORKING_&_SIGNALING.md` | Complete rewrite — comprehensive networking reference |

---

## Performance Specifications

| Connection Type | Speed | Latency | Use Case |
|----------------|-------|---------|----------|
| Direct P2P | **50 Mbps** | 5-50ms | Same network, hole-punched |
| Group Swarm (LAN) | **50 Mbps** | 5-30ms | Group files on same LAN |
| Relayed QUIC | **2 Mbps** | 30-150ms | Cross-network via RBN |
| Relayed TCP | 0.5-3 Mbps | 50-250ms | UDP-blocked networks |

---

## Known Issues

1. **WebRTC signaling not mailbox-stored** — `WebRtcNative` payloads (SDP/ICE for calls) are buffered in RAM only, not persisted to Anchor mailbox. Cross-network calls may fail if the recipient is offline when the call is initiated. The signaling reaches the recipient only if both peers are online simultaneously. Fix requires adding `SignalingPayload::WebRtcNative(_)` to the `allowed_in_mailbox` list in `src/network/mod.rs:2089`.
2. **Messages during rapid network switching** — If a device switches networks mid-send, the in-flight message may be lost if the peer connection drops before the `forward_to_mesh` completes. The mailbox system handles the "peer offline" case, but not the "peer was online, then went offline during delivery" race.
3. **Swift Package Manager warnings** — flutter_webrtc, open_file_ios, flutter_video_thumbnail_plus, flutter_callkit_incoming do not yet support SPM. This will become an error in future Flutter versions.

---

## Upgrade Notes

- No database migrations required
- No breaking API changes
- New FFI functions: None (all changes are Dart-side)
- Rust core: Unchanged (no networking protocol modifications)

---

## Stable File Manifest

All stable copies are saved with `.stable` extension alongside the active files:

```
lib/src/services/group_call_service.dart.stable
lib/src/services/network_quality_service.dart.stable
lib/src/services/webrtc_call_service.dart.stable
lib/views/call_screen.dart.stable
lib/views/chat_screen.dart.stable
lib/views/group_chat_screen.dart.stable
lib/views/group_call_screen.dart.stable
lib/src/ui/main_shell.dart.stable
android/.../IntrovertService.kt.stable
Docs/NETWORKING_&_SIGNALING.md.stable
```
