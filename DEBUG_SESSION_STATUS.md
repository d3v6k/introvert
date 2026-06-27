# Debug Session Status — Group Chat & Messaging
**Date:** 2026-06-23/24
**Session:** Deep system audit + bug fixes + UX improvements

---

## CRITICAL BUG: Group messages not arriving in real-time

### Status: INVESTIGATING — Root cause not yet resolved

### Symptoms
- Zero Type=21 (Group Message) events in Flutter logs
- Group messages show on sender's device (stored locally) but never arrive on receiver
- "Sync Chat Messages" periodically drains mailbox — delayed messages arrive this way
- 1-to-1 chat works fine (Type=2 events arrive)
- Profile polling works (Type=25 events arrive)
- Group manifests arrive (Type=20 events)
- All devices on same WiFi — direct P2P connections should work

### Investigation Timeline

#### Attempt 1: Added gossipsub publish (REVERTED)
**Hypothesis:** Group messages only sent via `ForwardMeshSignaling`, not gossipsub.
**Fix:** Added `PublishGossipsub` before `ForwardMeshSignaling` loop.
**Result:** Did not fix the issue. Reverted — v34 didn't use gossipsub and worked fine.

#### Attempt 2: Removed GroupAction from Noise-eligible list
**Hypothesis:** `GroupAction` was marked as `noise_eligible` in `forward_to_mesh()` at `src/network/mod.rs:1752`. When sender had a Noise IK session, it encrypted the GroupAction. If receiver's session state was out of sync, the encrypted message was silently dropped.
**Fix:** Removed `GroupAction` from `noise_eligible` match arm in both `src/network/mod.rs` and `for_linux/src/network/mod.rs`. Group messages are already AES-256-GCM encrypted with the group secret — Noise encryption is redundant.
**Result:** Did not fix the issue on its own.

#### Attempt 3: Added diagnostic logging (CURRENT)
**Hypothesis:** The async network send task in `introvert_group_send_message` may be failing silently. The Dart `sendGroupMessage` was discarding the FFI result.
**Fix:** 
- Added FFI result checking and `debugPrint` logging to `sendGroupMessage` in `lib/src/native/introvert_client.dart:1159`
- Added `dispatch_debug_log` calls to the Rust async send task in `src/lib.rs:2586` to trace member count and forwarding
**Result:** Builds complete (macOS ✅, Android build timed out — needs rebuild). Logs not yet captured from device.

### Key Finding: Dart silently discards FFI errors
`sendGroupMessage` in `lib/src/native/introvert_client.dart:1159` previously used `using((Arena arena) { ... })` and **completely ignored the FfiResult**. If the Rust side returned an error (e.g., "Group secret not found", "Network not started"), the message showed locally but never went to the network. Now logs the result.

### Next Steps (NOT YET DONE)
1. **Capture logs from device** — `flutter run` on Android, send a group message, look for:
   - `✅ sendGroupMessage OK` or `❌ sendGroupMessage FAILED` (Dart side)
   - `[GroupSend] Sending to N members` or `[GroupSend] FAILED: No members found` (Rust side)
   - `[Mesh] Peer X is connected. Attempting direct delivery...` (network side)
2. **If "No members found"** — the group members aren't being stored in the DB. Check GroupInvite/GroupManifest handler.
3. **If "Sending to N members" but no delivery** — the `ForwardMeshSignaling` is failing. Check if peer is connected.
4. **If no GroupSend log at all** — the FFI call is returning an error before reaching the network send.

### Files Modified This Session
- `src/lib.rs` — Database safeguard, gossipsub publish (reverted), diagnostic logging
- `src/network/mod.rs` — Removed GroupAction from noise_eligible
- `for_linux/src/lib.rs` — Same changes as src/lib.rs
- `for_linux/src/network/mod.rs` — Same noise_eligible fix
- `lib/src/native/introvert_client.dart` — Added FFI result logging to sendGroupMessage
- `src/network/config.rs` — Added thinkpad.local RBN to bootstrap list

---

## BUG FIXED: File transfer shown as VoIP call

### Status: FIXED ✅

### Root Cause
When `SendFile` had no existing data channel, it called `InitiateWebRtc { media_type: 3 }` which created a full WebRTC connection and sent an SDP `offer`. The receiver saw `signal_type: "offer"` and dispatched Event 14 (incoming call) with no way to distinguish it from a real call.

### Fix Applied
1. Added `purpose: Option<String>` field to `WebRtcSignal` struct
2. `InitiateWebRtc` with `media_type == 3` now sets `purpose: Some("file_transfer")`
3. Receiver checks `purpose` — file transfer offers dispatch **Event 39** (auto-accept) instead of Event 14

---

## BUG FIXED: Gossipsub membership check rejecting relayed messages

### Status: FIXED ✅

### Root Cause
Gossipsub `Event::Message` handler checked `propagation_source` (relay peer) against group member list. When RBN relayed a group message, `propagation_source` was the RBN's PeerId — not in member list — message silently rejected.

### Fix Applied
Changed to use `message.source` (original author) when available, falling back to `propagation_source` only if `message.source` is None.

---

## BUG FIXED: Database "file is not a database" crash

### Status: FIXED ✅

### Root Cause
When a user ran a different Introvert identity (different seed), the old SQLCipher database was encrypted with a different key. `StorageService::new` failed with "file is not a database" — a hard crash.

### Fix Applied
Added retry logic in `src/lib.rs:273` and `for_linux/src/lib.rs:237`: if `StorageService::new` fails with "file is not a database", delete the corrupted file and retry with a fresh database.

---

