# Changelog

All notable changes to Introvert will be documented in this file.

## [0.29.0] - 2026-07-06 — Recovery, Telemetry Pipeline, Anti-Gaming IQR, Drive Rebuild, VPN Fix & Notification Hardening

### Milestone
**Full codebase recovery from GitHub source, telemetry pipeline for RBN reward tracking, IQR outlier mitigation for anti-gaming, 9-field shared metrics bridge, networking relay fixes for cross-network/VPN messaging, comprehensive backup system, drive folder manager rebuild, VPN tunnel fix, and notification hardening.**

### Added
- **Telemetry Pipeline**: 30-minute Tokio interval in `src/network/mod.rs` packages activity metrics via `RewardTracker::package_telemetry()` and pushes `SignalingPayload::TelemetryEnvelope` to connected RBNs with 5-minute cooldown guard.
- **9-Field Shared Metrics Bridge**: `Arc<RwLock<[u64; 9]>>` connecting `DailyRewardEngine` → `RewardTracker` → network telemetry. Activity counts (MsgSent, MsgRcvd, GrpMsg, GrpReact, FileSend, FileRecv, CallSecs, RelayBytes, Uptime) flow in real-time from the reward engine to the telemetry pipeline.
- **TelemetryEnvelope & TelemetryAck**: New `SignalingPayload` variants in `src/network/types.rs` for client↔RBN telemetry exchange.
- **IQR Anti-Gaming Filter**: `close_current_epoch()` in `for_linux/src/economy/daily_rewards.rs` implements Interquartile Range outlier mitigation. Collects all edge scores, computes Q1/Q3/IQR, clamps outliers to `Q3 + 1.5*IQR`, distributes rewards proportionally from 16,438 INTR daily pool.
- **IQR Unit Test**: `test_iqr_outlier_mitigation_and_batch_distribution` validates the filter with mock epoch data.
- **Anchor Relay Strategy**: Added connected anchor node relay as Strategy 3 in `dial_relay_path()`, between RBN relay and WebSocket tunnel fallback.
- **Undelivered Message Retry**: Periodic check in status check interval re-sends messages stuck at status=0 for >60s to connected recipients.
- **Economy Daemon Restored**: `introvert-daemon/` with `introvert-p2p` and `introvert-solana` crates copied from v54 backup.
- **Comprehensive Backup System**: `scripts/backup.sh` with `dd_mm_yy_time` naming, pre-backup doc updates, completeness verification, and recovery manifest generation.
- **Drive Folder Manager Rebuild**: Complete rebuild of `lib/src/ui/drive_tab.dart` (612 lines) with folder grouping (group name or contact name), minimized folder view, "Introvert Explained" pinned at top with 4 guide images, list/grid view toggle, file sizes, multi-select with batch operations (move, delete, share), breadcrumb navigation, storage usage bar, and file search.
- **Drive Folder Storage Layer**: Added `folder` column to `drive_files` table, `drive_folders` table, `upsert_drive_file_with_folder()`, `update_drive_file_folder()`, `get_group_name()`, `get_contact_alias()`, `get_drive_folders()` methods in `src/storage.rs`.
- **Drive Folder FFI**: Added `introvert_drive_add_file_with_folder()` and `introvert_drive_update_folder()` FFI functions in `src/lib.rs` with Dart bindings.
- **VPN Tunnel Port 80 Fallback**: Added `RBN_WS_URL_PLAIN` constant (`ws://47.89.252.80:80/tunnel`) in `src/network/types.rs`. When VPN detected (type 5), tunnel uses plaintext WebSocket on port 80 instead of TLS on port 443 (which many VPNs block).
- **VPN Bootstrap Isolation**: When VPN detected and tunnel activates, bootstrap list is isolated to tunnel loopback only (prevents failed direct dials to public IPs).

### Fixed
- **VPN Relay Regression**: Removed destructive `relay_reservations.clear()` on `SetConnectivityType` network transition that wiped all relay state and made devices unreachable behind VPN.
- **ListenerClosed Multiaddr**: Fixed relay re-reservation to use full multiaddr from `bootstrap_nodes` instead of relative `/p2p/X/p2p-circuit` that caused `MissingRelayAddr` errors.
- **In-flight Limits**: Reverted from relay=8/direct=12 to relay=4/direct=8 (v54 baseline) to prevent relay circuit saturation on VPN connections.
- **Android libc++_shared.so**: Updated `build_android.sh` to bundle `libc++_shared.so` from NDK alongside `libintrovert.so`, fixing `dlopen failed: library "libc++_shared.so" not found` crash.
- **FFI Consistency**: Added `introvert_storage_update_group_message_status_by_id` to `src/lib.rs`, resolving 1 missing FFI symbol between Dart and Rust.
- **Backup Script**: Fixed naming from `mm_yy_time` to `dd_mm_yy_time`, added `for_linux/`, `introvert-daemon/`, `plugins/`, root-level files, compiled binaries, and completeness verification.
- **Notification Spam (Native Android)**: Added 3-minute cooldown to `IntrovertFirebaseMessagingService.kt`. Added foreground detection — skips native notification when app is open. Added sound and vibration to notification channel.
- **Notification Spam (Dart)**: Added 3-minute cooldown and foreground suppression to `AlertService.showAlert()`. Sound plays for messages, group invites, and calls when app is open; native notification only when backgrounded.

### Changed
- **`RewardTracker::new()`** now requires `shared_metrics: Arc<RwLock<[u64; 9]>>` parameter.
- **`DailyRewardEngine::new()`** now requires `shared_metrics: Arc<parking_lot::RwLock<[u64; 9]>>` parameter.
- **`Makefile`**: Added `bk` target for comprehensive backup, fixed `build_android.sh` path.
- **`DriveFileMetadata`** now includes `folder: String` field.

### Infrastructure
- **RBN Deployed**: `introvertd` compiled on thinkpad.local, deployed to Alibaba RBN (47.89.252.80:443). Active and serving.
- **Economy Daemon**: `introvert-solana` running on localhost:9001 on RBN server.
- **Backup**: `06_07_26_0830` — 899 files, 638MB, complete snapshot with recovery manifest.

---

## [0.28.0] - 2026-07-04 — VPN Resilience, RBN Blacklisting & Active Session Hardening

### Milestone
**VPN connectivity hardening, intelligent RBN blacklisting with exponential cooldown, active chat session prioritization (aggressive upgrades + healing), and app launch warm-up.**

### Added
- **VPN Connection Detection & Bootstrap Isolation**: Mapped `ConnectivityResult.vpn` in `connectivity_listener.dart` to connection type `5` and automatically triggers a Scaffold SnackBar notification. The native layer now properly intercepts type `5` (VPN) in `SetConnectivityType` to clear stale blacklists/reservations and immediately trigger/reset the loopback WebSocket tunnel. The active bootstrap nodes list is isolated to **only** the local tunnel loopback address (`127.0.0.1`) while on VPN, preventing dead public IP dials from clogging the queue.
- **Chat Screen Offline Sync**: Updated the chat screen `networkStream` listener for Event 10 to force the status to "Offline" and deactivate E2EE immediately when the local node goes offline, preventing false "online" state displays during VPN disruptions.
- **Solana Registry Bypass for Mainnet**: Disabled dynamic on-chain RBN registry lookups entirely. The network core now defaults strictly to the hardcoded production RBN node (`47.89.252.80`), ensuring compatibility with the Mainnet deployment.
- **Queue Congestion Prevention**: Removed redundant carpet-bombing dial loops from `forward_to_mesh`'s fallback block. Outgoing RBN and anchor connection requests are handled solely by `dial_relay_path` and background resilience ticks, eliminating `PendingOutgoing` queue exhaustion.
- **Intelligent RBN Blacklisting & Cooldown Reset**: Tracks failed bootstrap connections and applies exponential cooldowns (2 min → 10 min → 1 hour). The blacklist is cleared on network switches, manual refreshes, and tunnel activations, and connected peers are immediately removed from the blacklist.
- **Active Chat Prioritization**: Flutter UI dynamically propagates chat session state (`setActiveChat` / `setActiveGroupMembers`); Rust engine bypasses cooldowns, aggressively punches direct holes (DCUtR) for relayed partners, and proactively heals offline targets on every tick.
- **App Launch Warm-Up**: `onAppLaunch()` executes initial warm-up connection pass.

### Removed
- **Thinkpad Local Node**: Cleaned Thinkpad RBN (`192.168.1.81`) from default configuration (`src/network/config.rs`).

---

## [0.27.0] - 2026-07-04 — VPN Resilience, Kliphy GIFs & UI Fixes

### Milestone
**VPN connectivity hardening, Kliphy GIF integration with attribution, group chat reactions restored, and external messenger WebView layout/scroll fixes.**

### Added
- **Kliphy GIF API integration** — Default test API key configured, "Powered by KLIPY" attribution logo displayed in GIF picker tab
- **VPN tunnel stale detection** — Force-resets WebSocket tunnel after 60s with 0 peers (resilience loop) and 30s (fast reconnect)
- **Tunnel lifecycle tracking** — `tunnel_started_at` field records activation time for stale connection detection
- **Messenger login detection** — JavaScript handler checks if user is already logged in, skips setup guide for activated accounts

### Fixed
- **Group chat reactions** — `group_chat_screen.dart` was populating `_reactionsCache` with empty lists instead of calling `_client.getMessageReactions(msgId)`
- **Messenger WebView gap** — Removed top padding that created visible gap between webview and Introvert header
- **Messenger WebView bottom overflow** — Added bottom padding to account for floating navigation bar, preventing "Got it" button from being blocked
- **Messenger vertical scroll** — Added `overScrollMode`, `nestedScrollEnabled`, and CSS injection for WhatsApp/Telegram chat message scrolling
- **Messenger setup guide flash** — Shows loading state while checking login, 5s timeout before showing setup guide
- **VPN tunnel stuck state** — When tunnel is active but VPN blocks WebSocket, code now detects stale tunnel and force-resets with re-activation

### Changed
- **Tunnel activation error handling** — Resets `tunnel_active` and `tunnel_started_at` on failure for proper retry
- **VPN diagnostic logging** — Added `[VPN]` prefixed logs for tunnel activation, stale detection, and force-reset events

---

## [0.26.0] - 2026-07-03 — DynamicPromoStack & Customizable Campaign Layer

