# Introvert Deep Audit Report — 2026-06-24

**Session Type:** Relay & Group Chat Debugging  
**Reference Version:** `stable_v34` (networking working, group chat working)  
**Current Version:** `v36+` (group chat broken, relay instability discovered and resolved)  
**Auditor:** Antigravity AI (automated session analysis)  
**Last Updated:** 2026-06-24 ~06:10 UTC+4

---

## 1. Executive Summary

Group chat messages were not delivering in real-time. Zero `Type=21` (GroupMessage) FFI events arrived on the receiver side. Messages appeared locally on the sender but never reached recipients, except via the periodic "Sync Chat Messages" mailbox drain.

**Ten structural bugs** were identified and fixed during this session. The RBN daemon on Alibaba Cloud was exhibiting a self-relay OFFLINE loop which has been fixed, recompiled, and redeployed. The RBN now achieves `RELAY CONNECTED` state on every restart and no longer cycles.

---

## 2. Build Status (as of 2026-06-24 ~06:10 UTC+4)

| Platform | Status | Notes |
|---|---|---|
| macOS (`.dylib`) | ✅ Built | `make mac` succeeded, includes diagnostic logging |
| Android (`.so` arm64 + x86_64) | ✅ Built | `make android` completed; artefacts in `android/app/src/main/jniLibs/` |
| iOS (`.a` device + simulator) | ✅ Built | `make ios` succeeded |
| Linux/RBN ELF (`introvertd`) | ✅ Deployed | **v3 of binary deployed today** — all RBN fixes applied, PID 11804 running |

---

## 3. Bugs Found & Fixed This Session

### 3.1 Noise IK Renegotiation Deadlock (CRITICAL — FIXED)

**Files:** `src/network/mod.rs`, `for_linux/src/network/mod.rs`

**Root cause:** When a Noise IK session became out-of-sync (app restart, differing session states), both sides would send `RequestHandshake` to each other, then each would clear its own session and wait for the *other* to initiate — resulting in a permanent deadlock. No messages could be delivered until a manual restart.

**Fix applied:** Deterministic role assignment based on lexicographic PeerId comparison:
- If `local_peer_id < remote_peer_id` → local node is the **initiator**: sends `EstablishSecureSession` immediately.
- If `local_peer_id > remote_peer_id` → local node is the **responder**: sends `RequestHandshake` to prompt the initiator.

This guarantees exactly one side initiates, eliminating the deadlock mathematically.

**Code location:** `handle_single_payload()` — the `SignalingPayload::RequestHandshake` arm.

---

### 3.2 GroupAction Noise-Encryption Causing Silent Drops (CRITICAL — FIXED)

**Files:** `src/network/mod.rs`, `for_linux/src/network/mod.rs`

**Root cause:** `GroupAction` was included in the `noise_eligible` match arm inside `forward_to_mesh()`. Group messages were double-encrypted: wrapped in `SignalingPayload::Secure(Transport(...))`. If the receiver's Noise session state was missing or out-of-sync, decryption failed silently — message lost entirely.

Group messages are already encrypted with AES-256-GCM using the shared group secret. Adding a second Noise layer was redundant and harmful.

**Fix applied:** Removed `SignalingPayload::GroupAction(_)` from the `noise_eligible` match arm.

```rust
// BEFORE (broken — GroupAction double-encrypted):
SignalingPayload::GroupAction(_) |   // ← WAS HERE

// AFTER (correct):
// GroupAction is NOT noise-eligible — already AES-256-GCM encrypted with group secret.
// Noise encryption caused silent delivery failures when session state was out of sync.
```

---

### 3.3 Gossipsub Propagation-Source vs. Message-Source Bug (FIXED)

**File:** `src/network/mod.rs` (Gossipsub event handler)

**Root cause:** The code used `propagation_source` (the peer that *forwarded* the gossip — potentially an RBN relay) as the author. When an RBN relayed a group message, its PeerId was not in the group member list, causing rejection:

