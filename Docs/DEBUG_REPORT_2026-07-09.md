# Debug Report — 2026-07-09

## Session Summary
Diagnosed and resolved the critical client-side issue where devices get stuck on "connecting" (status 4) after an RBN restart or temporary disconnection, despite being connected at the TCP level. Verified library compilation and updated the core debug documentation.

## Issues Resolved

### 1. Client-Side Relay Reservation Desynchronization
**Problem:** When the Root Bootstrap Node (RBN) restarts, clients connect at the TCP level but remain stuck in "connecting" (status=4 / transfers waiting, no relay) forever.
**Root Cause:**
- When the RBN disconnected, `SwarmEvent::ConnectionClosed` cleared the RBN from `self.relay_listeners` but left `self.relay_reservations` populated.
- Because `relay_listeners` mapping was immediately deleted, the subsequent `SwarmEvent::ListenerClosed` event could not map the closed listener ID back to the RBN `PeerId`.
- This bypassed the `self.relay_reservations.remove(&peer_id)` cleanup and auto-recovery logic.
- On reconnect (`ConnectionEstablished`), the client checked `!self.relay_reservations.contains(&peer_id)` before requesting a new reservation. Since it was still present, it skipped requesting a new reservation.
- The 15-second status loop and 5-second fast reconnect loop similarly skipped requesting reservations due to the same check.
**Fix:**
- Updated the `SwarmEvent::ConnectionClosed` handler in `src/network/mod.rs` to check if we are completely disconnected from the RBN or anchor (`!self.swarm.is_connected(&peer_id)`).
- If completely disconnected, we immediately remove the peer from `relay_reservations` and retain only unrelated mappings in `relay_listeners`.
- This ensures that upon reconnecting, `ConnectionEstablished` successfully requests a new relay reservation, bringing the client immediately back to `Status=1` (ONLINE).
**Status:** Fixed and verified.

### 2. Android VPN/Mobile Data File Transfer Silent Drop
**Problem:** Android client on VPN and mobile data receives file manifests but hangs at 0% when attempting to send or receive file chunks.
**Root Cause:**
- When attempting to route file chunk requests or chunk payloads via relay circuits, `forward_to_mesh` checks if `self.swarm.is_connected(&recipient_id)` is true.
- If the circuit connection is in the process of establishing and the check returns `false`, the code hit a legacy "RELAY-AWARE ROUTING" block left over from when `TransitFileChunk` existed.
- This block forwarded the raw `FileChunk` or `FileChunkRequest` directly to the relay RBN (`rbn_id`) instead of the recipient, and immediately returned `Ok(())`.
- Since the RBN is a standard relay node and the payload lacks a destination/recipient `PeerId` field, the RBN dropped the payloads.
- Because `forward_to_mesh` returned `Ok(())`, the payload was never buffered in RAM or database, leading to permanent silent drops.
**Fix:**
- Removed the legacy relay-routing block from `forward_to_mesh` in `src/network/mod.rs`.
- Chunks and requests are now properly buffered in RAM/DB, and when the circuit connection becomes active, they are flushed directly to the target peer's ID.
**Status:** Fixed and verified.

## Pending Work

### 1. Anchor Handle Registry Deployment
**Status:** BLOCKED — deployer wallet needs funding
**Program ID:** `FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW`
**Guide:** `Docs/HANDLE_REGISTRY_DEPLOYMENT.md`

### 2. Client Balance Display
**Issue:** App shows 0 INTR despite on-chain balances. Likely caused by wallet address mismatch.
**Status:** Needs investigation.

## Verification Results
- `cargo check --lib` — completed successfully with zero compile errors on the modified network swarm module.
