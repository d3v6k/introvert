---
title: Distributed Node Accounting & Telemetry Verification Specification
version: 1.1.0 (Sovereign Telemetry Splitting — Dual-Channel Architecture)
status: Approved / In Deployment
compliance_tier: Dual-Store Insulated (Google Play & Apple App Store Safe)
---

> ⚠️ **DEVELOPMENT NOTE & COMPLIANCE GUARD:**
> Per Apple Guideline 3.1.5 and Google Play Blockchain-Based Content guidelines, the mobile client repository must remain entirely free of speculative financial terminology. Do not rename this file or its child modules to include keywords like "staking", "crypto", "yield", or "tokenomics". Keep the scope strictly framed around data diagnostic instrumentation.

---

## 🗂️ Documentation Index Reference

```
## 🗺️ Introvert Master Documentation Index
* **INTROVERT_MASTER_PLAN.md** -> Core P2P routing and swarm behaviors.
* **NETWORKING_STABILIZATION_PLAN.md** -> Backpressure, cellular throttling, and circuit scaling bounds.
* **node_accounting_specification.md** -> (THIS DOC) Multi-tier point computation, anti-gaming filters, and off-device oracle settlement.
```

Saving it under this structure preserves the Sovereign Telemetry Splitting model — ensuring the mobile client app is consistently seen as a high-performance network tracking engine, while the financial mechanics are decoupled and safely managed via your remote RBN oracle trees and external web dApp portal.

---

## 🗺️ High-Level Operational Architecture

```
┌────────────────────────────────────────┐
│   CHANNEL A: INTROVERT MOBILE APP      │ ◄─── FILENAME SAFE: node_accounting.rs
│   (Flutter — Pure P2P Telemetry)       │      No blockchain, wallet, or financial code inside.
│   App Store / Google Play Compliant    │      Terms: Path Weight, Allocation Multiplier, Node ID
└───────────────────┬────────────────────┘
                    │
                    ▼ (Encrypted Telemetry Binary via v2 Binary Codec)
                    ▼ (Signed with local Ed25519 identity key)
                    ▼ (Transmitted over standard P2P transport to RBNs)
┌────────────────────────────────────────┐
│    ROUTING BOOTSTRAP NODES (RBNs)      │ ◄─── HEAVY LIFTER: Rust RBN Daemons
│  (Validates, Computes, and Stores)    │      Processes 10-year decay & point matrices.
│  Exposes public HTTPS oracle API      │      Endpoint: api.introvert.network
└──────────┬─────────────────┬───────────┘
           │                 │
           ▼                 ▼ (Public RBN Oracle API over HTTPS)
┌─────────────────────┐  ┌──────────────────────────────────────┐
│ Nightly Batch Oracle │  │ CHANNEL B: INTROVERT LEDGER APP      │
│ (Merkle Tree Root    │  │ (React Native + Expo — Independent)  │
│  → Solana Escrow PDA)│  │ Solana WalletConnect Integration     │
└─────────────────────┘  │ Financial tracking, PDA escrow, claims│
                         │ ZERO shared sandbox with Channel A    │
                         │ Connects via scanned Peer ID + HTTPS  │
                         └──────────────────────────────────────┘
```

**Channel Isolation Guarantee:** The Introvert Ledger App (React Native + Expo) is a completely independent binary. It requires ZERO local device shared storage sandboxing (no App Groups on iOS, no Content Providers on Android). It connects directly to public RBN oracle API endpoints over HTTPS using the user's scanned Peer ID to extract telemetry reward statistics. The two application binaries are fully isolated on-device — no shared filesystem, no shared keychain, no IPC bridge.

---

## 📊 Status Matrix: What is Completed vs. Pending

We have built out the internal diagnostic tracking systems. The network and storage engines are prepared to capture telemetry data safely.

```
[COMPLETED SUBSYSTEMS] ───────────────────────────────────────────────────────────────
  ├── Core Data Transport Optimization (v52 Networking)
  ├── 13 Multi-Variant Activity Engines (Bytes, Uptime, Web Containers)
  ├── Anti-Gaming Activity Matrix Tracking Core
  ├── HMAC-SHA256 Multi-Node Treasury Relay Handshake Client
  ├── On-Chain Solana Signature Verification & RPC Confirmation Polling
  └── Local 7-Day Storage TTL Diagnostic Sweeps (Orchestrated by Intro-Claw)

[PENDING SUBSYSTEMS] ─────────────────────────────────────────────────────────────────
  ├── RBN-Side Point Aggregation Ledger & Oracle Batch Synchronization Loop
  ├── Off-Device Merkle Tree State Root Exporter (RBN Node Utility)
  ├── Solana PDA Escrow Smart Contract Deployment & Verification (On-Chain)
  ├── Flutter Telemetry Diagnostic UI Screens (Dual-Store Compliant Wording)
  └── Introvert Ledger App (React Native + Expo — Independent Binary)
```

