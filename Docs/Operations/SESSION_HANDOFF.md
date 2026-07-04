# Introvert Session Handoff — 2026-07-02

## Session Goal
Implement all remaining phases of NETWORKING_STABILIZATION_PLAN: auth hardening (Phase 1), group gossip optimization and Sovereign Swarm seeding (Phase 2), Intro-Claw intelligent networking with DCUtR, adaptive pipelines, mobile data awareness, and VoIP throttling (Phase 3), and relay-aware cross-network routing (Phase 4).

## Status: ALL PHASES COMPLETE ✅
- **Phase 1**: Auth hardening, adaptive pipeline/pacing, IPv6, reconnect ladder
- **Phase 2**: Group gossip optimization, Sovereign Swarm seeding reorder
- **Phase 3**: DCUtR upgrades, adaptive pipeline, mobile awareness, VoIP throttling
- **Phase 4**: Relay-aware routing, relay hint population, fast reconnect, group ACK fix

---

## 1. Work Accomplished

### A. Network Status Semantics & False Positives (RESOLVED ✅)
- **Problem**: Connecting to the RBN was immediately dispatching `status=1 ONLINE`, indicating message readiness even when the relay circuit reservation hadn't finished. The user was misled into thinking they were online while outbound/inbound routes were actually dead.
- **Fix**: 
  - Gated `status=1 ONLINE` strictly on the presence of an active relay listener (`p2p-circuit`).
  - Added `status=4 CONNECTING` to denote that the node is connected to the RBN but waiting on the circuit reservation to be completed.
  - Refactored `main_shell.dart` to map status `4` to an amber "CONNECTING" status in the top bar.

### B. Progressive Reconnect Ladder (RESOLVED ✅)
- **Problem**: There was no background recovery logic if connections dropped or if direct NAT traversal failed under censored/corporate networks.
- **Fix**: Implemented a 4-step progressive escalation ladder in the 2-minute `status_check_interval` watchdog:
  1. **Step 1 (If connected but no relay)**: Re-requests relay reservations from all RBN nodes.
  2. **Step 2 (If fully offline)**: Re-dials all bootstrap nodes (QUIC and TCP) and runs Kademlia DHT bootstrap.
  3. **Step 3 (If direct dial fails)**: Auto-activates the local WebSocket loopback client tunnel (`wss://47.89.252.80/tunnel`) to bypass firewalls.
  4. **Step 4 (If everything fails)**: Transitions state to `status=0 OFFLINE` (red).

### C. TCP Port 80 Fallback Dialing (RESOLVED ✅)
- **Problem**: If the local network or VPN blocked UDP/QUIC traffic, `dial_relay_path` would queue the QUIC dial, return `Ok(queued)`, break early from the RBN dial loop, and never fall back to TCP.
- **Fix**: Modified `dial_relay_path` to dial ALL configured bootstrap node addresses (TCP port 443, TCP port 80, and QUIC/UDP) concurrently.

### D. Mailbox and Message Queue Drainage (RESOLVED ✅)
- **Problem**: When regaining connectivity, queued messages were not delivered instantly.
- **Fix**: Immediately after `ReservationReqAccepted` event, triggered `perform_mailbox_fetch()` and flushed all locally buffered RAM message queues via `ForwardMeshSignaling`.

### E. In-App Debug Log Capture (RESOLVED ✅)
- Exposed a 500-entry rolling ring-buffer in `IntrovertClient` (`event-99` native debug lines).
- Implemented "Network Debug Log" settings section in `main_shell.dart` to copy, clear, or save logs to `/storage/emulated/0/Download/introvert_netlog_*.txt` (or equivalent documents directory on macOS/iOS).

### F. Chronological Chat Sync & Timestamp Preservation (RESOLVED ✅)
- **Problem**: When syncing history, SQLite assigned `CURRENT_TIMESTAMP` (the sync time) to all incoming messages on the receiving device. This scrambled the display order and showed incorrect send dates in the UI.
- **Fix**:
  - Added an optional `timestamp` argument to `store_message_with_id` and `store_group_message` in `src/storage.rs` using `COALESCE(?timestamp, CURRENT_TIMESTAMP)`.
  - Parsed and propagated original timestamps in `ChatMessage` and `GroupAction` message handlers.
  - Passed original timestamps during `ChatSyncResponse` message storage on both 1:1 and Group syncs.

### G. Newest Missing Messages First (RESOLVED ✅)
- **Problem**: Synchronization returned historical messages in oldest-first order and capped at 100. If more than 100 messages were missing, today's latest messages remained un-synced.
- **Fix**: Re-ordered sync payload generation (`ChatSyncRequest` handler) to sort by `rev()` (newest-first) and collect up to `limit` messages before restoring chronological order for wire transfer.

### H. Chat Sync Performance & Wire Optimization (RESOLVED ✅)
- **Problem**: Sync requests fetched and transmitted the entire history of message IDs to compare differences, causing massive wire overhead.
- **Fix**: Gated fast sync (`is_full = false`) to collect and transmit only the latest 100 known message IDs to the peer.

### I. Instant UI Refresh & Event 23 Routing (RESOLVED ✅)
- **Problem**: 1:1 `ChatScreen` had no sync finish listener and relied on a 5-second sleep timer to reload messages after starting sync.
- **Fix**:
  - Integrated Event 23 (Chat Messages Synced) in `chat_screen.dart` to reload UI instantly when a sync batch is saved.
  - Filtered Event 23 in `group_chat_screen.dart` by group ID to prevent redundant reloads for unrelated groups.

