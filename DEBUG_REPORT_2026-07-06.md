# Introvert Network Debug Report
**Date:** 2026-07-06
**Devices:** Android (VPN), Mac (no VPN), iOS (no VPN)

---

## 1. Original Problem: Android on VPN Can't Connect

### Symptom
Android device behind VPN establishes TCP connection to RBN, gets `ReservationReqAccepted`, but never reaches `Status=1` (ONLINE). Never establishes relay circuits. Never receives group messages.

### Comparison with working devices

| Device | VPN | TCP Connect | Reservation | Status=1 | Circuits | Messages |
|--------|-----|-------------|-------------|----------|----------|----------|
| Android | Yes (WSS tunnel) | ✅ | ✅ | ❌ | ❌ | ❌ |
| Mac | No | ✅ | ✅ | ✅ | ✅ | ✅ |
| iOS | No | ✅ | ✅ | ✅ | ✅ | ✅ |

### Root cause hypothesis
The VPN's WebSocket proxy handles the initial HTTPS handshake (reservation works) but drops or stalls the longer-lived stream needed for circuit relay.

---

## 2. Critical Issue: Devices Stuck on "Connecting" After RBN Restart

### What was observed
- RBN restarted, devices lost connection
- Devices remained in CONNECTING status for extended period
- Connection state cycler was only evaluating on 5-minute intervals

### Resolution
- Updated connection state evaluation to run on 15-second status loop
- Devices now recover within 15-30 seconds

---

## 3. Telemetry Pipeline Issue

### Problem
Client telemetry had metric count mismatch (9 vs 13 metrics), causing deserialization failures. Telemetry was volatile (lost on restarts) and lacked cryptographic validation.

### Resolution
1. Aligned telemetry schema to 13 metrics with cryptographic signatures
2. Added persistent SQLite storage for telemetry data
3. Implemented signature validation at RBN entry point
4. Added midnight UTC scheduler for automated epoch processing
5. Deployed updated libraries to all clients and RBN

---

## 4. Resolutions Implemented

### Connection Recovery
- Connection state cycler now evaluates on 15-second intervals
- Disconnected clients recover within 15-30 seconds

### Telemetry Pipeline
- Unified 13-metrics schema with Ed25519 signatures
- Persistent SQLite storage survives daemon restarts
- Automated epoch closing and payout distribution
- Successfully verified on all devices
