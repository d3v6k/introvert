# Expert Consultation Package — Complete Summary

**Date:** 2026-07-18
**Project:** Introvert Sovereign Messenger
**Issue:** Same-network file transfer delay — 3 devices on same LAN experiencing multi-minute delays instead of near-instantaneous direct P2P transfers
**Current Status:** File transfers are now fast via relay after PR-1 fixes. mDNS discovery still not working — Direct path (P1) never triggered.

---

## 1. Project Overview

Introvert is an open-source, privacy-focused, decentralized P2P mesh messenger with an integrated token economy. It operates via a crowdsourced, self-healing peer-to-peer mesh network with no central servers.

- **Tech Stack:** Rust core (libp2p 0.56, SQLite/SQLCipher, WebRTC, Solana SDK) + Flutter/Dart UI
- **Platforms:** Android, macOS, iOS
- **Architecture:** Three daemons — Client (libintrovert), RBN (introvertd relay server on Alibaba Cloud), Economy (introvert-solana)

---

## 2. Problem Statement

Three devices (Android, Mac, iOS) on the **same LAN/WiFi network** sharing files in a group chat experienced significant multi-minute delays. All file transfers were routed through the RBN relay server on Alibaba Cloud instead of using direct peer-to-peer connections.

### Device Inventory

| Device | Peer ID |
|--------|---------|
| Android | `12D3KooWQM5mi5VV23k3APgXfafBpbiiG9QJEmXfmdLtipMdxECd` |
| Mac | `12D3KooWCSejiZ1V5UDg6tkFu7g1rHjYf1LnzMiThywrMAFNtYvf` |
| iOS | `12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP` |
| RBN Relay | `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a` (Alibaba Cloud 47.89.252.80) |

### Root Causes Identified (5, ranked by impact)

| RC | Problem | Evidence | Impact |
|----|---------|----------|--------|
| RC1 | ALL transfers route through cloud relay even on same LAN | `Relay hint: [peer] is behind RBN` — all flushes are "InboundCircuit DB flush" | ~200-400ms per chunk, limited to relay capacity |
| RC2 | Relay circuit flaps every 2-10 min | `Fast reconnect: transfers waiting, no relay` cycle repeats 4+ times in 26-min log | Transfers paused 10-30s every 2-10 min |
| RC3 | Same chunks re-sent 3-9x on each reconnect | Same transfer ID flushed 9 times in 26-min log | Massive wasted bandwidth, floods relay |
| RC4 | 5s mailbox drain cooldown blocks chunk delivery | `[Mailbox] Skipping drain` appears 400+ times in 26 min | Up to 5s added latency per chunk batch |
| RC5 | iOS stuck in perpetual reconnect loop | `No local push token found in DB` + continuous reconnect | iOS rarely "online", can't receive chunks |

---

## 3. Priority Model (Requested)

```
P1 — Direct P2P               (sender <=> receiver, no relay, as long as directly dialable)
P2 — Same-LAN mesh            (group members on same network pull direct P2P from local file holder)
P3 — Seeder cascade           (first downloader becomes local seeder for the rest)
P4 — RBN relay                (last resort — only when P1-P3 are impossible)
```

The current codebase has the data for P2 (mDNS) but never wires it into the routing decision. `forward_to_mesh()` effectively runs P4 first because gossipsub-over-relay succeeds before direct delivery is even attempted.

---

## 4. What Has Been Implemented

### PR-1: TransferRouter + Drain Cooldown Split (Commit `4a55fde`)

**Changes (2 files, 227 insertions, 65 deletions):**

1. **TransferRouter** — New `resolve()` function that prioritizes Direct > LocalSeeder > Relay
   - Uses `mdns_peers` set for same-network detection
   - `Direct`/`LocalSeeder` paths bypass gossipsub entirely, send via request-response codec
   - `Relay` path uses existing gossipsub (unchanged behavior)
   - Control topic (`file-transfer-{id}`) subscribed for ALL paths (seeder cascade readiness)
   - In-flight cap (8) enforced on Direct/LocalSeeder path
   - `mark_direct_failed()` cooldown: on Direct-path failure, peer is cooled down for 30s so next chunk falls through to Relay
   - `clear_transfer()` wired into all 4 transfer lifecycle cleanup points

