# Introvert Project Instructions

## Project Context
This project implements a privacy-focused, P2P communication system using Rust and libp2p.

## Core Documentation
- [Introvert Master Plan](./Docs/INTROVERT_MASTER_PLAN.md): Detailed execution strategy and vision.

## Technical Standards
- **Language:** Rust (Core), Flutter (UI).
- **Networking:** libp2p (QUIC/WebRTC).
- **Storage:** SQLite/SQLCipher with CRDTs.
- **Protocol:** Strict modular execution following the Master Plan.

## Workflows
- Always refer to `INTROVERT_MASTER_PLAN.md` before starting a new phase.
- Use the "Initialize Phase X" protocol to begin new segments of work.

## Infrastructure & Deployment
- Passwordless SSH login (using keys) is configured and active between the local Mac, the build machine (`dev@thinkpad.local`), and the RBN node (`root@47.89.252.80`).
- No interactive password prompts are required for remote operations or `./deploy_rbn.sh` execution. Scripts can be run fully asynchronously and unattended.

## Documentation & Memory
- All design plans, guides, audits, and specifications MUST be stored in the `Docs/` directory for reference across agent sessions.
- Key reference documents:
  - [RBN Holding Requirements](./Docs/RBN_HOLDING_REQUIREMENTS.md): Details the staking tiers (2M $INTR for RBN, 100k $INTR / 3x yield for Edge relay, 2M $INTR for app UI eligibility).
  - [RBN Community Hosting Plan](./Docs/RBN_COMMUNITY_HOSTING_PLAN.md): Operator guide for running remote RBN nodes.
  - [RBN Dashboard Access Guide](./Docs/RBN_DASHBOARD_ACCESS_GUIDE.md): Guide for port-tunneling and dashboard authentication.
  - [Solana RBN Registry Plan](./Docs/SOLANA_RBN_REGISTRY_PLAN.md): Architecture of the Anchor registration contract.
  - [Solana Mainnet Execution Addendum](./Docs/SOLANA_MAINNET_EXECUTION_ADDENDUM.md): Security mitigation specifications for Mainnet release.
  - [Introvert Marketing Report](./Docs/MARKETING_REPORT.md): Key differentiators and competitive landscape.
  - [Docs/Audits/](./Docs/Audits/): Subfolder containing security, network, FFI, storage, and UI audit reports.
