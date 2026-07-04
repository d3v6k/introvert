# Solana Mainnet Execution Plan Addendum
**Date:** June 27, 2026
**Version:** 1.0.0
**Status:** 🛡️ Phase 2 Addendum: Active Architecture Additions

This document outlines the specific security controls, governance policies, and network protocols implemented to address the key architectural gaps identified prior to Solana Mainnet deployment.

---

## 1. 🛡️ Treasury Fee Payer DDoS & Abuse Mitigation

To prevent malicious actors from spamming the Treasury Fee Payer API to drain the Treasury's native $SOL gas funds, the following validation pipeline is enforced:

```
[User App] ── Signed $INTR Tx ──> [Treasury Fee Payer] ──> [Solana RPC]
                                          │
                            ┌─────────────┴─────────────┐
                            ▼                           ▼
                     [Local Checks]              [On-Chain Checks]
                     1. Rate Limiting            1. Valid Holder Balance
                     2. Replay Protection           (≥ 1,000 $INTR)
                     3. Valid Signature
```

### Specifications:
1. **Local Rate-Limiting:** IP-based and PeerID-based rate limits are enforced at the fee payer gateway. A maximum of **3 gasless transactions per user per day** is permitted.
2. **On-Chain Balance Gating:** Before signing, the Fee Payer queries the Solana blockchain to verify the requesting public address holds at least **1,000 $INTR**. Accounts below this threshold are rejected.
3. **Transaction Inspection:** The Fee Payer deserializes the transaction payload locally to verify:
   * The destination account is valid (only transfers/actions inside the Introvert ecosystem are co-signed).
   * The signature matches the owner.
   * No malicious or foreign instructions are packed into the transaction block.

---

## 2. ⏳ Time-Locked Multisig Upgrade Governance

To prevent immediate, unannounced code modifications to the Anchor program that could put locked escrows at risk:

*   **Multisig Administration:** The program upgrade authority is owned by a **Squads V4 (3-of-5 threshold) Multisig**.
*   **Time-Lock Cooldown:** Any program upgrade proposed by the multisig enters an on-chain **7-day time-lock period**.
*   **Community Transparency:** The proposal and its target build hash are broadcast via P2P channels. If a vulnerability or malicious edit is detected during the 7-day window, users have sufficient time to unstake and exit before the code is executed.

---

## 3. 📊 Decentralized Hourly Telemetry Aggregation

To eliminate UI lag where clients calculate incorrect real-time earnings predictions due to daily points sync delays:

*   **DHT Epochs:** Global social and infrastructure points are compiled dynamically.
*   **Gossipsub telemetry:** RBNs run a background telemetry consensus loop. Every hour, active RBNs exchange their local activity snapshots over the Gossipsub channel `/introvert/telemetry/v1`.
*   **Weighted Consensus:** The median value of global points is written to a fast-sync cache.
*   **Hourly Client Sync:** Mobile and desktop clients fetch this dynamic estimate from RBNs every hour (rather than daily at UTC 00:00), ensuring the UI always displays accurate, real-time yield estimates.

---

## 🗺️ Next Steps: Step-by-Step Mainnet Integration

Following this plan, we will execute the following steps in sequence:

1. **Deploy Anchor Program to Mainnet:** Compile the Anchor contract and deploy it using the verified Mainnet program ID.
2. **Squads V4 Setup:** Create the Squads V4 vault, invite the 5 admin keys, set the 3-of-5 threshold, and bind it to the program upgrade slot.
3. **Rust Core Mainnet Configuration:** Update FFI bindings and `solana.rs` endpoints.
4. **Treasury Fee Payer Node Deployment:** Deploy the co-signing relay server with the rate-limiting and balance-gating checks.