2. **Drain Cooldown Split**
   - Mail drain: 5s → 30s (matches FCM echo-loop fix)
   - Chunk drain: new 250ms cooldown on both `OutboundCircuitEstablished` and `InboundCircuitEstablished` handlers

3. **Expert Review Gaps Fixed During Review:**
   - `InboundCircuitEstablished` handler got the 250ms chunk drain cooldown (was only on Outbound)
   - In-flight limit check (8) added to Direct/LocalSeeder path before send
   - `Direct` branch in `resolve()` now checks `is_seeder_in_cooldown` (was only on LocalSeeder)
   - `mark_direct_failed()` wired into both v1 and v2 `OutboundFailure` handlers

**Validation Results:**
- 30s mail drain cooldown: WORKING (`[Mailbox] Skipping drain — last drain was < 30s ago`)
- 250ms chunk drain cooldown: WORKING
- File transfers: FAST via relay (user reports "very fast")
- No regressions: Cross-network transfers still work
- TransferRouter Direct path: NOT TRIGGERED (mDNS not discovering peers)

### PR-1.5: Platform mDNS Permissions (Commit `66ca960`)

**Changes (6 files, 40 insertions):**

| Platform | Change | File |
|----------|--------|------|
| Android | `CHANGE_WIFI_MULTICAST_STATE` permission | `AndroidManifest.xml` |
| Android | MulticastLock in IntrovertService (foreground service lifecycle) | `IntrovertService.kt` |
| macOS | `com.apple.security.network.multicast` entitlement | `Release.entitlements`, `DebugProfile.entitlements` |
| macOS | `NSLocalNetworkUsageDescription` + `NSBonjourServices` | `Info.plist` |
| iOS | `_p2p._udp` added to `NSBonjourServices` | `Info.plist` |

**Validation Results:**
- Android `MulticastLock acquired for mDNS discovery`: CONFIRMED in logs
- macOS multicast entitlement: APPLIED
- iOS `_p2p._udp` added: APPLIED (harmless per expert — does not gate raw multicast)
- **mDNS peer discovery: STILL ZERO on all 3 platforms**

---

## 5. Current State — What's Working and What's Not

### Working
- File transfers are fast via relay path
- 30s mail drain cooldown prevents FCM echo loop
- 250ms chunk drain cooldown prevents thundering herd
- In-flight cap (8) prevents unlimited concurrent requests
- Direct failure cooldown gracefully degrades to Relay
- Android MulticastLock is acquired and released correctly
- **Group messaging fully functional** — text messages delivered to all 3 devices in <1s
- **File transfers completing successfully** — 7 files (~32MB total) transferred via relay in ~2 minutes
- **Relay circuit establishment stable** — both inbound and outbound circuits working
- **Push token auto-registration working on Android** — "Found local token. Auto-registering with RBN"

### Not Working
- **Zero mDNS peer discovery** on all 3 platforms despite permissions being added
- **TransferRouter Direct path never triggered** — `mdns_peers` is always empty
- **All transfers still go through relay** — `is_relayed: true` on all files
- **Push token missing on iOS and Mac** — "No local push token found in DB to auto-register" (Android works)
- **File transfer retry behavior** — files pulled again ~3 minutes after initial pull (possible reconnect-triggered re-pull)

### Key Insight
The PR-1 drain cooldowns and in-flight cap make relay transfers fast enough that the user reports "very fast" file transfers. The TransferRouter's Direct path is a further optimization (70+ Mbps LAN speed) that requires mDNS to work — which it currently doesn't.

---

## 6. mDNS Investigation