### Milestone
**DynamicPromoStack integrated into production daemon. Open-ended campaign management system allows runtime promotion adjustments without code rebuilds. Strategic Reserve ceiling enforced with automatic deduction and referral pool compression.**

### Added
- **DynamicPromoStack** — Runtime campaign registry with HashMap-based open/close/adjust operations
- **ActiveCampaign struct** — campaign_id, PromoType, daily_payout_allocation, expiration_epoch
- **PromoType enum** — CommunityThemeVote, EarlyAdopterBonus, DeveloperHackathonYield, DynamicBonusCampaign
- **compute_epoch_promo_distribution()** — Calculates adaptive referral pool after promo deductions
- **Auto-eviction** — Expired campaigns automatically removed at epoch close
- **Safety cap** — Promo deductions cannot exceed Strategic Reserve ceiling (3,287.60 INTR/day Year 1)
- **RbnDailyRewardEngine integration** — open_promo_campaign(), close_promo_campaign(), get_promo_distribution()

### Architecture
```
[Strategic Reserve Daily Ceiling: 3,287.60 INTR]
                    │
                    ├──► [- Minus] Active Campaigns (e.g., Theme: 1,000 INTR)
                    │
                    └──► [= Equals] Referral Pool (2,287.60 INTR)
```

### Deployment
- **Alibaba RBN** — introvert-p2p deployed with DynamicPromoStack active
- **Runtime management** — Campaign adjustments via admin API without rebuilds

---

## [0.25.0] - 2026-07-03 — Economy Chain Audit & TelemetryEnvelope Implementation

### Milestone
**Full economy chain audit completed. 8 critical blockers identified and resolved. TelemetryEnvelope with Ed25519 signing implemented across all codebases. System deployed to Alibaba RBN production.**

### Audit Findings & Fixes
- **Token mint unified** — All references changed to `EAXT8h2qTtS5RPfAPX3qpbn6b99bqKfNwLKyqZp9ZZPf`
- **Balance gate removed** — No minimum stake required, merit-based rewards only
- **Ed25519 signature verification** — Replaced stub `return true` with real cryptographic verification
- **TelemetryEnvelope struct** — 13 metrics array, wallet addresses, proof hash, client signature
- **Double-claim guard** — HashSet tracking `[epoch_id:peer_id]` pairs
- **Dynamic reward calculation** — Proportional pool-clearing formula, no hardcoded amounts

### Added
- **TelemetryEnvelope** — Signed telemetry packet with solana_wallet, solana_ata, metrics[13], proof_hash
- **package_telemetry()** — Client-side method to package metrics into signed envelope
- **send_telemetry_to_rbn()** — TCP send to RBN daemon
- **Ed25519 verification** — Real signature verification in verify_signature()
- **SHA-256 proof hash** — For relay bytes verification
- **All 13 record_*() methods** — message_sent, message_received, group_message_sent, etc.

### Security
- **Real Ed25519 verification** — No longer accepts unsigned packets
- **Proof hash validation** — SHA-256 of relay bytes prevents spoofing
- **Double-claim guard** — Prevents replay attacks across RBNs
- **Merit-based rewards** — No balance gate, pure contribution-based

### Deployment
- **Alibaba RBN (47.89.252.80)** — Both services active
- **Treasury** — `DZWeLhjPeH3q4Z45HyTh5BbWXiuXdHKK7od4yR9wGLQm` (0.098 SOL)
- **Treasury ATA** — `HobcUEUBHXfwRW1DWv1XaZkAqiMeghN14utUGXuFPauR`

---

## [0.24.0] - 2026-07-03 — Economy Daemon Production Deployment

### Milestone
**Production economy daemon deployed for automated staking validation and SPL token payouts.** This is a separate codebase (`introvert-daemon`) from the main RBN code (`introvert`), with dual-process architecture isolating libp2p networking from Solana SDK.

### Project Separation
- **introvert** (this repo) = RBN code — Flutter app, Rust networking core, relay daemon
- **introvert-daemon** (separate repo) = Economy code — Solana staking, token payouts, treasury management

### Added (introvert-daemon — Economy)
- **introvert-p2p daemon** — libp2p swarm with Kademlia DHT, noise encryption, yamux multiplexing. Fires JSON events on peer connection.
- **introvert-solana daemon** — Solana Mainnet RPC client, ATA derivation, staking validation, `transfer_checked` payout execution.
- **introvert-keygen utility** — Generates Ed25519 keypair for treasury authority with secure file permissions.
- **TCP loopback IPC bridge** — Port 9001 (127.0.0.1 only) for secure inter-process communication.
- **Launchd service configs** — Auto-restart on crash/reboot for Mac Mini deployment.
- **Systemd service configs** — Auto-restart on crash/reboot for Alibaba RBN deployment.
- **Treasury keypair management** — Secure storage with chmod 600, fail-fast startup if missing.

### Security
- **IPC isolation** — Port 9001 bound to localhost only, no external access.
- **Keypair protection** — chmod 700 on config directory, chmod 600 on treasury file.
- **.gitignore updates** — Excludes *.json, *.env, *.pem, *.key to prevent accidental key commits.
- **transfer_checked** — Explicit mint/decimal verification prevents fat-finger drains.

### Deployment (introvert-daemon)
- **Mac Mini** — Release binaries compiled, Launchd services active.
- **Alibaba RBN** — Cross-compiled via ThinkPad, Systemd services active at 47.89.252.80.
- **Treasury wallets** — Mac: GNNEC8q9urd6rBLeNrgGLME17T7winqqEes36cMh6wu8, Alibaba: DZWeLhjPeH3q4Z45HyTh5BbWXiuXdHKK7od4yR9wGLQm.

---

## [0.23.0] - 2026-07-03 — v53 "Beta Stability & Push Fixes"

### Milestone
**Beta stability fixes applied.** FCM push restored, connection limits fixed, cross-network relay hardened, Messenger WebView login fixed.

### Fixes
- **FCM push** — `message_type` → `msg_type` (reserved FCM key)
- **Connection limit** — 102 → 204 (`max_connections / 5`)
- **Group ACK** — `>= 1` instead of `>= total_members`
- **Messenger WebView** — login CSS fix, scroll fix, layout fix
- **Firebase key** — regenerated and deployed
- **CAMERA permission** — added to AndroidManifest

## [0.22.0] - 2026-07-02 — v52 "Adaptive Networking"

### Milestone
**All NETWORKING_STABILIZATION_PLAN phases 1–4 implemented.** Adaptive pipeline depth, DCUtR remote peer upgrades, mobile data awareness, VoIP-aware transfer throttling, relay-aware cross-network routing, group gossip optimization, and Sovereign Swarm seeding hardening.
- **Step 1 reconnect ladder** — Dials disconnected RBNs before requesting reservations.
- **Group ACK** — Changed from `>= total_members` to `>= 1`.
- **Messenger WebView** — Login page CSS fix, scroll interference removed.
- **Firebase key** — Regenerated and deployed to production RBN.

### Added
- **`should_attempt_dcutr()`** in `ConnectionOptimizer` — enables relay→direct DCUtR upgrades for remote peers with health score > 0.5 (intro_claw.rs:290)
- **`get_optimal_pipeline_depth()`** on `AdaptiveChunkSizer` — reads throughput sliding window to dynamically tune pipeline (4/8/16 chunks). Wired into manifest arrival, relay transition, and watchdog retry.
- **`is_mobile_data` + `network_type`** fields on `ClawTickContext` and `NetworkCommand::IntroClawTick` — propagates cellular state from Flutter to Rust engine.
- **`is_on_mobile_data()`** proxy on `IntroClawService` — exposes mobile state to network module.
- **IPv6 listeners** — `/ip6/::/tcp/{port}` and `/ip6/::/udp/{port}/quic-v1` for NAT64/mobile data reachability.
- **Proactive relay reservation on startup** — all devices request relay reservations immediately during bootstrap.
- **Progressive reconnect ladder** — 4-step escalation in status_check_interval: reservation → redial → tunnel → offline.
- **Relay hint in `FileChunkRequest`** — carries sender's RBN PeerId for optimized chunk routing.
- **`is_mobile_data` FFI parameter** — `intro_claw_trigger_tick(is_mobile_data: bool)` accepts cellular state from Dart.
- **VoIP-aware transfer throttling** — pipeline collapses to 2 chunks and pacing inflates to 250ms during active voice/video calls to prevent media buffer contention.
- **Relay-aware file payload routing** — `relay_hints` map + `request_response.send_request()` bypasses `is_connected()` check for relay-connected peers. Eliminates 2-minute buffering delay for cross-network file transfers.
- **Relay hint on InboundCircuitEstablished** — populates `relay_hints[src_peer_id] = rbn_id` when receiver establishes inbound circuit through RBN.
- **Proactive relay dial in SendFileChunk** — calls `dial_relay_path(peer_id, true)` before `forward_to_mesh` to start circuit establishment early.
- **Fast reconnect interval** — 5-second interval activates when transfers waiting + no relay listener. Self-healing: deactivates once relay establishes.

### Changed
- **RBN auth relaxation** — `is_bootstrap` now grants blanket access for FileChunkRequest (was restricted to group transfers only). Non-RBN peers must pass group-member or contact verification.
- **ChatSyncResponse auth hardening** — authorization now enforced for relayed messages (was skipped when `is_relay=true`).
- **`BroadcastGroupMessage` optimization** — direct `ForwardMeshSignaling` now filtered to connected/mesh peers only. Offline peers rely on gossipsub + mailbox drain. Applied to both client and RBN daemon.
- **Sovereign Swarm seeding reorder** — `std::fs::write` executes BEFORE `RegisterSeeder` + `kademlia.start_providing()`. Eliminates race where chunk requests could arrive before file lands on disk.
- **Pacing delay** — now 1.5x on mobile data (75ms relay / 15ms direct vs 50ms/10ms normal).
- **Mailbox fetch** — skips every other tick on mobile data to reduce radio wakeups.
- **Status check interval** — 120s → 30s for faster VPN stale reservation recovery.
- **`disconnect_peer_id`** — wrapped with `let _ =` to suppress unused Result warning (client + RBN).