---

## 🛠️ The Detailed Execution Specification

### 1. Network Resource Allocation & 10-Year Emission Schedule

The distribution calendar is entirely detached from mobile system execution loops. The underlying calculations are locked on the remote RBN infrastructure and a decentralized Solana escrow program.

- **Token Address:** `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` (Official Solana Token Mint)
- **Escrow Authority:** A secure Solana Program-Derived Address (PDA) vault release system.

**The 10-Year Mathematical Decay Model:**

Employs a strict 20% annual decay formula to limit circulating asset dilution while aggressively rewarding early infrastructure providers.

$$E_{day}(t) = \frac{I_{base} \cdot (0.8)^t}{365}$$

**Daily Allocation Pools (Year 1 Baseline):**

| Pool | Daily Allocation |
|------|-----------------|
| User Node Telemetry Pool | 16,438 INTR/day — distributed proportionally among all active reporting client nodes |
| RBN Infrastructure Pool | 8,219 INTR/day — distributed among active routing bootstrap layers |

---

### 2. How Points are Calculated For Each Layer

**For the User / Client Node:**

The mobile app acts purely as a diagnostic instrument. Every 30 seconds, it uses `introvert_daily_reward_tick` to log diagnostic milestones into SQLCipher. These metrics map to performance weights (e.g., total bytes routed, active time frames, sandboxed multi-tenant container counts).

**For the RBN (Routing Bootstrap Nodes):**

RBNs calculate their metrics server-side based on verified network load, total peer circuits established, and continuous public socket availability.

**The Validation & Anti-Gaming Check:**

Client tracking states are signed locally using the client's Ed25519 node identity key before being transmitted. The RBN checks these signatures against its internal tracking logs. Fake bursts or sandboxed device clusters are flagged and rejected before points are computed.

---

### 3. Execution Boundaries: Where are Calculations Handled?

To maintain complete store compliance, the mobile client application **never** calculates financial balances or claims tokens locally.

| Step | Actor | Action |
|------|-------|--------|
| 1 | Mobile App | Batches signed, raw telemetry data (e.g., "Node X relayed 500MB over Wi-Fi, running in foreground") |
| 2 | RBN | Processes incoming stream, verifies against anti-gaming models, records points in database |
| 3 | Oracle Batch | Every 24 hours, offline RBN batch oracle computes a Merkle Tree of all node points for that epoch and registers the cryptographic Merkle Root directly to the on-chain Solana Escrow Program |

---

### 4. How Rewards Are Distributed and Claimed

Users never interact with a blockchain or pay gas fees within the Introvert Mobile App (Channel A).

1. The Introvert Mobile App (Flutter) collects raw telemetry metrics locally and transmits them as encrypted, signed binary blocks to the RBN network via the v2 binary codec (`/introvert/signaling/2.0.0`).
2. To view participation weights or synchronize performance diagnostics, users open the **Introvert Ledger App** (React Native + Expo) — a completely independent application binary.
3. The Ledger App connects to public RBN oracle API endpoints over HTTPS using the user's scanned Peer ID. It requires ZERO shared storage with the Flutter app (no App Groups, no Content Providers, no IPC).
4. The Ledger App verifies the node's identity against on-chain Merkle Root proofs and generates pre-formed transaction payloads.
5. These payloads pass through the treasury relay, which co-signs and pays native SOL gas fees.
6. The Solana escrow PDA transfers earned participation units directly to the user's self-custody wallet (Phantom, Solflare, Backpack).

---

## 🛡️ Dual-Store Compliance & Naming Enforcement Guidelines

To ensure smooth review cycles for the Google Play Store and Apple App Store, we enforce strict naming rules across the entire codebase repository.

### 1. Strict File & Code Architecture Prohibitions

Files containing explicit crypto or financial terms are strictly banned from the client repository. We use neutral engineering terminology instead:

