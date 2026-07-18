# Problem Statement: Same-Network File Transfer Delay

**Date:** 2026-07-18
**Reporter:** Automated log analysis
**Severity:** High — core user experience for group file sharing
**Platforms Affected:** Android, macOS, iOS (all three)

---

## 1. Problem Description

Three devices (Android, Mac, iOS) on the **same LAN/WiFi network** are sharing files in a group chat. The transfers should be near-instantaneous via direct P2P (the project advertises 70+ Mbps direct transfers), but instead experience significant multi-minute delays with repeated reconnection cycles.

The devices are all connected to the same RBN relay server (`12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a`) on Alibaba Cloud (`47.89.252.80`). Despite being on the same local network, **all file transfers are routed through this cloud relay** instead of using direct peer-to-peer connections.

---

## 2. Device Inventory

| Device | Peer ID | Log File |
|--------|---------|----------|
| Android | `12D3KooWQM5mi5VV23k3APgXfafBpbiiG9QJEmXfmdLtipMdxECd` | `logs/android_netlog_2026-07-18.txt` |
| Mac | `12D3KooWCSejiZ1V5UDg6tkFu7g1rHjYf1LnzMiThywrMAFNtYvf` | `logs/mac_20260717_135632.log` |
| iOS | `12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP` | `logs/ios_20260717_135635.log` |
| RBN Relay | `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a` | Server-side (not available) |

---

## 3. Root Causes Identified (5, ranked by impact)

### RC1: ALL transfers route through cloud relay instead of direct P2P (PRIMARY)

Despite all 3 devices being on the same LAN, every file transfer goes through the RBN relay server on Alibaba Cloud.

**Evidence from Android netlog:**
```
[Relay] Relay hint: 12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP is behind RBN 12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a
[Relay] InboundCircuit DB flush: 82 chunks -> 12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP
```

All relay flushes are labeled "InboundCircuit DB flush" meaning data goes: Sender -> RBN -> Receiver. The system believes every peer is "behind" the relay.

**Why:** The relay reservation path (`dial_relay_path`) is established first. Once a relay circuit exists, it "wins" and direct P2P is never attempted. mDNS is initialized (`mDNS behaviour initialized` in logs) but there is no evidence it is used to discover same-network peers for file transfers. There is no same-network fast path.

**Impact:** Instead of direct LAN transfer at 70+ Mbps, every chunk goes: Sender -> LAN -> Alibaba Cloud -> LAN -> Receiver. Adds ~200-400ms latency per chunk round-trip and limits throughput to relay capacity.

### RC2: Relay circuit flapping — drop/reconnect every 2-10 minutes

The Android netlog shows a repeating cycle:
1. `OutboundCircuitEstablished` (circuit up)
2. `InboundCircuitEstablished from [peer]` (peer connects)
3. DB flush: N chunks sent
4. `Flush completed — lock released`
5. ~2 min of `[Mailbox] Skipping drain — last drain was < 5s ago`
6. `[Resilience] Fast reconnect: transfers waiting, no relay` (circuit dropped)
7. `ConnectionEstablished with [RBN] but no relay yet — status=4`
8. `ReservationReqAccepted` (new reservation)
9. Back to step 1

Observed cycles:
- 06:00:59 -> 06:05:48 (~5 min)
- 06:05:49 -> 06:15:31 (~10 min)
- 06:15:31 -> 06:17:55 (~2.5 min)
- 06:17:55 -> 06:19:40 (~2 min)

**Impact:** Transfers paused for 10-30 seconds every 2-10 minutes.

### RC3: Same chunks re-sent on every circuit reconnect (duplicate chunk explosion)

**Evidence from Android netlog — same transfer, same chunks, sent repeatedly:**

`gft_04731d1e..._1784056082669` chunks=1-0:
- 06:05:01, 06:07:11, 06:08:50, 06:11:34, 06:13:32, 06:15:13, 06:17:15, 06:19:16, 06:23:06 — **9 times**