```
[Mesh] Rejecting gossipsub message from non-member <RBN_PEER_ID> for topic <group_id>
```

**Fix applied:** Changed to use `message.source` (the cryptographic original author) with fallback:

```rust
// BEFORE: self.handle_single_payload(propagation_source, payload, false).await;
// AFTER:
let author_peer = message.source.unwrap_or(propagation_source);
self.handle_single_payload(author_peer, payload, false).await;
```

---

### 3.4 Dart FFI Error Silently Discarded (DIAGNOSTIC LOGGING ADDED)

**File:** `lib/src/native/introvert_client.dart` (~line 1159)

**Root cause:** The Dart `sendGroupMessage()` wrapper completely ignored the `FfiResult` returned by Rust. If Rust returned "Group secret not found" or "Network not started", the Dart side showed the message locally but nothing went to the network.

**Fix applied:** Added result checking with console logging. Rust-side also logs:
```
[GroupSend] Sending to N members for group <id>
[GroupSend] Forwarding to member <peer_id>
[GroupSend] FAILED: No members found for group <id>
```

---

### 3.5 GroupManifest `secret` Field in Wire Format (SECURITY FIX — FIXED)

**File:** `src/network/mod.rs`

**Root cause:** `SignalingPayload::GroupManifest` included the raw `secret` field in its wire serialisation. The AES-256-GCM group key was being broadcast in plaintext JSON to all members (and stored in RBN mailboxes).

**Fix applied:** Removed `secret` from the `GroupManifest` wire struct. The group secret is only exchanged through the secure `GroupInvite` flow.

---

### 3.6 Database "File Is Not A Database" Crash (FIXED)

**Files:** `src/lib.rs` (~line 273), `for_linux/src/lib.rs` (~line 237)

**Root cause:** Using a different identity seed caused `StorageService::new` to fail with "file is not a database" — a hard crash preventing the engine from starting.

**Fix applied:** If `StorageService::new` fails with "file is not a database", the mismatched database file is deleted and the engine retries fresh.

---

### 3.7 WebRTC File Transfer Offer Misidentified as VoIP Call (FIXED)

**Files:** `src/network/mod.rs`, `src/media/mod.rs`, `for_linux/` equivalents

**Root cause:** File transfers over WebRTC sent an SDP `offer` which triggered `Event 14` (incoming call) on the receiver — showing the VoIP call UI for a file transfer.

**Fix applied:**
- Added `purpose: Option<String>` to `WebRtcSignal`.
- File transfer offers set `purpose: Some("file_transfer")`.
- Receiver dispatches `Event 39` (auto-accept) for file transfers, `Event 14` for VoIP.

---

### 3.8 RBN Self-Relay OFFLINE Loop (CRITICAL — FIXED & DEPLOYED)

**Files:** `for_linux/src/network/mod.rs` (primary), `src/network/mod.rs` (defensive)

**Root cause:** The RBN daemon shares bootstrap config with the user client. The `bootstrap_nodes` list includes the RBN's own PeerId (`12D3KooWJqiNgP67...`). On every `dial_relay_path()` call, it constructed relay addresses through itself:

```
/ip4/47.89.252.80/tcp/443/p2p/<OWN_PEER_ID>/p2p-circuit/p2p/<TARGET>
```

This dial fails with `ResponseFromBehaviourCanceled(Canceled)`. Kademlia then removes the failed address, rediscovers the same peer via DHT, and retries the same path — an **infinite OFFLINE loop** every ~15 seconds that prevented all relay circuits from forming.

**Evidence before fix:**
```
Connection Status Change: OFFLINE   (every 15s, tight loop — never RELAY CONNECTED)
Outgoing connection error: Transport([(.../p2p/12D3KooWJqiNgP67.../p2p-circuit/...,
  ResponseFromBehaviourCanceled(Canceled))])
```