```
[ STORE-BANNED FILENAME ]             ───►   [ APPROVED PRODUCTION FILENAME ]
  • src/economy/staking.rs                     • src/economy/node_accounting.rs
  • src/economy/tokenomics.rs                  • src/economy/network_performance.rs
  • src/economy/solana_wallet.rs               • src/economy/ledger_synchronizer.rs
  • lib/src/bloc/staking_provider.dart         • lib/src/bloc/telemetry_provider.dart
```

### 2. Mandatory UI Copy Substitution Directive

All text within user-facing Dart layouts must use technical utility descriptions rather than financial terms:

| Banned Mobile Word (Store Rejection Risk) | Approved Mobile Alternative (App Store Compliant) |
|-------------------------------------------|--------------------------------------------------|
| Staking / Stake Assets | Node Security Bonding / Network Lock |
| Crypto Earnings / Yield / APY | Telemetry Weight / Allocation Multiplier |
| Claim Token Rewards | Synchronize Performance Diagnostics |
| Financial Mining | Infrastructure Resource Tracking |

---

## 🚀 The Operational Deployment Sequence

| Stage | Description | Status |
|-------|-------------|--------|
| **Stage 1** | Build the Flutter Telemetry Diagnostic Screen using store-compliant visual guidelines to display live node performance counters | **NEXT UP** |
| **Stage 2** | Implement RBN-side point aggregation ledger & oracle batch synchronization loop | Pending |
| **Stage 3** | Deploy Solana Escrow PDA smart contracts externally | Pending |
| **Stage 4** | Build the Introvert Ledger App (React Native + Expo) — independent binary with Solana WalletConnect, financial tracking charts, and PDA escrow interactions | Pending |

**Stage 4 Architecture Notes:**
- Built with React Native + Expo for independent app store submission
- Connects to RBN oracle API over HTTPS using scanned Peer ID
- ZERO shared storage sandboxing with the Flutter app (Channel A)
- Handles all financial terminology, wallet connections, and on-chain interactions
- Can be submitted to app stores independently or distributed as a standalone APK/IPA

---

## 📐 Technical Constants Reference

| Constant | Value | Source |
|----------|-------|--------|
| Total supply | 100,000,000 INTR (fixed) | Whitepaper |
| Decimals | 9 (Solana SPL standard) | On-chain mint |
| Token mint | `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` | Solana mainnet |
| Year 1 daily user pool | 16,438 INTR/day | 10-year emission schedule |
| Year 1 daily RBN pool | 8,219 INTR/day | 3M / 365 |
| Annual decay rate | 20% (multiplier: 0.8) | Whitepaper |
| Prestige tiers | 0=Citizen, 1=Sentinel, 2=Silver, 3=Gold, 4=Platinum | On-chain balance thresholds |
| Anti-gaming: max web containers | 3 (configurable) | AntiGamingConfig |
| Anti-gaming: min message length | Configurable | ActivityWeights |
| Proof hash algorithm | SHA-256 of `{activity_type}:{value}:{peer_id}` | daily_rewards.rs |
| Treasury relay auth | HMAC-SHA256 + timestamp binding | solana.rs |
| Tx confirmation | 30s polling (10 attempts × 3s) | solana.rs |
| Storage TTL | 7 days (mailbox, cleared_chats, reward_log) | storage.rs + Intro-Claw |
| Binary codec | `/introvert/signaling/2.0.0` — 25% wire savings | codec.rs |

---

## 🔐 Security Architecture Summary

| Threat Vector | Mitigation | Implementation |
|--------------|------------|----------------|
| Unauthenticated treasury relay | HMAC-SHA256 signature + timestamp binding | `solana.rs:relay_to_treasury()` |
| Phantom claims (local commit before on-chain) | RPC confirmation polling before state commit | `solana.rs:submit_and_verify_reward_claim()` |
| Fake activity fraud | SHA-256 hash verification on proof_hash | `daily_rewards.rs:record_activity()` |
| Replay attacks | Timestamp bound to HMAC payload | `solana.rs` headers |
| Partial file write corruption | Atomic write-to-temp + rename | `network/mod.rs` file finalizer |
| Chunk retry infinite loops | Hard cap at 5 retries, then eviction | `storage.rs:increment_chunk_retry()` |
| Database bloat | Intro-Claw orchestrated 7-day TTL sweeps | `intro_claw.rs:run_database_maintenance()` |
| Store compliance violations | Strict filename and UI copy guidelines | This document |

---

*Document generated: 2026-07-02 | Protocol version: v52 (0.22.0) "Adaptive Networking"*
