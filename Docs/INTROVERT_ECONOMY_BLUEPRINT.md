Introvert Protocol: Sovereign Mesh Tokenization Blueprint v4.0

This document serves as the final technical and economic specification for the $INTR integration into the Introvert communication ecosystem. The strategy prioritizes capital efficiency, user sovereignty, and deflationary stability within a fixed-supply environment.

1. Core Token Specifications
The token is deployed as a permanent, immutable asset on the Solana blockchain.

Token Name: Introvert Token
Symbol: $INTR
Mint Address: NCdrqtdCzUBkmNFHEBKLqkcppGj7GW8gfCSEhoWoSMn
Total Supply: 100,000,000 (Fixed/Non-inflationary)
Authority Status: Mint and Freeze authorities are permanently disabled to ensure censorship resistance.

Primary Treasury Account: 9jauyKiimh6SBnpoRXcNXiLXZKSnN4h2gWKoqMcG4zHy

2. The Lifecycle of an Introvert Peer
The tokenization logic is baked directly into the Rust core (src/crypto/solana.rs) to maintain cross-platform consistency.

Phase I: Sovereign Onboarding (The Seed Balance)
Mechanic: Every new identity derived from a BIP-39 seed receives a "Seed Balance" from the Treasury.
The Coasting Period: This balance covers the first 3–4 days of the Identity Lease.
Gasless Execution: The Treasury Fee Payer co-signs the distribution transaction, allowing the user to receive tokens without holding any SOL.

Phase II: The Identity Lease (Mesh Maintenance Fee)
The Burn/Recirculation Mechanic: A recurring daily fee is deducted from the user’s wallet to maintain their status in the Kademlia DHT.
Account Pruning: If the wallet balance falls below the cost of a single day’s lease, the instance enters a "Dormant" state and is pruned from the routing table.
Economic Sink: These fees are either burned (to increase scarcity) or returned to the Treasury Vault to fund future onboarding.

Phase III: Activity Yield (The Reward Engine)
Usage Rewards: Users earn INTR based on authentic communication metrics (messages sent/received).
Profitability Rule: The system is calibrated so that active human usage generates rewards exceeding the daily Identity Lease, ensuring net profitability for participants.
Proof of Work: Anchor Nodes earn higher distributions for providing Zero-Knowledge storage and P2P relay services.

3. Strategic Lockup & Status (The Prestige Plane)
To protect the token value and encourage long-term holding, the app implements the following social-economic gates:
The Instance Reserve: A minimum amount of $INTR (e.g., 500) is required to be held locally to keep the app functional. This prevents total liquidity drainage during market panics.
Tiered Status Rings: The Rewards HUD calculates real-time "Prestige Tiers" based on wallet balances:
- Silver: Top 20% of network holders.
- Gold: Top 5% of network holders.
- Platinum: The top 1% (Sovereign Tier).
Utility NFTs (Artifacts): Distributed to long-term Anchor Nodes, these grant permanent discounts on Identity Leases or priority routing in the mesh.

4. Integrated Safety Module (Aave-Style Staking)
A dedicated staking site acts as the protocol's backstop.
stkINTR Receipts: Users stake $INTR to receive a liquid stkINTR token, representing their share of the mesh security pool.
Slashing Protocol: If the mesh suffers from critical relay failures, a portion of the staked tokens is "slashed" to re-collateralize the system.
Fee Abstraction: All staking actions (stake, unstake, claim) are performed gaslessly using $INTR as the "virtual gas," with the Treasury handling the SOL settlement out-of-band.

5. Implementation Roadmap for Developers
- Identity Hub: Real-time balance and Prestige Ring rendering (lib/src/ui/widgets/rewards_hud.dart)
- Economy Layer: Reward tracking and signed work-proof generation (src/economy/mod.rs)
- Solana Engine: SPL Token balance and gasless claim logic (src/economy/solana.rs)
- Relay Logic: Verification of work-proofs for Anchor Node rewards (src/network/mod.rs)

Expert Note: This "Fixed-Supply Sovereign Mesh" model is designed for a 10-year horizon. By utilizing the Treasury Fee Payer, we remove the "SOL barrier," allowing the app to scale virally like a standard messaging tool while maintaining a high-performance Web3 backbone.
