# Release Notes — v52 (0.22.0) "Adaptive Networking"
**Date:** 2026-07-02

## Milestone
All NETWORKING_STABILIZATION_PLAN phases 1–4 implemented and compiled clean (0 errors, 29 warnings). Adaptive pipeline depth, DCUtR remote peer upgrades, mobile data awareness, VoIP-aware throttling, relay-aware cross-network routing, group gossip optimization, and Sovereign Swarm seeding hardening.

---

## Phase 1: Speed & Auth Hardening

### Pipeline Depth — Now Adaptive
- `get_optimal_pipeline_depth()` reads per-peer throughput sliding window (10 observations)
- Tiers: ≥10 MB/s → 16 chunks, ≥1 MB/s → 8 chunks, <1 MB/s → 4 chunks
- No observation history → fallback to v51 defaults (12 direct / 8 relay)
- Wired into: manifest arrival, relay mid-transfer transition, watchdog pull retry

### Pacing — Now Adaptive
- Base: 50ms relay / 10ms direct
- On mobile data: 75ms relay / 15ms direct (1.5x multiplier)

### Auth Hardening
- **RBN auth relaxation**: `is_bootstrap` grants blanket access for `FileChunkRequest`. Non-RBN peers verified via group-member or contact check.
- **ChatSyncResponse auth**: Authorization now enforced for relayed messages (was bypassed when `is_relay=true`).
- **Stale FileTransferComplete guard**: Checks `active_seeders.contains_key()` before processing completion ACKs.

### Network Resilience
- **IPv6 listeners**: `/ip6/::/tcp/{port}` and `/ip6/::/udp/{port}/quic-v1` for NAT64/mobile data reachability.
- **Proactive relay reservation**: All devices request relay reservations immediately during bootstrap.
- **Progressive reconnect ladder**: 4-step escalation in `status_check_interval` (30s): reservation → redial → tunnel → offline.
- **Status check interval**: 120s → 30s for faster VPN stale reservation recovery.

---

## Phase 2: Group Messaging & Sovereign Swarm

### Group Gossip Optimization
- `BroadcastGroupMessage` handler now snapshots `connected_peers` + `mesh_active_peers` before `tokio::spawn`
- Direct `ForwardMeshSignaling` only spawned for peers in either set
- Offline peers handled by gossipsub propagation + mailbox drain
- Applied to both `src/network/mod.rs` and `for_linux/src/network/mod.rs`

### Sovereign Swarm Seeder Ordering
- `RegisterSeeder` + `kademlia.start_providing()` now execute AFTER `std::fs::write()` succeeds
- Eliminates race condition: Kademlia provider records could advertise files before disk flush
- Wrapped in `if write_result.is_ok()` guard — disk failures don't register stale seeder paths

---

## Phase 3: Intro-Claw Intelligent Networking

### DCUtR Upgrade Support (`src/intro_claw.rs:290`)
- New `should_attempt_dcutr()` method on `ConnectionOptimizer`
- Gates remote peer upgrades on `peer_scores` > 0.5
- Unknown peers get baseline discovery attempt (`true`)
- `should_attempt_direct_upgrade()` now returns `has_mdns || self.should_attempt_dcutr(peer_id)`
- `execute_claw_actions` already calls `swarm.dial(peer_id)` — triggers DCUtR automatically when relay reservation exists

### Adaptive Pipeline Depth (`src/intro_claw.rs:557`)
- New `get_optimal_pipeline_depth()` on `AdaptiveChunkSizer`
- Reads throughput sliding window (same data as `get_optimal_chunk_size`)
- Three tiers: ≥10 MB/s → 16, ≥1 MB/s → 8, <1 MB/s → 4
- Proxy on `IntroClawService` passes `self.is_mobile_data` automatically

### Mobile Data Awareness (6 files)
- `ClawTickContext` extended with `is_mobile_data: bool` + `network_type: String`
- `NetworkCommand::IntroClawTick` variant extended with same fields
- `IntroClawService` stores mobile state from tick context
- FFI: `intro_claw_trigger_tick(is_mobile_data: bool)` accepts cellular state
- Dart: all 3 call sites pass `_lastConnectivity == ConnectivityResult.mobile`
- Throttling rules on cellular:
  - Pipeline hard-cap: 4 relay / 6 direct
  - Pacing 1.5x: 75ms relay / 15ms direct
  - Mailbox fetch: skip every other tick

