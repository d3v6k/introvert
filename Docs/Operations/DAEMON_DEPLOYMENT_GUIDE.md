# Introvert Daemon — Production Deployment Guide

## Overview

The Introvert Daemon (`introvert-daemon`) is a **separate codebase** from the main Introvert RBN code (`introvert`). It handles the economy layer — automated staking validation and SPL token payouts on Solana Mainnet.

| Repository | Purpose |
|------------|---------|
| **introvert** | RBN code — Flutter app, Rust networking core, relay daemon |
| **introvert-daemon** | Economy code — Solana staking validation, token payouts, treasury management |

This separation exists because the Solana SDK (100+ crates) conflicts with libp2p's dependency tree. Keeping them isolated prevents version mismatches and allows independent deployment.

The daemon consists of two isolated processes that communicate via a secure local TCP loopback bridge.

---

## Architecture

```
[Client Device]
       │
       │ TelemetryEnvelope (Ed25519 signed)
       │ 13 activity metrics + wallet addresses
       ▼
[introvert-p2p on Alibaba]
       │
       │ RbnDailyRewardEngine calculates payout
       │ Double-claim guard checks [epoch_id:peer_id]
       │
       │ ClaimRequest JSON over port 9001
       ▼
[introvert-solana on Alibaba]
       │
       │ Verifies claim, derives ATA
       │ transfer_checked to client ATA
       ▼
[Solana Mainnet] → [Client receives $INTR]
```

### Process Isolation

- **introvert-p2p**: Handles libp2p networking, peer identity, and Kademlia DHT routing. No Solana dependencies.
- **introvert-solana**: Handles Solana RPC connections, staking validation, and token payouts. No libp2p dependencies.

This isolation prevents dependency conflicts between the libp2p and Solana SDKs.

---

## Wallet Addresses & Public Keys

| Identifier | Address | Purpose |
|------------|---------|---------|
| **Mac Mini Treasury** | `GNNEC8q9urd6rBLeNrgGLME17T7winqqEes36cMh6wu8` | Local development treasury authority |
| **Alibaba Treasury** | `DZWeLhjPeH3q4Z45HyTh5BbWXiuXdHKK7od4yR9wGLQm` | Production cloud treasury authority |
| **$INTR Mint Address** | `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` | SPL Token mint address |
| **P2P Peer ID (Mac)** | `12D3KooWRQZUe1UosEYmmn4wdZq9sZ1uQNp7YYnhf2DWn2J6FT75` | libp2p swarm identity |
| **P2P Peer ID (Alibaba)** | `12D3KooWMuxiZaCvs7ZMEbT3tqdayNH71gxG3mMMmKbt7bmy4qzB` | libp2p swarm identity |

---

## Configuration Constants

| Constant | Value | Notes |
|----------|-------|-------|
| `INTR_MINT_ADDRESS` | `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` | Real $INTR mint address |
| `MINIMUM_STAKE_THRESHOLD` | `1000.0` | Minimum $INTR for payout eligibility |
| `PAYOUT_AMOUNT_TOKENS` | `50` | $INTR paid per verified event |
| `MINT_DECIMALS` | `9` | Standard Solana token decimals |
| `MAINNET_RPC` | `https://api.mainnet-beta.solana.com` | Public endpoint, swap for Helius/QuickNode |
| IPC Port | `9001` | Loopback only (127.0.0.1) |

---

## Dependencies

### introvert-p2p/Cargo.toml

```toml
libp2p-identity = "0.2.7"
libp2p-swarm = "0.44.0"
libp2p-tcp = { version = "0.41.0", features = ["tokio"] }
libp2p-noise = "0.44.0"
libp2p-yamux = "0.45.0"
libp2p-kad = "0.45.0"
libp2p-core = "0.41.3"
tokio = { version = "1.35", features = ["full"] }
futures = "0.3"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
static_assertions = "=1.1.0"
```

### introvert-solana/Cargo.toml

```toml
solana-client = "=1.18.26"
solana-sdk = "=1.18.26"
solana-zk-token-sdk = "=1.18.26"
spl-token = { version = "=4.0.0", features = ["no-entrypoint"] }
spl-associated-token-account = { version = "=2.3.0", features = ["no-entrypoint"] }
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
dirs = "5.0"
```

---

## How It Works

