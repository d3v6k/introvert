# Release Notes v53 — Economy Daemon, TelemetryEnvelope & DynamicPromoStack

**Release Date:** 2026-07-03
**Status:** Production-ready for beta launch

---

## Executive Summary

Major release implementing the full economy chain with TelemetryEnvelope, Ed25519 signing, double-claim guard, proportional reward calculation, and DynamicPromoStack for customizable campaign management. Deployed to Alibaba RBN production.

---

## Key Features

### 1. TelemetryEnvelope Implementation
- Signed telemetry packet with 13 activity metrics
- Client's Solana wallet and ATA addresses included
- SHA-256 proof hash for relay bytes verification
- Ed25519 signature for cryptographic verification

### 2. Real Signature Verification
- Replaced stub `return true` with actual Ed25519 verification
- Validates signature against client's Solana public key
- Reconstructs signed message from epoch_id + metrics + timestamp
- Rejects packets with invalid signatures

### 3. Double-Claim Guard
- HashSet tracking `[epoch_id:peer_id]` pairs
- Prevents replay attacks across multiple RBNs
- Memory-based, no disk I/O overhead
- Instant rejection of duplicate claims

### 4. Proportional Reward Calculation
- Pool-clearing formula based on actual contributions
- No hardcoded flat payouts
- Dual-pool system (social + infra)
- Prestige multipliers (1.0x to 1.5x)

### 5. Merit-Based Rewards
- Balance gate removed (was 1000 INTR)
- Pure contribution-based rewards
- Any active client can earn $INTR

### 6. DynamicPromoStack (Customizable Campaign Layer)
- Runtime campaign management without code rebuilds
- Strategic Reserve ceiling: 3,287.60 INTR/day (Year 1)
- Campaign types: CommunityThemeVote, EarlyAdopterBonus, DeveloperHackathonYield, DynamicBonusCampaign
- Auto-eviction of expired campaigns
- Safety cap prevents over-emission
- Referral pool compression protects core rewards

---

## Token Configuration

| Parameter | Value |
|-----------|-------|
| $INTR Mint | `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf` |
| Treasury Wallet | `DZWeLhjPeH3q4Z45HyTh5BbWXiuXdHKK7od4yR9wGLQm` |
| Treasury ATA | `HobcUEUBHXfwRW1DWv1XaZkAqiMeghN14utUGXuFPauR` |
| Daily User Pool | 16,438 INTR (Year 1) |
| Daily RBN Pool | 8,219 INTR (Year 1) |
| Strategic Reserve | 3,287.60 INTR/day (Year 1) |
| Annual Decay | 20% |

---

## Activity Types (13 Metrics)

| # | Activity | Weight | Cap |
|---|----------|--------|-----|
| 0 | MessageSent | 10.0/msg | 200 |
| 1 | MessageReceived | 5.0/msg | 300 |
| 2 | GroupMessageSent | 8.0/msg | 150 |
| 3 | GroupReaction | 3.0/react | 100 |
| 4 | FileTransferSent | 20.0/file | 20 |
| 5 | FileTransferRecv | 10.0/file | 20 |
| 6 | CallDurationSecs | 1.0/sec | 3,600 |
| 7 | RelayBytes | 0.01/KB | 10,240 KB |
| 8 | UptimeSeconds | 0.005/sec | 86,400 |
| 9 | WebFocusedActiveTime | 0.1/sec | 86,400 |
| 10 | SandboxWebPacketData | 0.02/KB | 10,240 KB |
| 11 | WebViewMediaCallHook | 0.2/sec | 1,800 |
| 12 | UniquePeerHandshakes | 1.0/peer | 500 |

---

## Security Improvements

| Feature | Before | After |
|---------|--------|-------|
| Signature verification | Stub (`return true`) | Real Ed25519 |
| Balance gate | 1000 INTR required | Disabled (merit-based) |
| Payout amount | Hardcoded 50 INTR | Dynamic proportional |
| Claim format | Simple `peer_connected` | Structured `ClaimRequest` |
| Double-claim protection | None | HashSet guard |

---

## Deployment

### Alibaba RBN (47.89.252.80)

| Service | Status | Purpose |
|---------|--------|---------|
| introvert-p2p | Active | DailyRewardEngine, epoch timer, telemetry |
| introvert-solana | Active | Double-claim guard, SPL transfers |

### Systemd Services

```bash
# Check status
systemctl status introvert-p2p introvert-solana

# Restart
systemctl restart introvert-p2p introvert-solana

# View logs
tail -f /root/introvert-daemon/p2p.log
tail -f /root/introvert-daemon/solana.log
```

---

## Compilation Status

| Target | Status |
|--------|--------|
| introvert-p2p | ✅ Compiles |
| introvert-solana | ✅ Compiles |
| for_linux | ✅ Compiles |

---

## Files Modified

| File | Changes |
|------|---------|
| `introvert-daemon/introvert-p2p/src/daily_rewards.rs` | TelemetryEnvelope, Ed25519 verification |
| `introvert-daemon/introvert-p2p/src/main.rs` | PeerTelemetry with wallet fields |
| `introvert-daemon/introvert-solana/src/main.rs` | ClaimRequest parsing, double-claim guard |
| `for_linux/src/economy/mod.rs` | All 13 record_*() methods, package_telemetry() |
| `for_linux/src/economy/daily_rewards.rs` | TelemetryEnvelope struct |
| `for_linux/src/lib.rs` | Epoch-based telemetry sending |

---

## Known Limitations

| Issue | Status | Impact |
|-------|--------|--------|
| Client telemetry not wired | Pending integration | No real rewards yet |
| PDA escrow not implemented | Designed | RBN bond verification missing |
| Global points estimate | Static 100,000 | Economic distortion |
| Proof hash validation | Partial | Relay spoofing possible |

---

## Next Steps

1. Deposit $INTR tokens into treasury wallet
2. Wire client to send real TelemetryEnvelope packets
3. Implement PDA escrow for RBN bond verification
4. Dynamic global_points_estimate from network data
5. Full proof hash validation

---

## Backup Location

`/Volumes/512-SSD-External/introvert back up/stable_v53_economy_daemon/`

---

**Status: Production-ready for beta launch**