**Fix applied in `for_linux/src/network/mod.rs`:**
```rust
fn dial_relay_path(&mut self, recipient_id: PeerId) {
    // SELF-RELAY GUARD: Never construct a relay path through ourselves.
    let local_id = *self.swarm.local_peer_id();
    for (rbn_id, rbn_addr) in self.bootstrap_nodes.clone() {
        if rbn_id == local_id {
            debug!("[Mesh] Skipping self-relay path for local RBN node {}", rbn_id);
            continue; // NEVER relay through ourselves
        }
        if rbn_addr.to_string().contains("443") {
            // ... construct relay addr normally
        }
    }
}
```

**Evidence after fix (Alibaba live logs, PID 11804):**
```
[Network] Local Node Status: ONLINE (Listening)
[Network] Local Node Status: RELAY CONNECTED     ← NEW — never seen before fix
[Network] Local Node Status: RELAY CONNECTED     ← Stable, no OFFLINE cycling
```

---

### 3.9 mDNS Enabled on Headless Cloud RBN (FIXED & DEPLOYED)

**File:** `for_linux/src/lib.rs` (line ~367)

**Root cause:** `NetworkService::new()` was called with `enable_mdns: true` for the RBN daemon. mDNS is a LAN-only broadcast discovery protocol (UDP port 5353). On a cloud server it is architecturally useless, generates spurious peer discovery events, and contributed to OFFLINE state churn.

**Evidence before fix:** `mDNS behaviour initialized` appeared in every daemon startup log.

**Fix applied:**
```rust
// for_linux/src/lib.rs — NetworkService::new() call
false, // enable_mdns: RBN is a headless cloud relay — mDNS is LAN-only, useless here
true,  // enable_listeners
```

**Verification:** No `mDNS behaviour initialized` in any post-fix logs. ✅

---

### 3.10 Duplicate Peer Discovery FFI Callbacks (FIXED & DEPLOYED)

**File:** `for_linux/src/network/mod.rs`

**Root cause:** `kad::Event::RoutingUpdated` fires **once per address** learned for a peer. A peer with 10 known addresses triggers 10 FFI event type 1 callbacks, flooding logs with 20+ identical "Peer Discovered (Binary/Hex): `<same hex>`" lines per peer.

**Fix applied:** Added `announced_peers: HashSet<PeerId>` session-scoped dedup set:

```rust
// Struct field:
/// Peers already announced to FFI via event type 1 (dedup guard).
/// Kademlia fires RoutingUpdated once per address learned — same peer can appear dozens of times.
announced_peers: HashSet<PeerId>,

// Event handler:
IntrovertBehaviourEvent::Kademlia(kad::Event::RoutingUpdated { peer, .. }) => {
    if self.announced_peers.insert(peer) {  // insert() returns false if already present
        let data = peer.to_bytes();
        crate::dispatch_global_event(1, &data);
    }
}
```

**Evidence after fix:** Each peer now appears 1-2 times in logs (vs 20+ before). ✅

---

## 4. RBN Deployment History — Today's Session

| Deploy | Binary | Changes | Result |
|---|---|---|---|
| Deploy 1 (earlier session) | v1 | Noise IK deadlock fix | PID 10607 — still had self-relay loop |
| Deploy 2 (~02:02 UTC) | v2 | Self-relay guard (§3.8) + mDNS disable (§3.9) | **`RELAY CONNECTED` achieved** ✅ |
| Deploy 3 (~02:09 UTC) | v3 | Peer discovery dedup (§3.10) | PID 11804 — clean logs, stable ✅ |

---

## 5. Network Configuration (v34 Baseline — Restored)

| Parameter | v34 ✅ | Was Broken As | Current |
|---|---|---|---|
| Gossipsub heartbeat | 10s | 30s | **10s ✅** |
| Gossipsub max_transmit_size | unlimited | 1MB | **unlimited ✅** |
| Request-response max | 10MB | 2MB | **10MB ✅** |
| Relay max_circuit_bytes | 1GB | 100MB | **1GB ✅** |
| Relay max_circuit_duration | 1 hour | 30 min | **1 hour ✅** |
| Relay max_reservations | 8192 | 256 | **8192 ✅** |
| Relay max_circuits | 4096 | 100 | **4096 ✅** |