### What We Know
- All 3 platforms show `mDNS behaviour initialized` — libp2p mDNS starts successfully
- Android shows `MulticastLock acquired for mDNS discovery` — OS permission is active
- Zero `mDNS discovered peer` lines on any platform
- libp2p-mdns 0.48.0 uses `_p2p._udp` as its service name (confirmed from crate source)

### Hypothesis A — OS Permission Gap (Partially Falsified)
The expert's leading hypothesis was that OS-level permissions were blocking mDNS. PR-1.5 added the missing permissions, but mDNS still doesn't work. This partially falsifies the hypothesis — at least for the current test environment.

### Hypothesis B — Router/AP Client Isolation
Some routers block mDNS multicast between clients on the same SSID. This is plausible but unconfirmed.

### Hypothesis C — libp2p mDNS Configuration Issue
The mDNS behaviour initializes but may not be sending/receiving packets correctly. Needs socket-level investigation.

### Expert's Suggested Fallback
Exchange local candidate addresses via RBN signaling (like ICE candidates in WebRTC) and attempt direct dial. This sidesteps mDNS entirely. Deferred to a future PR.

### Next Steps for mDNS
1. Add `dispatch_debug_log` calls to TransferRouter (the `info!()` calls from `forward_to_mesh` are invisible in Flutter output — this was always the case)
2. Investigate mDNS socket-level behavior (is it actually sending/receiving packets?)
3. Consider the candidate-address-exchange fallback if mDNS can't be fixed

---

## 7. TransferRouter Log Visibility Issue

The TransferRouter `info!("[Mesh] TransferRouter: ...")` log lines don't appear in Flutter output. This is because `forward_to_mesh()` is called from `handle_command()` which uses a different tracing context than the swarm event loop. The `info!()` calls from `forward_to_mesh` were NEVER visible in Flutter logs — even the pre-PR-1 `info!("[Mesh] Published ...")` lines didn't appear.

**Fix needed:** Add `crate::dispatch_debug_log(&format!(...))` alongside the `info!()` calls. `dispatch_debug_log` is what actually routes Rust logs to Flutter's debug output.

This is a **visibility fix, not a logic fix** — the TransferRouter code IS being reached, we just can't see the logs.

---

## 8. Files in This Package

### Documents
| File | Description |
|------|-------------|
| `PROBLEM_STATEMENT.md` | Original problem statement with 5 root causes, log evidence, expert questions |
| `RECTIFICATION_PLAN_2026-07-18_INTELLIGENT_TRANSFER_ROUTING_v2.md` | Merged rectification plan — expert design + log analysis |
| `RECTIFICATION_PLAN_PR-1.5_mDNS_PERMISSIONS.md` | Platform mDNS permissions fix plan with expert feedback |
| `CLAUDE RECTIFICATION_PLAN_2026-07-18_INTELLIGENT_TRANSFER_ROUTING.md` | Original expert plan (pre-merge) |
| `SUMMARY_FOR_EXPERT.md` | This document — comprehensive summary |

### Logs
| File | Description |
|------|-------------|
| `logs/android_netlog_2026-07-18.txt` | Android Rust-level network debug (507 lines, pre-PR-1) |
| `logs/ios_20260717_135635.log` | iOS Flutter/Rust log (609+ lines, pre-PR-1) |
| `logs/mac_20260717_135632.log` | Mac Flutter/Rust log (692+ lines, pre-PR-1) |

