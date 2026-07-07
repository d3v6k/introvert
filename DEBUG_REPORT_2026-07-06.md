# Introvert Network Debug Report
**Date:** 2026-07-06
**Devices:** Android (VPN), Mac (no VPN), iOS (no VPN)
**RBN:** 47.89.252.80 — PeerID `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a`

---

## 1. Original Problem: Android on VPN Can't Connect

### Symptom
Android device behind VPN establishes TCP connection to RBN, gets `ReservationReqAccepted`, but never reaches `Status=1` (ONLINE). Never establishes `OutboundCircuitEstablished` or `InboundCircuitEstablished`. Never receives group messages.

### Comparison with working devices

| Device | VPN | TCP Connect | Reservation | Status=1 | Circuits | Messages |
|--------|-----|-------------|-------------|----------|----------|----------|
| Android | Yes (WSS tunnel) | ✅ | ✅ | ❌ | ❌ | ❌ |
| Mac | No | ✅ | ✅ | ✅ | ✅ | ✅ |
| iOS | No | ✅ | ✅ | ✅ | ✅ | ✅ |

### Android-specific observations from logs
- VPN activates WSS tunnel on port 36649 → `wss://47.89.252.80/tunnel`
- Dials bootstrap including `127.0.0.1/tcp/36649` (loopback tunnel endpoint)
- `ReservationReqAccepted` succeeds
- Repeatedly auto-registers push token with RBN (5+ times in 1 minute) — indicates polling because relay isn't stable
- Never transitions from status=4 (CONNECTING) to status=1 (ONLINE)

### Root cause hypothesis
The VPN's WebSocket proxy handles the initial HTTPS handshake (reservation works) but drops or stalls the longer-lived stream needed for circuit relay. The data-plane connection for `OutboundCircuitEstablished` never completes through the VPN tunnel.

---

## 2. Current Critical Issue: ALL Devices Stuck on "Connecting" After RBN Restart

### Timeline
- **21:46** — RBN was ONLINE intermittently, devices connected
- **21:55** — RBN restarted (`systemctl restart introvertd`)
- **21:55–22:07** — RBN shows `OFFLINE` with zero libp2p connections from devices

### RBN Status After Restart
- **Service:** Active (running), PID 300827
- **Identity key:** Persisted at `/opt/introvert/data/identity.key` (MD5: `8fb215716846f074801b285e3193a9fd`)
- **Peer ID:** `12D3KooWJqiNgP67shH4m1usQtMPQyCqwCWQrnHx6bgmkGNmhz8a` ✅ (matches config)
- **Listeners:** TCP 443 (IPv4+IPv6), UDP 443 (QUIC), TCP 80 (WS tunnel), TCP 8080 (dashboard) ✅
- **External IP:** `47.89.252.80` ✅
- **Port accessibility:** Ports 443 and 80 reachable from outside ✅
- **Firewall:** iptables policy ACCEPT on all chains ✅
- **CPU usage:** 0% — event loop idle
- **Threads:** 10 (1 tokio-rt-worker in R state, rest sleeping)

### What the RBN sees
- `Peer Discovered` events from Kademlia DHT — swarm is working
- `Local Node Status: ONLINE (Listening)` — listener active
- `Connection Status Change: OFFLINE` — `swarm.connected_peers().count() == 0`
- **ZERO** `ConnectionEstablished`, `Identify`, `ReservationReq`, or `IncomingConnectionError` events

### What was observed before restart
- TCP connections from `86.99.227.92` (UAE ISP — likely Android VPN exit) in ESTABLISHED state
- RBN had 6 ESTABLISHED TCP connections on port 443 from this IP
- But libp2p layer saw ZERO connected peers
- **This means the noise handshake is failing silently after TCP connect**

### Key finding
The RBN's `ConnectionEstablished` handler logs at `debug!` level (not `info!`), so successful libp2p connections wouldn't appear in INFO-level logs. However, `IncomingConnectionError` logs at `warn!` level and shows nothing — meaning no connection errors are being reported either. The TCP connections establish but the libp2p noise/yamux upgrade never completes or never fires.

---

## 3. Potential IntroClaw Regression (Introduced This Session)

### Bug: Bootstrap re-dial disabled when IntroClaw is active

When IntroClaw is active, the resilience ladder's Step 2-4 (bootstrap re-dial, tunnel activation) is **completely skipped**:

```rust
// src/network/mod.rs ~line 569
if self.intro_claw.is_active() {
    // Skip steps 2-4; IntroClaw's tick will handle escalation
} else {
    // Original re-dial logic runs here
}
```

But IntroClaw's `ConnectionStateCycler` only evaluates on the **5-minute tick interval**, not the 15-second status-check interval. This means:
- **Before IntroClaw active:** Devices re-dial bootstrap every 15 seconds
- **After IntroClaw active:** Devices wait up to 5 minutes between connection attempts

**This is likely the cause of "all devices stuck on connecting"** — the code change this session broke the fast reconnection loop.

