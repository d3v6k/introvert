# Introvert Documentation Index

This directory contains the technical blueprints, specifications, audits, guides, and plans for the Introvert P2P network.

**Important**: Introvert operates as two separate codebases:

| Repository | Purpose | Location |
|------------|---------|----------|
| **introvert** (this repo) | RBN code — Flutter app, Rust networking core, relay daemon | `/Users/dev/Development/introvert/` |
| **introvert-daemon** | Economy code — Solana staking validation, token payouts, treasury management | `/Users/dev/Documents/introvert-token/introvert-daemon/` |

To keep the repository clean, the documentation has been organized into logical subfolders. Below is the master sitemap.

---

## 📂 Documentation Sitemap

### 1. ⚙️ Root Documents (Core Reference)
*   **[INTROVERT_MASTER_PLAN.md](./INTROVERT_MASTER_PLAN.md):** The core execution strategy, technical phases, and vision.
*   **[ARCHITECTURE_BLUEPRINT.md](./ARCHITECTURE_BLUEPRINT.md):** System component layers, token-gating engine, and design principles.
*   **[CONFIGURATION_REFERENCE.md](./CONFIGURATION_REFERENCE.md):** Environment variables, client settings, and engine configurations.
*   **[CHANGELOG.md](./CHANGELOG.md):** Complete development history and version changes.
*   **[DEBUG_SESSION_STATUS.md](./DEBUG_SESSION_STATUS.md):** The active debugging log (large file transfer stall & handover solutions).
*   **[DEBUG_DOCUMENT.md](../DEBUG_DOCUMENT.md):** Cross-network file transfer debug document with root cause analysis, all changes, and expert handoff notes.

### 2. 🔌 Protocols & Signaling (`Protocol/`)
Technical specifications of the networking and data serialization formats:
*   **[FILE_TRANSFER_PROTOCOL.md](./Protocol/FILE_TRANSFER_PROTOCOL.md):** Adaptive chunking (64KB/256KB), pacing, and pull-based pipeline recovery.
*   **[FFI_API_REFERENCE.md](./Protocol/FFI_API_REFERENCE.md):** API specification for the Rust-to-Dart FFI C-bindings.
*   **[BINARY_CODEC_UPGRADE_PLAN.md](./Protocol/BINARY_CODEC_UPGRADE_PLAN.md):** The v2.0.0 binary codec specifications and fallback logic.
*   **[NETWORK_&_SIGNALING.md](./Protocol/NETWORK_&_SIGNALING.md):** Basic libp2p swarm and signaling configurations.
*   **[PROTOCOL_SPECIFICATION.md](./Protocol/PROTOCOL_SPECIFICATION.md):** Message exchange sequence rules.
*   **[introvert_codec.md](./Protocol/introvert_codec.md):** The binary/JSON hybrid serialization specification.
*   **[introvert_system.md](./Protocol/introvert_system.md):** High-level system interaction protocol flow.

### 3. 🪙 Tokenomics & Economy (`Economy/`)
The mathematical specs and staking architecture of the $INTR economy:
*   **[INTROVERT_TOKEN_WHITEPAPER.md](./Economy/INTROVERT_TOKEN_WHITEPAPER.md):** Staking tiers, allocations, and consensus model.
*   **[DAILY_REWARDS_SYSTEM.md](./Economy/DAILY_REWARDS_SYSTEM.md):** Contribution-weight algorithm for daily emissions.
*   **[ECONOMY_MATHEMATICAL_SPECIFICATION.md](./Economy/ECONOMY_MATHEMATICAL_SPECIFICATION.md):** Proof of Staking (PoS) and emissions curves.
*   **[ECONOMY_PULL_BASED_REWARD_ARCHITECTURE.md](./Economy/ECONOMY_PULL_BASED_REWARD_ARCHITECTURE.md):** Decentralized claiming and proof mechanics.
*   **[ECONOMY_V3_REVISED_REWARD_MODEL.md](./Economy/ECONOMY_V3_REVISED_REWARD_MODEL.md):** Revised anti-inflationary yield allocations.
*   **[ECONOMY_V3_SIMULATION_SCENARIOS.md](./Economy/ECONOMY_V3_SIMULATION_SCENARIOS.md):** Stress simulations for reward sustainability.
*   **[INTROVERT_ECONOMY_BLUEPRINT.md](./Economy/INTROVERT_ECONOMY_BLUEPRINT.md):** Escrow and program governance models.
*   **[RBN_HOLDING_REQUIREMENTS.md](./Economy/RBN_HOLDING_REQUIREMENTS.md):** Staking threshold rules (100k for edge, 2M for RBN).

### 4. 🔗 Solana Integration (`Solana/`)
Anchor programs, PDA registries, and Mainnet-Beta execution:
*   **[SOLANA_RBN_REGISTRY_PLAN.md](./Solana/SOLANA_RBN_REGISTRY_PLAN.md):** Architecture of the Anchor registration contract.
*   **[SOLANA_MAINNET_EXECUTION_ADDENDUM.md](./Solana/SOLANA_MAINNET_EXECUTION_ADDENDUM.md):** Security mitigations for mainnet release.