### Fixed
- **Stale FileTransferComplete guard** — checks `active_seeders.contains_key()` before processing. Prevents old mailbox-drained ACKs from marking inactive transfers as verified.
- **Cross-network file transfer delay** — relay-aware routing eliminates 2-minute buffering loop. Chunks flow through relay circuit within seconds of `InboundCircuitEstablished`.
- **Group ACK completion** — sender shows "verified" when ANY group member confirms receipt (`current_completions >= 1`), not when ALL members confirm. Prevents indefinite "waiting for recipient" when some members are offline.
- **Step 1 reconnect ladder** — now dials disconnected RBNs before requesting reservations. Previously only checked `is_connected(rbn_id)` which was always false when no relay existed.
- **OutboundCircuitEstablished flush** — delay increased from 500ms to 1500ms. Gives `is_connected()` time to update after relay dial completes.

## [0.21.4] - 2026-07-01 — v51 "Cross-Network Success & Sync Integrity"

### Milestone
**Cross-network file transfers verified working on VPN and mobile data.** This is the first stable release where file transfers work reliably across different networks, VPNs, and mobile data connections.

### Added
- **`cleared_chats` table**: Tracks when a chat was cleared with `peer_id` + `cleared_at` timestamp. Prevents mailbox drain from re-delivering old messages after chat clear.
- **`message_exists()`**: O(1) dedup check for mailbox-drained messages. Skips messages already in storage.
- **`should_skip_mailbox_message()`**: Compares message timestamp against clear timestamp; returns `true` if message predates the clear.
- **`cleanup_cleared_chats()`**: Prunes `cleared_chats` entries older than 7 days.
- **`ClearMailboxForPeer` command**: Triggers proactive mailbox drain immediately after clearing chat, so old messages get fetched and discarded before the next periodic cycle.
- **File transfer timestamps**: `FileTransferBubble` now displays HH:MM below status text, matching sticker/voice memo style.

### Fixed
- **Chat list sorting**: Changed from `MAX(id)` (insertion order) to `MAX(timestamp)` (chronological order) in `get_last_messages_all()` and `get_last_group_messages_all()`. Old sync'd messages no longer bubble to top of chat list.
- **Mailbox drain dedup**: Added three guards in `MailboxDrained` handler: (1) `message_exists()` check, (2) `[FILE]:` filter, (3) cleared-chat timestamp guard.
- **Create/Join Group dialogs**: Fixed by using parent widget context instead of invalidated bottom sheet context. Added `_showCreateGroupDialog()` and `_showJoinGroupDialog()` helper methods.
- **Mailbox re-delivery after chat clear**: `delete_chat` now records clear timestamp in `cleared_chats` table and clears local mailbox entries. `MailboxDrained` handler skips messages from before the clear.

### Networking Success
- **Cross-network file transfers**: Verified working on VPN and mobile data connections.
- **Relay reservation**: Three-tier fallback (bootstrap_nodes → anchor_mappings → filtered listen_addrs) prevents VPC private IP leakage.
- **VPN resilience**: Stale reservation detection at 30s intervals, force-clear and re-dial on VPN changes.
- **DCUtR hole-punching**: `InboundCircuitEstablished` triggers `swarm.dial()` for direct connection upgrade.

## [0.21.3] - 2026-07-01 — v50 "Delivery Fixes & System Hardening"

