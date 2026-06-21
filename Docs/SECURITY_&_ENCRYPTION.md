# Security & Encryption Enclave

## 1. Zero-Knowledge Foundation
Introvert is built on the principle that no intermediary, including community-operated RBN nodes, should ever be able to decrypt or read user content, files, or link network identity to real-world profiles.

## 2. Deterministic Identity Derivation
Identity is never derived from or associated with an email address, phone number, or centralized server account. Onboarding utilizes a purely sovereign **32-byte Master Seed** (BIP-39). Introducing an email address establishes a central database vector, creating liabilities under global compliance parameters (GDPR/CCPA).

### HKDF-SHA256 Derivation Paths
Using distinct salt strings to ensure domain separation:
- **libp2p Identity:** `b"introvert_p2p_identity"` -> Ed25519 Keypair (PeerId + **p2p_pubkey**).
- **E2EE Identity:** `b"introvert_e2ee_identity"` -> X25519 Static Secret (Noise IK).
- **Storage Key:** `b"introvert_storage_key"` -> 256-bit SQLCipher Key.
- **Solana Wallet:** `b"introvert_solana_wallet"` -> Ed25519 Signing Key (Manages $INTR balance checks).

---

## 3. Autonomous Infrastructure Safeguards

### A. Program-Derived Address (PDA) Isolation
The protocol's funds, operator stakes, and token distributions are entirely segregated inside a Program-Derived Address (PDA) account. This ensures that no individual developer, node operator, or compromised user wallet possesses the private key required to authorize token transfers out of the ecosystem vault. All transfers are hardcoded via on-chain contract logic.

### B. Governance Decoupling (Squads V4 Standard)
The smart contract registry program and the upgrade tokens are bound to the **Squads V4 Multisig Program** (`SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`) on Solana Mainnet. 
- **Threshold Rule:** Administrative state updates require a strict **3-of-5 vote threshold**.
- **The Cryptographic Shield:** By removing individual developer keys from the contract's upgrade track, the author achieves full legal separation as a software publisher, protecting the codebase from developer-level vulnerabilities or external legal targeting.

### C. Time-Locked Unstaking Controls
To prevent infrastructure churn or sudden dropouts that could disrupt the Kademlia DHT routing plane, the registry contract implements an unalterable **7-day unbonding cooldown period**. When an RBN operator initiates an unstake, the node is pruned from user lookup directories instantly, but the 50,000 $INTR token bond is held inside the PDA vault for 604,800 seconds to allow final cycle verification.