### Documentation
| File | Description |
|------|-------------|
| `docs/DEBUG_DOCUMENT.md` | Master debug document with current system state |
| `docs/RECTIFICATION_PLAN_2026-07-15_CROSS_NETWORK_AND_UNRESPONSIVENESS.md` | Most recent rectification plan |
| `docs/RECTIFICATION_PLAN_2026-07-11_CROSS_NETWORK_FILE_TRANSFER.md` | Cross-network file transfer fixes |
| `docs/RECTIFICATION_PLAN_2026-07-13_ANDROID_STABILITY_AND_CROSS_NETWORK_TRANSFER.md` | Topic isolation fixes |
| `docs/RECTIFICATION_PLAN_2026-07-09_FILE_TRANSFER_STALL.md` | Original stall fixes |
| `docs/RECTIFICATION_PLAN_2026-07-10_RELAY_STALL.md` | Relay stall fixes |
| `docs/DEBUG_REPORT_2026-07-10.md` | Performance tuning report |
| `docs/DEBUG_REPORT_2026-07-09.md` | VPN/relay desync report |
| `docs/INTRO_CLAW_TRANSFER_ENHANCEMENT_PLAN.md` | IntroClaw transfer optimization |
| `docs/ARCHITECTURE_BLUEPRINT.md` | System architecture overview |
| `docs/NETWORK_ARCHITECTURE_EXPERT_CONSULTATION.md` | Prior network architecture expert review |

### Source Code (Extracted Key Sections)
| File | Description |
|------|-------------|
| `src/network_mod_relevant_sections.rs` | mDNS handler, relay circuit handlers, forward_to_mesh, dial_relay_path |
| `src/storage_relevant_sections.rs` | pending_file_chunks schema, enqueue/dequeue lifecycle |
| `src/intro_claw_relevant_sections.rs` | Connection optimizer, recommended path, mDNS usage |

---

## 9. Pending Questions for Expert

1. **mDNS not discovering peers despite permissions being added.** What's the next diagnostic step? Socket-level packet capture? Candidate-address-exchange fallback?

2. **TransferRouter logs invisible in Flutter output.** The `info!()` calls from `forward_to_mesh` don't route to Flutter. Need to add `dispatch_debug_log` calls. Is this a known issue with libp2p tracing on mobile?

3. **"Very fast" is unmeasured.** The user reports fast transfers via relay, but we don't have actual throughput numbers. Should we measure before pursuing the Direct path further?

4. **Relay-path-with-throttling may be sufficient.** PR-1's drain cooldowns and in-flight cap make relay transfers fast. Is the Direct path (P1) worth pursuing if relay is "fast enough" for the user's use case?

5. **PR-2 (seeder cascade) depends on mDNS.** The `LocalSeeder` path is gated on `mdns_peers.contains()`. If mDNS can't be fixed, should PR-2 use a different detection mechanism (e.g., candidate-address-exchange)?

---

## 10. Commit History

| Commit | Date | Description |
|--------|------|-------------|
| `4a55fde` | 2026-07-18 07:17 | PR-1: TransferRouter + drain cooldown split |
| `66ca960` | 2026-07-18 08:17 | PR-1.5: Platform mDNS permissions |

---

## 11. Latest Test Session Log Analysis (2026-07-18 15:35–16:12 UTC)

### Test Setup
- **Group:** `f0c2a00be8d2ed210cc09c9ed16d6927` (3 members)
- **Group secret:** `233c82d5...` (not all zeros — encryption working)
- **Duration:** ~37 minutes of continuous logging

### Device Peer IDs
| Device | Peer ID | Push Token |
|--------|---------|------------|
| Android | `12D3KooWQM5mi5VV23k3APgXfafBpbiiG9QJEmXfmdLtipMdxECd` | Found (auto-registering) |
| Mac | `12D3KooWCSejiZ1V5UDg6tkFu7g1rHjYf1LnzMiThywrMAFNtYvf` | Missing |
| iOS | `12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP` | Missing |

### Timeline of Key Events