### P2P Daemon (`introvert-p2p`)

1. Generates Ed25519 keypair on startup
2. Builds TCP transport with noise encryption and yamux multiplexing
3. Initializes Kademlia DHT for peer routing
4. Listens on ephemeral port for incoming peer connections
5. On `ConnectionEstablished` event:
   - Prints peer ID
   - Serializes JSON payload: `{"event":"peer_connected","peer_id":"<peer_id>"}`
   - Spawns async task to send payload to `127.0.0.1:9001`

### Solana Daemon (`introvert-solana`)

1. Loads treasury keypair from `~/.config/introvert/treasury-authority.json`
2. Connects to Solana Mainnet RPC
3. Listens on `127.0.0.1:9001` for IPC events
4. On each event:
   - Parses peer ID as Solana public key
   - Derives Associated Token Account (ATA) for peer + $INTR mint
   - Queries Mainnet for token balance
   - If balance >= 1000 $INTR:
     - Fetches fresh blockhash
     - Constructs `transfer_checked` instruction (50 $INTR)
     - Signs with treasury keypair
     - Broadcasts to Mainnet
     - Logs transaction signature

---

## Security Measures

| Layer | Implementation |
|-------|----------------|
| Keypair storage | `chmod 600` on `treasury-authority.json` |
| Config directory | `chmod 700` on `~/.config/introvert/` |
| IPC binding | `127.0.0.1:9001` only, no external access |
| Process management | Launchd (Mac) / Systemd (Linux) with auto-restart |
| .gitignore | Excludes `*.json`, `.env`, `*.pem`, `*.key` |
| Fail-fast startup | Refuses to start if keypair missing |
| Transaction safety | `transfer_checked` with explicit mint/decimal verification |

---

## Deployment

### Mac Mini (Launchd)

```bash
# Build release binaries
cd introvert-daemon/introvert-p2p && cargo build --release
cd introvert-daemon/introvert-solana && cargo build --release

# Generate treasury keypair
./introvert-solana/target/release/introvert-keygen

# Lock permissions
chmod 700 ~/.config/introvert
chmod 600 ~/.config/introvert/treasury-authority.json

# Load Launchd services
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.introvert.p2p.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.introvert.solana.plist

# Check status
launchctl list | grep introvert

# View logs
tail -f ~/Documents/introvert-token/introvert-daemon/solana.log
```

### Alibaba RBN (Systemd)

```bash
# Cross-compile on ThinkPad
scp -r introvert-daemon/ dev@thinkpad.local:~/introvert-daemon/
ssh dev@thinkpad.local "source ~/.cargo/env && cd ~/introvert-daemon/introvert-p2p && cargo build --release"
ssh dev@thinkpad.local "source ~/.cargo/env && cd ~/introvert-daemon/introvert-solana && cargo build --release"

# Transfer binaries to Alibaba
scp dev@thinkpad.local:~/introvert-daemon/introvert-p2p/target/release/introvert-p2p root@47.89.252.80:/root/introvert-daemon/bin/
scp dev@thinkpad.local:~/introvert-daemon/introvert-solana/target/release/introvert-solana root@47.89.252.80:/root/introvert-daemon/bin/

# Generate keypair on server
ssh root@47.89.252.80 "/root/introvert-daemon/bin/introvert-keygen"

# Lock permissions
ssh root@47.89.252.80 "chmod 700 /root/.config/introvert && chmod 600 /root/.config/introvert/treasury-authority.json"

# Create systemd services
# See /etc/systemd/system/introvert-p2p.service and introvert-solana.service

# Enable and start
ssh root@47.89.252.80 "systemctl daemon-reload && systemctl enable introvert-p2p introvert-solana && systemctl start introvert-p2p introvert-solana"

# Check status
ssh root@47.89.252.80 "systemctl is-active introvert-p2p introvert-solana"

# View logs
ssh root@47.89.252.80 "tail -f /root/introvert-daemon/solana.log"
```

---

## DynamicPromoStack Management

The DynamicPromoStack allows runtime campaign adjustments without code rebuilds.

### Campaign Types

| PromoType | Description |
|-----------|-------------|
| CommunityThemeVote | Daily theme competition with community voting |
| EarlyAdopterBonus | Early user onboarding rewards |
| DeveloperHackathonYield | Developer contribution bounties |
| DynamicBonusCampaign | Custom promotional campaigns |

