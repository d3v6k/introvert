# Introvert File Transfer Stall Audit & Rectification Plan

**Date:** 2026-06-27  
**Status:** Completed and Rectified  
**Issue:** Image / File transfers get stuck at 51% (or similar midpoint progress) on relayed connections, even when devices are on the same local network.

---

## 1. Findings & Root Causes

Through a deep audit of the file transfer protocol (`src/network/mod.rs`) and the `IntroClaw` connection optimizer, three distinct defects were identified that combine to cause the file transfer stall:

### Finding 1: mDNS Discovered Peers Missing from Tracker (HIGH)
*   **Root Cause:** In the `libp2p::mdns::Event::Discovered` handler, the peer IDs of discovered local network devices were dialed but **never** added to the `self.mdns_peers` tracking set.
*   **Impact:** When the `IntroClaw` connection optimizer ticked, it received an empty mDNS set and determined that direct connections were not possible. It never triggered direct P2P connection upgrades. Consequently, devices on the same local network fell back to RBN (Relayed Bootstrap Node) routing.

### Finding 2: Offline Provider Selection Stall (CRITICAL)
*   **Root Cause:** In `select_best_providers_static`, if no connected direct or relayed providers are found, the function fell back to returning `providers.to_vec()`.
*   **Impact:** In a group context, this list includes offline or unreachable group members. When a chunk was requested using the sliding window index `next_idx % providers.len()`, the receiver would inevitably target an offline peer. The request was buffered in RAM (`pending_messages`) and never answered. Since the pull model only continues when a chunk is received, the entire file transfer stalled permanently.
*   **Solution:** Return an empty `Vec` when no connected providers exist. This forces the caller to fall back to the active `peer` who sent the chunk (guaranteed online).

### Finding 3: Bootstrap Node Authorization Rejection (HIGH)
*   **Root Cause:** The `FileChunkRequest` handler added strict security checks verifying that the requesting peer is a group member or known contact.
*   **Impact:** Relay bootstrap nodes (RBNs) performing offline caching or relaying requests on behalf of other peers were blocked since they are not group members.
*   **Solution:** Authorize bootstrap nodes (`is_bootstrap`) to request chunks.

---

## 2. Rectification & Code Implementations

The following modifications were implemented in `src/network/mod.rs`:

1.  **mDNS Peer Insertion:**
    Added `self.mdns_peers.insert(peer_id);` in the mDNS discovery loop to ensure the connection optimizer knows when local LAN peers are available.
2.  **Robust Provider Fallback:**
    Refactored `select_best_providers_static` to return an empty `Vec` when no direct/relayed connections are ready, allowing fallback to the current active chunk peer.
3.  **RBN Bypass in Security Checks:**
    Authorized bootstrap nodes (`is_bootstrap`) in both the active seeder check and the Sovereign Drive fallback path of `FileChunkRequest`.
4.  **Accept Invite Gossipsub Subscription:**
    Added immediate Gossipsub subscription inside `AcceptGroupInvite` so that newly joined members start receiving group signaling immediately without needing an app restart.

---

## 3. RBN / Headless Daemon Core Updates (`for_linux/src/network/mod.rs`)

The RBN daemon (running headless in `for_linux/`) does not perform app-specific security/authorization checks on `FileChunkRequest`s (it acts as a public relayer/seeder fallback). However, it runs the same `select_best_providers_static` chunk provider balancing logic when caching group files/chunks for offline group members.

To prevent the RBN daemon from stalling during file prefetching:
*   Refactored `select_best_providers_static` in `for_linux/src/network/mod.rs` to return `Vec::new()` instead of `providers.to_vec()` when no active provider links are open. This ensures the RBN daemon correctly falls back to retrieving from the active online peer.
*   Verified the `for_linux` daemon crate compiles successfully.


---

## 4. Relayed Transfer Speed & Congestion Optimization

During production testing of the upgraded 256KB/8-deep pipeline over cellular networks (relayed WAN connections), we encountered extreme packet drops, stream timeouts, and congestion collapse. The 256KB chunks (base64 encoded to ~341KB) were too large for high-latency, limited-bandwidth mobile links. Requesting 8 chunks in parallel (2.7MB in-flight) flooded Yamux multiplexer windows, leading to connection resets and watchdog loops.

### Optimizations Implemented (Adaptive Pipeline):
1.  **Adaptive Chunk Sizing:**
    *   **Direct P2P/LAN Connections:** Continue using the high-speed **256KB** chunks.
    *   **Relayed Connections:** Dynamically scale down to **64KB** chunks. This prevents individual chunk responses from exceeding libp2p's default request-response timeouts (20-30s).
2.  **Adaptive Pipeline Depth:**
    *   Direct connections use a **12-chunk** pipeline.
    *   Relayed connections restrict the pipeline to **4-chunk** depth (up to 256KB of data in-flight), maintaining parallel pipelining while preventing thundering-herd congestion.
3.  **Adaptive Pacing Delay:**
    *   Paced requests are delayed by **10ms** on direct connections, and **100ms** on relayed connections to allow remote buffers to clear.
4.  **Watchdog Window Alignment:**
    *   Watchdog window size scaled from hardcoded `8` to match the connection's active pipeline depth (`4` for relay, `12` for direct).
    *   Aligned `next_pull_idx` dynamic initialization mapping to match the active pipeline size.