| Time (UTC) | Event | Device |
|------------|-------|--------|
| 15:32:28 | Push token auto-registration | Android |
| 15:37:28 | Push token auto-registration | Android |
| 15:42:28 | Push token auto-registration | Android |
| 15:47:28 | Push token auto-registration | Android |
| 15:48:56 | `ReservationReqAccepted` (renewal=true) | iOS |
| 15:52:28 | Push token auto-registration | Android |
| 15:57:20 | `OutboundCircuitEstablished` via RBN | iOS |
| 15:57:20 | `InboundCircuitEstablished` from iOS — flush completed | Mac |
| 16:06:41 | Group message "hello" sent | iOS → Mac + Android |
| 16:06:42 | Group message "hello" received | Android |
| 16:06:41 | Group message "hello" received | Mac |
| 16:07:25 | iOS sends file notification to group | iOS |
| 16:07:26 | Mac receives 1 FILE notification (`Off-Grid_Messaging_Delivery_Manual.png`, 20.5MB) | Mac |
| 16:07:27 | Android receives 1 FILE notification | Android |
| 16:08:47 | Mac receives 6 FILE notifications from Android | Mac |
| 16:08:47 | iOS receives 6 FILE notifications from Android | iOS |
| 16:08:48 | Android sends 6 files to group (forwarded to 2 members) | Android |
| 16:10:55 | File pull retries triggered | iOS |
| 16:10:58 | File pull retries triggered | Mac |
| 16:12:19 | `InboundCircuitEstablished` from iOS | Android |
| 16:12:20 | `InboundCircuitEstablished` from Mac | Android |
| 16:12:20 | `OutboundCircuitEstablished` via RBN (3x) | Android |

### Files Transferred (all `is_relayed: true`)

| Filename | Size | Sender | Recipients |
|----------|------|--------|------------|
| `Off-Grid_Messaging_Delivery_Manual.png` | 20.5 MB | Mac | Android, iOS |
| `1000136018.jpg` | 1.97 MB | Android | Mac, iOS |
| `1000135897.jpg` | 3.16 MB | Android | Mac, iOS |
| `1000135896.jpg` | 935 KB | Android | Mac, iOS |
| `1000135927.jpg` | 2.13 MB | Android | Mac, iOS |
| `1000135883.jpg` | 2.16 MB | Android | Mac, iOS |
| `1000135882.jpg` | 2.15 MB | Android | Mac, iOS |

**Total: ~32 MB transferred via relay in ~2 minutes**

### Key Observations

1. **Push token asymmetry** — Android has push token and auto-registers with RBN on every Identify event. iOS and Mac show "No local push token found in DB to auto-register" on every Identify event. This doesn't block transfers (relay works) but may affect push notification delivery.

2. **Relay circuit stability** — Circuits established successfully on all 3 devices. No circuit flapping observed in this session (previous sessions showed flapping every 2-10 min).

3. **File transfer retry pattern** — Files pulled at 16:08:48 are pulled again at 16:10:55-58 (~2 min later). This suggests reconnect-triggered re-pulls. The `start_pull` FFI calls are identical transfer IDs, indicating the Flutter UI is re-requesting already-transferring files.

4. **ConnectionEstablished but no relay** — Android shows `ConnectionEstablished with [peer] but no relay yet — status=4 (connecting)` multiple times at 16:08:08-10. This is normal during relay negotiation but appears 6+ times in rapid succession.

5. **App state cycling** — All devices show Foreground → Backgrounded → Wake-on-push cycles. The Wake-on-push pattern (`BackgroundedPendingWake`) is triggered by incoming messages/files.

6. **30s mail drain cooldown working** — Every device shows consistent "Skipping drain — last drain was < 30s ago" messages, confirming the PR-1 cooldown is active.

7. **Group encryption working** — All GroupAction messages verified successfully, decrypted with non-zero group secret (`233c82d5...`).

---

## 12. Key Takeaway

**The latest version of the code is transferring files fast in the current setup when all 3 devices are on the same network.** The PR-1 drain cooldowns and in-flight cap resolved the pathological slowness. The TransferRouter's Direct path is a further optimization (70+ Mbps LAN speed vs relay speed) that requires mDNS peer discovery to work — which it currently doesn't despite OS permissions being added. The mDNS issue needs deeper investigation (socket-level) before the Direct path can be activated.

