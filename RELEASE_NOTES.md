# Release Notes - Stable v35 "Sovereign Audit"

**Version:** 0.12.0
**Date:** 2026-06-21
**Codename:** Sovereign Audit

## Overview

Stable v35 represents the most comprehensive security and stability audit in Introvert's history. Three rounds of deep code review identified and resolved **54 issues** across the Rust core engine, FFI bridge, Flutter UI, and networking layers. All fixes verified with zero-error builds across macOS, Android, and iOS.

## Major Achievements

### Security Hardening (16 fixes)
- **Gossipsub sender membership verification** — Non-members rejected before message processing
- **Group secret removed from plaintext wire** — Only delivered via ECDH-wrapped GroupInvite
- **FileChunkRequest authorization** — Verifies contact/group membership on both active seeder and fallback paths
- **ChatSyncResponse sender verification** — Validates sender is group member or known contact
- **PoW 24-bit difficulty** — Increased from 16-bit with plus/minus 5 minute timestamp staleness check
- **Tunnel server localhost-only binding** — No longer exposed on 0.0.0.0
- **Request-Response 2MB limit** — Reduced from 10MB to prevent memory exhaustion
- **Relay 100MB/circuit limit** — Reduced from 1GB
- **INTROVERT_TRUST_ALL_WITNESSES debug-only** — Gated behind cfg(debug_assertions)
- **All bounded buffers with FIFO eviction** — IndexMap for true oldest-first eviction

### Performance Optimization (13 fixes)
- **DuplicateSuppressor** — O(n) Vec replaced with O(1) HashSet plus VecDeque
- **getLastMessage/getLastGroupMessage FFI** — LIMIT 1 queries for chat list previews (O(N*M) to O(N))
- **fetch_balance** — Replaced getProgramAccounts with lightweight getAccountInfo using derived ATA
- **SQL LIKE pre-filter** — Pre-filters groups before JSON deserialization
- **reqwest::Client reuse** — Single instance in SolanaIncentiveEngine
- **bootstrap_nodes by reference** — No more Vec cloning
- **Avatar decode cache** — 100-entry LRU cache for base64 decoded avatars
- **GroupChatScreen._displayMessages caching** — Version-based cache invalidation
- **In-chat search debounce** — 300ms Timer prevents per-keystroke DB queries
- **Drive file existence async** — Moved from UI thread to background helper
- **ClawTerminalDialog cursor stop** — Animation stops when final report shown

### Stability Improvements (17 fixes)
- **FFI memory leaks** — 6 methods missing _freeBinary, _handleFfiResult success path, error-path leaks
- **Null pointer checks** — Added to 4 FFI functions
- **std::thread::sleep replaced** — All async contexts now use tokio::time::sleep
- **Dialog controller leaks** — barrierDismissible: false on 2 dialogs
- **setState after await** — Added mounted check in _sendMessage
- **Error swallowing** — Replaced let _ with tracing::error! in economy module
- **GroupChatScreen dispose** — Added missing controller disposal
- **_applySearchFilter** — Moved inside setState block

### FFI Safety (8 fixes)
- **_handleFfiResult** — Now frees on success path too (defensive)
- **pollPeerProfile/syncChatMessages** — Converted to Arena allocation
- **getContacts/getAllGroups/getUnreadCounts** — try/finally with _freeBinary
- **getProfile/getHandleStatus** — try/finally with _freeBinary
- **6 IntroClaw methods** — All properly free via try/finally
- **Event callback** — All event types properly free media/economy/debug data

## Build Verification

All platforms build with zero errors:
- **macOS:** libintrovert.dylib (40 MB)
- **Android arm64:** libintrovert.so (36 MB)
- **Android x86_64:** libintrovert.so (39 MB)
- **iOS Device:** libintrovert_device.a (170 MB)
- **iOS Simulator:** libintrovert_simulator.a (168 MB)

## RBN Code Included

This release includes all RBN (Relay Backbone Node) infrastructure:
- **RBN Operator Guide** — Full deployment instructions
- **RBN Security Documentation** — Hardening guidelines
- **Solana Registry Integration** — Dynamic bootstrapping code
- **PDA Escrow Vault** — Staking and unstaking logic
- **Squads V4 Governance** — Multisig upgrade authority

## Migration Notes

- Version bumped from 0.11.0 to 0.12.0
- All existing databases fully compatible
- No breaking API changes
- RBN operators should update to benefit from security hardening

## Known Limitations

- 4 nested Tokio runtimes in FFI calls (low severity, works but wasteful)
- get_storage_usage returns drive plus mesh sum, not real disk capacity

## Credits

Comprehensive audit performed across 4 parallel verification agents:
- Rust Core Engine audit (22 fixes verified)
- FFI Bridge audit (8 fixes verified)
- Flutter UI audit (13 fixes verified)
- Networking Security audit (16 fixes verified)