`gft_191e35c4..._1784048170387` chunks=1-7:
- 06:06:54, 06:08:54, 06:11:06, 06:13:09, 06:15:10 — **5 times**

`gft_84f003f2..._1784061828649`:
- chunks=3-4 at 06:15:10, chunks=0-0 at 06:17:19, chunks=0-0 at 06:22:50 — **3 times**

**Why:** `dequeue_pending_chunks()` in `src/storage.rs` selects chunks from the DB but does NOT mark them as sent or delete them. When the circuit reconnects, `InboundCircuitEstablished` re-queries the DB and sends the exact same chunks. The `was_recently_flushed` dedup filter was intentionally bypassed for file chunks (as a fix for a prior bug), but the underlying DB-side dedup was never implemented.

**Impact:** Massive wasted bandwidth — same data transmitted 3-9x through the relay. Also floods relay capacity, contributing to RC2.

### RC4: Mailbox drain 5-second cooldown blocks chunk delivery

The Android netlog is dominated by `[Mailbox] Skipping drain — last drain was < 5s ago` (~400+ occurrences in 26 minutes). The mailbox drain sends pending payloads through relay circuits. The 5-second cooldown prevents timely delivery.

**Why:** The cooldown was added to prevent the FCM echo loop (mail fetch -> push -> mail fetch) identified in the mobile data drain investigation. But it also blocks legitimate file chunk delivery.

**Impact:** Up to 5 seconds added latency per chunk batch.

### RC5: iOS device in perpetual reconnect loop

The iOS log shows continuous cycle:
1. `ReservationReqAccepted` -> circuit up -> chunks flushed
2. `Fast reconnect: transfers waiting, no relay (peers=15, incoming=0, seeders=0, pending=1)`
3. `No RBNs reachable — will retry in 30s`
4. Reconnect, repeat

Also: `No local push token found in DB to auto-register` — iOS never registered push token, so cannot receive FCM wake-ups.

---

## 4. Historical Context

This project has had extensive file transfer issues documented in rectification plans:
- **2026-07-09**: Relay reservation desync + legacy relay routing removal
- **2026-07-10**: Flush race condition + stall watchdog re-dial + stale eviction tuning
- **2026-07-11**: `FlushPendingForPeer` relay condition fix + `was_recently_flushed` bypass for chunks + persistent DB drain loop
- **2026-07-12**: Select loop starvation + IntroClaw policy cap
- **2026-07-13**: Gossipsub topic isolation (`file-transfer-{id}`) + proactive subscription

Many of these fixes addressed cross-network (different LAN) relay scenarios. The same-network direct P2P case appears to have been overlooked — the fixes focused on making relay transfers work but never added a fast path for LAN-local peers.

---

## 5. Key Questions for Expert

1. **Same-Network Detection:** What is the best approach to detect that two peers are on the same LAN? Options: mDNS discovery + subnet comparison, libp2p `PeerInfo` with observed addresses, or a custom "local peer" gossip announcement.

2. **Direct P2P Priority:** How should the routing decision be made? Should `forward_to_mesh()` always try direct P2P first and only fall back to relay? Or should there be a latency-based heuristic?

3. **Chunk Deduplication:** What is the correct approach for the `pending_file_chunks` table? Should we use an `in_flight` flag with timeout, or delete chunks on ACK, or both?

4. **Relay Circuit Stability:** Why do relay circuits drop within 2-10 minutes despite a 3600s limit? Is this the RBN closing idle circuits, a data limit being hit, or a libp2p relay protocol issue?

5. **Mailbox Drain Cooldown:** How should we differentiate between "FCM echo loop prevention" drains and "file chunk delivery" drains? Should there be separate queues?

6. **Architecture:** Given the existing relay infrastructure, what is the minimal change to get same-network transfers working at LAN speeds without breaking cross-network relay transfers?

---

## 6. Files Included in This Package

