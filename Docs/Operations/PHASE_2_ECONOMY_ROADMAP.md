# Phase 2 Economy Roadmap — Anchor PDA Escrow & Governance

**Status:** Active Development Plan (Next Sprint)
**Date:** 2026-07-03
**Prerequisites:** Phase 1 Beta Launch COMPLETE

---

## Phase 1 Archive (Completed 2026-07-03)

| Item | Value | Status |
|------|-------|--------|
| Treasury wallet | `DZWeLhjPeH3q4Z45HyTh5BbWXiuXdHKK7od4yR9wGLQm` | ✅ Active |
| Treasury ATA | `HobcUEUBHXfwRW1DWv1XaZkAqiMeghN14utUGXuFPauR` | ✅ Active |
| $INTR balance | 51,000 INTR | ✅ Funded |
| SOL balance | 0.098 SOL | ✅ Funded |
| $INTR mint | `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` | ✅ Unified |
| Lease threshold | 100,000 INTR | ✅ Enforced |
| RPC endpoint | `SOLANA_RPC_URL` env var (Helius recommended) | ✅ Configured |
| IPC security | HMAC-SHA256 signed messages | ✅ Active |
| Payout ledger | SQLite persistent deduplication | ✅ Active |
| Circuit breaker | SOL/INTR balance monitoring | ✅ Active |
| End-to-end test | Mainnet tx confirmed | ✅ Passed |

### Transaction Signature
```
5gA9AjfEEYwRni7b4uTZ4WaM9yZDSMCUihNq47oQvK4ofArs3n75EnXzus4Ztv9VFx7WtvFz1DurZKSdnAgMFCyz
```

---

## Phase 2 Technical Agenda

### Sprint Objective
Transition token distribution from hot wallet to immutable on-chain PDA escrow, establish multisig governance, and launch staking interface.

---

### Task 1: Anchor Smart Contract — PDA Escrow Program

**Goal:** Replace hot wallet distribution with on-chain Program-Derived Address escrow vault.

#### 1.1 Program Instructions Required

| Instruction | Purpose | Parameters |
|-------------|---------|------------|
| `register_rbn` | Register operator with real 2M INTR bond | `operator_pubkey`, `multiaddresses`, `node_name` |
| `update_multiaddr` | Update operator network addresses | `new_multiaddresses` |
| `heartbeat` | Liveness proof (prevents slashing) | `timestamp`, `slot` |
| `unstake` | Initiate 7-day cooldown withdrawal | `amount` |
| `slash_rbn` | Governance-controlled penalty | `operator_pubkey`, `reason`, `amount` |
| `claim_rewards` | Distribute daily rewards from escrow | `epoch_id`, `amount` |

#### 1.2 PDA Derivation

```rust
seeds = [b"rbn_bond", operator_pubkey.as_ref()]
program_id = RBNRegXy4vQszN2Cg8gqf91mYyL24p8cT32d1mY1111
```

#### 1.3 Escrow State Account

```rust
#[account]
pub struct RbnBondEscrow {
    pub operator_pubkey: Pubkey,      // 32 bytes
    pub bonded_amount: u64,           // 8 bytes (2,000,000 INTR = 2_000_000_000_000_000 nano)
    pub bonded_mint: Pubkey,          // 32 bytes
    pub execution_nonce: u64,         // 8 bytes
    pub bonded_at: i64,               // 8 bytes
    pub unbond_requested_at: Option<i64>, // 9 bytes
    pub is_active: bool,              // 1 byte
}
```

#### 1.4 Token Flow

```
[RBN Operator] ──2M INTR──► [PDA Escrow Vault]
                                    │
                                    ├──► Daily rewards distributed to verified peers
                                    │
                                    └──► 7-day unbond cooldown → return to operator
```

#### 1.5 Files to Modify

| File | Action |
|------|--------|
| `solana_program/programs/introvert_registry/src/lib.rs` | Rewrite with full instruction set |
| `solana_program/programs/introvert_registry/src/state.rs` | NEW — Escrow state structs |
| `solana_program/programs/introvert_registry/src/errors.rs` | NEW — Custom error codes |
| `solana_program/Anchor.toml` | Update program ID, add devnet config |
| `solana_program/tests/introvert_registry.ts` | NEW — Anchor integration tests |

#### 1.6 Verification

```bash
# Build and test on devnet
anchor build
anchor test

# Deploy to devnet
anchor deploy --provider.cluster devnet

# Verify program
solana program show RBNRegXy4vQszN2Cg8gqf91mYyL24p8cT32d1mY1111
```

---

### Task 2: Squads V4 Multisig Governance

**Goal:** Remove single-point-of-failure by establishing 3-of-5 multisig governance.

#### 2.1 Setup Steps

1. **Create Squads V4 vault** at `https://app.squads.so`
2. **Invite 5 admin keyholders** (team members + hardware wallets)
3. **Configure 3-of-5 threshold** for transaction approval
4. **Transfer program upgrade authority** to multisig
5. **Transfer treasury authority** to multisig (post-beta)

