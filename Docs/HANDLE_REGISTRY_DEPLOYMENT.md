# Introvert Handle Registry — Mainnet Deployment Guide

**Date:** 2026-07-08
**Status:** Ready for mainnet deployment (verified on local validator)
**Program ID:** `FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW`

---

## Overview

The Anchor Handle Registry is an on-chain Solana program that permanently binds Introvert handles (e.g., `i@alice`) to libp2p peer IDs. This replaces the previous SQLite + Kademlia DHT storage with immutable on-chain records funded by the protocol treasury.

**Architecture:** Treasury-funded PDA pool. A single global registry PDA funded once by the protocol. Handles stored as individual PDA accounts derived from `[b"handle", handle_string]`. Treasury pays rent; claimant signs to prove ownership.

---

## What Was Built

### New Files Created

| File | Purpose |
|------|---------|
| `solana_program/programs/introvert_handle_registry/Cargo.toml` | Anchor 0.29.0 dependencies |
| `solana_program/programs/introvert_handle_registry/src/lib.rs` | Program: `initialize` + `claim_handle` instructions |
| `solana_program/deployer-keypair.json` | Dedicated deployment keypair (not treasury) |
| `solana_program/Anchor.toml` | Updated to mainnet cluster |

### Modified Files

| File | Change |
|------|--------|
| `solana_program/Cargo.toml` | Added `introvert_handle_registry` to workspace |
| `for_linux/src/lib.rs` | Added `send_handle_registration_to_treasury()` IPC function |
| `for_linux/src/network/mod.rs` | Wired handle registration into claim flow |
| `introvert-daemon/introvert-solana/src/main.rs` | Added `process_handle_registration()` handler |
| `for_linux/src/lib.rs` | Fixed epoch close: `now - 13h` → `now - 0h` for midnight UTC |
| `for_linux/src/lib.rs` | Changed epoch close from 12:00 → 17:00 UTC |

### Anchor Program

```rust
// Program ID: FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW

#[program]
pub mod introvert_handle_registry {
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> { ... }
    pub fn claim_handle(ctx: Context<ClaimHandle>, handle: String, peer_id: String, timestamp: i64) -> Result<()> { ... }
}

// PDA Seeds:
// - Registry: [b"handle_registry"] — single global account
// - Handle entry: [b"handle", handle.as_bytes()] — one per handle

// Account Sizing:
// - HandleRegistry: 8 + 32 + 8 = 48 bytes
// - HandleEntry: 8 + 4+64 + 4+64 + 32 + 8 + 1 = ~185 bytes per handle
```

---

## Deployment Steps

### Step 1: Fund Deployer Wallet

Deployer wallet: `2RhPjPgttAHZe5cEGdsZ4hLyznEQBKVXiC1T36MTZyWj`
Required: **1.51 SOL** (rent-exempt for 211KB program)

Transfer SOL to this address from your mainnet wallet.

### Step 2: Configure CLI

```bash
cd /Users/dev/Development/introvert/solana_program
~/.local/share/solana/install/active_release/bin/solana config set \
  --url https://api.mainnet-beta.solana.com \
  --keypair ./deployer-keypair.json
```

### Step 3: Verify Balance

```bash
~/.local/share/solana/install/active_release/bin/solana balance
```

### Step 4: Deploy to Mainnet

```bash
cd /Users/dev/Development/introvert/solana_program
~/.local/share/solana/install/active_release/bin/solana program deploy \
  target/deploy/introvert_handle_registry.so \
  --program-id FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW \
  --with-compute-unit-price 10000
```

### Step 5: Verify Deployment

```bash
~/.local/share/solana/install/active_release/bin/solana program show \
  FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW
```

---

## Post-Deployment Integration

After mainnet deployment, update the Solana daemon to build and send the actual `claim_handle` instruction:

1. In `introvert-daemon/introvert-solana/src/main.rs`, update `process_handle_registration()` to build an Anchor CPI instruction
2. Use the treasury keypair (`~/.config/introvert/treasury-authority.json`) as payer
3. The claimant's public key (from `claimant_pubkey` field) becomes the owner signer

---

## IPC Flow (Already Deployed)

```
Client claims handle → RBN witnesses → sends HandleRegistration IPC to Solana daemon
  → Solana daemon validates HMAC signature
  → Logs: [HandleRegistry] Handle registration validated: {handle} -> {peer_id}
  → TODO: Build and send Anchor claim_handle instruction
```

**Files modified:**
- `for_linux/src/lib.rs`: `send_handle_registration_to_treasury()` function
- `for_linux/src/network/mod.rs`: Trigger on-chain registration after witness quorum
- `introvert-daemon/introvert-solana/src/main.rs`: `process_handle_registration()` handler

---

## Epoch Close Configuration

| Setting | Value |
|---------|-------|
| Epoch close time | 17:00 UTC |
| Epoch ID calculation | `now - 0 hours` (matches client) |
| TTL purge | 48 hours (stale telemetry removed) |
| Wallet cross-check | Enabled (rejects mismatched peer_id/wallet) |

---

## Verified Local Deployment

```
Program ID: FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW
Signature: j5JzkgoR2ox3g9hknL8ENqCKrTBCfhrgHkRy6Acu2dcpyJQ4Qzu8vjv7N4C3uN1E7icnrvbVBKZmUgJJsEq8Vbd
Cluster: Local validator (http://127.0.0.1:8899)
Binary: target/deploy/introvert_handle_registry.so (211KB)
```

---

## Build Notes

- **Rust toolchain issue:** Mac Rust 1.95 generates Cargo.lock v4 which Anchor's bundled Cargo 1.75 can't read. Workaround: manually set lockfile version to 3 via `sed`
- **SBF tools:** Solana CLI v4.0.0 (stable) with `cargo-build-sbf`
- **Anchor CLI:** 0.29.0 installed via `avm`
- **Compilation:** Must use `cargo-build-sbf` directly, not `anchor build` (Cargo.lock version conflict)
- **Cross-compilation:** All RBN daemon code must be compiled on `dev@thinkpad.local` (Debian x86_64), never on Alibaba RBN server (1GB RAM constraint)

---

## Security Notes

- Deployer keypair is separate from treasury keypair (`DZWeLh...wGLQm`)
- Treasury keypair stored at `~/.config/introvert/treasury-authority.json` on RBN
- IPC secret at `/etc/introvert/ipc.secret` (64 hex chars, chmod 600)
- Handle claims are immutable once verified on-chain
- Treasury pays rent; claimant signs to prove ownership
