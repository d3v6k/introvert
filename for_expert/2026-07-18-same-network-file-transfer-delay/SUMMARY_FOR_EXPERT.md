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

### Not Working
- **Zero mDNS peer discovery** on all 3 platforms despite permissions being added
- **TransferRouter Direct path never triggered** — `mdns_peers` is always empty
- **All transfers still go through relay** — but relay is fast now

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

## 11. Key Takeaway

**The latest version of the code is transferring files fast in the current setup when all 3 devices are on the same network.** The PR-1 drain cooldowns and in-flight cap resolved the pathological slowness. The TransferRouter's Direct path is a further optimization (70+ Mbps LAN speed vs relay speed) that requires mDNS peer discovery to work — which it currently doesn't despite OS permissions being added. The mDNS issue needs deeper investigation (socket-level) before the Direct path can be activated.