### Open Issues for Next Session
1. **Push token missing on iOS/Mac** — investigate why push token is not being saved to DB on these platforms
2. **File transfer retry behavior** — files being pulled again ~2 min after initial pull; may be wasting bandwidth
3. **mDNS zero discovery** — needs socket-level packet capture to determine if mDNS packets are actually being sent/received
4. **TransferRouter log visibility** — add `dispatch_debug_log` calls to see routing decisions in Flutter output

---

## 13. mDNS Isolation Test Results (2026-07-18 16:34 UTC)

### Test: `dns-sd` on Mac Mini

**Finding: mDNS multicast WORKS on this network.** The router is NOT blocking multicast.

```
$ dns-sd -B _services._dns-sd._udp local
_p2p._udp          (registered on interfaces 7 and 15)
_androidtvremote2._tcp  → "Mi TV Stick"
_smb._tcp               → "MOTHERSHIP", "THINKPAD"
```

- `_p2p._udp` is registered as a service type (interfaces 7 and 15)
- Other mDNS services are visible (`Mi TV Stick`, `MOTHERSHIP`, `THINKPAD`)
- **Zero `_p2p._udp` instances** discovered — meaning no Introvert app is currently advertising

### Interpretation

The bug is in the app, not the network. mDNS multicast packets are flowing on this LAN. The libp2p mDNS behaviour initializes (`"mDNS behaviour initialized"` confirmed in logs) but never discovers peers. Possible causes:

1. **libp2p mDNS socket binding issue** — the mDNS socket may not be binding to the correct interface
2. **libp2p mDNS not sending/receiving packets** — needs socket-level packet capture to confirm
3. **mDNS service not being advertised** — the app may not be registering `_p2p._udp` instances

### Code Changes Made (PR-2 Diagnostic Logging)

**File: `src/network/mod.rs`**
- Added `dispatch_debug_log` calls to mDNS `Discovered` handler (line ~1653) — logs raw entries and grouped peer count
- Added new `Expired` handler — logs expired peers and removes from `mdns_peers` set (previously missing, causing `mdns_peers` to grow unbounded)

**File: `src/network/behaviour.rs`**
- Enhanced mDNS initialization log to include peer ID
- Added log when mDNS is disabled by config

**File: `src/network/service.rs`**
- Added `dispatch_debug_log` to `resolve()` — logs `mdns_peers` size, connected status, cooldown state, and chosen path (Direct/LocalSeeder/Relay)

### What to Look For in Next Log Export

After rebuilding and running with the new diagnostic logs:
1. `[mDNS] Behaviour initialized for peer <id>` — confirms mDNS started
2. `[mDNS] Discovered event: N raw entries` — if N=0, mDNS is not receiving packets
3. `[mDNS] Raw entry: peer=... addr=...` — if present, mDNS is working
4. `[TransferRouter] resolve(...) mdns_peers=N` — if N=0, `mdns_peers` is empty (expected if mDNS discovers nothing)
5. `[TransferRouter] → Relay(...)` — confirms all transfers falling through to relay

---

## 14. BREAKTHROUGH: mDNS Working, Direct P2P Achieved (2026-07-18 17:23 UTC)

### Test Results

After rebuilding with diagnostic logs and starting all 3 devices:

**Mac (`12D3KooWCSejiZ1V5UDg6tkFu7g1rHjYf1LnzMiThywrMAFNtYvf`):**
- mDNS initialized: `Behaviour initialized for peer 12D3KooWCSeji...`
- Discovered iOS with 5 addresses on `192.168.1.194` and `192.168.1.241`
- Discovered Android with 4 addresses on `192.168.1.172`
- **36 mDNS log entries total**
- **188 TransferRouter decisions: ALL Direct** (172 to Android, 16 to iOS)

**Android (`12D3KooWQM5mi5VV23k3APgXfafBpbiiG9QJEmXfmdLtipMdxECd`):**
- mDNS initialized: `Behaviour initialized for peer 12D3KooWQM5m...`
- Discovered Mac with 5 addresses on `192.168.1.194`
- Discovered iOS with 5 addresses on `192.168.1.241`
- **34 mDNS log entries total**
- **21 TransferRouter decisions: ALL Direct** (12 to Mac, 9 to iOS)