## Other Fixes Applied

| Fix | Status | Files |
|-----|--------|-------|
| Peer attribution regression (`transfer.peer_id`) | ✅ Fixed | `src/network/mod.rs`, `for_linux/src/network/mod.rs` |
| Hardcoded Klipy API key | ✅ Fixed | `lib/views/chat_features.dart` |
| `prestige_tier` missing in for_linux ProfileResponse | ✅ Fixed | `for_linux/src/network/mod.rs` |
| v34 network config restored | ✅ Fixed | `src/network/behaviour.rs` |
| `upsert_handle_claim` → `insert_handle_claim` | ✅ Fixed | `src/network/mod.rs` |
| `get_profile()` 5-tuple destructuring | ✅ Fixed | `src/network/mod.rs` |
| FFI `store_message_async` missing `is_me` param | ✅ Fixed | `tests/foundation_test.rs` |
| `create_peer_connection` missing `command_tx` | ✅ Fixed | `tests/webrtc_stress_test.rs` |
| `set_protocol_names` removed from libp2p | ✅ Fixed | `tests/global_swarm_audit.rs` |
| All-zero seed rejected | ✅ Fixed | `tests/persistence_audit.rs` |
| GroupAction noise-eligible causing silent drops | ✅ Fixed | `src/network/mod.rs`, `for_linux/src/network/mod.rs` |
| Database "file is not a database" crash | ✅ Fixed | `src/lib.rs`, `for_linux/src/lib.rs` |

---

## Build & Deploy Status (as of 2026-06-24 01:00 UTC)

| Component | Status | Notes |
|-----------|--------|-------|
| macOS native library | ✅ Built | `make mac` — includes diagnostic logging |
| Android native library | ⚠️ Stale | Last successful build before logging changes. Needs `make android` rebuild |
| iOS native library | ✅ Built | `make ios` |
| Alibaba RBN (47.89.252.80) | ✅ Deployed | Running with noise_eligible fix |
| thinkpad.local RBN (192.168.1.81:8443) | ✅ Deployed | systemd user service, auto-starts on boot |
| RBN cross-compile | ✅ Built | `cargo zigbuild --target x86_64-unknown-linux-gnu --release` |

---

## Network Configuration (v34 Baseline — Restored)

| Parameter | v34 (working) | v35 (broken) | Current |
|-----------|---------------|-------------|---------|
| Gossipsub heartbeat | **10s** | 30s | **10s** ✅ |
| Gossipsub max_transmit_size | **unlimited** | 1MB | **unlimited** ✅ |
| Request-response max | **10MB** | 2MB | **10MB** ✅ |
| Relay max_circuit_bytes | **1GB** | 100MB | **1GB** ✅ |
| Relay max_circuit_duration | **1 hour** | 30 min | **1 hour** ✅ |
| Relay max_reservations | **8192** | 256 | **8192** ✅ |
| Relay max_circuits | **4096** | 100 | **4096** ✅ |

---

## RBN Infrastructure

### RBN 1: Alibaba Cloud
- **IP:** 47.89.252.80:443
- **PeerId:** 12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a
- **Deployed via:** `deploy_rbn.sh`

### RBN 2: thinkpad.local
- **IP:** 192.168.1.81:8443
- **PeerId:** 12D3KooWGzorWx3pLhJCSdSZPApADf7aDM1g71WwvjjzubWSkCkG (new seed each restart)
- **Service:** systemd user service (`~/.config/systemd/user/introvertd.service`)
- **Auto-start:** Enabled via `loginctl enable-linger dev`

---

## Test Suite Status

**17/18 test suites pass (35 tests + 1 ignored):**

| Test | Status |
|------|--------|
| lib (9 unit tests) | ✅ Pass |
| asynchronous_contiguity_audit | ✅ Pass |
| economic_cohesion_audit | ✅ Pass |
| group_file_transfer_audit | ✅ Pass |
| nat_traversal_audit | ✅ Pass |
| persistence_audit | ✅ Pass |
| foundation_test | ✅ Pass |
| webrtc_stress_test | ⏭️ Ignored (requires network for ICE) |
| All others | ✅ Pass |

---

## Key Files Reference

| File | Purpose |
|------|---------|
| `src/lib.rs` | FFI functions including `introvert_group_send_message` |
| `src/network/mod.rs` | Network service, `forward_to_mesh`, noise_eligible list, message routing |
| `src/network/behaviour.rs` | libp2p behaviour config (v34 restored) |
| `src/network/config.rs` | Bootstrap node list (Alibaba + thinkpad) |
| `src/media/mod.rs` | WebRtcSignal struct with `purpose` field |
| `src/storage.rs` | Database operations |
| `lib/src/native/introvert_client.dart` | Dart FFI bindings (now with logging) |
| `lib/views/group_chat_screen.dart` | Group chat UI |
| `lib/src/ui/main_shell.dart` | Main shell, event dispatch |
| `for_linux/src/network/mod.rs` | RBN network service |
| `for_linux/src/lib.rs` | RBN FFI functions |

---

## Version History Reference

See `VERSION_CHANGELOG.md` in project root for full version table (v27-v36).

**Key dates:**
- v34 (0.11.0 "Iron Claw") — 2026-06-20 — FCM replaces polling, heartbeat 300s. **Group chat worked.**
- v35 (0.12.0 "Sovereign Audit") — 2026-06-21 — Security hardening (broke group chat)
- v36 (0.12.0) — 2026-06-21 — $INTR whitepaper, daily rewards
- v36+ (current) — 2026-06-23/24 — Debugging session. Multiple fixes applied. Group chat still broken.
