# RBN Scaling Crisis — Expert Consultation

**Date:** 2026-07-17
**Severity:** CRITICAL — Fundamental architecture flaw
**Status:** Needs expert review before any implementation

---

## 1. Executive Summary

A stress test with 50 virtual nodes connected to our single RBN server caused our Android client to become nearly unusable — CPU spike, phone hot, UI sluggish. This exposed a fundamental flaw: **our architecture concentrates all mesh traffic through a single RBN, which doesn't scale beyond ~50 peers.**

Our target is **1,000,000+ active users**. At 1000 users per RBN, we'd need 1000+ RBNs. But the real problem isn't the number of RBNs — it's that **every client sees every other client as a gossipsub mesh peer**, creating O(n) traffic per device.

---

## 2. Original Design Intent

The RBN was designed to serve two purposes:
1. **Relay** — When peers cannot directly connect (NAT, VPN, different networks), the RBN relays traffic between them
2. **Mailbox** — When a peer is offline, the RBN stores messages for later delivery

The RBN was NOT designed to be a central hub that all traffic flows through. But that's what it became.

---

## 3. What Actually Happens (Current Architecture)

```
                    +-------------+
                    |  RBN Server |
                    |  (1 node)   |
                    +------+------+
                           |
        +------------------+------------------+
        |                  |                  |
   +----+----+        +---+----+        +----+----+
   | Android |        |  Mac   |        |  iOS    |
   | (Peer)  |        | (Peer) |        | (Peer)  |
   +---------+        +--------+        +---------+
```

Every peer connects to the same RBN. The RBN is:
1. The gossipsub anchor for ALL topics
2. The relay server for ALL connections
3. The mailbox for ALL offline messages
4. The FCM push sender for ALL notifications

### 3.1 Gossipsub Mesh Problem

When Android, Mac, and iOS all connect to the same RBN, they form a gossipsub mesh. The gossipsub protocol maintains a mesh of `mesh_n` peers per topic (default: 6). With 3 peers, this works fine.

With 50 stress test nodes, the mesh grew to 50+ peers per topic. Each peer sends:
- `IHAVE` messages every heartbeat (10s) to all mesh peers
- `IWANT` messages to request missed messages
- `GRAFT`/`PRUNE` messages to manage mesh membership
- Message propagation to all mesh peers

At 50 peers: ~50 heartbeat messages per 10s = 5 messages/second per device
At 1000 peers: ~1000 heartbeat messages per 10s = 100 messages/second per device

### 3.2 Relay Bottleneck

All relay traffic flows through the RBN. With 50 stress test nodes:
- 950+ TCP connections to the RBN
- Every file transfer chunk goes through the RBN
- Every message delivery goes through the RBN
- Every circuit establishment triggers DB flushes

The RBN becomes the bottleneck, not the enabler.

### 3.3 Client-Side Impact

The Android device in the stress test showed:
- 37 connected peers (50 stress nodes + real devices)
- Relay circuit flapping every ~30 seconds
- idle_mode oscillation (true/false toggling every few seconds)
- Excessive chunk flushes (30 chunks every circuit re-establishment)
- Group message fan-out to 37 peers instead of 3

---

## 4. The Fundamental Problem

The current architecture treats the RBN as a central hub, not a relay.

In a proper P2P mesh:
- Peers discover each other via DHT or signaling
- Peers connect directly when possible
- RBNs only relay traffic for peers that CAN'T connect directly
- Each peer maintains connections to a SMALL subset of the mesh (not all peers)

In our current architecture:
- ALL peers connect to the RBN
- ALL gossipsub traffic flows through the RBN
- ALL peers see ALL other peers in their gossipsub mesh
- The RBN is the single point of failure and the single point of congestion

---

## 5. Scale Analysis

| Metric | Current (3 peers) | Stress (50 peers) | Target (1000 peers) | Goal (1M peers) |
|--------|-------------------|-------------------|---------------------|-----------------|
| Peers per gossipsub mesh | 3 | 50 | 1000 | 1,000,000 |
| Heartbeats per 10s per device | 3 | 50 | 1000 | 1,000,000 |
| RBN TCP connections | 3 | 950 | 20,000+ | Impossible |
| Relay circuits | 3 | 50+ | 1000+ | Impossible |
| Group message fan-out | 3 | 50 | 1000 | 1,000,000 |

At 1000 peers, each device would process 100 gossipsub heartbeats/second. That's 100 messages/second just for mesh maintenance — before any actual messages.

At 1M peers, this is completely impossible.

---

## 6. Root Cause Analysis

### 6.1 Gossipsub Mesh Is Flat
The gossipsub mesh is flat — all peers in a topic form a single mesh. There's no hierarchy, no sharding, no partitioning. Every peer sees every other peer.