---

## 6. Message Delivery Path (GroupAction)

```
Sender: introvert_group_send_message() [FFI]
   │
   ├─ AES-256-GCM encrypt with group secret
   ├─ Sign with sender's Ed25519 key (GroupManager::sign_action)
   ├─ Store locally in DB (store_group_message)
   └─ Spawn async task: for each member, ForwardMeshSignaling
         │
         └─► NetworkCommand::ForwardMeshSignaling → forward_to_mesh()
                │
                ├─[1] WebRTC Data Channel (if open) — fastest
                ├─[2] Direct libp2p request-response (if connected) — PLAIN (fixed §3.2)
                ├─[3] Relay via RBN circuit dial (self-relay guard now applied §3.8)
                └─[4] Mailbox on anchor/RBN (polled every 30s by recipient)

Receiver: handle_single_payload() → SignalingPayload::GroupAction(signed)
   │
   ├─ Verify Ed25519 signature (GroupManager::verify_action)
   ├─ Decrypt AES-256-GCM with group secret
   ├─ Store in DB (store_group_message)
   └─ dispatch_global_event(21, ...) → Flutter Event 21 (group message received)
```

---

## 7. Cross-Compilation Notes (macOS → Linux)

The `for_linux/` daemon **must** be cross-compiled on a machine with >2GB RAM using `cargo-zigbuild`. Never compile directly on the 1GB Alibaba server.

### Commands
```bash
cd /Users/dev/Development/introvert/for_linux
ulimit -n 65536   # macOS default of 256 is too low for the linker — raises to 65536
cargo zigbuild --target x86_64-unknown-linux-gnu --release --bin introvertd
```

### Why `ulimit -n 65536`
macOS default soft fd limit is 256. The Rust linker for a project this size requires ~2000+ file descriptors simultaneously. Without raising the limit, the link step fails with:
```
error: unable to search for static library '...': ProcessFdQuotaExceeded
```
The hard limit is `unlimited` on this machine, so `ulimit -n 65536` works without root.

### Deployment Procedure
```bash
# Step 1: Stop daemon (binary is locked while running — scp will fail otherwise)
ssh root@47.89.252.80 "systemctl stop introvertd"

# Step 2: Upload new binary
scp for_linux/target/x86_64-unknown-linux-gnu/release/introvertd \
    root@47.89.252.80:/opt/introvert/bin/introvertd

# Step 3: Start and verify
ssh root@47.89.252.80 "systemctl start introvertd && sleep 20 \
  && journalctl -u introvertd --no-pager -n 30 --since '15 seconds ago'"
```

### Success Criteria After Deploy
```
[Network] Local Node Status: ONLINE (Listening)
[Network] Local Node Status: RELAY CONNECTED      ← Must appear within 15s
```
If only `ONLINE (Listening)` appears and `RELAY CONNECTED` never comes, the self-relay loop may still be present.

---

## 8. RBN Infrastructure Status

| Node | Address | PeerId | Status |
|---|---|---|---|
| Alibaba Cloud | `47.89.252.80:443` | `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a` | ✅ RELAY CONNECTED — all fixes deployed (PID 11804) |
| thinkpad.local | `192.168.1.81:8443` | `12D3KooWGzorWx3pLhJCSdSZPApADf7aDM1g71WwvjjzubWSkCkG` | ✅ Running — systemd, auto-start |

---

## 9. Files Modified — Complete List

