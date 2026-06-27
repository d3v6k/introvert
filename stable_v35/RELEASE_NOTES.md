# Release Notes - Stable v35 "Sovereign Audit"

**Version:** 0.12.0
**Date:** 2026-06-21
**Codename:** Sovereign Audit

## Overview

Stable v35 represents the most comprehensive security and stability audit in Introvert's history. Three rounds of deep code review identified and resolved **54 issues** across the Rust core engine, FFI bridge, Flutter UI, and networking layers. All fixes verified with zero-error builds across macOS, Android, and iOS.

## Major Achievements

### Security Hardening (16 fixes)
- Gossipsub sender membership verification
- Group secret removed from plaintext wire
- FileChunkRequest authorization on both paths
- ChatSyncResponse sender verification
- PoW 24-bit difficulty with timestamp validation
- Tunnel server localhost-only binding
- Request-Response 2MB limit
- Relay 100MB/circuit limit
- INTROVERT_TRUST_ALL_WITNESSES debug-only
- All bounded buffers with FIFO eviction (IndexMap)

### Performance Optimization (13 fixes)
- DuplicateSuppressor O(1) HashSet+VecDeque
- getLastMessage LIMIT 1 FFI queries
- fetch_balance ATA optimization
- SQL LIKE pre-filter for JSON columns
- reqwest::Client reuse
- bootstrap_nodes by reference
- Avatar decode cache (100 entries)
- GroupChatScreen displayMessages caching
- In-chat search 300ms debounce
- Drive file existence async
- ClawTerminalDialog cursor stop

### Stability Improvements (17 fixes)
- FFI memory leaks (6 methods, success path, error paths)
- Null pointer checks (4 FFI functions)
- std::thread::sleep replaced with tokio::time::sleep
- Dialog controller leaks fixed
- setState after await mounted check
- Error swallowing replaced with tracing::error!
- GroupChatScreen dispose added
- _applySearchFilter inside setState

### FFI Safety (8 fixes)
- _handleFfiResult frees on success path
- pollPeerProfile/syncChatMessages Arena allocation
- 6 IntroClaw methods properly free via try/finally
- Event callback properly frees all data types

## Build Verification

All platforms build with zero errors:
- macOS: libintrovert.dylib (40 MB)
- Android arm64: libintrovert.so (36 MB)
- Android x86_64: libintrovert.so (39 MB)
- iOS Device: libintrovert_device.a (170 MB)
- iOS Simulator: libintrovert_simulator.a (168 MB)

## RBN Code Included

This release includes all RBN infrastructure:
- RBN Operator Guide
- RBN Security Documentation
- Solana Registry Integration
- PDA Escrow Vault
- Squads V4 Governance

## Known Limitations

- 4 nested Tokio runtimes in FFI calls (low severity)
- get_storage_usage returns drive+mesh sum, not real disk capacity
