# Protocol Specification & FFI Bridge

## 1. Global Event Codes
The Rust core dispatches events to the UI using a `u8` type code followed by a binary payload.

| Code | Name | Payload Description |
| :--- | :--- | :--- |
| **1** | Peer Discovered | Binary `PeerId`. |
| **2** | Message Received | Decrypted message string (UTF-8). |
| **4** | Mailbox Drained | Decrypted message from offline buffer. |
| **7** | E2EE Active | `PeerId_String` + `\0` (Separator) + `0` (Success byte). |
| **8** | Peer Status | `PeerId_String` + `\0` (Separator) + `[0=Direct, 1=Relay, 2=Offline]`. |
| **10** | Mesh Status | `[0=Offline, 1=Active, 2=RelayReady, 3=Syncing]`. |
| **11** | Anchor Mode | `[0=Disabled, 1=Enabled]`. |
| **12** | File Progress | JSON `FileTransferProgress` object. |
| **13** | Message Ack | `StatusByte` + `msg_id_bytes`. |
| **20** | Group Joined | `GroupId` + `0` (Separator) + `GroupName`. |
| **21** | Group Message | `GroupIdLen` + `GroupId` + `SenderIdLen` + `SenderId` + `Content`. |
| **22** | Node Eligible | `[0=Ineligible, 1=Eligible]` (Based on storage/stability). |
| **23** | Mesh Capacity | `i64` (8-byte Little-Endian) total bytes available. |

## 2. Messaging Lifecycle (P2P)

1.  **Send:** User types "Hi" -> Dart calls `introvert_network_send_chat(peerId, "Hi", msgId)`.
2.  **Encryption:** Rust fetches `NoiseSession` for peer -> Encrypts "Hi" -> Wraps in `ChatMessage` signaling payload.
3.  **Transport:** `libp2p` sends Request to Peer -> Peer returns "ACK".
4.  **Remote Receipt:** Remote Rust decrypts payload -> Stores in DB -> Dispatches Event **2** to Remote UI.
5.  **Acknowledgement:** Remote Rust sends `Acknowledgement { msg_id, status: 1 }` signaling payload back to Sender.
6.  **UI Tick:** Sender Rust receives Ack -> Updates DB -> Dispatches Event **13** to Sender UI -> Dart shows double tick.

## 3. File Transfer Lifecycle (Sovereign Swarm)

1.  **Manifest:** Sender sends `FileTransfer` manifest signaling payload. If peer is offline, manifest is stored in Mesh Mailbox. Manifest includes `group_id` context.
2.  **Smart Hybrid Entry:** 
    - **Direct Path:** If direct P2P/WebRTC exists, Sender sequentially **pushes** chunks (256KB @ 20ms).
    - **Relayed Path:** If direct path is blocked, Receiver initiates **Redundancy-Filtered Pull** requests (16KB chunks @ 250ms).
3.  **Discovery:** Receiver queries Kademlia DHT for chunk providers matching the file hash using `get_providers`.
4.  **Parallel Swarm Pull:** Receiver requests chunks from all discovered seeders (Original Sender + Anchors + Group Members) in parallel, maintaining a 2-deep request pipeline for relay stability.
5.  **Participating Seeding (Mandates):**
    - Receiver verifies SHA-256 hash.
    - **Group Mode:** Receiver calls `start_providing(hash)` and registers as seeder.
    - **1-to-1 Mode:** Receiver skips seeding to preserve individual privacy.
6.  **Pacing & Reliability:** 
    - Sender applies **250ms pacing** for relayed chunks. 
    - Receiver uses an **8-second watchdog** to catch dropped requests and re-queue them.
    - **RAM Filter:** Duplicate requests are purged from `pending_messages` to prevent congestion upon reconnection.
7.  **No-Mailbox Rule:** Raw chunks are strictly RAM-buffered; only metadata (manifests/acks) is allowed in persistent mailbox storage.
8.  **Cleanup:** Once all group members confirm receipt, participating nodes purge temporary mesh storage according to their local 1GB cache quota.


## 4. Mailbox Flow (Zero-Knowledge)

1.  **Storage:** Sender cannot reach Peer -> Sender wraps payload in `MailboxStore { recipient_id, payload }` -> Sends to an RBN/Anchor.
2.  **Indexing:** Anchor hashes `recipient_id` -> Stores payload in `mailbox` table.
3.  **Retrieval:** Peer comes online -> Peer calls `introvert_network_fetch_mailbox()` -> Sends `MailboxDrain` to Anchor.
4.  **Push:** Anchor matches Hash -> Sends `MailboxDrained(messages)` signaling payload to Peer.
5.  **Assembly:** Peer loops through messages -> Decrypts -> Dispatches Event **4** to UI.

## 5. Group Mesh Lifecycle (Decentralized)

1.  **Creation:** Creator generates `GroupId` and `GroupSecret`. Metadata (manifest) is stored in local SQLCipher.
2.  **Invitation (1-on-1):** Creator sends `GroupInvite` to existing contacts. Includes `GroupSecret` wrapped with contact's X25519 key.
3.  **Discovery (Join by Code):**
    - Admin creates human code -> Derives key from code -> Encrypts manifest -> Publishes to **Kademlia DHT**.
    - Joiner enters code -> Retrieves manifest from DHT -> Decrypts and saves to local SQLCipher.
4.  **Propagation:** Members subscribe to `/introvert/groups/{GroupId}` topic. All messages are broadcast via **Gossipsub**.
5.  **Administrative Proof:** Any membership change is signed by the admin using Ed25519. Peers reject Gossipsub packets that lack a valid cryptographic signature from a known authority.

## 6. Media (WebRTC) Signaling

WebRTC SDP and ICE candidates are exchanged over the **Signaling Plane** (Request-Response). They are **always** encrypted via the Noise session to prevent metadata leakage or connection hijacking.
- Prefix: `WEBRTC:` followed by JSON signal.
- Handler: Recursive logic in `handle_signaling_payload` identifies the prefix and routes to `MediaManager`.
