# Introvert Master Plan: Sovereign P2P Architecture

## 1. Vision & Core Philosophy
Project Introvert is a privacy-first, decentralized communication platform. It eliminates central servers entirely by utilizing true Peer-to-Peer (P2P) networking, end-to-end encryption (E2EE), and a dynamic, sovereign Solana-based token economy. 

Rather than relying on fixed or corporate-hosted bootstrap routing infrastructure, the network operates via a crowdsourced, incentivized, self-healing mesh layer. Entry points are dynamically coordinated on-chain, transforming Introvert from an isolated chat application into a zero-knowledge, autonomous utility network.

## 2. Technical Stack
- **Core Engine:** Rust (`libintrovert`).
- **User Interface:** Flutter (Dart).
- **FFI Bridge:** Asynchronous, non-blocking bridge using Tokio `spawn_blocking` and Dart `NativeCallable.listener`.
- **Identity:** Deterministic HKDF-SHA256 derivation from a 32-byte master seed (Zero Phone/Email).
- **Persistence:** SQLCipher (Encrypted SQLite) with thread-safe `Mutex` handles.
- **Networking:** libp2p (v0.56) with Kademlia DHT, Gossipsub, and WebRTC data channels. Standardized on **Port 443 (HTTPS Bypass)**.
- **Consensus & Economy:** Solana Mainnet-Beta via a unified Program-Derived Address (PDA) escrow vault, controlled securely by a Squads V4 (3-of-5) Multisig.

---

## 3. Execution Roadmap

### Phase 1: Foundational Hardening [COMPLETE]
Establish an unbreakable, non-blocking core foundation.
- [x] **Deterministic Identity:** Implement `NodeIdentity` using HKDF-SHA256 for domain-separated keys (P2P vs. Storage vs. Solana Wallet).
- [x] **Encrypted Persistence:** Initialize `SQLCipher` with high-integrity key management.
- [x] **Async FFI Bridge:** Transition to a non-blocking architecture using `tokio::task::spawn_blocking` to protect the UI thread.
- [x] **Callback Synchronization:** Implement `NativeCallable.listener` in Dart to handle Rust background task results via `Completers`.

### Phase 2: Autonomous Infrastructure Integration [IN PROGRESS]
Decouple infrastructure from developer dependencies to maximize legal neutrality and global bypass reachability.
- [ ] **On-Chain Registry:** Deploy the Anchor `introvert-registry` program to manage dynamic Root Bootstrap Node (RBN) addresses.
- [ ] **Squads V4 Governance:** Permanently sign over the contract's upgrade authority to a 3-of-5 Squads Multisig to strip individual keys of central control.
- [ ] **Dynamic Discovery Refactor:** Re-engineer `src/network/service.rs` to fetch active bootnodes dynamically from Solana via an internal RPC lookup rather than hardcoding IP profiles.
- [ ] **Structural Token Gating:** Implement Rust-side balance filters that require peers to hold specific tiers of $INTR (e.g., 500 $INTR minimum) to activate edge-routing tasks (Event Code 22), mitigating Sybil attacks.
- [ ] **RBN Staking & Cooldown Vault:** Establish a mandatory 50,000 $INTR lockup within the PDA escrow with an automated 7-day unbonding script to guarantee infrastructure stability.

---

## 4. Token Economy Overview

| Allocation | % | Amount |
|-----------|---|--------|
| Ecosystem Rewards Pool | 50% | 50,000,000 $INTR |
| Community Growth & Grants | 20% | 20,000,000 $INTR |
| Developer Launch Reimbursement | 10% | 10,000,000 $INTR |
| Core Team Vesting | 5% | 5,000,000 $INTR |
| Initial Liquidity | 15% | 15,000,000 $INTR |

**10-year emission:** ~40.17M $INTR via 20% annual decay. Year 1 daily cap: 16,438 $INTR.

**Full specification:** See `Docs/INTROVERT_TOKEN_WHITEPAPER.md` and `Docs/INTROVERT_ECONOMY_BLUEPRINT.md`.
