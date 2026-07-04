# Release Notes: v50 — "Delivery Fixes & System Hardening"
**Date:** July 1, 2026
**Version:** `0.21.3`
**Predecessor:** v49 (`0.21.2`)

---

## Executive Summary

v50 addresses critical delivery, sync, and cross-network issues:

1. **Persistent File Chunk Queue**: New `pending_file_chunks` SQLite table ensures file chunks survive app restarts. No more data loss when sender is killed mid-transfer.
2. **VPN Stale Reservation Detection**: Status check interval reduced from 120s to 30s. Force-clears and re-dials when RBN connections exist but no relay reservation.
3. **DCUtR Hole-Punching**: `InboundCircuitEstablished` now triggers `swarm.dial()` for direct connection upgrade. If successful, transfers switch from relay to direct P2P.
4. **Status Downgrade Protection**: New `update_message_status_if_higher()` with monotonic transition rules prevents message status from going backward (e.g., Read→Sent).
5. **Sync Hardening**: `sync_in_progress` HashMap with 60s timeout prevents concurrent syncs and permanent lockout. Sender authorization added to for_linux ChatSyncResponse handler.
6. **Relay Reservation Three-Tier Fallback (Gemini)**: `Identify` handler now prioritizes `bootstrap_nodes` (public IPs) over `info.listen_addrs` (which contained private VPC IP `172.19.0.4`). Fixes relay reservation failures on Alibaba Cloud RBN. Deployed to `47.89.252.80`.

---

## Changes

### Networking
- `dial_relay_path` parameterized with `for_file_chunk: bool` — file chunks iterate ALL RBNs without breaking
- `pending_file_chunks` SQLite table with UNIQUE(transfer_id, chunk_index) constraint
- Relay hint optimization — RBNs sorted with hinted RBN first (priority 0), then by latency
- VPN stale reservation detection — force-clear and re-dial when RBN connected but no reservation
- DCUtR on InboundCircuitEstablished — triggers hole-punch attempt for direct upgrade
- **Relay reservation three-tier fallback** — `bootstrap_nodes` → `anchor_mappings` → filtered `listen_addrs` (private IPs excluded)

### Sync & Delivery
- `update_message_status_if_higher()` — monotonic transition rules (0→3, 0→1, 0→2, 3→1, 3→2, 1→2)
- `sync_in_progress` HashMap with 60s timeout cleanup
- Fixed permanent lockout bug: sync_in_progress removed on unauthorized ChatSyncResponse
- [FILE]: filter in ChatSyncResponse (defense in depth)
- for_linux sender authorization for ChatSyncResponse

### Data Integrity
- Chunks NOT removed after forwarding — deferred until FileTransferComplete arrives
- Deferred chunk removal prevents data loss on failed delivery
- Stale chunks (>24h) cleaned up periodically

---

## Files Modified

| File | Changes |
|------|---------|
| `src/network/mod.rs` | dial_relay_path parameterized, relay hint, VPN detection, DCUtR, sync hardening, three-tier fallback |
| `src/network/types.rs` | Added `relay_hint` to FileChunkRequest |
| `src/storage.rs` | `pending_file_chunks` table, `update_message_status_if_higher`, chunk queue methods |
| `for_linux/src/network/mod.rs` | Same changes + sender authorization, relay reservation fix |
| `for_linux/src/storage.rs` | Same storage changes |

---

## Gemini Session Fixes (Verified)

### Relay Reservation Three-Tier Fallback
**Root cause:** `Identify` handler prioritized `info.listen_addrs`, which included the RBN's private VPC IP (`172.19.0.4`). Relay reservation dials to this address failed silently until the 30s status check retry.

**Fix:** Three-tier fallback in both client and RBN daemon:
1. `bootstrap_nodes` lookup (always public IPs) — first choice
2. `anchor_mappings` lookup (captured from direct connections) — second choice
3. Filtered `info.listen_addrs` (private IPs excluded) — last resort

**Deployed:** Compiled and deployed to Alibaba RBN server (`47.89.252.80`).

### File Transfer Bubble Receiver UI
**Claim:** "Removed suppression guard hiding entire transfer card for unverified incoming transfers."

**Actual behavior:** `SizedBox.shrink()` at line 948 (inside `_buildThumbnailWidget()`) only suppresses the **thumbnail preview** for unverified incoming transfers — NOT the entire card. The main `build()` method (line 475) has no early-return guard.

**What the receiver sees:**
- Progress indicator (CircularProgressIndicator)
- Filename (for non-media)
- Status text "pulling from mesh"
- Cancel button
- Linear progress bar

**What is hidden:** Thumbnail preview for media files until `isVerified` is true.

**Verdict:** Correct behavior. Show transfer progress and controls, but don't render unverified media content.

---

## Build & Deploy

```bash
cargo check             # Verify both trees compile
make mac                # Rebuild macOS client
make android            # Rebuild Android client
./deploy_rbn.sh         # Deploy to RBN (relay reservation fix)
```

## How to Test

1. **VPN test**: Mac on VPN → send text to Android on mobile data → arrives
2. **Cross-network file test**: Mac on WiFi, Android on cellular → send image → arrives
3. **Sync test**: Open chat after sync → messages NOT rolled back, status doesn't go backward
4. **Direct P2P regression**: Both on same WiFi → files and messages work normally
5. **App restart test**: Start file transfer → kill sender app → restart → chunks resume from DB
6. **Relay reservation test**: Check logs for `bootstrap_nodes` priority in Identify handler