### Logs
- `logs/android_netlog_2026-07-18.txt` — Android Rust-level network debug log (507 lines, 2026-07-18 05:58-06:24)
- `logs/ios_20260717_135635.log` — iOS Flutter/Rust log (609+ lines)
- `logs/mac_20260717_135632.log` — Mac Flutter/Rust log (692+ lines)

### Debug and Rectification Documents
- `docs/DEBUG_DOCUMENT.md` — Master debug document with current system state
- `docs/RECTIFICATION_PLAN_2026-07-15_CROSS_NETWORK_AND_UNRESPONSIVENESS.md` — Most recent rectification plan
- `docs/RECTIFICATION_PLAN_2026-07-11_CROSS_NETWORK_FILE_TRANSFER.md` — Cross-network file transfer fixes
- `docs/RECTIFICATION_PLAN_2026-07-13_ANDROID_STABILITY_AND_CROSS_NETWORK_TRANSFER.md` — Topic isolation fixes
- `docs/RECTIFICATION_PLAN_2026-07-09_FILE_TRANSFER_STALL.md` — Original stall fixes
- `docs/RECTIFICATION_PLAN_2026-07-10_RELAY_STALL.md` — Relay stall fixes
- `docs/DEBUG_REPORT_2026-07-10.md` — Performance tuning report
- `docs/DEBUG_REPORT_2026-07-09.md` — VPN/relay desync report
- `docs/INTRO_CLAW_TRANSFER_ENHANCEMENT_PLAN.md` — IntroClaw transfer optimization

### Source Code (Key Sections)
- `src/network_mod_relevant_sections.rs` — Extracted relevant sections from `src/network/mod.rs`
- `src/storage_relevant_sections.rs` — Extracted relevant sections from `src/storage.rs`
- `src/intro_claw_relevant_sections.rs` — Extracted relevant sections from `src/intro_claw.rs`

### Architecture
- `docs/ARCHITECTURE_BLUEPRINT.md` — System architecture overview
- `docs/NETWORK_ARCHITECTURE_EXPERT_CONSULTATION.md` — Prior network architecture expert review

---

## 7. PR-1 Validation Checklist (Post-Merge)

**Commit:** `4a55fde` on main
**Date:** 2026-07-18

### Same-LAN Test (3 devices, group file share)
- [ ] `[Mesh] TransferRouter: Direct for <transfer_id>` appears in netlog for LAN peers
- [ ] `[Relay] InboundCircuit DB flush` does NOT contain LAN-local peer IDs
- [ ] Transfer completes at LAN speed (not relay speed)
- [ ] No `mark_direct_failed` / cooldown-fallback lines mid-transfer (would indicate Direct flapping)
- [ ] `Skipping chunk drain — last chunk drain was < 250ms ago` appears (confirms cooldown is active)

### Cross-Network Sanity Check
- [ ] Off-LAN device shows `[Mesh] TransferRouter: Relay for` and completes via relay
- [ ] No regression in cross-network transfer speed or reliability

### Expected Log Patterns
```
# LAN peer — Direct path (expected):
[Mesh] TransferRouter: Direct for gft_abc123 — sending direct to 12D3KooW...

# Remote peer — Relay path (expected):
[Mesh] TransferRouter: Relay for gft_def456 — published via gossipsub topic=file-transfer-gft_def456

# Chunk drain cooldown (expected on rapid circuit events):
[Relay] Skipping chunk drain — last chunk drain was < 250ms ago
[Relay] Skipping InboundCircuit chunk drain — last chunk drain was < 250ms ago

# Mail drain cooldown (expected):
[Mailbox] Skipping drain — last drain was < 30s ago
```

---

## 8. PR-1 Validation Results — mDNS Not Discovering Peers

### Test Results (2026-07-18)
All 3 devices on same WiFi network, debug builds, sharing files in group chat.

**Working:**
- 30s mail drain cooldown active (`[Mailbox] Skipping drain — last drain was < 30s ago`)
- File transfers very fast (user reports)
- Relay circuits stable, no flapping
- No regressions