### Files changed this session
1. `src/intro_claw.rs` — Added `ConnectionStateCycler`, `ConnectionStrategy`, `VpnConfig`, updated `ClawActions` and `ClawTickContext`
2. `src/network/mod.rs` — Updated `execute_claw_actions()`, `ClawTickContext` construction, resilience ladder deferral
3. `src/network/service.rs` — Added `consecutive_zero_peers_ticks` field
4. `lib/src/ui/main_shell.dart` — Added 3-minute notification rate limiter
5. `lib/connectivity_listener.dart` — Removed duplicate snackbar notifications

---

## 4. RBN Server Issues

### Missing APNs Key
```
[Push] ❌ APNs key file not found at: /opt/introvert/config/apns-key.p8
[Push] ⚠️ No APNs config — iOS push disabled
```
- iOS push notifications will not work
- Firebase (Android) push is working: `[Push] ✅ Firebase service account loaded`

### Solana Registration Failed
```
[SolanaRegistry] On-chain registration FAILED: AccountNotFound
"Attempt to debit an account but found no record of a prior credit."
```
- Operator wallet `EHpjT1G4xPnZh5jsRqcUGuh15ZswUoUHYsj4o4qnvUGg` needs SOL on devnet
- Non-critical for connectivity — only affects on-chain RBN registry

### RBN Binary Version
- Compiled with `libp2p = "0.56.0"` — matches client version ✅
- Internal versions: `libp2p-noise-0.46.1`, `libp2p-core-0.43.2`, `yamux-0.12.1/0.13.10`
- Binary last deployed: Jun 23, 2026

---

## 5. Questions for Expert

### Priority 1: Why is the noise handshake failing?
- TCP connections arrive at the RBN but `ConnectionEstablished` never fires
- No `IncomingConnectionError` logged either
- The connections hang in ESTABLISHED state at TCP level but libp2p doesn't upgrade them
- Could this be a yamux version mismatch between the deployed RBN binary (older) and the client code (newer)?

### Priority 2: Is the IntroClaw deferral the root cause?
- Should the resilience ladder always run the bootstrap re-dial on the 15s interval regardless of IntroClaw state?
- Or should the ConnectionStateCycler also evaluate on the 15s interval?

### Priority 3: VPN circuit establishment
- Why does `ReservationReqAccepted` succeed but `OutboundCircuitEstablished` fail behind VPN?
- Is the WSS tunnel (TCP→WebSocket proxy) dropping the circuit upgrade frames?
- Should we try QUIC-only behind VPN instead of the TCP/WSS tunnel?

### Priority 4: RBN event loop
- RBN shows 0% CPU with ESTABLISHED connections — is the tokio event loop deadlocked?
- The `voluntary_ctxt_switches: 25` is very low for a process running for several minutes
- Could the RBN's swarm.poll() be stuck?

---

## 6. Device Peer IDs
| Device | Peer ID |
|--------|---------|
| Android | `12D3KooWQM5mi5VV23k3APgXfafBpbiiG9QJEmXfmdLtipMdxECd` |
| Mac | `12D3KooWCSejiZ1V5UDg6tkFu7g1rHjYf1LnzMiThywrMAFNtYvf` |
| iOS | `12D3KooWN6Hu1AZwZyRhaFiWrxA8fkpDnpc32Mtgm1SyRDUyajWP` |

## 7. RBN Server Access
- **SSH:** `ssh root@47.89.252.80`
- **Dashboard:** `http://47.89.252.80:8080` (requires session token)
- **Service:** `systemctl status introvertd`
- **Logs:** `journalctl -u introvertd -f`
- **Data dir:** `/opt/introvert/data/`
- **Config dir:** `/opt/introvert/config/`

---

## 8. Resolutions (Implemented 2026-07-06)

### Issue: ALL Devices Stuck on "Connecting"
- **Cause**: The IntroClaw connection state cycler (`ConnectionStateCycler`) was only being evaluated on the 5-minute tick loop. When a client disconnected, it would remain in `CONNECTING` status for up to 5 minutes before trying alternate paths (Direct, WebTunnel, VPN configurations).
- **Resolution**: Exposed `evaluate_connection_strategy` on `IntroClawService` and called it directly on the 15-second status loop. Disconnected client nodes now recover snappily within 15–30 seconds.

### Issue: Client Declares Telemetry but RBN Receives No Data
- **Cause**: Metric count mismatch (client transitioned to 13 metrics while network envelope only carried 9). This resulted in deserialization failures over the wire. Telemetry was also volatile on the RBN (lost on restarts) and lacked cryptographic validation.
- **Resolution**:
  1. Aligned the `SignalingPayload::TelemetryEnvelope` to carry the 13 metrics, Ed25519 signature, Solana wallet, and Solana ATA.
  2. Modified the RBN backend to store raw telemetry envelopes inside the encrypted SQLite database (`client_telemetry` table) to survive daemon restarts.
  3. Implemented cryptographic signature validation of telemetry envelopes at the RBN libp2p entry point.
  4. Added a midnight scheduler UTC cron task to automatically close the epoch, verify the data, and send claims using HMAC-SHA256 signatures to the Solana daemon on port 9001.
  5. Successfully compiled and deployed updated native libraries to all three clients (macOS, Android, iOS) and the RBN production node. Verified receipt of RBN confirmations on all devices.