---

## Phase 3.4: VoIP-Aware Transfer Throttling

### Pipeline Collapse During Active Calls
- `get_optimal_pipeline_depth()` checks `is_call_active` FIRST — returns 2 when true
- Prevents media buffer contention during voice/video calls
- Proxy on `IntroClawService` passes `self.voip_monitor.is_call_active()` automatically

### Pacing Inflation
- Base pacing inflated to 250ms when VoIP call is active (was 50ms relay / 10ms direct)
- Applied at `src/network/mod.rs:5165` in the `FileTransfer` manifest handler

---

## Phase 4: Relay Routing & Cross-Network Fixes

### Relay-Aware File Payload Routing (`forward_to_mesh:2552`)
- **Root cause**: `swarm.is_connected(&recipient_id)` returns false for relay-connected peers. File chunks buffered in `pending_messages` for minutes until relay circuit establishes.
- **Fix**: Check `relay_hints` map for recipient. If RBN connected, send directly via `request_response.send_request(rbn_id, payload)`. Bypasses `is_connected()` check entirely.

### Relay Hint Population on InboundCircuitEstablished (`mod.rs:1549`)
- When receiver establishes inbound circuit through RBN, records `relay_hints[src_peer_id] = rbn_id`
- Populates the relay-aware routing map for subsequent chunk sends

### Proactive Relay Dial in SendFileChunk (`mod.rs:3474`)
- Calls `dial_relay_path(peer_id, true)` before `forward_to_mesh`
- Starts relay circuit establishment when FIRST chunk is sent, not when `forward_to_mesh` buffers

### Fast Reconnect Interval (`mod.rs:366`)
- 5-second interval activates when transfers waiting AND no relay listener
- Dials disconnected RBNs and requests relay reservations
- Self-healing: deactivates once relay establishes

### Step 1 Reconnect Ladder Fix (`mod.rs:598`)
- Now dials disconnected RBNs before requesting reservations
- Previously only checked `is_connected(rbn_id)` which was always false

### Group ACK Completion Fix (`mod.rs:5965`)
- Changed from `current_completions >= total_members` to `current_completions >= 1`
- Sender shows "verified" when ANY group member confirms, not all

### OutboundCircuitEstablished Flush Delay (`mod.rs:1505`)
- Increased from 500ms to 1500ms
- Gives `is_connected()` time to update after relay dial completes

---

## Files Modified

| File | Phase | Changes |
|------|-------|---------|
| `src/intro_claw.rs` | 3 | `ClawTickContext` +2 fields, `should_attempt_dcutr()`, `get_optimal_pipeline_depth(is_mobile, is_call_active)`, `is_on_mobile_data()` proxy, `is_mobile_data` field + init |
| `src/network/mod.rs` | 1, 2, 3, 4 | Pipeline adaptive (3 sites), pacing 1.5x mobile, VoIP pacing 250ms, mobile mailbox skip, group gossip filter, seeder write-before-register, relay-aware routing, relay hint on InboundCircuit, proactive relay dial, fast reconnect, Step 1 ladder fix, group ACK fix, flush delay 1500ms |
| `src/network/service.rs` | 4 | `last_file_chunk_dial` field |
| `src/network/types.rs` | 3 | `IntroClawTick` variant +`is_mobile_data`/`network_type` |
| `src/lib.rs` | 3 | `intro_claw_trigger_tick(is_mobile_data: bool)` FFI signature |
| `for_linux/src/network/mod.rs` | 2, 4 | Group gossip filter, `disconnect_peer_id` fix |
| `lib/src/native/introvert_client.dart` | 3 | FFI typedef +`Bool isMobileData`, public method named param |
| `lib/src/ui/main_shell.dart` | 3 | All 3 `triggerIntroClawTick()` call sites pass `isMobileData` |

---

## Compilation Status
- **Client (`cargo check`)**: 0 errors, 29 warnings
- **RBN (`for_linux/`)**: 0 errors, 20 warnings

---

## Known Limitations
- **Relay establishment time**: 15-30s for cross-network transfers (libp2p circuit negotiation protocol limitation)
- **Exit/re-enter chat workaround**: Triggers `ForceMeshRefresh` for faster relay recovery
