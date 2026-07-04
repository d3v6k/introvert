# Protocol Specification & FFI Bridge

## 1. Global Event Codes
The Rust core dispatches events to the UI using a `u8` type code followed by a binary payload.

| Code | Name | Payload Description |
| :--- | :--- | :--- |
| **1** | Peer Discovered | Binary `PeerId`. |
| **2** | Message Received | Decrypted message string (UTF-8). |
| **7** | E2EE Active | `PeerId_String` + `\0` (Separator) + `0` (Success byte). |
| **8** | Peer Status | `PeerId_String` + `\0` (Separator) + `[0=Direct, 1=Relay, 2=Offline]`. |
| **11** | Anchor Mode | `[0=Disabled, 1=Enabled]`. |
| **22** | Node Eligible | `[0=Ineligible, 1=Eligible]` (Enforced based on the 100,000 $INTR balance tier verification step). |

---

## 2. Decentralized RBN Infrastructure Lifecycle

### Step 1: On-Chain Initialization
The operator launches the headless Linux server daemon binary (`src/main.rs`). The daemon interacts with the Anchor token module to lock up **2,000,000 $INTR** from their private key into the program's secure PDA Escrow, uploading their listener path configuration (`/ip4/x.x.x.x/tcp/443/p2p/PeerId`).

### Step 2: Dynamic Directory Retrieval
A sovereign user mobile app runs initialization. It fires a block query to parse the `introvert-registry` account table, matches active nodes with non-zero escrow balances, and maps the target bootstrap paths.

### Step 3: Work Verification & Continuous Emission
As the user client runs messaging routing or handles file chunks, it signs localized cryptographic performance tickets (`RbnPerformanceTicket`). The node operator submits these tickets to the escrow program every epoch. The contract runs signature and oracle validation, then transfers daily $INTR yields directly out of the unified PDA vault to the node provider.

### Step 4: Governance-Gated Upgrades
If a bug occurs in the registry, the 3-of-5 Squads Multisig updates the program instructions. No single developer possesses an overrides path to alter structural rules, reward issuance rates, or the unbonding parameters.