### 6.2 RBN Is the Only Anchor
The RBN is the only gossipsub anchor node. All peers subscribe to topics through the RBN. This means the RBN is the center of the mesh, not a relay.

### 6.3 No Peer Limiting
There's no mechanism to limit the number of peers a client maintains. A client will accept connections from any peer, regardless of how many it already has.

### 6.4 No Topic Partitioning
All group messages go to a single gossipsub topic per group. With 1000 peers in a group, every message is propagated to all 1000 peers.

### 6.5 Relay Is Not Scoped
The relay is used for ALL traffic, not just for peers that can't connect directly. Even peers on the same network relay through the RBN.

---

## 7. What Needs to Change

### 7.1 Gossipsub Topic Partitioning
Instead of one topic per group, use hierarchical topics:
- `group/{id}/shard/{n}` — each shard has ~50-100 peers
- Peers are assigned to shards based on their PeerID hash
- Messages are propagated within shards, not across the entire mesh

### 7.2 Multiple RBNs with Sharding
Deploy multiple RBNs, each responsible for a subset of peers:
- Peers are assigned to RBNs based on their PeerID hash
- RBNs communicate with each other for cross-shard messages
- No single RBN handles all traffic

### 7.3 Direct P2P Priority
Prioritize direct P2P connections over relay:
- Use DHT for peer discovery
- Use WebRTC for NAT traversal
- Only use relay when direct connection fails
- Maintain a small set of "close" peers (DHT neighbors)

### 7.4 Peer Limiting
Limit the number of peers each client maintains:
- Maximum ~20-50 active connections
- Use DHT to find the "closest" peers
- Drop connections to distant peers
- Use gossipsub's built-in peer scoring to prune bad peers

### 7.5 Hierarchical Mesh
Use a hierarchical mesh structure:
- Core layer: RBNs (few, high-bandwidth)
- Edge layer: Regular peers (many, low-bandwidth)
- Peers connect to 1-2 RBNs and a few direct peers
- Messages propagate through the hierarchy, not the flat mesh

---

## 8. Questions for Expert Review

1. Is gossipsub the right protocol for this scale? Should we use a different pub/sub protocol (e.g., Floodsub, EPIS, or a custom protocol)?

2. How should we shard the gossipsub mesh? By group? By peer? By geography?

3. Should the RBN be a gossipsub participant or just a relay? Currently, the RBN subscribes to all topics. Should it only relay traffic?

4. How do we handle cross-shard communication? When a message needs to reach peers in different shards, how does it propagate?

5. What's the right peer limit per device? 20? 50? 100? What's the tradeoff between connectivity and resource usage?

6. Should we use Kademlia DHT for peer discovery instead of gossipsub? The DHT is designed for this scale. Should we use it for message routing too?

7. How do we handle the transition? We can't break the existing 3-device mesh. How do we migrate to a scalable architecture?

8. What's the minimum viable scaling solution? What's the smallest change that would let us handle 1000 peers? 10,000 peers? 100,000 peers?

---

## 9. Files Included

| File | Description |
|------|-------------|
| PROBLEM_STATEMENT.md | This document |
| ARCHITECTURE_BLUEPRINT.md | Current architecture overview |
| SOVEREIGN_P2P_ARCHITECTURE_PLAN.md | Planned architecture (not yet implemented) |
| android_netlog_2026-07-17.txt | Android network debug log during stress test |
| android_log_2026-07-17.log | Android device log |
| mac_log_2026-07-17.log | Mac device log |
| ios_log_2026-07-17.log | iOS device log |
| rbn_gossipsub_config.txt | RBN gossipsub configuration |
| client_gossipsub_config.txt | Client gossipsub configuration |
| rbn_relay_references.txt | RBN relay code references |
| client_relay_references.txt | Client relay code references |
| rbn_gossipsub_references.txt | RBN gossipsub code references |

---

## 10. Constraints

- Must not break existing 3-device mesh — Android, Mac, iOS must continue working
- Must support file transfers — Current 70+ Mbps direct P2P pipeline must be preserved
- Must support group chats — Gossipsub is used for group message propagation
- Must support offline delivery — Mailbox store-and-forward must work
- Must work behind VPN — Current VPN tunnel strategy must be preserved
- Rust/libp2p v0.56 — Core engine is Rust with libp2p v0.56
- Flutter/Dart — Frontend is Flutter with FFI bridge to Rust

---

## 11. Success Criteria

A successful solution should:
1. Support 1000+ concurrent peers per device without CPU/memory issues
2. Support 1M+ total users across the network
3. Maintain sub-second message delivery for direct P2P
4. Maintain relay functionality for NAT/VPN peers
5. Preserve the 70+ Mbps file transfer pipeline
6. Not require a complete rewrite of the networking stack
