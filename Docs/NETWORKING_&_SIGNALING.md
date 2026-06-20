# Networking & Signaling Deep Dive

## 1. Network Stack (libp2p v0.56)
Introvert uses a multi-transport stack configured for maximum NAT traversal and firewall bypass.

### A. Core Transports
- **QUIC (UDP):** Preferred transport for low-latency, high-performance data (Port 443).
- **TCP:** Fallback transport for strict network environments (Port 443).
- **WebRTC:** Used for direct browser-to-node or node-to-node direct transport, leveraging libp2p's WebRTC integration for direct hole-punching.

### B. Port 443 Strategy
Standardizing on **Port 443 (HTTPS)** is the primary mechanism for bypassing carrier firewalls and Deep Packet Inspection (DPI). All RBN nodes and user nodes (when possible) listen on 443 to appear as standard web traffic.

### C. Message Size & Headroom
To support robust file transfers on relayed connections, the network stack is hardened with:
- **Relay Headroom:** RBN nodes configure `max_circuit_bytes` to **1GB** to allow high-volume file sharing.
- **Request-Response:** Configured for **2MB** payloads to ensure large file manifests and base64 overhead never trigger protocol-level drops.

## 2. Peer Discovery & Routing
- **mDNS:** Local network discovery for instant P2P connection when peers are on the same Wi-Fi.
- **Kademlia DHT:** Global decentralized routing. Each peer stores a subset of the network routing table. Also used for **Mesh Code** manifest storage and **Sovereign Swarm** seeder lookup using SHA-256 file hashes.
- **Identify:** Protocol exchange to verify peer capabilities (Signaling, WebRTC, Relay, Gossipsub support).
- **Gossipsub:** Efficient multi-point message propagation for Sovereign Groups.

## 3. Signaling Protocol
Messaging and file coordination occur over the **Request-Response** protocol for 1-on-1, and **Gossipsub** for Groups.

### A. Signaling Payload Types (JSON)
All signaling is wrapped in a `SignalingPayload` enum:
- `Standard(String)`: Plain text signaling.
- `ChatMessage { content, msg_id, timestamp }`: E2EE user messages with 64-bit timing.
- `FileTransfer { transfer_id, filename, file_hash, total_size, chunk_size, group_id }`: Manifest for a new transfer.
- `FileChunkRequest { transfer_id, chunk_index }`: Pull-based chunk request from a receiver.
- `FileChunk { ... }`: Direct or relayed file segment.
- `FileTransferComplete/Error`: Coordination signals for transfer lifecycle.
- `MeshCleanup { file_hash }`: Command to purge temporary mesh storage after group delivery.
- `WebRtc(signal)`: Encrypted SDP/ICE signals for media streams.
- `Acknowledgement { msg_id, status }`: Real-time status updates (1=Delivered).
- `MailboxStore/Drain`: Asynchronous mailbox signaling.

## 4. Connection Lifecycle
1.  **Discovery:** Peer found via Kademlia or mDNS.
2.  **Dialing:** Multi-path dial attempt (Direct QUIC -> Direct TCP -> Relay Reservation).
3.  **Handshake:** 
    - libp2p Security (Noise/TLS).
    - Introvert Noise (E2EE) Handshake using X25519 static keys.
4.  **Reachability Maintenance:**
    - **Proactive Relay:** Nodes re-request relay reservations every **5 minutes** from RBNs to ensure reachability after carrier IP changes.
    - **Pending Retry:** Nodes scan the `pending_messages` buffer every **30s** and re-attempt relay discovery for disconnected recipients.
    - **Intro-Claw Self-Healing:** 5-strategy connection recovery (direct dial, relay circuit, anchor routing, WebSocket tunnel, mailbox fallback) for automatic network resilience.
5.  **Offline Reporting:** Status only changes to "Offline" (Event 8, Status 2) if **all** paths (direct and relay) are closed.

## 5. Hybrid File Transfer (Smart Adaptive)
Introvert employs a hybrid engine to balance performance and reliability:

- **Direct P2P / WebRTC (High-Speed Push):**
    - Chunks: 256KB
    - Model: Sequential Push
    - Pacing: 20ms (~12.8 MB/s)
- **Relay / Swarm (Sovereign Swarm Pull):**
    - Chunks: 64KB (Optimized for reliability and throughput)
    - Model: Pipelined Pull (4-deep requests)
    - Discovery: Multi-source seeder discovery via Kademlia DHT using `start_providing` and `get_providers`.
    - Resiliency: Active Outbound Tracker for reliable retries and automatic 1s fast-polling during transfers.
- **Participating Seeding:** Every node that verifies a file becomes a seeder automatically.

## 6. Global Event Codes (Group Mesh)
- **Type 20:** Group Metadata Update (Name, Members).
- **Type 21:** Group Message Received.