### 5. 🛠️ Operations & Guides (`Operations/`)
Setup references, build steps, and deployment procedures:
*   **[PHASE_2_ECONOMY_ROADMAP.md](./Operations/PHASE_2_ECONOMY_ROADMAP.md):** Active development plan for Anchor PDA Escrow, Squads V4 Multisig, and Staking Site.
*   **[DAEMON_DEPLOYMENT_GUIDE.md](./Operations/DAEMON_DEPLOYMENT_GUIDE.md):** Production daemon deployment for staking validation and token payouts.
*   **[BUILD_&_DEPLOYMENT_GUIDE.md](./Operations/BUILD_&_DEPLOYMENT_GUIDE.md):** Prerequisites and native compilation guides.
*   **[RBN_COMMUNITY_HOSTING_PLAN.md](./Operations/RBN_COMMUNITY_HOSTING_PLAN.md):** Staking and server provisioning for community operators.
*   **[RBN_DASHBOARD_ACCESS_GUIDE.md](./Operations/RBN_DASHBOARD_ACCESS_GUIDE.md):** Port-tunneling and dashboard authentication.
*   **[RBN_OPERATOR_GUIDE.md](./Operations/RBN_OPERATOR_GUIDE.md):** Installation, telemetry, and log monitoring scripts.
*   **[RBN_PHASE_2_DEPLOYMENT_PLAN.md](./Operations/RBN_PHASE_2_DEPLOYMENT_PLAN.md):** Scaling and automation milestones.
*   **[ENVIRONMENT_VARIABLES.md](./Operations/ENVIRONMENT_VARIABLES.md):** All configurations supported by the Rust core.
*   **[FIREBASE_FCM_SETUP.md](./Operations/FIREBASE_FCM_SETUP.md):** Setup guidelines for Android push notifications.
*   **[IOS_PUSH_SETUP.md](./Operations/IOS_PUSH_SETUP.md):** Setup guidelines for Apple APNs.
*   **[PUSH_NOTIFICATION_ARCHITECTURE.md](./Operations/PUSH_NOTIFICATION_ARCHITECTURE.md):** Sleep state handling and RBN relaying.
*   **[REBUILD_GUIDE.md](./Operations/REBUILD_GUIDE.md):** How to force clean re-builds.
*   **[RELEASE_PROCESS.md](./Operations/RELEASE_PROCESS.md):** Semantic versioning and production releases.
*   **[STABLE_VERSION_PROCESS.md](./Operations/STABLE_VERSION_PROCESS.md):** Creation and recovery of stable builds.
*   **[TESTING_GUIDE.md](./Operations/TESTING_GUIDE.md):** Unit and integration testing protocols.
*   **[TROUBLESHOOTING.md](./Operations/TROUBLESHOOTING.md):** Diagnostic playbooks.

### 6. 🛡️ Audits & stress Tests (`Audits/`)
Security, cryptographic, network, and database audit reports:
*   **[INTROVERT_FILE_TRANSFER_STALL_AUDIT.md](./Audits/INTROVERT_FILE_TRANSFER_STALL_AUDIT.md):** Audit of Yamux stream buffer congestion.
*   **[DEEP_AUDIT_v31_REGRESSION_REPORT.md](./Audits/DEEP_AUDIT_v31_REGRESSION_REPORT.md):** Code regression report for core version v3.1.
*   **[MESH_STRESS_TEST_REPORT_2026_06_07.md](./Audits/MESH_STRESS_TEST_REPORT_2026_06_07.md):** Stress test results for multi-peer networks.
*   **[network_performance.md](./Audits/network_performance.md):** Peer-to-peer transport latency benchmarks.

### 7. 📈 Marketing & Vision (`Marketing/`)
Introvert competitive analysis, sustainability plans, and the network manifesto:
*   **[MARKETING_REPORT.md](./Marketing/MARKETING_REPORT.md):** Differentiators, token utilities, and competitive positioning.
*   **[INTROVERT_MANIFESTO.md](./Marketing/INTROVERT_MANIFESTO.md):** Principles of decentralized ownership.
*   **[GREEN_ENERGY_&_SUSTAINABILITY.md](./Marketing/GREEN_ENERGY_&_SUSTAINABILITY.md):** Sustainability credentials of a zero-datacenter mesh.

### 8. 📦 Database & Components (`Components/`)
Database schemas, component registries, and layout manifests:
*   **[DATABASE_SCHEMA.md](./Components/DATABASE_SCHEMA.md):** Detailed table layout (18 tables) of the SQLCipher database.
*   **[UI_COMPONENT_MANIFEST.md](./Components/UI_COMPONENT_MANIFEST.md):** Registry of reusable Flutter elements.
*   **[encrypted_drive.md](./Components/encrypted_drive.md):** Sovereign Drive directory trees and storage layers.
*   **[MODULE_REFERENCE.md](./Components/MODULE_REFERENCE.md):** Architecture of the 12 Intro-Claw maintenance modules.
*   **[DEPLOYMENT_ARCHITECTURE.md](./Components/DEPLOYMENT_ARCHITECTURE.md):** Swarm network deployment architectures.
*   **[SECURITY_&_ENCRYPTION.md](./Components/SECURITY_&_ENCRYPTION.md):** Wire and persistence encryption details.

### 9. 📋 Release Notes (`Releases/`)
Version-specific release documentation:
*   **[RELEASE_NOTES_v49.md](./Releases/RELEASE_NOTES_v49.md):** Cross-Network Delivery & Mailbox Integrity (latest)
*   **[RELEASE_NOTES_v42.md](./Releases/RELEASE_NOTES_v42.md):** Reliable Push Wakeups
*   **[RELEASE_NOTES_v40.md](./Releases/RELEASE_NOTES_v40.md):** High-Speed Relays
*   **[RELEASE_NOTES_v34.md](./Releases/RELEASE_NOTES_v34.md):** Iron Claw (gold standard for cross-network)
