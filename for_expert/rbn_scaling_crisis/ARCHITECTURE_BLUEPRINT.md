# Introvert Architecture Blueprint

## 1. System Overview
**Million-Node Mandate:** Introvert MUST scale flawlessly for **over 1,000,000 active users**. All features, routing protocols (e.g., Gossipsub), and database interactions are designed against this extreme scale to prevent loop starvation or O(N) degradation.

Introvert avoids central application-layer servers entirely, relying on a distributed mesh of user nodes and dynamic, community-operated Root Bootstrap Nodes (RBNs). The discovery layer is decentralized; regular user app clients discover the network geometry by fetching signed, active multiaddresses directly from an immutable Solana registry contract, eliminating central application servers. To prevent Solana RPC clusters from becoming a single point of failure (SPOF), the system implements redundant fallback mechanisms, including local IP caching, raw configuration fallbacks, and DNS-over-HTTPS TXT record queries.

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

### C. Delivery Confirmation System
Messages flow through a 4-tier delivery pipeline with end-to-end confirmation:
1. **Direct P2P** — WebRTC Data Channel or libp2p request-response (256KB chunks)
2. **Relay Circuit** — libp2p circuit relay through RBN (64KB chunks)
3. **Anchor Mailbox** — Persistent storage on verified RBN nodes
4. **RAM Buffer** — Pending messages flushed on circuit establishment

**Message Status Flow:**
- Status 0 (Sent): Message created locally, single tick
- Status 3 (In Mailbox): Anchor confirmed storage via `MailboxStored` ACK, clock icon
- Status 1 (Delivered): Recipient's node processed the message, double grey tick
- Status 2 (Read): Recipient opened the chat, double blue tick

**Mailbox Integrity:**
- `MailboxStored` ACK confirms anchor storage before recipient delivery
- `store_message_if_new` (INSERT OR IGNORE) prevents sync from overwriting current messages
- `verified_rbns` filter ensures only trusted RBNs receive mailbox payloads
- File messages excluded from chat sync to prevent metadata corruption
- Stale `FileTransferComplete` ACKs are dropped if no active seeder exists

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

### DynamicPromoStack (Customizable Campaign Layer)

The DynamicPromoStack enables runtime promotion adjustments on the 10% Strategic Reserve allocation:

**Year 1 Strategic Reserve:** 3,287.60 INTR/day

**Campaign Types:**
- CommunityThemeVote — Daily theme competitions with community voting
- EarlyAdopterBonus — Early user onboarding rewards
- DeveloperHackathonYield — Developer contribution bounties
- DynamicBonusCampaign — Custom promotional campaigns

**Math Model:**
```
[Strategic Reserve Daily Ceiling: 3,287.60 INTR]
                    │
                    ├──► [- Minus] Active Campaigns (e.g., Theme: 1,000 INTR)
                    │
                    └──► [= Equals] Referral Pool (2,287.60 INTR)
```

**Safety Features:**
- Auto-eviction — Expired campaigns automatically removed at epoch close
- Safety cap — Promo deductions cannot exceed Strategic Reserve ceiling
- Runtime adjustments — No code rebuilds required
- Referral pool compression — Core referral rewards always protected