### J. Hardened Sync Security (RESOLVED ✅)
- **Problem**: Relayed group sync bypassed membership checks, and 1:1 sync allowed arbitrary contacts to request/send messages for unrelated chats.
- **Fix**:
  - Always verify that the sync source is a member of the group for both direct and relayed group sync.
  - Verify that the 1:1 sync source exactly matches the chat participant's Peer ID, rejecting spoofed messages.

---

## 2. Files Modified

### Session 1-4 (v49)
| File | Change |
|:-----|:-------|
| [`src/storage.rs`](file:///Users/dev/Development/introvert/src/storage.rs) | Preserved original timestamps in database inserts |
| [`src/network/mod.rs`](file:///Users/dev/Development/introvert/src/network/mod.rs) | Reconnect ladder, TCP fallbacks, newest-first sync sorting, limit known IDs, security hardening |
| [`src/lib.rs`](file:///Users/dev/Development/introvert/src/lib.rs) | Adapted FFI callers of database inserts |
| [`lib/views/chat_screen.dart`](file:///Users/dev/Development/introvert/lib/views/chat_screen.dart) | Integrated Event 23 instant sync reload |
| [`lib/views/group_chat_screen.dart`](file:///Users/dev/Development/introvert/lib/views/group_chat_screen.dart) | Filtered Event 23 reload by active group ID |
| [`lib/src/native/introvert_client.dart`](file:///Users/dev/Development/introvert/lib/src/native/introvert_client.dart) | 500-entry ring-buffer for native Event 99 messages |
| [`lib/src/ui/main_shell.dart`](file:///Users/dev/Development/introvert/lib/src/ui/main_shell.dart) | Added settings section and status `4` (CONNECTING) mapping |
| [`Docs/CHANGELOG.md`](file:///Users/dev/Development/introvert/Docs/CHANGELOG.md) | Added version `0.21.1` changelog details |
| [`Docs/DEBUG_SESSION_STATUS.md`](file:///Users/dev/Development/introvert/Docs/DEBUG_SESSION_STATUS.md) | Updated implementation status |

### Session 5 (v50 — Delivery Fixes & System Hardening)
| File | Change |
|:-----|:-------|
| `src/network/mod.rs` | dial_relay_path parameterized, VPN detection, DCUtR, sync_in_progress, relay_hint optimization, [FILE]: filter |
| `src/network/types.rs` | Added relay_hint to FileChunkRequest |
| `src/network/service.rs` | Added sync_in_progress, relay_hints fields |
| `src/storage.rs` | Added pending_file_chunks table, update_message_status_if_higher, chunk queue methods |
| `for_linux/src/network/mod.rs` | Same changes + for_linux relay reservation fix + sender authorization |
| `for_linux/src/network/types.rs` | Added relay_hint to FileChunkRequest |
| `for_linux/src/storage.rs` | Added pending_file_chunks table, update_message_status_if_higher, chunk queue methods |
| `for_linux/src/network/types.rs` | **DELETED** (dead code, never imported) |
| `Docs/DEBUG_SESSION_STATUS.md` | Added v50 session 5 implementation details |
| `Docs/CHANGELOG.md` | Added v50 changelog entry |
| `VERSION_CHANGELOG.md` | Added v50 version entry |
| `Docs/Components/DATABASE_SCHEMA.md` | Added pending_file_chunks table documentation |
| `Docs/Components/MODULE_REFERENCE.md` | Added new functions and fields |
| `DEBUG_DOCUMENT.md` | Added v50 delivery fixes documentation |

### v52 (Phase 1–3: Adaptive Networking)
| File | Change |
|:-----|:-------|
| `src/intro_claw.rs` | `ClawTickContext` +2 fields, `should_attempt_dcutr()`, `get_optimal_pipeline_depth(is_mobile)`, `is_on_mobile_data()`, `is_mobile_data` state |
| `src/network/mod.rs` | Pipeline adaptive (3 sites), pacing 1.5x mobile, mailbox skip, group gossip filter, seeder reorder, periodic tick mobile pass |
| `src/network/types.rs` | `IntroClawTick` +`is_mobile_data`/`network_type` |
| `src/lib.rs` | `intro_claw_trigger_tick(is_mobile_data: bool)` FFI |
| `for_linux/src/network/mod.rs` | Group gossip filter, `disconnect_peer_id` fix |
| `lib/src/native/introvert_client.dart` | FFI typedef +`Bool isMobileData` |
| `lib/src/ui/main_shell.dart` | All 3 `triggerIntroClawTick()` call sites pass `isMobileData` |

---

## 3. Pending & Unresolved (Next Steps)

### v52 Remaining
1. **Phase 3.4**: VoIP-Aware Transfer Throttling — pause file transfers during active calls
2. **Phase 4**: Verification & Testing — regression tests, performance benchmarks, integration scenarios
3. **Device testing**: Verify adaptive pipeline and mobile data awareness on Android + iOS

### v50 Testing (Still Valid)
1. **VPN test**: Mac on VPN → send text to Android on mobile data → arrives
2. **Cross-network file test**: Mac on WiFi, Android on cellular → send image → arrives
3. **Sync test**: Open chat after sync → messages NOT rolled back, status doesn't go backward
4. **Direct P2P regression**: Both on same WiFi → files and messages work normally
5. **App restart test**: Start file transfer → kill sender app → restart → chunks resume from DB
