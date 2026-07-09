# Economy Pipeline Architecture

**Date:** 2026-07-06
**Scope:** Full epoch cycle — client activity recording through INTR token payout

---

## Epoch Cycle Flow

```
Client DailyRewardEngine.record_activity()
  → shared_metrics bridge
  → RewardTracker.package_telemetry()
  → TelemetryEnvelope (cryptographically signed, 13 metrics)
  → Forward to connected RBN via mesh network

RBN RbnDailyRewardEngine.process_telemetry()
  → Signature verification
  → Double-claim guard
  → score_activities() with ActivityWeights
  → dual-pool split (social vs infra)
  → pool-clearing formula: user_share = points / global_estimate
  → prestige multiplier (1.0x–1.5x)
  → ClaimRequest generated

Midnight UTC: close_current_epoch()
  → collect all edge scores
  → IQR outlier mitigation
  → clamp outliers
  → proportional distribution from daily pool
  → Vec<ClaimRequest> returned

RBN → IPC → Solana daemon
  → Verify authentication
  → Double-claim check (persistent)
  → Circuit breaker (balance thresholds)
  → transfer_checked on Solana Mainnet
  → Treasury ATA → User ATA
```

---

## Security Architecture

The economy pipeline implements multi-layer security:

- **Cryptographic Authentication**: All telemetry envelopes are signed with Ed25519 keys derived from the client's Solana identity
- **Double-Claim Protection**: Dual-layer guard (in-memory + persistent SQLite) prevents replay attacks
- **Outlier Mitigation**: IQR-based anti-gaming filter clamps anomalous scores before distribution
- **IPC Authentication**: All inter-process claims are authenticated with cryptographic signatures
- **Circuit Breaker**: Automatic halt on insufficient treasury balances
- **ATA Derivation**: Proper on-chain token account derivation for all payouts

---

## Key Constants

| Constant | Value |
|----------|-------|
| Year 1 Daily Pool (users) | 16,438 INTR |
| Year 1 Daily Pool (RBNs) | 8,219 INTR |
| Strategic Reserve | 3,287.60 INTR/day |
| Annual Decay | 0.8x (20%/yr) |
| Daily Point Cap (standard) | 5,000 |
| Daily Point Cap (edge, 24h uptime) | 15,000 |
| Identity Lease Minimum | 100,000 INTR |
| TGE Date | 2026-01-01 |

---

## Prestige Tier Reference

| Tier | Name | Threshold | Reward Multiplier |
|------|------|-----------|-------------------|
| 0 | Citizen | < 100K INTR | 1.0x |
| 1 | Sentinel | >= 100K | 1.05x |
| 2 | Silver | >= 250K | 1.10x |
| 3 | Gold | >= 500K | 1.20x |
| 4 | Platinum | >= 1M | 1.50x |
| 5 | Catalyst | activity-based | 1.15x |
| 6 | Pulsar | activity-based | 1.15x |

---

## 13 Activity Metrics

| Index | Activity | Default Weight | Cap |
|-------|----------|---------------|-----|
| 0 | MsgSent | 1.0 | 5,000 pts |
| 1 | MsgReceived | 0.5 | 5,000 pts |
| 2 | GroupMessageSent | 1.2 | 5,000 pts |
| 3 | GroupReaction | 0.3 | 5,000 pts |
| 4 | FileSend | 2.0 | 5,000 pts |
| 5 | FileRecv | 1.0 | 5,000 pts |
| 6 | CallSeconds | 0.1/sec | 5,000 pts |
| 7 | RelayBytes | 0.001/KB | 50 MB (RBN) / 10 MB (edge) |
| 8 | UptimeSeconds | 0.01/sec | 86,400s |
| 9 | WebFocusedActiveTime | 0.1/sec | 86,400s, container ≤3 |
| 10 | SandboxWebPacketData | 0.02/KB | container ≤3 |
| 11 | WebViewMediaCallHook | 0.2/sec | container ≤3 |
| 12 | UniquePeerHandshakes | 1.0/peer | 500 peers |
