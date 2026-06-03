# Introvert: RBN & Mesh Security Specification
**Version:** 1.0 (Phase 4.1 Production Hardened)
**Date:** May 19, 2026

## 1. Executive Summary
The Introvert network is designed as a zero-trust, sovereign P2P communication mesh. The Root Bootstrap Nodes (RBNs) act as the backbone for discovery and mailboxing without ever compromising the end-to-end encryption (E2EE) of the users. Security is enforced through multi-layered cryptographic barriers, deterministic identity derivation, and protocol-level isolation.

---

## 2. Core Security Pillars

### 2.1 Identity Unity (The Master Seed)
Security starts with the **32-byte Master Seed** (BIP-39 derived). All cryptographic materials are deterministically branched from this root:
*   **Network Identity:** Ed25519 Keypair for libp2p `PeerId`.
*   **Transport Security:** X25519 Static Keys for the Noise Protocol.
*   **Economic Identity:** Ed25519 Keypair for Solana wallet signatures.
*   **Persistence Security:** AES-256-GCM key for SQLCipher database encryption.

This derivation ensures that an identity is immutable and globally unique. A node cannot "spoof" an Introvert instance without the specific mathematical branching logic implemented in the `NodeIdentity` core.

### 2.2 Transport Layer Security (libp2p Noise)
All raw TCP/QUIC connections to the RBN are wrapped in the **Noise Protocol**. 
*   **Mutual Authentication:** Every connection requires a valid Ed25519 signature exchange.
*   **Perfect Forward Secrecy (PFS):** Session keys are ephemeral and discarded after the connection closes.
*   **Traffic Obfuscation:** The underlying data is indistinguishable from random noise, preventing Deep Packet Inspection (DPI) by ISPs from identifying Introvert traffic.

### 2.3 Protocol-Level Isolation (The "Introvert Dialect")
Introvert nodes communicate over a specialized namespace that isolates them from the general libp2p network:
*   **Custom DHT Protocol:** `/introvert/kad/1.0.0`
*   **Custom Signaling:** `/introvert/signaling/1.0.0`
*   **Custom Identification:** `/introvert/1.0.0`
Any node attempting to connect using standard libp2p protocols will be rejected during the multistream-select negotiation phase.

---

## 3. RBN-Specific Protections

### 3.1 Anchor Node Hardening (Mailbox Security)
When an RBN acts as an **Anchor Node**, it provides a decentralized "Mailbox" for offline peers.
*   **Zero-Knowledge Storage:** The RBN only sees an opaque `SignalingPayload::Secure(SecureMessage::Transport(encrypted_blob))`. It cannot read the sender, the message content, or the metadata.
*   **Sender Verification:** The RBN verifies the `PeerId` of the storer via the authenticated transport layer before accepting a mailbox payload.
*   **TTL Enforced Pruning:** Mailbox entries have a strict Time-To-Live (TTL). Stale data is automatically wiped to prevent the RBN from being used as a persistent data silo.

### 3.2 DDoS & Connection Management
The RBN is hardened against Sybil and DDoS attacks:
*   **Connection Limits:** Production RBNs (like the Alibaba instance) use `connection_limits` to cap concurrent streams at 1,000,000, preventing resource exhaustion.
*   **Liveness Probing:** Active K-bucket probes prune any node that does not respond to Introvert-specific liveness checks within a 300-second window.
*   **K-Bucket Churn Resistance:** The RBN prioritizes long-lived, high-reputation nodes in its routing table.

---

## 4. Mesh Security (End-to-End)

### 4.1 Noise IK Handshakes
Introvert uses the **Noise IK** handshake pattern for application-layer E2EE. This allows peers to establish a secure session in a single round-trip if they have previously seen each other's static public keys.
*   **Pre-Shared Key (PSK) Logic:** Handshakes are anchored in the initial "Trust Exchange" (Magic Wormhole).
*   **Double Ratchet Integration:** Long-lived messaging sessions utilize the Double Ratchet algorithm for granular forward and post-compromise secrecy.

### 4.2 WebRTC Media Hardening
Voice and Video streams (WebRTC) are negotiated *inside* the established Noise IK signaling channel.
*   **SDP Masking:** Session Description Protocol (SDP) blobs are never sent in plain text. They are encrypted via Noise before being transmitted over the libp2p signaling plane.
*   **DTLS-SRTP:** The actual media packets are further protected by DTLS-SRTP, providing a secondary layer of encryption for real-time data.

---

## 5. Audit Status
The RBN Security architecture was audited on **May 18, 2026**, and passed all "Production Scale" requirements. The Alibaba RBN deployment is currently enforcing these standards globally.