### Math Model

```
[Strategic Reserve Daily Ceiling: 3,287.60 INTR]
                    │
                    ├──► [- Minus] Active Campaigns (e.g., Theme: 1,000 INTR)
                    │
                    └──► [= Equals] Referral Pool (2,287.60 INTR)
```

### Launch a Campaign

```bash
ssh root@47.89.252.80 'curl -X POST http://localhost:41761/admin/promo/open \
  -H "Content-Type: application/json" \
  -d "{\"campaign_id\":\"theme_vote_july\",\"promo_type\":\"CommunityThemeVote\",\"daily_payout_allocation\":1000.0,\"expiration_epoch\":\"2026_08_02\"}"'
```

### Close a Campaign

```bash
ssh root@47.89.252.80 'curl -X POST http://localhost:41761/admin/promo/close \
  -H "Content-Type: application/json" \
  -d "{\"campaign_id\":\"theme_vote_july\"}"'
```

### View Active Campaigns

```bash
ssh root@47.89.252.80 'curl http://localhost:41761/admin/promo/list'
```

### Safety Features

- **Auto-eviction** — Expired campaigns automatically removed at epoch close
- **Safety cap** — Promo deductions cannot exceed Strategic Reserve ceiling
- **Runtime adjustments** — No code rebuilds required

---

## Pre-Launch Checklist

- [ ] Fund treasury wallet with 0.05-0.1 $SOL for transaction fees
- [ ] Deposit $INTR tokens into treasury wallet for payouts
- [ ] If using Helius/QuickNode, replace RPC URL in `main.rs`
- [ ] Verify firewall blocks external access to port 9001
- [ ] Confirm keypair permissions are `600` and directory is `700`

---

## Service Management

### Mac Mini (Launchd)

```bash
# Restart
launchctl kickstart -k gui/$(id -u)/com.introvert.p2p
launchctl kickstart -k gui/$(id -u)/com.introvert.solana

# Stop
launchctl bootout gui/$(id -u)/com.introvert.p2p
launchctl bootout gui/$(id -u)/com.introvert.solana
```

### Alibaba RBN (Systemd)

```bash
# Restart
systemctl restart introvert-p2p introvert-solana

# Stop
systemctl stop introvert-p2p introvert-solana

# View logs
journalctl -u introvert-solana -f
```

---

## Troubleshooting

### Service fails to start with "Treasury Keypair not found"

The daemon refuses to auto-generate a keypair on Mainnet for security. Generate one manually:

```bash
./introvert-keygen
chmod 600 ~/.config/introvert/treasury-authority.json
```

### Port 9001 already in use

Another process is binding to the loopback port. Find and stop it:

```bash
lsof -i :9001
kill <PID>
```

### RPC connection failures

Check if the public Solana RPC is rate-limited. Consider switching to a paid provider:

```rust
let rpc_url = "https://mainnet.helius-rpc.com/?api-key=YOUR_KEY".to_string();
```

---

## File Locations

### Mac Mini

| Path | Purpose |
|------|---------|
| `~/Documents/introvert-token/introvert-daemon/` | Project source code |
| `~/.config/introvert/treasury-authority.json` | Treasury keypair |
| `~/Library/LaunchAgents/com.introvert.*.plist` | Launchd configs |
| `~/Documents/introvert-token/introvert-daemon/*.log` | Service logs |

### Alibaba RBN

| Path | Purpose |
|------|---------|
| `/root/introvert-daemon/` | Project source and binaries |
| `/root/.config/introvert/treasury-authority.json` | Treasury keypair |
| `/etc/systemd/system/introvert-*.service` | Systemd configs |
| `/root/introvert-daemon/*.log` | Service logs |

---

## Deployment Timeline

1. Fixed libp2p type mismatch by upgrading sub-crate versions
2. Built dual-process architecture with TCP loopback IPC
3. Implemented keypair auto-generation and secure storage
4. Wired staking validation with ATA derivation and balance checks
5. Implemented `transfer_checked` payout execution
6. Compiled release binaries with optimizations
7. Deployed to Alibaba RBN via ThinkPad cross-compilation
8. Configured systemd services with auto-restart
9. Verified security isolation on all layers

---

**Status: Production-ready for beta launch.**
