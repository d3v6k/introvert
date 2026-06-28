# Introvert Architecture Blueprint

## 1. System Overview
**Million-Node Mandate:** Introvert MUST scale flawlessly for **over 1,000,000 active users**. All features, routing protocols (e.g., Gossipsub), and database interactions are designed against this extreme scale to prevent loop starvation or O(N) degradation.

Introvert avoids central servers entirely, relying on a distributed mesh of user nodes and dynamic, community-operated Root Bootstrap Nodes (RBNs). The infrastructure layer is entirely automated; regular user app clients discover the network geometry by fetching signed, active multiaddresses directly from an immutable Solana registry contract, eliminating static points of failure.

## 2. Component Layers

### A. Core Engine (Rust)
The engine is responsible for all heavy lifting:
- **Networking Swarm:** Managed by `libp2p` v0.56, handling discovery (mDNS), multi-source routing (Kademlia DHT), and transport (QUIC/TCP/WebRTC on Port 443).
- **Dynamic Bootstrapping:** Replaces hardcoded bootnode arrays with a real-time, block-queried lookup routine during swarm initialization.
- **Token Gating Engine:** Interlaces local SQLCipher wallet states with the networking layer. If the local balance falls below the specified tier threshold, the core enforces strict client-only constraints, preserving mesh bandwidth.
- **Security Enclave:** Handles deterministic key derivation (HKDF-SHA256) and E2EE Noise sessions.
- **FFI Bridge:** An asynchronous interface providing thread-safe communication with the Dart/Flutter layer via 50+ exported C functions.

### B. UI Layer (Flutter)
The frontend handles user interaction and presentation:
- **Main Shell (`lib/src/ui/main_shell.dart`):** Handles UI loops and serves as the presentation entry point.
- **Sovereign Local Moderation:** To remain fully compliant with Apple and Google User-Generated Content (UGC) regulations without engineering a central censorship master-key, the client manages a localized block list inside SQLCipher. When a user blocks an offender, Flutter instructs the Rust core to drop all incoming Gossipsub frames from that specific `PeerId`.

---

## 3. The Autonomous Escrow & Reward Pipeline

To insulate developers from hosting liability, the platform uses an automated, on-chain smart contract framework to run its backbone.



+---------------------------------------------------------------------------------+
|                              SOLANA MAINNET-BETA                                |
|                                                                                 |
|   +--------------------------+               +------------------------------+   |
|   |   Squads V4 Multisig     | ------------> |  Introvert Registry Program  |   |
|   |     (3-of-5 Admin)       |  (Upgrades)   | (RBN Staking & Lookup State) |   |
|   +--------------------------+               +------------------------------+   |
|                                                              |                  |
|                                                              v (Controls via)   |
|                                              +------------------------------+   |
|                                              |  Program-Derived Address     |   |
|                                              |      (PDA Escrow Vault)      |   |
|                                              +------------------------------+   |
|                                                /                          \     |
|                   (Stakes 2M $INTR to Vault) /                            \    |
|                                              /                              \   |
|                                             v                                v  |
|                                    +-----------------+              +-----------------+
|                                    | Community RBNs  |              |   Edge Nodes    |
|                                    | (Server Daemon) |              | (Mobile Client) |
|                                    +-----------------+              +-----------------+
+---------------------------------------------------------------------------------+


#### The Unified Escrow PDA Vault
All network stakes and emission balances are consolidated into a single **Program-Derived Address (PDA)** on Solana. This account has no cryptographic private key; it is governed purely by the execution parameters of the immutable `introvert-registry` program.

#### The Token Sink Mechanics
1. **RBN Bonding Sinks:** Operators must transfer and bond exactly 2,000,000 $INTR into the PDA Escrow to declare their multiaddress on the active network directory.
2. **Unbonding Cooldown:** If an RBN withdraws from the network, their stake enters an unalterable 7-day on-chain cooldown state. This prevents exit-scams if the node drops offline or serves faulty data blocks.
3. **Edge Node Tiers:** Standard client apps query the blockchain to check token balances. Mobile devices must maintain a fixed amount of $INTR to qualify for active P2P background relay features.
