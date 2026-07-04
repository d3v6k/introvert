# Fix Plan: Relay Messaging & In-App Debug Logging
**Created:** 2026-07-01
**Priority:** P0
**Status:** IMPLEMENTED — awaiting device test

---

## Issue 1 — In-App Debug Log Capture (Prerequisite)

### Problem
When Android is on mobile data or VPN, there is no way to see Rust-level logs
without a USB-connected debugger. The `dispatch_debug_log` function (event 99)
already fires for some paths, but:
1. The logs are only printed to the Flutter debug console — they vanish on release builds.
2. There is no in-memory ring-buffer capturing them.
3. The existing "Save Log" button in Settings only saves the IntroClaw activity log,
   NOT the Rust network debug output (event 99).

### Fix
**In `introvert_client.dart`:**
- Maintain a rolling in-memory ring buffer of the last 500 event-99 (Rust debug) messages.
- Expose `getDebugLogs()` to retrieve the buffer as a formatted string.

**In `main_shell.dart` (Settings panel):**
- Add a "Save Network Log" button that saves Rust debug events to the Downloads folder.

**In Rust (`src/network/mod.rs`):**
- Add `crate::dispatch_debug_log(...)` calls to ALL key relay events:
  - `ReservationReqAccepted`, `OutboundCircuitEstablished`, `InboundCircuitEstablished`
  - `dial_relay_path()` — log each strategy attempt and outcome
  - `forward_to_mesh()` — log path taken (WebRTC / Direct / Relay / Mailbox)

---

## Issue 2 — Messages Not Flowing Through Relay (P0)

### Root Cause Analysis

**Scenario (Mobile Data / VPN -> Mac):**
1. Android connects to RBN -> `ConnectionEstablished` fires OK
2. `Identify` fires -> relay reservation requested -> `ReservationReqAccepted` fires OK
   Status dispatched as Online (status=1) OK
3. Mac sends a message -> calls `forward_to_mesh(android_peer_id, ...)`
4. Mac checks `swarm.is_connected(android_peer_id)` -> FALSE (KEY ISSUE)
5. Mac calls `dial_relay_path(android_peer_id)` -> tries circuit via RBN
6. The circuit dial succeeds OR the message falls through to mailbox
7. If mailbox: Android needs to drain it on next connection to RBN

**Why the circuit dial may silently fail:**
- `dial_relay_path` breaks out of the loop on the FIRST successful `swarm.dial()` call
- `swarm.dial()` returning `Ok()` means the dial was QUEUED, NOT that the circuit is established
- If QUIC/UDP is blocked on the VPN, the circuit dial silently fails with no retry on TCP

**Why the mailbox fetch may not help:**
- `perform_mailbox_fetch()` only fetches if `swarm.is_connected(anchor_id)`
- If Android cant connect to the RBN anchor via the circuit, it never drains

### Fixes to Implement

#### Fix A: Force All Transport Strategies in `dial_relay_path` (Don't Break Early)
Currently breaks on the first `is_ok()` RBN entry.
Change: try ALL bootstrap node addresses (TCP + QUIC) without breaking early.
This ensures TCP port 80 is tried even if QUIC/UDP is blocked.

#### Fix B: Mailbox Drain on ReservationReqAccepted
After relay reservation is accepted, Android immediately triggers mailbox drain.
This delivers any messages that were queued before the reservation.

#### Fix C: Flush Pending Messages After Relay Reservation
After `ReservationReqAccepted`, flush all pending messages for peers that are
connected via this relay. Currently pending messages are only flushed on `Identify`.

#### Fix D: Add Relay Debug Events for All Key State Transitions
Add `dispatch_debug_log` to every relay-related event so they appear in the in-app log.

---

## Implementation Order

| Step | Change | File | Impact |
|:-----|:-------|:-----|:-------|
| 1 | Rust debug ring buffer in Flutter | `introvert_client.dart` | Low risk |
| 2 | Save Network Log button in Settings | `main_shell.dart` | Low risk |
| 3 | Add dispatch_debug_log to relay events | `src/network/mod.rs` | Low risk |
| 4 | Fix dial_relay_path to try ALL transports | `src/network/mod.rs` | Medium risk |
| 5 | Mailbox drain on ReservationReqAccepted | `src/network/mod.rs` | Medium risk |
| 6 | Flush pending on relay reservation | `src/network/mod.rs` | Medium risk |

---

## Build & Deploy Requirements

After Rust changes (Steps 3-6):
- `make android` -> rebuild `libintrovert.so`
- `flutter run -d <device>` -> install updated APK
- `./deploy_rbn.sh` -> NOT needed (client-only changes)

---

## Acceptance Criteria

- [ ] Rust relay events visible in in-app log
- [ ] "Save Network Log" button saves Rust debug events to file
- [ ] Android on mobile data: messages delivered within 30s via mailbox
- [ ] Android on VPN: messages delivered via relay circuit or mailbox
- [ ] InboundCircuitEstablished and OutboundCircuitEstablished events visible in logs