#### 2.2 Multisig Members (To Be Populated)

| Slot | Role | Key Type |
|------|------|----------|
| 1 | Lead Developer | Hardware wallet (Ledger) |
| 2 | Operations | Hardware wallet (Ledger) |
| 3 | Security Auditor | Air-gapped key |
| 4 | Community Representative | Software wallet |
| 5 | Backup/Cold Storage | Cold storage key |

#### 2.3 Governance Parameters

| Action | Threshold | Timelock |
|--------|-----------|----------|
| Program upgrade | 3-of-5 | 48 hours |
| Treasury withdrawal | 3-of-5 | 24 hours |
| Slash operator | 3-of-5 | 24 hours |
| Emergency pause | 2-of-5 | None |
| Config update | 3-of-5 | 24 hours |

#### 2.4 Files to Create/Modify

| File | Action |
|------|--------|
| `Docs/Operations/SQUADS_V4_SETUP_GUIDE.md` | NEW — Step-by-step setup |
| `Docs/Operations/MULTISIG_GOVERNANCE.md` | NEW — Governance rules |
| `solana_program/Anchor.toml` | Update authority to multisig |

---

### Task 3: $INTR Node Staking Site

**Goal:** Frontend interface for operators to bond/unbond, view rewards, and track prestige tiers.

#### 3.1 Features

| Feature | Description |
|---------|-------------|
| Bond INTR | Deposit 2M INTR to PDA escrow |
| Unbond INTR | Initiate 7-day cooldown withdrawal |
| View Balance | Display bonded amount + pending rewards |
| Prestige Tier | Show current tier + multiplier |
| Reward History | Daily reward distribution log |
| Heartbeat Status | Liveness proof timestamp |
| Slashing History | Any penalties applied |

#### 3.2 Tech Stack

| Component | Technology |
|-----------|------------|
| Frontend | React/Next.js or Flutter Web |
| Wallet | Solana Wallet Adapter (Phantom, Solflare, Backpack) |
| RPC | Helius private endpoint |
| State | On-chain PDA account queries |

#### 3.3 API Endpoints (Backend)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/operator/:pubkey` | GET | Operator status + balance |
| `/api/operator/:pubkey/rewards` | GET | Reward history |
| `/api/operator/:pubkey/heartbeat` | POST | Submit liveness proof |
| `/api/network/stats` | GET | Global network statistics |

#### 3.4 Files to Create

| File | Purpose |
|------|---------|
| `staking-site/` | NEW — Frontend project directory |
| `staking-site/src/components/BondForm.tsx` | Bond/unbond interface |
| `staking-site/src/components/OperatorDashboard.tsx` | Status display |
| `staking-site/src/hooks/useStaking.ts` | Solana program interaction |
| `staking-site/src/api/endpoints.ts` | Backend API client |

---

## Funding Model

| Period | Amount | Destination |
|--------|--------|-------------|
| Beta (current) | 51,000 INTR | Hot wallet |
| 6 months | 5,086,056 INTR | PDA escrow |
| 1 year | 10,199,816 INTR | PDA escrow |

**Rule:** Full allocation deposited ONLY after Anchor program + Squads V4 multisig are deployed and verified.

---

## Sprint Timeline

| Day | Task | Deliverable |
|-----|------|-------------|
| 1-2 | Anchor program development | `register_rbn`, `unstake`, `heartbeat` |
| 2-3 | Anchor program completion | `slash_rbn`, `claim_rewards`, tests |
| 3 | Devnet deployment + testing | Verified program on devnet |
| 4 | Squads V4 setup | Multisig vault created, members invited |
| 4 | Mainnet deployment | Program deployed, authority transferred |
| 5 | Staking site scaffolding | React/Next.js project initialized |
| 5-7 | Staking site development | Bond/unbond, dashboard, rewards display |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Anchor program bugs | Test on devnet first, use Anchor's built-in security |
| Multisig key loss | 3-of-5 threshold, backup keys in cold storage |
| Hot wallet compromise | Only 50K INTR during beta, PDA holds full allocation |
| RPC rate limits | Helius private endpoint |
| Slashing disputes | 24-hour timelock on slash operations |

---

## Verification Checklist

### After Task 1 (Anchor Program)
- [ ] `anchor build` succeeds
- [ ] `anchor test` passes on devnet
- [ ] Program deployed to devnet
- [ ] `register_rbn` instruction works
- [ ] `unstake` instruction works with 7-day cooldown
- [ ] `heartbeat` instruction works

### After Task 2 (Squads V4)
- [ ] Vault created at `https://app.squads.so`
- [ ] 5 members invited and accepted
- [ ] 3-of-5 threshold configured
- [ ] Program upgrade authority transferred
- [ ] Test transaction approved by 3 members

### After Task 3 (Staking Site)
- [ ] Bond/unbond interface works
- [ ] Operator dashboard displays correct data
- [ ] Reward history displays correctly
- [ ] Wallet connection works (Phantom, Solflare)
- [ ] All API endpoints return correct data
