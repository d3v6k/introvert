# Introvert Handle Registry — Deployment Guide

**Date:** 2026-07-08
**Status:** Ready for deployment (verified on local validator)
**Program ID:** `FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW`

---

## Overview

The Anchor Handle Registry is an on-chain Solana program that permanently binds Introvert handles (e.g., `i@alice`) to libp2p peer IDs. This replaces the previous SQLite + Kademlia DHT storage with immutable on-chain records funded by the protocol treasury.

**Architecture:** Treasury-funded PDA pool. Handles stored as individual PDA accounts derived from `[b"handle", handle_string]`.

---

## Program Structure

```rust
#[program]
pub mod introvert_handle_registry {
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> { ... }
    pub fn claim_handle(ctx: Context<ClaimHandle>, handle: String, peer_id: String, timestamp: i64) -> Result<()> { ... }
}

// PDA Seeds:
// - Registry: [b"handle_registry"] — single global account
// - Handle entry: [b"handle", handle.as_bytes()] — one per handle
```

---

## Deployment Steps

### Step 1: Fund Deployer Wallet

Deployer wallet requires **1.51 SOL** (rent-exempt for program binary).

### Step 2: Configure CLI

```bash
solana config set --url https://api.mainnet-beta.solana.com --keypair ./deployer-keypair.json
```

### Step 3: Deploy Program

```bash
solana program deploy --program-id ./target/deploy/introvert_handle_registry-keypair.json ./target/deploy/introvert_handle_registry.so
```

### Step 4: Initialize Registry

After deployment, call the `initialize` instruction to create the global registry PDA.

---

## Integration

After deployment:
1. Wire `claim_handle` instruction in treasury daemon
2. Initialize on-chain registry PDA
3. Migrate existing handles from SQLite to on-chain