### Fixed (Gemini Session)
- **Relay reservation VPC leak**: `Identify` event handler was prioritizing `info.listen_addrs` (which included the RBN's private VPC IP `172.19.0.4`) over the public IP from `bootstrap_nodes`. Relay reservation dials to the private address failed silently until the 30s status check retry. Fixed with three-tier fallback: `bootstrap_nodes` (always public) → `anchor_mappings` (captured from direct connections) → filtered `listen_addrs` (private IPs excluded). Applied in both client and RBN daemon. Deployed to Alibaba RBN (`47.89.252.80`).
- **File transfer bubble receiver UI clarity**: The `SizedBox.shrink()` guard in `_buildThumbnailWidget()` (line 948) only suppresses the thumbnail preview for unverified incoming transfers — the transfer card with progress indicator, status text ("pulling from mesh"), filename, and cancel button is always rendered. This is correct behavior: show transfer controls but don't render unverified media content.

### Added
- **`pending_file_chunks` table**: Persistent SQLite queue for file chunks when no RBNs are connected. Prevents data loss on app restart. Uses UNIQUE(transfer_id, chunk_index) constraint with INSERT OR REPLACE for dedup.
- **`update_message_status_if_higher()`**: New storage function with monotonic status transition rules (0→3, 0→1, 0→2, 3→1, 3→2, 1→2). Prevents status downgrades. Integrated into all 6 ACK handlers.
- **`relay_hint: Option<String>`**: New field on `FileChunkRequest` with `#[serde(default)]` for backward compatibility. Sender populates with PeerId of their RBN. Receiver uses to prioritize RBN when dialing.
- **`sync_in_progress: HashMap<String, Instant>`**: Tracks active syncs per chat with 60s timeout cleanup. Prevents concurrent syncs and permanent lockout.
- **`relay_hints: HashMap<PeerId, PeerId>`**: Stores relay hints from FileChunkRequest to prioritize RBN when sending chunks back.
- **Stale reservation detection**: In `status_check_interval`, if RBN connections exist but NO relay reservation, force-clear and re-dial. Fixes VPN-induced stale reservations.
- **DCUtR on InboundCircuitEstablished**: Triggers `self.swarm.dial(src_peer_id)` for hole-punch attempt. If succeeds → direct connection; if fails → relay remains.

### Changed
- **`dial_relay_path` parameterized**: Added `for_file_chunk: bool` parameter. When true: iterate ALL RBNs without breaking, skip rate limiter. When false: keep single-RBN optimization. File chunks have no mailbox fallback and MUST succeed.
- **File chunk persistence**: When no RBNs connected, chunks persist directly to `pending_file_chunks` table (skip RAM). Flush on OutboundCircuitEstablished and periodic tick (30s). Chunks NOT removed after forwarding — deferred until FileTransferComplete arrives.
- **RBN sorting with relay_hint**: RBNs sorted with hinted RBN first (priority 0), then by latency. Reduces search space for relay dial.
- **sync_in_progress timeout**: Changed from `HashSet` to `HashMap<String, Instant>` with 60s periodic cleanup. Prevents permanent lockout on lost responses.
- **Status downgrade protection**: All ACK handlers now use `update_message_status_if_higher` instead of blind `update_message_status`.
- **for_linux relay reservation**: Added `is_rbn_or_anchor` check to only clear reservations when RBN/anchor disconnects (was clearing on ALL disconnects).
- **for_linux sender authorization**: Added security check to ChatSyncResponse handler (was missing).
- **DCUtR logging**: Demoted `InboundCircuitEstablished` from `info!` to `debug!` to reduce noise.

### Fixed
- **sync_in_progress permanent lockout**: Added `sync_in_progress.remove()` before early return on unauthorized ChatSyncResponse. Previously, unauthorized syncs permanently blocked that chat.
- **Data loss in DB chunk flush**: Deferred chunk removal until FileTransferComplete arrives. Previously, chunks were removed after `forward_to_mesh` returned Ok, but Ok doesn't mean delivery confirmed.
- **Dead code `update_message_status_if_higher`**: Integrated into all 6 ACK handlers (was created but never called).
- **Duplicate enum in for_linux**: Deleted dead code `types.rs` file (was never imported, maintenance hazard).
- **for_linux relay reservation bug**: Fixed unconditional `relay_reservations.remove()` on ALL ConnectionClosed events.

### Removed
- **for_linux/src/network/types.rs**: Dead code file (never imported, all definitions duplicated in mod.rs).

## [0.21.2] - 2026-07-01 — v49 "Cross-Network Delivery & Mailbox Integrity"

### Added
- **`MailboxStored` ACK**: Anchor nodes now confirm successful mailbox storage back to the sender via a new `SignalingPayload::MailboxStored` payload. Sender receives confirmation that the message is safely stored on the relay network.
- **Message Status 3 (In Mailbox)**: New intermediate status between Sent (0) and Delivered (1). Renders as a clock icon in the UI. Indicates the message has been stored on the anchor and is awaiting recipient delivery.
- **`verified_rbns` field**: New `HashSet<PeerId>` on `NetworkService` that tracks which peers are trusted for persistent mailbox storage. Populated from `bootstrap_nodes` (hardcoded) and designed for future Solana registry integration.
- **`store_message_if_new`**: New storage function using `INSERT OR IGNORE` — sync-safe insert that never overwrites existing messages. Prevents stale sync data from rolling back current messages.
- **`fetch_undelivered_messages(age_secs)`**: New storage function that retrieves sent messages stuck at status=0 older than a threshold, used for automatic retry.
- **Retry logic**: Periodic mailbox fetch now re-sends messages stuck at status=0 for >60 seconds to connected recipients.
- **Caption dialog thumbnails**: File send dialog now shows image/video thumbnails above the text input with Cancel/Send buttons.
- **DEBUG_DOCUMENT.md**: Comprehensive debug document covering architecture, root cause analysis, all changes, and unresolved issues for expert handoff.

### Changed
- **Relay reservation uses full multiaddr**: Changed from relative `/p2p/{rbn}/p2p-circuit` to full multiaddr (includes IP/port) in `ConnectionEstablished`, startup, and proactive heartbeat handlers. Fixes `MissingRelayAddr` errors that prevented reservations after network switches.
- **Mailbox replication to ALL verified RBNs**: Changed from storing on ONE anchor to ALL connected verified RBNs. Ensures message availability regardless of which anchor the recipient drains from.
- **Anchor filtering**: Only hardcoded bootstrap nodes (`verified_rbns`) receive `MailboxStore` payloads. Discovered anchors (peers with HOP protocol) are used for relay circuits only. Prevents regular peers from being treated as mailbox storage nodes.
- **`OutboundCircuitEstablished` flush**: Added pending message flush when relay circuit establishes (was only on `ReservationReqAccepted`). Clears rate limiter and dials peers with pending messages before flushing.
- **`FileTransferComplete` guard**: Only processes ACKs for active transfers. Stale ACKs from mailbox drain are silently dropped, preventing premature "verified" state.
- **File messages excluded from chat sync**: `[FILE]:` messages are filtered out of `ChatSyncResponse` to prevent metadata corruption (different `is_outgoing` values between sender/receiver).
- **Chat sync uses `INSERT OR IGNORE`**: Changed from `ON CONFLICT DO UPDATE` to `INSERT OR IGNORE` for sync responses. Sync now only fills gaps, never overwrites existing content.
- **Relay dialing simplified**: Changed from dialing ALL RBNs on ALL ports to dialing ONE RBN by latency with early break. Prevents `ResourceLimitExceeded` on RBNs.
- **TransitFileChunk removed**: File chunks now flow through the normal relay circuit path instead of being wrapped in a transit envelope routed through a random RBN.
- **Adaptive chunk sizing restored**: Removed IntroClaw adaptive override. Restored v34 pattern: 64KB for relay, 256KB for direct P2P.
- **Caption dialog redesigned**: Changed from Skip/Send to Cancel/Send. Cancel aborts the entire file send operation.
- **Removed duplicate `_addSendingPlaceholder`**: Was causing double thumbnails in chat. The real transfer bubble from Event 12 provides the UI.

### Fixed
- **Compilation errors in for_linux tree**: Added missing `ActivateTunnel` variant, `RBN_PEER_ID`/`RBN_WS_URL` constants, fixed `String`→`PeerId` type mismatch.
- **Premature verified tick on files**: `is_verified` was set to `true` when the last chunk was sent, not when the recipient confirmed. Now only set on `FileTransferComplete`.
- **Stale `FileTransferComplete` overwriting messages**: A guard now checks for active seeder before processing the ACK.
- **Sync rolling back messages**: `store_message_if_new` prevents stale sync data from overwriting current messages.
- **Cross-network relay reservation**: Full multiaddr fix resolves `MissingRelayAddr` that prevented relay reservations after network switches.

### Known Issues
- **Cross-network file transfer**: File chunks require a live relay circuit and cannot go through the anchor mailbox. When the relay circuit can't establish (different RBNs, VPN blocking), chunks pile up in RAM and get dropped. See `DEBUG_DOCUMENT.md` for detailed analysis.

## [0.21.1] - 2026-07-01 — v48 "Relay Resiliency & Network Debugging"

### Added
- **In-App Network Debug Log**: Integrated a 500-entry rolling ring-buffer in `IntrovertClient` capturing native Event 99 Rust network debug events.
- **Network Settings Controls**: Added a "Network Debug Log" section in Settings with options to copy logs, clear logs, and save logs directly to the device's Downloads directory as a text file.
- **TCP Port 80 Fallback dialing**: `dial_relay_path` now attempts all bootstrap addresses (QUIC and TCP) concurrently to ensure corporate bypass and firewall traversal works when UDP is blocked.
- **Progressive Reconnect Ladder**: Implemented a 4-step resilience strategy in the background `status_check_interval` watchdog:
  * Step 1: Re-request reservations if connected but missing a relay listener.
  * Step 2: Re-dial all bootstrap nodes and bootstrap Kademlia if fully offline.
  * Step 3: Activate local WebSocket loopback client tunnel as a secure fallback.
  * Step 4: Transition to explicit Offline reporting when all paths are exhausted.
- **Mailbox & Queue Drainage**: Trigger immediate RBN mailbox drainage (`perform_mailbox_fetch`) and flush all pending RAM message queues immediately upon receiving `ReservationReqAccepted`.
- **Chronological Chat Sync**:
  * Gated 1:1 and Group sync to retrieve and store messages using their **original timestamps** instead of SQLite's `CURRENT_TIMESTAMP`, preventing out-of-order messages.
  * Prioritized **newest missing messages** first when sending or receiving sync packets, ensuring the most recent messages load instantly when opening a chat.
  * Optimized fast sync wire overhead by only sending the last 100 known message IDs to the syncing peer.
  * Integrated Event 23 in 1:1 ChatScreen to reload messages instantly upon sync finish, and optimized GroupChatScreen Event 23 to reload only when the active group ID matches the event.
  * Hardened security: always verify sender is in the group for GroupSync, and verify sender is exactly the peer for 1:1 Sync, preventing spoofed sync responses.

### Changed
- **Network Status Semantics**: Status `1` (Online) is now gated on having an active relay reservation. Raw connections without a reservation are reported as status `4` (Connecting).
- **Status UI Indicators**: "CONNECTING" status displays in amber, and "OFFLINE" displays in red, giving users an accurate indicator of when outgoing messages will be queued instead of delivered immediately.

## [0.21.0] - 2026-07-01 — v47 "Token Economy v3.1 & FFI Hardening"

### Added
- **13 activity types**: 9 original + 3 web view primitives (WebFocusedActiveTime 0.1 pts/sec, SandboxWebPacketData 0.02 pts/KB, WebViewMediaCallHook 0.2 pts/sec) + UniquePeerHandshakes 1.0 pt/peer for RBN gateway utility.
- **Dynamic 15,000 social cap**: Edge nodes with verified 86,400s uptime unlock 15k cap; drops back to 5,000 if uptime < 86,400.
- **Active container sandgating**: `active_web_containers > 3` rejects all web view telemetry. Validated across WhatsApp, Telegram, Discord, Slack, Messenger, Google Messages.
- **RBN readiness model**: 3-pronged infra scoring — UptimeSeconds (648 pts max, 1.5x yield at ≥22h), RelayBytes (512 pts, 50 MB flat cap), UniquePeerHandshakes (500 pts, 500 unique clients/day). Max 1,660 infra points.
- **`daily_reward_records` table**: Consolidated cycle persistence with `total_social_points`, `total_infra_points`, `active_containers_highwater`, `total_cycle_uptime_secs`. Anti-farming gate rejects writes if `active_containers_highwater > 3`.
- **`FFIDailyState` struct**: `#[repr(C)]` fixed-width layout (f64+f64+u32+u64+u8+u8, 40 bytes). `get_current_rewards_state` extern "C" function returns by value — zero heap allocations cross FFI boundary.
- **Dart FFI bindings** (`lib/src/native/rewards_bridge.dart`): `FFIDailyState extends Struct`, typedefs, `loadGetRewardsState()` loader, `FFIDailyStateExt` boolean extension. 16 unit tests validating struct offsets, byte probing, round-trips, typedefs.
- **Macro pool clearing tests**: 5 new tests — RBN Scenario X (100 quiet, 82.19 INTR/day), RBN Scenario Y (10 strategic + 90 quiet, 43% premium), User/Edge pool (3:1 ratio), prestige multiplier scaling, annual decay (20%/yr).
- **SQLCipher corruption detection**: `SELECT count(*) FROM sqlite_master` probe after key PRAGMA. Catches `SQLITE_NOTADB` (26) and `SQLITE_CORRUPT` (11). Returns `STORAGE_DECRYPT_FAILED` error string to FFI layer (code -10).
- **Cross-compilation verified**: `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`, `x86_64-linux-android` all compile clean in release profile.
- **DAILY_REWARDS_SYSTEM.md**: Complete v3.1.0 specification (247 lines) covering macro framework, activity weights, guardrails, RBN economy, technical architecture.

### Changed
- **Package renamed** `introvert_tests` → `introvert` across 46 occurrences in 20 files (pubspec.yaml, Android/iOS/macOS/Windows/Linux/Web configs, Kotlin package declarations, docs).
- **SQLCipher PRAGMAs**: Added explicit `cipher_page_size=4096`, `kdf_iter=256000`, `cipher_hmac_algorithm=HMAC_SHA512`, `cipher_kdf_algorithm=PBKDF2_HMAC_SHA512` for deterministic cross-platform behavior.
- **Storage key lifecycle**: `storage_key` zeroed via `zeroize::Zeroize` on both success and failure paths after `StorageService::new()`. Key derived via HKDF-SHA256 with salt `b"introvert_storage_key"`.
- **Android cross-compilation**: Fixed `.cargo/config.toml` paths from Linux (`/home/dev/...linux-x86_64`) to macOS (`/Users/dev/...darwin-x86_64`). Added `CC`/`CXX` env vars for `cc-rs` build scripts. NDK bin directory must be in PATH (clang wrapper relative path dependency).
- **`ActivityWeights` defaults**: Added `web_focused_active_time: 0.1`, `sandbox_web_packet_data: 0.02`, `webview_media_call_hook: 0.2`, `unique_peer_handshakes: 1.0`, `cap_unique_peer_handshakes: 500`, `max_third_party_containers: 3`.
- **`DailyRewardState`**: Added `active_containers_highwater: u32` field, tracked in `record_activity()`, reset on cycle transition, persisted to `daily_reward_records`.
- **Deterministic rounding**: All activity points truncated to 4 decimal places; INTR rewards truncated to 6 decimal places. `FFIDailyState` values truncated before FFI transfer.

### Fixed
- **Duplicate Hero Tags crash**: Fixed an assertion crash (`multiple heroes that share the same tag within a subtree`) when multiple tabs are kept alive in PageView, by adding explicit `heroTag: null` settings to the FloatingActionButtons in `drive_tab.dart` and `notes_tab.dart`.
- **iOS handle resolution crash**: Fixed a Flutter framework assertion crash (Cupertino route/dependent element deactivation) by refactoring the "Add by Introvert Handle" input and "Handle Resolved" dialogs into a single, unified stateful dialog (`_ResolveHandleDialog`) that transitions internally (`input` -> `resolving` -> `resolved`/`failed`), eliminating overlapping Navigator pop/push transitions, duplicate GlobalKeys, and rendering clashes.
- **Android mutual connection visibility timing race**: Fixed a bug where Android didn't display the newly accepted contact. Added a `600ms` delay after `sendDirectInvite` to let the database write fully commit on the FFI thread before calling `_loadContacts()`.
- **Deadlock in `record_activity`**: Removed duplicate `let mut state = self.state.write()` that caused a deadlock when crypto validation was accidentally placed inside the state lock scope.

## [0.20.0] - 2026-07-01 — STABLE v46 "Security Hardening & UX Polish"

### Added
- **SignedRewardEnvelope**: `ActivityWeights` and `AntiGamingConfig` now require Ed25519-signed envelopes before processing. Trusted RBN public keys hardcoded. Monotonic sequence + 24h timestamp freshness.
- **Prestige tier on-chain verification**: `ProfileResponse.prestige_tier` verified against Solana RPC balance via `verify_prestige_tier()`. Async background verification corrects mismatched tiers.
- **Rust-side tier computation**: New FFI `introvert_compute_and_set_tier()` derives Solana address from seed, queries balance, computes tier — all in Rust.
- **Terms of Use mandatory boot gate**: Blocking modal with agreement checkbox. Engine refuses to start until accepted. Stored in SharedPreferences. Setup guide for same-phone QR scanning.
- **Messenger WebView tabs**: 6 web messengers (WhatsApp, Telegram, Discord, Slack, Messenger, Google Messages) as optional tabs. Max 3 active. Unread badge counts. Navigation allowlist, desktop User-Agent, camera/mic permissions, cookie persistence.
- **Intelligent theme color recommendation**: HSL color theory extraction from wallpaper images. Analogous accent shift, harmonious bg/surface/text. "Auto-Generate" sparkle button.
- **FIFA World Cup 2026 themes**: 18 country themes with color theory extraction.
- **Optimistic file send placeholders**: Immediate FileTransferProgress bubble when sending files. Auto-scroll to bottom.
- **Edit icon on all themes**: Built-in themes now editable (auto-generates custom name).
- **Network Tune/Heal in profile**: Replaced hard reset with Network Tune/Heal bottom sheet.
- **Sovereign Distribution Notice**: Expandable notice in Sovereign Wallet section explaining non-custodial nature.
- **Legal & Sovereignty section**: Merged Info & Legal + Legal & Sovereignty Disclaimers into single section. Open Source Licenses & Attribution replaces ZeroClaw Attribution.

### Changed
- **Sovereign Earnings → Sovereign Wallet**: Merged into single section showing Solana Wallet ID, INTR/SOL/USDC balances, points earned this cycle, expandable distribution notice. Claim button removed — rewards auto-sent to wallet.
- **CLAW tab moved to Settings**: Intro-Claw accessible via expander in Settings. Description, activity log, RECON/HEAL buttons.
- **Bottom navigation**: Dynamic messenger tabs (off by default). Labels shortened. `PageController` recreation on tab count change.
- **Chat sync banner**: Timer increased 3s→5s for 1:1. Messages reload on completion. Manual sync dismisses dialog first.
- **`is_lease_valid()`**: Now checks real balance ≥100K INTR and last claim within 30 days (was always-true bypass).
- **`DAILY_REWARD_ESCROW`**: Updated from placeholder to treasury address.
- **`send_transaction_raw`**: Uses configured RPC endpoint instead of hardcoded devnet URL.
- **Points display**: Header shows "pts today" instead of "INTR today".
- **Disclaimer check**: Uses SharedPreferences only (avoids SQLCipher FFI crash on iOS).
- **`startNetwork()` calls**: Wrapped in try/catch to prevent -10 errors when engine already running.

### Security
- **RewardConfig signature verification**: Ed25519 verification required before applying ActivityWeights or AntiGamingConfig.
- **Prestige tier on-chain validation**: Tier verified against Solana RPC balance; prevents self-assertion of inflated tiers.
- **Lease validation**: Real balance and timeout enforcement (was bypassed).
- **Escrow and RPC**: Placeholder address and hardcoded devnet URL removed.

### Added
- **Mandatory Terms of Use & Liability Disclaimer**: Blocking onboarding modal with two mandatory checkboxes. Engine and network remain completely uninitialized until accepted. Stored in SQLCipher `economy_meta` table.
- **Legal & Sovereignty Disclaimers section** in Settings: Permanent, scrollable view of the Terms of Use text.
- **Sovereign Distribution Notice** in Sovereign Earnings dashboard: Clarifies that Introvert is non-custodial, rewards are auto-sent to wallet, and lists recommended Solana wallets (Phantom, Solflare, Backpack).

### Changed
- **Sovereign Earnings**: Removed claim button. Rewards are automatically sent to wallet address on Solana network. Added wallet address display.
- **Boot sequence**: Engine start is gated behind disclaimer acceptance. No libp2p swarm, no RBN connections, no heartbeats until terms accepted.

### Security
- **Boot gate**: The Rust core refuses to initialize the network swarm until the front-end passes `accepted = true`. Prevents any network activity before legal acceptance.

## [0.19.0] - 2026-06-30 — v45 "Intelligent Themes & UX Polish"

### Added
- **Intelligent Theme Color Recommendation**: When uploading a wallpaper image, Intro-Claw analyzes pixels using HSL color theory (analogous accent shift, harmonious bg/surface/text) and auto-generates a cohesive palette. "Auto-Generate" sparkle button re-runs extraction on demand.
- **FIFA World Cup 2026 Themes**: 18 country themes (France, Brazil, USA, Mexico, Argentina, England, Italy, Colombia, Senegal, Uruguay, Croatia, Belgium, Netherlands, Morocco, Canada, Germany, Spain, Portugal). Images resized to 720px JPEG (~220KB). Colors extracted with color theory algorithm.
- **Optimistic File Send Placeholder**: Sending images/videos/files now shows an immediate placeholder bubble in the chat with auto-scroll to bottom. No more 2-3 second blank gap.
- **Network Tools in Profile**: Replaced old "Network Hard Reset" button with Network Tune/Heal bottom sheet (same as home screen).
- **Edit Built-in Themes**: Edit icon restored on all built-in themes in the theme picker. Editing a default theme auto-generates a custom name (custom01, custom02, etc.) — default is never overwritten.

### Changed
- **Theme live update**: `ListenableBuilder` on `AppTheme.current` — theme changes apply instantly without tab switching.
- **Chat sync banner**: Timer increased from 3s to 5s for 1:1 chat. Messages reload after sync completes.
- **Manual sync UX**: "Sync Chat" in both 1:1 and group chat now dismisses the dialog first and shows the syncing banner immediately.

### Fixed
- **Theme not updating on select**: `AppTheme` is a `ChangeNotifier` but `_MainShellState` wasn't listening. Fixed with `ListenableBuilder` wrapper.
- **Edit icon missing on default themes**: Only custom themes had edit buttons. Now all themes (built-in and custom) have the edit icon.
- **Chat sync banner not visible**: Auto-sync timer was too short (3s) and didn't reload messages. Increased to 5s with `_loadMessages()` on completion.
- **Manual sync dialog stuck**: 1:1 chat "Sync Chat" didn't dismiss the contact info dialog. Now dismisses first, then shows banner.

## [0.18.0] - 2026-06-30 — v44 "Messenger Integration"

### Added
- **Messenger WebView Tabs**: WhatsApp Web, Telegram Web, Discord, Slack, Messenger, Google Messages — all embedded via `flutter_inappwebview`. Each works like the desktop app. Max 3 active at once.
- **Unified Messenger Settings**: Single "Other Instant Messenger Clients" expander in Settings with all toggles. Instant nav bar update on toggle (no restart needed).
- **WebView Security**: Navigation allowlist, desktop User-Agent, camera/mic permissions, session persistence, direct traffic (not P2P), privacy disclaimers, setup guide overlay.
- **Intro-Claw in Settings**: Moved from dedicated tab into Settings expander with description, activity log, RECON/HEAL buttons.
- **Badge Counts**: WhatsApp and Telegram tab icons show unread message count badges parsed from page title.
- **Network Stabilization (from v43)**: Cross-network file transfer fix, relay pipeline/pacing restored to v40 values, relay reservation scoping, ListenerClosed auto-recovery, Intro-Claw adaptive chunk sizing, NAT64/IPv6 mobile data support.

### Changed
- **Bottom Navigation**: Removed CLAW tab. Dynamic messenger tabs appear when enabled. Labels shortened (WA, TG, DC, SL, MS, GM, DRIVE, NOTES, SET).
- **Settings Layout**: Messenger section below Appearance. Icon-only expander with instant toggle updates.
- **Relay Pipeline**: Depth 4→8, pacing 100ms→50ms, in-flight 4→8 (restored v40 values).
- **Intro-Claw Activity Log**: Now accessible from Settings expander, refreshes every 10 seconds.

### Fixed
- **Red screen on launch**: `_tabs` was `late final` and never initialized synchronously.
- **Nav bar not updating on toggle**: Two separate copies of messenger booleans caused stale reads. Fixed by re-reading SharedPreferences in `_rebuildMessengerTabs()`.
- **Cross-network file transfers stuck at 0%**: Gossipsub manifest `is_relayed=false` caused receiver to wait for impossible direct push. Fixed by forcing `is_relayed=true` when sender not directly connected.
- **Relay reservation lost on arbitrary disconnect**: `ConnectionClosed` cleared reservations for all peers. Scoped to RBN/anchor only.
- **ListenerClosed auto-recovery**: Relay listener closure now immediately re-requests reservation if RBN connection still active.

### Dependencies
- Added `flutter_inappwebview: ^6.1.5` for WebView support.

## [0.17.0] - 2026-06-29 — STABLE v43 "Network Stable with Mobile Data"

## [0.17.0] - 2026-06-29 — STABLE v43 "Network Stable with Mobile Data"

### Added
- **Intro-Claw Adaptive Chunk Sizing**: `AdaptiveChunkSizer` wired into file transfer pipeline. Chunk size selected per-peer based on observed throughput (512KB >10MB/s, 256KB >1MB/s, 64KB <1MB/s).
- **Throughput Recording**: `record_throughput()` called on every chunk receipt, feeding the adaptive sizer with real-time performance data.
- **Diagnostic Logging**: Full FFI→network loop command chain logging for `start_pull`. Select loop logs `HandleIncomingPayload` receipt.
- **ListenerClosed Auto-Relay Recovery**: When relay listener closes, immediately re-requests reservation if RBN connection still active.
- **NAT64/IPv6 Mobile Data Support**: Bootstrap resolver uses `ToSocketAddrs` on `47.89.252.80.sslip.io` for DNS64 synthesized IPv6 on cellular networks.

### Changed
- **Cross-Network File Transfer**: Forced `is_relayed=true` when sender is NOT directly connected. Receiver immediately enters pull mode instead of waiting for impossible direct push.
- **Relay Pipeline Depth (v40 restore)**: Increased from 4 to 8 chunks for relay transfers (matching v40 golden baseline).
- **Relay Pacing (v40 restore)**: Reduced from 100ms to 50ms between chunk requests (matching v40).
- **In-Flight Concurrency Limit**: Relay limit raised from 4 to 8; direct from 8 to 12. Removes backpressure bottleneck.
- **Relay Reservation Cleanup Scoped**: `ConnectionClosed` only clears relay reservation for RBN/anchor peers, not arbitrary peer disconnects.
- **RBN FileChunkRequest Authorization**: RBNs can now serve chunks for all transfer types (restored v40 behavior). Previously restricted to group transfers only.
- **ChatSyncResponse [FILE]: Filter**: File manifests excluded from sync responses (restored v40 behavior). Prevents duplicate processing.

### Fixed
- **Cross-Network Group File Stuck at 0%**: Root cause was gossipsub manifest arriving with `is_relayed=false` causing receiver to wait for direct push that could never arrive cross-network.
- **Relay Reservation Lost on Arbitrary Disconnect**: Any peer disconnect would wipe the RBN relay reservation, making node unreachable via relay.
- **Stall Recovery Pipeline Depth**: Aligned from 4 to 8 for relay transfers.

## [0.16.0] - 2026-06-27 — STABLE v40 "High-Speed Relays"

### Added
- **Connection Optimizer Action Execution**: Restored and wired the `ClawActions` execution blocks inside `src/network/mod.rs` (both the automatic tick loop and manual tick handler). The network swarm now actively triggers upgrades from relay to direct P2P connections on mDNS discovery.
- **mDNS Tracker Integration**: Discovered mDNS local peers are now correctly collected and passed to the `IntroClaw` context instead of passing an empty list.

### Changed
- **Relayed Transfer Chunk Size (4x speedup)**: Upgraded the default chunk size for relayed/pull file transfers from `64KB` to `256KB` in both the client (`src/network/mod.rs`) and RBN daemon (`for_linux/src/network/mod.rs`). This dramatically cuts down stream handshake overhead by 75% over relays.
- **Expanded Pipeline Window (2x speedup)**: Expanded the receiver's primed pull sequence and pull sliding window limits from `4` to `8` parallel requests to saturate high-latency relayed connections.
- **Watchdog Recovery Window**: Aligned the watchdog recovery window size to `8` in-flight requests.

### Fixed
- **Image Transfer Stuck at 51%**: Refactored `select_best_providers_static` to return `Vec::new()` instead of `providers.to_vec()` when no active provider links are open. This allows the pull loop to fall back to retrieve chunks from the active online peer.
- **Invite Accept Gossipsub Sync**: Added immediate Gossipsub subscription inside `AcceptGroupInvite` to ensure newly accepted members receive signaling without needing a restart.

## [0.15.0] - 2026-06-25 — STABLE v39 "Relay Resiliency"

### Added
- **ListenerClosed Relay Auto-Recovery**: Added `SwarmEvent::ListenerClosed` handler to client networking mod (`src/network/mod.rs`) that automatically removes dropped relay reservation listeners and requests a fresh circuit relay reservation if the connection to the RBN remains active.
- **RBN Code Sync**: Updated RBN (Relayed Byte Network) daemon source code (`for_linux/src/network/mod.rs`) with v2.0.0 protocol binary codec compatibility.
- **Makefile All Targets**: Verified full workspace cross-compilation pipeline (`make all`) compiling macOS (`libintrovert.dylib`), iOS (`libs/`), and Android (`jniLibs/`) native outputs.

### Changed
- **Weak Network Handling UI/UX**: Replaced the blocking interactive dialog on weak connectivity in `lib/views/chat_screen.dart` with a rate-limited background refresh (`forceNetworkRefresh()` capped to once every 2 minutes) and a discreet, non-disruptive `SnackBar` alert: `"Weak network detected... optimizing..."`.
- **File Transfer Authorization**: Hardened verification constraints on fallback seeder lookups in client and daemon network stacks.

### Fixed
- **Contact Info Layout Overflow**: Wrapped the contact settings details block in a `SingleChildScrollView` to prevent screen boundary layout errors on mobile screens.

> [!NOTE]
> Text messaging across networks is fully verified via circuit relays (RelayActive state). However, media sharing across different physical networks (e.g., direct download issues) has been improved but still requires thorough verification on multiple physical device setups.

## [0.14.0] - 2026-06-24 — STABLE v38 "Unified Drive"

### Added
- **Drive UX Redesign**: Created folder-based storage view, expandable category sections, visual thumbnail grids, and a "Download All" capability in the file explorer.
- **Hardened Reactions**: Integrated reaction delivery with SQLite `StoreInMailbox` fallback, ensuring reaction indicators persist and propagate reliably when direct transport drops.
- **Editable Themes**: Added interface configuration tools allowing customization of any system default color theme.

### Changed
- **Mailbox Delivery Fallback**: Applied mailbox store mechanics for reaction syncing.
- **File Manifest Sync**: Auto-schedules manifest sync scans on chat message updates.

## [0.13.0] - 2026-06-24 — STABLE v37 "Mesh Resurrection"

### Added
- **Group Invite Wrappers**: Hardened group invitations by wrapping Manifest keys via ECDH key exchanges.

### Fixed
- **Group Chat Restoration**: Resolved severe connectivity bugs including Noise IK session deadlocks, `GroupAction` double-encryption, Gossipsub `propagation_source` peer lookup errors, and RBN infinite loop bugs.
- **Theme Rendering**: Fixed Winter Wonderland styling anomalies.

## [0.12.0] - 2026-06-21 — STABLE v36 "Sovereign Audit (Economy)"

### Added
- **Economy Integration**: Deployed $INTR token whitepaper, dynamic staking specifications, daily activity incentives, and daily rewards formula logic.

## [0.12.0] - 2026-06-21 — STABLE v35 "Sovereign Audit"

### Added
- **$INTR Token Whitepaper**: Full economic blueprint (`Docs/INTROVERT_TOKEN_WHITEPAPER.md`) with 50/20/10/5/15 allocation matrix, 10-year emission schedule, 4-tier ownership system, gasless flow, and developer launch strategy
- **Daily Rewards System**: New `src/economy/daily_rewards.rs` module tracking 9 activity types with configurable weights, daily caps, anti-gaming measures, and dynamic pool-clearing formula. Integrated with existing RewardTracker and Solana claim flow
- **Economy Blueprint v5.0**: Updated `Docs/INTROVERT_ECONOMY_BLUEPRINT.md` with full allocation matrix, emission schedule, RBN staking parameters, and 4-tier token ownership
- **Universal Search across all tabs**: Chats tab search bar (filters by alias/peerId/handle/globalName/lastMessage), Notes/Drive tabs show "X results" indicator
- **Semantic Search tile in CLAW tab**: Combines exact substring matching with Intro-Claw `processAssistantQuery()` semantic search, deduplicates overlapping results
- **In-chat search**: Search within a conversation via 3-dots menu with 300ms debounce
- **Media, Links & Docs viewer**: 3-dots menu option showing all shared content categorized into Media/Links/Docs tabs
- **Elevated Messages**: Bookmark messages via long-press → "Elevate", view in dedicated tab, long-press to unelevate. Persists across sessions in `elevated_messages` SQLite table
- **INTR balance in header**: Live $INTR balance display with accent glow effects, updated via economy stream
- **Network status indicator**: Minimal dot + status text in header, tapping opens Network Tune/Heal bottom sheet
- **`getLastMessage` / `getLastGroupMessage` FFI**: Optimized LIMIT 1 queries for chat list previews (replaced O(N*M) pattern)
- **Avatar decode cache**: `_avatarCache` Map with LRU eviction (100 entries max)
- **Wormhole 4-word codes**: Increased from 2 to 4 words (~52 bits entropy)

### Changed
- **Sovereign P2P Architecture docs**: All project documentation updated to reflect autonomous, crowdsourced, self-healing mesh with Solana-based dynamic RBN discovery
- **Master Plan**: Vision reframed around dynamic blockchain bootstrapping, token gating, PDA escrow vault, Squads V4 governance
- **Gossipsub security**: Sender membership verification before processing messages, max_transmit_size 1MB, heartbeat 10s→30s
- **PoW difficulty**: Increased from 4 to 6 hex chars (24-bit) with ±5 minute timestamp validation
- **Request-Response codec**: Reduced from 10MB to 2MB max payload
- **Relay server limits**: Reduced from 1GB/1h/8192/4096 to 100MB/30min/256/100
- **Tunnel server**: Binds to 127.0.0.1 only (was 0.0.0.0)
- **Group secret**: Removed from GroupManifest wire format — only delivered via ECDH-wrapped GroupInvite
- **fetch_balance**: Replaced `getProgramAccounts` with lightweight `getAccountInfo` using derived ATA address
- **reqwest::Client**: Reused in SolanaIncentiveEngine instead of creating per-request
- **bootstrap_nodes**: Iterated by reference instead of cloning Vec
- **get_profile()**: Called once per event handler instead of triple
- **HashMap eviction**: Uses `indexmap::IndexMap` with `shift_remove` for true FIFO ordering on bounded buffers
- **INTROVERT_TRUST_ALL_WITNESSES**: Gated behind `#[cfg(debug_assertions)]` to prevent production use

### Fixed
- **54 audit issues resolved across 3 rounds** (24 Critical, 19 Medium, 11 Low)
- **FFI memory leaks**: 6 methods missing `_freeBinary`, `_handleFfiResult` success path, ~10 error-path leaks, pollPeerProfile/syncChatMessages Arena conversion
- **Null pointer checks**: Added to `intro_claw_process_query`, `intro_claw_heal_peer`, `intro_claw_voip_start_call`, `intro_claw_voip_record_sample`
- **`std::thread::sleep` in async**: Replaced with `tokio::time::sleep` at 3 locations
- **DuplicateSuppressor**: O(n) Vec → O(1) HashSet+VecDeque
- **GroupChatScreen**: Added `_messageController` and `_scrollController` dispose, `_displayMessages` version caching
- **`_applySearchFilter`**: Moved inside `setState` block
- **Dialog controller leaks**: `_showInChatSearch` and `_showMasterSearch` set `barrierDismissible: false`
- **`setState` after `await`**: Added `mounted` check in `_sendMessage`
- **Error swallowing**: Replaced `let _ =` with `tracing::error!` in economy module
- **GroupChatSync authorization**: Verifies sender is group member or known contact
- **FileChunkRequest authorization**: Verifies contact for Sovereign Drive fallback AND active seeder path
- **Bounded buffers**: early_chunks (100/1000/50MB), pending_messages (50/peer), incoming_transfers (50), resolved_group_codes (500), active_providers (1000), pending_claims (1000)
- **`get_storage_usage`**: Removed unreliable `fs::metadata("/")` call
- **N+1 contact query**: Pre-fetch contacts into HashMap for group messages
- **Drive file existence**: Moved to async helper to avoid UI thread blocking
- **In-chat search**: Added 300ms debounce timer
- **ClawTerminalDialog**: Cursor animation stops when final report is shown

### Security
- Gossipsub sender membership verification (non-members rejected)
- Group secret removed from plaintext wire format
- ChatSyncResponse sender authorization
- FileChunkRequest contact/group membership verification
- PoW 24-bit difficulty with timestamp staleness check
- Tunnel server localhost-only binding
- All bounded buffers with FIFO eviction (IndexMap)
- Request-Response 2MB limit
- Relay 100MB/circuit limit
- `INTROVERT_TRUST_ALL_WITNESSES` debug-only

## [0.11.0] - 2026-06-20 — STABLE v34 "Iron Claw"

### Changed
- **Master Plan overhaul:** Vision reframed around autonomous, crowdsourced, self-healing mesh with dynamic Solana-based infrastructure coordination
- **Architecture Blueprint:** Replaced hardcoded bootstrap nodes with dynamic blockchain bootstrapping via `introvert-registry` Solana program. Added Token Gating Engine, Unified Escrow PDA Vault, and Token Sink Mechanics sections. Removed detailed UI/storage/economy/automation layer sections (moved to Module Reference)
- **Economy Blueprint:** Streamlined to core token specs and self-sustaining utility loops. Removed peer lifecycle phases, prestige plane, and staking module (pending finalization). Added Squads V4 multisig address
- **Networking & Signaling:** Replaced peer discovery/signaling/file transfer sections with Dynamic Blockchain Bootstrapping and Financial Shielding Against Sybil Floods sections
- **Protocol Specification:** Replaced messaging/file/mailbox/group/media lifecycles with Decentralized RBN Infrastructure Lifecycle (on-chain init, dynamic directory, work verification, governance-gated upgrades)
- **Security & Encryption:** Replaced E2EE/storage/mailbox/FFI sections with Autonomous Infrastructure Safeguards (PDA isolation, Squads V4 governance, time-locked unstaking)
- **README.md:** Updated intro, core features, and tech stack to reflect sovereign P2P architecture with Solana-based dynamic bootstrapping, PDA vault, and token gating
- **Deployment Architecture:** RBN deployment now requires 2,000,000 $INTR stake in PDA escrow. Added on-chain registration and governance sections
- **Rebuild Guide:** RBN setup now requires $INTR tokens and on-chain registration instead of hardcoded IP lists
- **Configuration Reference:** Bootstrap nodes section updated to reflect dynamic Solana-based discovery with legacy fallback
- **Module Reference:** `network/config.rs` noted as legacy fallback; dynamic discovery is primary
- **Contributing:** Added architecture reading list for new contributors

### Architecture Decisions
- Bootstrap nodes are discovered dynamically from Solana on-chain registry at app startup
- Hardcoded IP arrays in `network/config.rs` serve as fallback only when Solana RPC is unreachable
- RBN operators must bond 2,000,000 $INTR into PDA escrow with 7-day unbonding cooldown
- Edge nodes require 100,000 $INTR minimum for active relay routing (Event Code 22)
- Contract upgrades controlled by Squads V4 3-of-5 Multisig — no single developer override

## [0.11.0] - 2026-06-20 — STABLE v34 "Iron Claw"

### Added
- 10 new Intro-Claw intelligence modules: offline queue, dead letter detection, peer reconnection scoring, bandwidth-aware transfer, group sync optimization, connection pre-warming, storage-aware caching, night maintenance window, VoIP call quality monitor, pre-call network check
- VoIP Intro-Claw integration: call quality tracking (RTT, loss, jitter, bitrate), activity log entries, adaptive bitrate detection, pre-call network check, call history analytics
- Real network recon implementation: live ReconContext from swarm state, peer routing table, connection analysis, storage metrics, security audit
- Real network heal implementation: multi-strategy execution (direct dial → relay → anchor routing), detailed heal reports
- Network change detection: connectivity_plus listener, auto-recon on WiFi↔Cellular↔None transitions
- Auto-recon on chat start for 1:1 and group chats
- CLAW tab live tile values: Engine shows Active/Inactive, Storage shows MB, Battery shows status, Bandwidth shows quality
- CLAW tab activity log: real-time view of all Intro-Claw operations, LOG toggle in header
- 17 MODULES info button in settings with all module descriptions and active/inactive explanations
- Anchor Mode INFO button in settings with full explanation of relay, DHT, mailbox, group storage functions
- CLAW tab result items are tappable: contacts open chat, files open, groups open group chat
- VoIP FFI functions: voip_start_call, voip_end_call, voip_record_sample, voip_get_quality
- VoIP NetworkCommand variants for all VoIP operations
- Idle mode: FCM replaces background mailbox polling and heartbeat when app is idle
- Anchor mode battery protection: auto-disable at 30% battery with activity log warning
- Anchor nodes keep 30s heartbeat (regular devices use 5 min with FCM)

### Changed
- Hybrid AI mode completely removed — Intro-Claw now 100% local, sandboxed, zero external calls
- process_assistant_query() simplified to local-only (2 parameters, was 5)
- intro_claw_get_status() returns { "is_active": bool, "mode": "local" }
- CLAW tab description updated to remove external LLM reference
- Settings Intro-Claw section simplified — removed hybrid toggle, endpoint, API key fields
- GlassmorphicContainer now uses two-layer approach: overlay + accent tint
- Network intervals optimized for FCM: heartbeat 30s→300s, republication 60s→300s, mailbox 120s→300s
- Intro-Claw tick runs idle-only maintenance when backgrounded (DB pruning, dead letters, offline queue only)
- Drive tab refresh reduced from 30s to 120s
- Background sync service disabled polling — FCM handles all wake-ups

### Removed
- llm_query() async function and all LLM integration code
- intro_claw_get_endpoint(), intro_claw_set_endpoint() FFI functions
- Hybrid mode toggle, endpoint URL field, API key field from settings UI
- _isHybridMode variable and _loadAiMode() from assistant tab
- _setIntroClawMode(), _saveIntroClawApiKey() methods from settings
- Background mailbox polling timer (FCM replaces it)

### Architecture Decisions
- DO NOT TOUCH: Direct P2P 1:1 file transfer pipeline is locked. Intro-Claw must not modify, intercept, or throttle direct 1:1 transfers.
- FCM push replaces all idle polling — devices sleep when backgrounded, wake on push
- Anchor nodes are exempt from idle mode — they maintain full mesh presence
- Battery <30% auto-disables anchor mode to protect device

## [0.10.0] - 2026-06-20 — STABLE v33 "Sovereign Palette"

### Added
- 5 new image themes: Canyon, Desert, Winter Wonderland, Morning Dew, Golden Hour, Azure Sky, Cyber City II, Cyber City III
- GlassmorphicContainer overlay layer — theme-aware (black 30% for dark, white 30% for light) for legibility
- overlayAlpha parameter for per-widget tint tuning

### Changed
- All 17 themes sorted alphabetically in picker (Introvert Dark stays as default)
- All theme images optimized: PNG→JPEG, 720px width, quality 80 (95% size reduction, 11.3MB→1.5MB)
- All Rust/RBN logging migrated from println!/eprintln! to tracing macros (info!/warn!/error!/debug!)
- Structured logging with tracing-subscriber EnvFilter initialization

### Removed
- Linen Mist, Glacier Bloom, Rose Quartz plain white themes (replaced by image themes)
- macOS Finder (1) duplicate files from project tree
- .stable, .bak, .clean, .tail backup files from project tree
- solana (Copy).rs junk files

### Fixed
- Event type 35/40 verified consistent (35=Handle Resolve Failed, 40=Message Reaction)

### Architecture Decisions
- **DO NOT TOUCH**: Direct P2P 1:1 file transfer pipeline is locked. Intro-Claw must not modify, intercept, or throttle direct 1:1 transfers. Only observation for health scoring is permitted.

## [0.9.0] - 2026-06-20 — STABLE v32 "Sovereign Glass"

### Added
- Glassmorphism UI across all tabs — reusable GlassmorphicContainer widget with BackdropFilter blur
- 5 new image themes: Beach House (light), Cyber City, Mountain Peak, Mountain Ridge, Forest
- SovereignWallpaper supports both asset paths and file paths for built-in and custom themes
- CLAW tab dual-mode: Local mode shows 3x3 query tile grid, Hybrid mode shows chat interface
- CLAW tile results shown in floating overlay dialog (not chat mode)
- Brain logo (psychology icon) above CLAW tile grid
- Network recon/heal terminal overlay with detailed milestones, timestamps, and confirmations
- Network Tune/Heal popup menu replacing NetworkOptimizationButton in top bar
- ZeroClaw attribution in Info & Legal with full MIT/Apache 2.0 license text
- Custom theme editing auto-generates custom name (custom01, custom02) — defaults never overwritten
- Delete button only shown for custom themes
- Notes tab inline header matching Drive tab style (no AppBar)
- Combined header glassmorphic boxes for Notes tab and CLAW tab
- FAB consistent positioning across all tabs (bottom: 80, endFloat)
- All tabs start below Introvert top bar (MediaQuery padding + kToolbarHeight)

### Changed
- AppBar transparent with BackdropFilter(blur: 20) at 60% opacity, extendBodyBehindAppBar: true
- Bottom navigation frosted glass pill with ClipRRect + BackdropFilter(blur: 20)
- Scaffold backgroundColor changed from Colors.transparent to AppTheme.current.bg
- Light themes show clean white bg (no dark default wallpaper)
- Light theme wallpaper opacity set to 0.45 (Linen Mist, Glacier Bloom, Rose Quartz)
- Image themes use wallpaperOpacity: 1.0 (full image, no overlay)
- SovereignEarnings Card color changed to Colors.transparent for glassmorphism
- Drive mesh capacity card wrapped with GlassmorphicContainer
- Settings merged 5 sections into "Introvert Mesh Swarm Settings"
- Settings Fano icon pushed below AppBar (SizedBox height: padding.top + kToolbarHeight + 16)
- CLAW bottom bar buttons use GlassmorphicContainer (no outer box)
- FAB padding increased from 48px to 80px for Android compatibility

### Fixed
- Android ForegroundServiceDidNotStartInTimeException — startForeground() now passes FOREGROUND_SERVICE_TYPE_SPECIAL_USE on API 34
- Light themes showing dark wallpaper bg
- FAB partially covered by navigation bar on Android
- Tab content covered by top bar on all tabs
- Notes tab title styling inconsistent with Drive tab

## [0.8.0] - 2026-06-19 — STABLE v31 "Intelligent Mesh"

### Added
- Direct P2P file transfer speed restored to 70+ Mbps (removed app-level Noise double-encryption on FileChunk)
- File transfer notification suppressed during download — image appears instantly when verified
- Early chunk buffering to fix race condition where chunks arrive before manifest
- Custom theme wallpaper support (image picker, auto-resize to 720px, JPEG quality 80)
- Wallpaper opacity slider (0-100%) in custom theme creator
- Voice memo recording (AAC-LC, temp file path with timestamp)
- Forward message option in chat and group chat context menus
- Reply Privately in group chats (only for 1:1 connected contacts)
- Reply Privately uses custom user-arrow icon and includes quoted original message
- Mesh-aware status labels ("pulling from mesh", "pushing to mesh", "tap to pull from mesh")
- File transfer bubbles use compact placeholder during download (100px)

### Changed
- Voice memo plugin uses auto-generated temp path (fixes Android MediaMuxer crash)
- Voice memos use DeviceFileSource instead of UrlSource (fixes macOS AVPlayer playback)
- File transfer timestamps fall back to current time when start_time_ms is 0 or null
- SovereignWallpaper converted to StatefulWidget (listens to AppTheme ChangeNotifier)
- ThemeConfig now includes wallpaperPath and wallpaperOpacity fields (backward compatible)
- Default is_relayed set to false for direct P2P transfers
- Initial push delay reduced from 500ms to 200ms for direct P2P
- Custom theme name auto-filled with custom01, custom02, etc.
- Custom theme name validation shows error when empty
- In-flight limit restored to 8 for direct P2P (was 16, caused relay flooding)

### Fixed
- Voice memo crash on Android (empty string passed to MediaMuxer)
- Voice memo playback failure on macOS (UrlSource to DeviceFileSource)
- Date separator showing 1 Jan 1970 for file transfers with null/zero timestamps
- Custom theme wallpaper not displaying (SovereignWallpaper not listening to theme changes)
- Edit theme flow now always applies the saved theme
- Sender "waiting for recipient" not updating after ACK (ValueKey forces rebuild)
- Receiver thumbnail not showing for verified transfers (ValueKey forces rebuild)
- File transfer notification appearing before download completes
- Group chat file transfer race condition (chunks arriving before manifest)

### Dependencies Added
- flutter_svg: ^2.0.10 (custom Reply Privately icon)

## [0.7.1] - 2026-06-19 — Intro-Claw (Phase 7)

### Added
- **Intro-Claw core automation engine** — 12 modules running on a 5-minute tick loop: battery throttling, DB pruning, media cleanup, connection optimization, message batching, predictive prefetch, sync prioritization, duplicate suppression, health scoring, storage quota, adaptive chunking, tick integration
- **Local assistant UI** — Chat-style interface in the CLAW tab with natural language queries for messages, files, contacts, notes, calls, and storage status; generic type queries (e.g., "show my photos") list all items of that type
- **Semantic intent engine** — BERT-based embedding model (all-MiniLM-L6-v2 via candle-core, 384-dim embeddings) with 12 action intents using cosine similarity matching; keyword fallback when model not loaded
- **Network recon & self-healing** — Diagnostic reports (mesh overview, peer routing, connection analysis, anchors, security audit) and 5-strategy connection recovery (direct dial, relay circuit, anchor routing, WebSocket tunnel, mailbox fallback)
- **Hybrid AI mode** — OpenAI-compatible endpoint integration with encrypted API key storage (SQLCipher) and LLM v1 API with local search context
- **FCM push notifications** — Firebase Admin SDK on RBN with direct FCM v1 API calls (no third-party bridge)
- **Intro-Claw security sandbox** — Zero access to master keys, message content, or session blobs; network isolation in Offline mode
- **FFI functions** — intro_claw_get_ai_mode, intro_claw_set_ai_mode, intro_claw_trigger_tick, intro_claw_set_active, intro_claw_get_status, intro_claw_get_endpoint, intro_claw_set_endpoint, intro_claw_process_query, intro_claw_run_network_recon, intro_claw_heal_peer
- **Embedding engine tests** — 16/16 passing

### Changed
- `economy_meta` table now stores intro_claw_active, intro_claw_ai_mode, intro_claw_api_key, intro_claw_endpoint config keys
- RBN FCM config path: `/opt/introvert/config/firebase-service-account.json`

## [0.6.0] - 2026-06-18

### Added
- Voice memo recording (AAC-LC, temp file path with timestamp)
- Custom theme wallpaper support (image picker, auto-resize to 720px, JPEG quality 80)
- Wallpaper opacity slider (0-100%) in custom theme creator
- Forward message option in chat and group chat context menus
- Reply Privately in group chats (only shown for 1:1 connected contacts)
- Reply Privately uses custom user-arrow icon from The Noun Project
- Reply Privately includes quoted original message in 1:1 chat

### Changed
- Voice memo plugin now uses auto-generated temp path (fixes Android MediaMuxer crash)
- Voice memos use DeviceFileSource instead of UrlSource (fixes macOS AVPlayer playback)
- File transfer timestamps fall back to current time when start_time_ms is 0 or null (fixes 1 Jan 1970 date separator)
- SovereignWallpaper converted from StatelessWidget to StatefulWidget (listens to AppTheme ChangeNotifier)
- ThemeConfig now includes wallpaperPath and wallpaperOpacity fields (backward compatible)

### Fixed
- Voice memo crash on Android (empty string passed to MediaMuxer)
- Voice memo playback failure on macOS (UrlSource to DeviceFileSource)
- Date separator showing 1 Jan 1970 for file transfers with null/zero timestamps
- Custom theme wallpaper not displaying (SovereignWallpaper not listening to theme changes)
- Edit theme flow now always applies the saved theme

### Dependencies Added
- flutter_svg: ^2.0.10 (for custom Reply Privately icon)

## [0.4.0] - 2026-06-16

### Added
- Typing indicator (TypingStart/TypingStop signaling, Event 39)
- Last seen status (Heartbeat every 30s, stored in contacts table)
- Message search (SQL LIKE query for 1:1 and group chats)
- Call history (DB table, 3 FFI functions, auto-logged on call end)
- Call history logging on call end in CallScreen
- BackgroundSyncService with 5-min Timer.periodic fallback

### Changed
- TypingStart, TypingStop, Heartbeat payloads added to SignalingPayload enum
- contacts table: added `last_seen INTEGER` column
- call_history table: peer_id, call_type, media_type, duration, is_incoming, timestamp
- FFI functions: introvert_send_typing_start/stop, introvert_get_last_seen, introvert_search_messages, introvert_search_group_messages, introvert_call_history_log/get/count
- Package name: chat.introvert.app
- Firebase project: introvert-p2p (Sender ID: 302706705869)

### Fixed
- WorkManager incompatibility removed (uses Flutter v1 embedding APIs)
- Fallback to Timer.periodic(5min) for background sync

## [0.3.0] - 2026-06-16

### Added
- WebSocket tunnel for NAT traversal
- Handle registry with PoW consensus
- Group muting/unmuting functionality
- Message editing and deletion
- Connection diagnostics overlay
- Video player with controls
- Location picker with map

### Changed
- Upgraded libp2p to v0.56
- Migrated to Solana SDK 4.0
- Improved direct connection auto-upgrade
- Optimized DHT replication factor (20 → 5)

### Fixed
- FFI memory leaks in file transfer
- iOS sandbox path resolution
- Relay connection overwrite issue
- Group action signature verification

## [0.1.0] - 2026-06-08

### Added
- Core engine with FFI bridge (50+ functions)
- 1-on-1 encrypted messaging
- File transfer with chunking
- Sovereign Drive encrypted storage
- Gossipsub group mesh
- WebRTC voice/video calls
- Magic Wormhole onboarding
- Solana $INTR token economy
- SQLCipher encrypted database (18 tables)
- Dark mesh isolation
- Port 443 strategy
- Multi-platform support (Android, iOS, macOS, Linux)

### Security
- Noise IK transport encryption
- HKDF-SHA256 identity derivation
- AES-256-CBC database encryption
- Zero-knowledge mailbox

## [0.0.1] - 2026-05-01

### Added
- Initial project structure
- Basic libp2p integration
- Simple messaging prototype
- FFI bridge foundation

---

## Version History

| Version | Date | Codename | Key Features |
|---------|------|----------|--------------|
| 0.11.0 | 2026-06-20 | Iron Claw | Local-only Intro-Claw, 10 intelligence modules, VoIP monitoring, real recon/heal, network change detection |
| 0.10.0 | 2026-06-20 | Sovereign Palette | 17 themes, glassmorphism overlay, tracing logging, theme optimization |
| 0.9.0 | 2026-06-20 | Sovereign Glass | Glassmorphism UI, 5 image themes, CLAW redesign, terminal overlay, Android 14 fix |
| 0.8.0 | 2026-06-19 | Intelligent Mesh | Intro-Claw AI engine, local assistant, BERT semantic search, FCM push |
| 0.7.1 | 2026-06-19 | Intro-Claw | Automation engine, local assistant, semantic AI, FCM push |
| 0.7.0 | 2026-06-18 | Sovereign Velocity | 70+ Mbps P2P, silent download, custom wallpapers |
| 0.6.0 | 2026-06-18 | Sovereign Velocity | Voice memos, themes, forward, reply privately |
| 0.1.0 | 2026-06-08 | Sovereign | Full P2P mesh with economy |
| 0.0.1 | 2026-05-01 | Genesis | Initial prototype |

## Release Process

1. Update version in `pubspec.yaml` and `Cargo.toml`
2. Update `CHANGELOG.md` with new changes
3. Create git tag: `git tag -a v0.1.0 -m "Release 0.1.0"`
4. Build platform binaries:
   ```bash
   make mac
   make android
   make ios
   ```
5. Test on all platforms
6. Push tag: `git push origin v0.1.0`
7. Create GitHub release with binaries