**iOS (`12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP`):**
- **0 mDNS log entries** — iOS is NOT discovering any peers
- Transfers from Mac/Android to iOS route through relay

### TransferRouter Decision Summary

| Device | Direct | Relay | Direct % | mDNS Peers |
|--------|--------|-------|----------|------------|
| Mac | 188 | 0 | **100%** | 2 (Android + iOS) |
| Android | 21 | 0 | **100%** | 2 (Mac + iOS) |
| iOS | 0 | 0 | N/A | 0 |

### Sample TransferRouter Log (Android)
```
[TransferRouter] resolve(tid=gft_0238a7e62abc0cc4, recipient=12D3KooWN6Hu...): mdns_peers=true, connected=true, cooldown=false, mdns_set_size=2
[TransferRouter] → Direct(12D3KooWN6Hu...)
```

### Key Findings

1. **mDNS is fully working on Mac and Android** — the previous session's zero-discovery was because the 500-entry ring buffer evicted the mDNS lines before export. The diagnostic logs confirm mDNS discovers peers within seconds of app startup.

2. **Direct P2P transfers are live** — Mac↔Android transfers route via Direct path (LAN speed, no relay). This is the P1 priority from the original design.

3. **iOS mDNS is broken** — iOS discovers zero peers. This is an iOS-specific issue (likely Multicast entitlement or network permission). iOS always uses relay.

4. **The `dns-sd` test was misleading** — the initial `dns-sd -B _p2p._udp local` showed zero instances because the app wasn't running at that moment. Once the app is running, mDNS works correctly.

5. **`mdns_peers` set is never cleared** — the new `Expired` handler will fix this, but in the current test the set grew to size 2 (correct — Mac + Android on LAN).

### Remaining Issues

1. **iOS mDNS** — needs investigation (Multicast entitlement, `NSLocalNetworkUsageDescription`, or iOS-specific libp2p mDNS config)
2. **`known_seeders` hardcoded empty** — `LocalSeeder` path (P2/P3) can never trigger until this is wired
3. **`mdns_peers` never shrinks** — Expired handler added but not yet tested
4. **File transfer retry behavior** — files being re-pulled after initial transfer

### Commit History

| Commit | Date | Description |
|--------|------|-------------|
| `4a55fde` | 2026-07-18 07:17 | PR-1: TransferRouter + drain cooldown split |
| `66ca960` | 2026-07-18 08:17 | PR-1.5: Platform mDNS permissions |
| (uncommitted) | 2026-07-18 17:17 | PR-2: mDNS diagnostic logging + TransferRouter visibility |

---

## 15. Cross-Network Transfer Stall Fix (2026-07-18 17:40 UTC)

### Root Cause
When Android moved from LAN to VPN, `mdns_peers` retained stale entries (Mac + iOS). `resolve()` saw `mdns_peers=true` and chose `Direct` path. Direct failed (peer not reachable on VPN), 30s cooldown, repeat for ~2 minutes.

### Fix
Updated `resolve()` in `src/network/service.rs` to check `is_relayed_map` before choosing Direct:
- **Before:** `if (is_mdns || is_connected) && !in_cooldown` → Direct
- **After:** `if is_mdns && !relayed && !in_cooldown` → Direct

This prevents stale `mdns_peers` from choosing Direct when the connection is actually relayed (different network via RBN).

### Code Changes
| File | Change |
|------|--------|
| `src/network/service.rs:39-74` | Added `is_relayed` parameter to `resolve()`, changed Direct condition |
| `src/network/mod.rs:3143-3149` | Updated call site to pass `is_relayed_map` |

### Verification
1. Mac + Android on same LAN → Direct path (unchanged)
2. Android on VPN → immediately falls through to Relay (no 2-minute stall)
3. Android back on LAN → recovers to Direct path
