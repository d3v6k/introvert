# Introvert: Network Performance, Latency, & Speed Specifications

This document outlines the expected latency profiles, transfer speeds, and packet specifications of the Introvert network under different connectivity environments.

---

## 1. Summary of Network Profiles

| Connection Type | Target Protocol | Average Latency | Expected Speeds | Chunk Size | Primary Use Case |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Direct P2P** | WebRTC / QUIC (UDP) | **5ms – 50ms** | **10 Mbps – 150+ Mbps** | 256 KB | Local/Wi-Fi transfers, active direct messaging, and video calls. |
| **Group Swarm (LAN)** | Sovereign Swarm Pull (Direct) | **5ms – 30ms** | **10 Mbps – 100+ Mbps** | 64 KB | Group file transfers on same LAN. 8-deep pipeline @ 10ms pacing. Scales up as peers join the swarm. |
| **Relayed QUIC** | RBN Relay (UDP/443) | **30ms – 150ms** | **1.5 Mbps – 8 Mbps** | 64 KB | Cross-network transfers, NAT-traversal fallback, and sliding-window pulls. |
| **Relayed TCP** | RBN Relay (TCP/443) | **50ms – 250ms** | **0.5 Mbps – 3 Mbps** | 64 KB | Extreme carrier/firewall environments where UDP traffic is blocked. |
| **Mesh Mailbox** | Asynchronous (Draining) | **N/A** (Offline Queue) | **Throttled (Mesh Queue)** | 64 KB | Offline messaging, store-and-forward routing, and background sync. |

---

## 2. Detailed Performance Profiles

### A. Direct P2P (WebRTC & Direct QUIC)
* **How it works**: Established using dynamic STUN/TURN hole punching. Devices communicate directly without intermediate hops.
* **Latency Profile**: Extremely low. Governed entirely by physical network distance (RTT).
* **Speed Capacity**: High-speed bandwidth saturation. On high-speed fiber or Wi-Fi, it easily exceeds **100 Mbps**.
* **Chunking Strategy**: Uses **256 KB sequential push chunks**. Because the connection is direct and stable, there is no need for pull throttling.

### B. Relayed QUIC (RBN Relay Node)
* **How it works**: Traffic is routed through a Root Bootstrap Node (RBN) acting as a libp2p circuit relay over UDP port 443.
* **Latency Profile**: Medium. Includes the network hop to the closest RBN node.
* **Speed Capacity**: 1.5 Mbps to 8 Mbps. Bandwidth is throttled to ensure relay servers are not overwhelmed by raw TCP streams.
* **Chunking Strategy**: Uses **64 KB pipelined pull chunks** with a 4-deep sliding window (keeping exactly 4 requests in flight).

### C. Relayed TCP (RBN Relay Node)
* **How it works**: Fallback connection when firewall setups block all UDP traffic on port 443. routes over TCP streams.
* **Latency Profile**: Higher. Prone to TCP handshake latency and head-of-line blocking on packet loss.
* **Speed Capacity**: 0.5 Mbps to 3 Mbps.
* **Chunking Strategy**: Uses **64 KB chunks** with strict flow control (concurrency caps) to prevent relay socket flooding.

### D. Mesh Mailbox (Asynchronous Routing)
* **How it works**: If a recipient is offline, encrypted chunks are stored on a matching Kademlia DHT anchor. The receiver drains these chunks when they reconnect.
* **Latency Profile**: Asynchronous. Polling occurs every **5 seconds** during active client use, and falls back to **15 minutes** in passive background state.
* **Store-and-Forward Capacity**: Files sent via this method are capped at **150 MB** maximum to preserve the distributed storage capacity of anchor nodes.

---

### E. Group Swarm Pull — Direct LAN (Sovereign Swarm)
* **How it works**: Group file transfers use a **receiver-driven pull** model over the Sovereign Swarm. All group members on the same LAN connect directly to the seeder(s) and request chunks via `FileChunkRequest`. Completed peers immediately register as additional seeders on the DHT.
* **Latency Profile**: Extremely low. Governed by physical LAN RTT (equivalent to Direct P2P).
* **Speed Capacity**: Matches or approaches direct P2P speeds (10 Mbps+). Throughput scales as more peers complete and join the swarm.
* **Chunking Strategy**: Uses **64KB pipelined pull chunks**. Initial pipeline depth is **8 chunks in-flight at 10ms pacing** when a direct connection is confirmed, vs. the standard 4-deep at 50ms for relayed connections.
* **Dynamic Seeder Discovery**: Receivers re-issue a Kademlia `FindProviders` query every **5 seconds** during active downloads. This allows latecoming seeders (peers who recently finished) to be added to the active provider pool, increasing download speed over time.
* **Completion Semantics**: The sender dispatches `is_complete: true` and `is_verified: true` **only when all group members** have sent their `FileTransferComplete` confirmation. This uses a `HashSet<PeerId>` tracked in the sender's `ActiveSeeder.completions` field. A 3-member group requires 2 confirmations (all non-sender members).
