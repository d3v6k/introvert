# Release Notes v54 — VPN Resilience, RBN Blacklisting & Active Session Hardening

**Release Date:** 2026-07-04
**Status:** Stable / Production-ready

---

## Executive Summary

Major networking and self-healing release implementing **VPN Adaptive Pathing**, **Intelligent RBN Blacklisting**, and **Active Chat Session Prioritization** in the Intro-Claw engine. These changes resolve long-standing connection stale loops when devices are on VPN or mobile networks and drastically improve message delivery speeds during active chat sessions.

---

## Key Features

### 1. VPN Adaptive Pathing & Tunneling
* **VPN Connection Detection**: Mapped `ConnectivityResult.vpn` in `connectivity_listener.dart` to connection type `5` and automatically triggers a Scaffold SnackBar notification.
* **VPN Bootstrap Node Isolation**: Intercepts connectivity type `5` (VPN) in `SetConnectivityType` to clear stale reservations/blacklists and immediately trigger the loopback WebSocket tunnel. The active bootstrap nodes list is isolated to **only** the local tunnel loopback address (`127.0.0.1`) while on VPN, preventing dead public IP dials from clogging the queue.
* **Solana Registry Bypass for Mainnet**: Bypasses the on-chain Solana registry queries entirely. The network core now defaults strictly to the hardcoded production RBN node (`47.89.252.80`), ensuring complete compatibility with the Mainnet environment.
* **Queue Congestion Prevention**: Removed redundant carpet-bombing dial loops from `forward_to_mesh`'s fallback block. Outgoing RBN and anchor connection requests are handled solely by `dial_relay_path` and background resilience ticks, eliminating `PendingOutgoing` queue exhaustion.
* **Thinkpad Local Node Removal**: Completely removed the hardcoded test RBN `192.168.1.81` from production configuration (`src/network/config.rs`) to prevent dead routing loops.
* **Chat Screen Offline Sync**: Updated the chat screen `networkStream` listener for Event 10 to force the status to "Offline" and deactivate E2EE immediately when the local node goes offline, preventing false "online" state displays during VPN disruptions.

### 2. Intelligent RBN Blacklisting & Cooldown Reset
* **Back-off Blacklist**: Introduced `rbn_blacklist` to track failed bootstrap node connections.
* **Dynamic Cooldown**: Blacklists unreachable RBNs with exponential cooldown windows (2 min → 10 min → 1 hour).
* **Self-Healing Routing**: The Step 2 resilience and fast-reconnect loops skip blacklisted RBNs, allowing Intro-Claw to route traffic through operational backbones instead of blocking connection threads.
* **Blacklist Cooldown Reset**: Clear the entire `rbn_blacklist` when a network transition occurs, a manual refresh is triggered, or the WebSocket tunnel is activated. Connected peers are also immediately removed from the blacklist upon successful `ConnectionEstablished`. This prevents stale, carryover failures from blocking dial attempts on the new network interface.

### 3. Active Chat Session Prioritization
* **Chat Context Aware**: Flutter UI notifies the native layer using `setActiveChat` / `clearActiveChat` / `setActiveGroupMembers` when entering or exiting 1:1 or group chats.
* **Aggressive Upgrades**: Cooldown intervals are bypassed for the active chat partner. Direct DCUtR hole-punching attempts are triggered immediately if the partner is currently relayed.
* **Proactive Connection Healing**: 
  - If the active 1:1 partner is offline, Intro-Claw prioritizes healing their connection proactively on every tick cycle.
  - If the active chat is a group, Intro-Claw proactively heals up to 3 offline group members to establish robust message delivery paths.

### 4. App Launch Warm-Up
* **Immediate Optimizations**: Added `onAppLaunch()` to run a full warm-up connection pass, dial top contacts, and refresh the mesh immediately on startup.

---

## Compilation Status

| Target | Status | Notes |
|--------|--------|-------|
| `make mac` | ✅ Compiles Clean | macOS dylib generated and copied |
| `make android` | ✅ Compiles Clean | arm64 and x86_64 so libraries generated |
| `cargo check` | ✅ Passed | 0 errors |

---

## Files Modified

| File | Changes |
|------|---------|
| `src/network/config.rs` | Hardcoded Thinkpad LAN IP bootstrap node removed |
| `src/network/service.rs` | Added `rbn_blacklist` map definition |
| `src/network/mod.rs` | Added FFI command cases; RBN blacklist tracking in OutgoingConnectionError; Step 2 & fast-reconnect blacklist-aware routing |
| `src/network/types.rs` | Added active chat, group members, and app launch command variants |
| `src/intro_claw.rs` | Added active chat tracking fields; implemented aggressive direct upgrade bypass; implemented proactive chat/group connection healing |
| `src/lib.rs` | Exported FFI functions `intro_claw_set_active_chat`, `intro_claw_clear_active_chat`, `intro_claw_set_active_group_members`, `intro_claw_on_app_launch` |
| `lib/src/native/introvert_client.dart` | Added VPN mapping (type 5); declared new FFI pointers and safeLookup bindings; exported Dart wrappers |
| `lib/main.dart` | Imported connectivity listener; called `onAppLaunch()` |
| `lib/connectivity_listener.dart` | Restored direct widget creation; implemented SnackBar notifications and self-healing for `vpn` and `none` connection results |
| `lib/views/chat_screen.dart` | Set/clear active chat context on entering/exiting |
| `lib/views/group_chat_screen.dart` | Set/clear active chat and group member contexts on entering/exiting |

---

**Status: Production-ready for stable v54 release**
