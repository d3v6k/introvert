# Introvert Session Handoff — 2026-06-23

## Issues Summary

### Critical: Group Chat Broken (STILL NOT RESOLVED)
Group messages not arriving in real-time. Zero Type=21 events in Flutter logs. Messages show on sender device but never arrive on receiver. Periodic "Sync Chat Messages" drains mailbox for delayed delivery.

### Attempts Made (none fully resolved the issue)
1. **Added gossipsub publish** — Reverted. Didn't help, v34 didn't use it.
2. **Removed GroupAction from noise_eligible** — Applied. Group messages were being Noise-encrypted, silently dropped when receiver session was out of sync. Correct fix but not sufficient alone.
3. **Added diagnostic logging** — Applied to `sendGroupMessage` (Dart) and async send task (Rust). **Logs not yet captured from device.**

### Key Finding
Dart `sendGroupMessage` was silently discarding FFI errors. Now logs them. Need to capture logs to see if Rust is returning an error (e.g., "Group secret not found") or if the network send is failing.

### Next Steps
1. `flutter run` on Android, send a group message, capture logs
2. Look for `sendGroupMessage FAILED` or `GroupSend` log lines
3. If "No members found" → group members not stored in DB
4. If "Sending to N members" but no delivery → ForwardMeshSignaling failing
5. Rebuild Android with `make android` (last build was before logging changes)

### Files Modified This Session

| File | Change |
|------|--------|
| `src/lib.rs:2565-2584` | Added gossipsub publish to group message send path |
| `for_linux/src/lib.rs:1991-2010` | Same fix for RBN daemon |
| `src/network/behaviour.rs` | v34 network config restored (heartbeat 10s, 10MB request-response, 1GB relay) |
| `src/media/mod.rs` | Added `purpose` field to WebRtcSignal (file transfer vs VoIP) |
| `src/network/mod.rs` | Gossipsub `message.source` fix, peer attribution fix |
| `for_linux/src/media/mod.rs` | Same purpose field for RBN |
| `for_linux/src/network/mod.rs` | Same gossipsub fix for RBN |
| `lib/views/chat_screen.dart` | Emoji picker, reaction bar, UX improvements |
| `lib/views/group_chat_screen.dart` | Same UX improvements |
| `lib/src/ui/widgets/file_transfer_bubble.dart` | Thumbnail shadow effect |
| `lib/src/ui/widgets/image_stack_bubble.dart` | Contact sheet grid |
| `lib/src/ui/main_shell.dart` | Handle Event 39 (file transfer auto-accept) |
| `DEBUG_SESSION_STATUS.md` | Debug session documentation |
| `VERSION_CHANGELOG.md` | Version history table |
| `Docs/RBN_PHASE_2_DEPLOYMENT_PLAN.md` | New: Full Phase 2 security/registry plan |
| `Docs/*.md` (10 files) | Updated all docs to reflect current v36 status |

### Docs Updated
- `DEPLOYMENT_ARCHITECTURE.md` — Marked dynamic discovery as "Planned"
- `NETWORKING_&_SIGNALING.md` — Split current/planned sections
- `SECURITY_&_ENCRYPTION.md` — Added security gaps table
- `RBN_OPERATOR_GUIDE.md` — Updated for current manual registration
- `CONFIGURATION_REFERENCE.md` — Current hardcoded bootstrap
- `MODULE_REFERENCE.md` — Updated config.rs description
- `PROTOCOL_SPECIFICATION.md` — Marked on-chain steps as planned
- `ARCHITECTURE_BLUEPRINT.md` — Marked PDA vault as planned
- `Docs/README.md` — Distinguished planned vs implemented features
- `REBUILD_GUIDE.md` — Updated RBN section

### Test Status
- 9/9 unit tests pass
- 8/8 integration tests pass (foundation, economy, group_file_transfer, nat_traversal, persistence, mailbox, async_contiguity, economy_audit)

## Network Config (v34 Baseline)

| Parameter | Value |
|-----------|-------|
| Gossipsub heartbeat | 10s |
| Gossipsub max_transmit_size | unlimited |
| Request-response max | 10MB |
| Relay max_circuit_bytes | 1GB |
| Relay max_circuit_duration | 1 hour |
| Relay max_reservations | 8192 |
| Relay max_circuits | 4096 |

## Production RBNs

### RBN 1: Alibaba Cloud
- **IP:** 47.89.252.80:443
- **PeerId:** 12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a
- **Deployed via:** `deploy_rbn.sh`

### RBN 2: thinkpad.local (just deployed)
- **IP:** 192.168.1.81:8443
- **PeerId:** 12D3KooWGzorWx3pLhJCSdSZPApADf7aDM1g71WwvjjzubWSkCkG
- **Status:** Running, connected to Alibaba RBN via relay circuit
- **External address:** `/ip4/47.89.252.80/tcp/443/p2p/12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a/p2p-circuit/p2p/12D3KooWGzorWx3pLhJCSdSZPApADf7aDM1g71WwvjjzubWSkCkG`
- **Relay reservation:** Accepted by Alibaba RBN
- **Added to bootstrap:** `src/network/config.rs`
- **macOS native library:** Rebuilt with `make mac` ✅

## Next Steps

1. **Deploy local RBN on thinkpad.local** — Set up second RBN for testing
2. **Rebuild native libraries** — `make mac` and `make android` with the gossipsub fix
3. **Test group chat on-device** — Verify messages arrive in real-time
4. **Test media/file transfer in groups** — Verify files send through group chat
5. **Verify 2-RBN setup** — Both RBNs operational, clients connect to both

## Open Questions
- Is the gossipsub fix sufficient for real-time group delivery, or is there a deeper issue?
- Are file transfers in groups using the same send path (they should be, since they go through `sendGroupMessage`)?
- Does the RBN need to be redeployed with the fix for relay to work?