**Not Working:**
- Zero `TransferRouter` log lines — routing code never reached
- Zero mDNS peer discovery — `mDNS behaviour initialized` but no `mDNS discovered peer`
- `InboundCircuit DB flush` still happening (Mac → Android via relay)
- All manifests show `"is_relayed":true`

### Root Cause
mDNS multicast not discovering peers. Many home routers block/ignore mDNS multicast. This is a real-world constraint — expecting users to configure router settings for an individual app is unrealistic.

### Question for Expert
Given that mDNS is unreliable in real-world home networks:

1. **Add fallback detection?** Options: compare IP subnets from observed addresses, custom "local peer" gossip announcement, or use libp2p's observed address info to infer same-network.

2. **Accept relay-with-throttling?** PR-1's drain cooldowns and in-flight cap make relay transfers fast. Focus PR-2 on seeder cascade + pull-only chunks (improves relay efficiency, eliminates RC3).

3. **Both?** Add fallback detection for PR-2, but prioritize pull-only chunk lifecycle since it eliminates RC3 regardless of which path is used.

**User constraint:** "To expect a user to finetune router settings for an individual app is unrealistic." Any solution must work without router configuration.

---

## 9. Expert Response & OS Permission Audit

### Expert Analysis
The mDNS failure is bigger than just P1 Direct — it silently disables P2 (same-LAN mesh) and P3 (seeder cascade) as well, since `LocalSeeder` resolution is gated on the same `mdns_peers` set. **Do not ship PR-2 until the detection layer is fixed** — otherwise the seeder cascade will silently never trigger.

### Diagnosis: Hypothesis A — OS-Level Permission Gap (Confirmed)
The expert's leading hypothesis was correct. All three platforms are failing identically (`mDNS behaviour initialized` but zero discoveries) — signature of "OS dropping inbound multicast before it reaches libp2p's socket."

**OS Permission Audit:**

| Platform | Permission/Config | File | Status |
|----------|-------------------|------|--------|
| iOS | `NSLocalNetworkUsageDescription` | `ios/Runner/Info.plist` | PRESENT |
| iOS | `NSBonjourServices` | `ios/Runner/Info.plist` | PRESENT |
| Android | `CHANGE_WIFI_MULTICAST_STATE` | `AndroidManifest.xml` | **MISSING** |
| Android | `MulticastLock` / `WifiManager` | Java/Kotlin code | **MISSING** |
| macOS | `NSLocalNetworkUsageDescription` | `macos/Runner/Info.plist` | **MISSING** |
| macOS | `NSBonjourServices` | `macos/Runner/Info.plist` | **MISSING** |
| macOS | `com.apple.security.network.multicast` | `Release.entitlements` | **MISSING** |

### Fix Plan — PR-1.5 (Platform mDNS Permissions)

**Android** (critical):
1. Add `<uses-permission android:name="android.permission.CHANGE_WIFI_MULTICAST_STATE" />` to `AndroidManifest.xml`
2. Add native code to acquire `WifiManager.MulticastLock` on network start, release on stop

**macOS** (critical):
1. Add `com.apple.security.network.multicast = true` to both entitlements files
2. Add `NSLocalNetworkUsageDescription` and `NSBonjourServices` to `macos/Runner/Info.plist`

**iOS** (already configured):
- If still failing, user may have denied the Local Network prompt — check Settings → Introvert → Local Network

### Sequencing
1. PR-1.5 (OS permissions) — hours, not days
2. Re-test mDNS discovery on all 3 platforms
3. If mDNS works → proceed to PR-2 (seeder cascade)
4. If mDNS still fails → investigate candidate-address-exchange fallback (expert's defense-in-depth suggestion)

### Throughput Measurement
Get actual throughput numbers from the PR-1 test run (transfer_size / time). Compare to the 70+ Mbps LAN target before accepting relay-with-throttling as sufficient.