| File | Changes Applied |
|---|---|
| `src/network/mod.rs` | Noise IK deadlock fix (§3.1); GroupAction removed from noise_eligible (§3.2); gossipsub author fix (§3.3); GroupManifest secret removed (§3.5); self-relay guard defensive (§3.8); v34 config restored (§5) |
| `for_linux/src/network/mod.rs` | All of above + self-relay guard primary (§3.8); peer discovery dedup `announced_peers` (§3.10) |
| `src/lib.rs` | DB retry on corruption (§3.6); GroupSend diagnostic logging (§3.4) |
| `for_linux/src/lib.rs` | DB retry on corruption (§3.6); GroupSend diagnostic logging (§3.4); **mDNS disabled** `enable_mdns: false` (§3.9) |
| `src/media/mod.rs` | Added `purpose` field to `WebRtcSignal` (§3.7) |
| `for_linux/src/media/mod.rs` | Same `purpose` field (§3.7) |
| `lib/src/native/introvert_client.dart` | FFI result logging in `sendGroupMessage` (§3.4) |
| `src/network/behaviour.rs` | v34 network parameters restored (§5) |
| `src/network/config.rs` | thinkpad.local RBN added to bootstrap list |

---

## 10. All Regressions — Final Status

| Regression | Severity | Fixed? |
|---|---|---|
| `GroupAction` added to `noise_eligible` — silent drops | Critical | ✅ Fixed (§3.2) |
| Gossipsub `propagation_source` used as author — relay messages rejected | Critical | ✅ Fixed (§3.3) |
| Noise IK deadlock on session desync — permanent stall | High | ✅ Fixed (§3.1) |
| RBN self-relay loop — OFFLINE cycling, no relay circuits | High | ✅ Fixed & Deployed (§3.8) |
| mDNS on headless RBN server — noise, state churn | Medium | ✅ Fixed & Deployed (§3.9) |
| Duplicate peer discovery callbacks — 20× same peer | Medium | ✅ Fixed & Deployed (§3.10) |
| Gossipsub heartbeat 30s (was 10s) — slow propagation | Medium | ✅ Restored (§5) |
| Request-response max 2MB (was 10MB) — large payloads fail | Medium | ✅ Restored (§5) |
| `GroupManifest` included `secret` — key leaked in wire format | Security | ✅ Fixed (§3.5) |
| DB crash on seed mismatch — engine fails to start | Medium | ✅ Fixed (§3.6) |
| WebRTC file transfer triggers VoIP UI | Low | ✅ Fixed (§3.7) |

---

## 11. Remaining Next Steps

### Priority 1 — On-Device Group Chat Test
With the RBN now achieving `RELAY CONNECTED`, rebuild the Android `.so` and test end-to-end:

```bash
# On Android device:
adb logcat | grep -E "GroupSend|sendGroupMessage|Event 21|Type=21|Mesh.*connected"
```

Expected: `[GroupSend] Sending to N members` on sender → `Event 21` on receiver.

### Priority 2 — Rebuild macOS + Android with Latest Fixes
The self-relay guard (§3.8) and peer-discovery dedup (§3.10) were also applied to `src/network/mod.rs`. Rebuild native libs:

```bash
make mac      # macOS dylib
make android  # Android arm64 + x86_64
```

### Priority 3 — Monitor RBN Under Real Load
After clients connect, verify relay circuits stay stable:

```bash
ssh root@47.89.252.80 "journalctl -u introvertd -f 2>&1 | grep -v 'Binary/Hex'"
```

---

## 12. Test Suite Status

**17/18 suites pass (35 tests + 1 ignored):**

| Suite | Result |
|---|---|
| lib (9 unit tests) | ✅ Pass |
| asynchronous_contiguity_audit | ✅ Pass |
| economic_cohesion_audit | ✅ Pass |
| group_file_transfer_audit | ✅ Pass |
| nat_traversal_audit | ✅ Pass |
| persistence_audit | ✅ Pass |
| foundation_test | ✅ Pass |
| webrtc_stress_test | ⏭️ Ignored (needs live ICE) |
| All others | ✅ Pass |
