# Introvert Version Changelog

_Stable version history with key changes. Updated at every stable backup._

| Version | Date | Codename | Networking | UI/UX | Security | Economy |
|---------|------|----------|-----------|-------|----------|---------|
| v0.1.0 | 2026-07-04 | Beta Launch | Cross-network relay fix (rbn_blacklist removed, in-flight limits restored 8/12, VPN fallback). File chunk dial flood removed. Webview scroll fix (CSS targeted containers, MutationObserver, gesture arena fix). PageView horizontal swipe disabled. | Emoji reactions (system keyboard + View Reactions toolbar). WhatsApp/Telegram Web embed scroll working. Drive tab: "Introvert Explained" guide images bundled. Settings: GitHub repo link, update URL. | IPC shared secret rotated (32-byte hex). Solana registry bypassed. RBN hardcoded to Alibaba. | Economy pipeline verified (TelemetryEnvelope → ClaimRequest → transfer_checked). Treasury funded 51K $INTR. Introvert Explained guide images seeded to Drive. |
| v27 (0.4.0) | 2026-06-16 | — | Core mesh (Gossipsub, WebRTC, WebSocket tunnel). Heartbeat 30s. DHT replication 20→5. | Typing indicator, last seen, message search, call history | Noise IK, HKDF-SHA256, AES-256-CBC | Initial $INTR token |
| v28 (0.5.0) | 2026-06-16 | — | No changes | 7-theme UI overhaul | No changes | No changes |
| v29 (0.6.0) | 2026-06-18 | Sovereign Velocity | No changes | Voice memos, forward, reply privately | No changes | No changes |
| v30 (0.7.0) | 2026-06-18 | Sovereign Velocity | **70+ Mbps achieved** — removed double Noise on FileChunk. Push delay 500→200ms. In-flight 16→8. | Silent download, custom wallpapers | No changes | No changes |
| v31 (0.8.0) | 2026-06-19 | Intelligent Mesh | 70+ Mbps maintained. Adaptive chunking 64KB–512KB. FCM push. | Themes, voice memos, forward, reply privately | Intro-Claw sandbox | Intro-Claw AI |
| v32 (0.9.0) | 2026-06-20 | Sovereign Glass | No changes | Glassmorphism UI, 5 image themes | No changes | No changes |
| v33 (0.10.0) | 2026-06-20 | Sovereign Palette | No changes | 17 themes, tracing logging | No changes | No changes |
| **v34 (0.11.0)** | **2026-06-20** | **Iron Claw** | **FCM replaces polling**: heartbeat 30s→300s, republication 60s→300s, mailbox 120s→300s. **DO NOT TOUCH direct P2P pipeline.** | 10 Intro-Claw modules, VoIP monitoring | Idle mode, anchor battery protection | No changes |
| **v35 (0.12.0)** | **2026-06-21** | **Sovereign Audit** | ⚠️ **Gossipsub heartbeat 10s→30s**. ⚠️ **max_transmit_size: unlimited→1MB**. ⚠️ **Request-response 10MB→2MB**. ⚠️ **Relay 1GB→100MB, 8192→256 reservations, 4096→100 circuits**. | Universal search, elevated messages, INTR balance | Sender membership verification, group secret removed from wire, PoW 24-bit | Daily rewards |
| v36 (0.12.0) | 2026-06-21 | Sovereign Audit | Same as v35 | Same as v35 | Same as v35 | $INTR whitepaper, daily rewards system |
| **v37 (0.13.0)** | **2026-06-24** | **Mesh Resurrection** | **Group chat RESTORED** after Claude/Gemini debugging. 10+ bugs fixed: Noise IK deadlock, GroupAction double-encryption, gossipsub propagation_source bug, RBN self-relay loop, v34 config restored. RBN achieves RELAY CONNECTED. | Winter Wonderland theme fix, editable themes | GroupInvite ECDH-wrapped, GroupManifest secret removed, auto-accept on join | No changes |
| **v38 (0.14.0)** | **2026-06-24** | **Unified Drive** | Reactions use StoreInMailbox for reliable delivery. File manifest sync from messages. | **Drive redesign**: folder-based, expandable, thumbnail grid, file explorer with download all. **Reactions**: reliable propagation, counts, details. **Themes**: edit any default. **Weak network**: discreet SnackBar. | Reaction delivery hardened via mailbox fallback | No changes |
| **v39 (0.15.0)** | **2026-06-25** | **Relay Resiliency** | **Relay reservation recovery** on ListenerClosed; bootstrap/RBN seeder bypasses. *Note: cross-network media transfer needs thorough device testing.* | **Weak network** discreet SnackBar auto-optimization (non-blocking). **Scrollable contact settings** info dialog (no overflow). | Hardened file auth logic on seeder fallback path. | No changes |
| **v40 (0.16.0)** | **2026-06-27** | **High-Speed Relays** | **4x relay speedup**: chunk size 64KB→256KB for relayed transfers. Pipeline window 4→8. Connection Optimizer action execution restored. mDNS tracker integration. | Image transfer stuck fix (select_best_providers_static). Invite accept gossipsub sync. | No changes | No changes |
| **v43 (0.17.0)** | **2026-06-29** | **Network Stable with Mobile Data** | **Cross-network file transfer fix**: forced is_relayed=true for non-direct senders. **Relay pipeline 4→8**, pacing 100ms→50ms, in-flight 4→8. **Relay reservation scoped cleanup** (RBN-only). **ListenerClosed auto-recovery**. **RBN FileChunkRequest auth restored**. **ChatSyncResponse [FILE]: filter**. **Intro-Claw adaptive chunk sizing** wired into transfer pipeline. **NAT64/IPv6** via sslip.io DNS. **Diagnostic logging** for FFI→network loop command chain. | No changes | No changes | No changes |
| **v44 (0.18.0)** | **2026-06-30** | **Messenger Integration** | No changes | **6 messenger WebView tabs** (WhatsApp, Telegram, Discord, Slack, Messenger, Google Messages). **Instant toggle updates** (no restart). **Intro-Claw moved to Settings** expander. **Nav bar live update fix** (SharedPreferences re-read). **Red screen launch fix**. | Navigation allowlist, desktop User-Agent, camera/mic perms, privacy disclaimers, setup guide overlay | No changes |
| **v45 (0.19.0)** | **2026-06-30** | **Intelligent Themes & UX Polish** | No changes | **Intelligent theme color recommendation** from wallpaper (HSL color theory). **18 FIFA World Cup themes**. **Optimistic file send placeholder** with auto-scroll. **Edit icon on all themes**. **Theme live update** (ListenableBuilder). **Chat sync banner** improved (5s timer, reload on complete). **Manual sync** dismisses dialog first. **Network Tune/Heal** in profile screen. | No changes | No changes |
| **v46 (0.20.0)** | **2026-07-01** | **Security Hardening & UX Polish** |
| **v49 (0.21.2)** | **2026-07-01** | **Cross-Network Delivery & Mailbox Integrity** | **Relay reservation full multiaddr fix** (MissingRelayAddr). **Mailbox replication to ALL verified RBNs**. **`verified_rbns` filter** (bootstrap nodes only). **`OutboundCircuitEstablished` flush** with rate limiter clear. **TransitFileChunk removed** — chunks via normal relay circuit. **64KB relay chunks restored** (was 256KB). **Relay dial simplified** (one RBN, early break). | **Caption dialog thumbnails** (Cancel/Send). **Status 3 clock icon** (In Mailbox). **Double thumbnail fix** (removed `_addSendingPlaceholder`). **Stale FileTransferComplete guard**. | **`MailboxStored` ACK** from anchor to sender. **`store_message_if_new`** (INSERT OR IGNORE) for sync. **File messages excluded from sync**. **Chat sync no longer overwrites** existing messages. **Retry undelivered messages** (60s threshold). | **Known issue**: cross-network file chunks need live relay circuit (no mailbox fallback). See DEBUG_DOCUMENT.md. |
| **v50 (0.21.3)** | **2026-07-01** | **Delivery Fixes & System Hardening** | **`dial_relay_path` parameterized** (`for_file_chunk: bool`). **ALL RBNs for file chunks** (no early break). **Persistent file chunk queue** (`pending_file_chunks` SQLite table). **Relay hint optimization** (prioritize hinted RBN). **VPN stale reservation detection** (force-clear and re-dial). **DCUtR on InboundCircuitEstablished** (hole-punch attempt). **Gemini: Relay reservation three-tier fallback** (bootstrap_nodes → anchor_mappings → filtered listen_addrs; fixes VPC private IP leak). | **Gemini: File transfer bubble** — thumbnail suppression scoped to `_buildThumbnailWidget()` only; transfer card with progress/status/cancel always shown to receiver. | **`update_message_status_if_higher`** (monotonic transitions). **`sync_in_progress` timeout** (60s cleanup). **[FILE]: filter in sync** (defense in depth). **for_linux sender authorization** (ChatSyncResponse). **for_linux relay reservation fix** (RBN-only cleanup). | **Status downgrade protection** (all ACK handlers). **Data loss prevention** (defer chunk removal to FileTransferComplete). **sync_in_progress lockout fix** (remove on unauthorized). |
| **v54 (0.28.0)** | **2026-07-04** | **VPN Resilience & Session Optimization** | **VPN Adaptive Pathing**: Isolate bootstrap nodes list to ONLY the tunnel loopback address (`127.0.0.1`) when VPN (type 5) is active, preventing dead dials to public RBNs from clogging the queue. **Solana Registry Bypass**: Disabled on-chain Solana registry queries entirely for Mainnet, falling back strictly to the hardcoded Alibaba RBN node (`47.89.252.80`). **Queue Congestion Prevention**: Removed redundant carpet-bombing dial loops from `forward_to_mesh`'s fallback block. **Blacklist Cooldown Bypass**: Clears RBN blacklists on network switch, manual refresh, and tunnel activation to prevent stale blocks; removes connected RBNs from blacklist immediately. **LAN Node Removal**: Cleaned Thinkpad RBN (`192.168.1.81`) from default configuration. | **Active Chat Session Prioritization**: Flutter UI dynamically propagates chat session state (`setActiveChat` / `setActiveGroupMembers`); Rust engine bypasses cooldowns, aggressively punches direct holes (DCUtR) for relayed partners, and proactively heals offline targets on every tick. **Chat Screen Offline Sync**: Updates chat status to Offline when local node goes offline. **App Launch Warm-Up**: `onAppLaunch()` executes initial warm-up connection pass. | Monotonic status upgrades and RBN blacklisting protect mesh integrity. | No changes |
| **v56 (0.30.0)** | **2026-07-06** | **Sovereign Economy & Snappy Mesh** | **Connection State Cycler 15s Status Check Integration**: Evaluates ConnectionStateCycler on the 15-second status loop when IntroClaw is active instead of waiting for 5-minute ticks, achieving rapid (15–30s) connection drop recovery on all devices. | No changes | **Cryptographic Telemetry Authentication**: Telemetry envelopes signed with client-derived Ed25519 Solana keys and validated at the RBN libp2p entry point. Persistent storage of Raw Envelopes in encrypted SQLCipher database to survive restarts. | **Stage 1-3 Rewards Pipeline Implementation**: 13-metrics schema alignment between client and RBN. SQLite telemetry persistence and recovery. Midnight UTC cron task to trigger epoch clearing, calculate rewards with IQR outlier mitigation, sign with HMAC-SHA256, and dispatch claims to Solana daemon on port 9001. |
| **v57 (0.31.0)** | **2026-07-17** | **Push Dedup & Peer Caching** | **FCM echo loop fix**: 30s cooldown on `onPushNotification` (was unlimited). **Peer count caching**: `AtomicUsize` replaces O(n) `swarm.connected_peers().count()` at 5 call sites. **RBN push dedup**: SHA-256 payload hash prevents duplicate MailboxStore. **RBN push cooldown**: 30s per-recipient cooldown prevents FCM spam (was 91 pushes/burst). **RBN binary updated**: May 18 → Jul 17 (2-month gap closed). | No changes | No changes | No changes |
| **v58 (0.32.0)** | **2026-07-18** | **Intelligent Transfer Routing** | **TransferRouter** (`src/network/service.rs`): new routing module that resolves optimal path (Direct > LocalSeeder > Relay) before `forward_to_mesh` dispatches. Direct/LocalSeeder paths bypass gossipsub entirely, sending via request-response codec. **Drain cooldown split**: mail drain 5s→30s (FCM echo-loop), chunk drain 250ms (thundering herd). Both `InboundCircuitEstablished` and `OutboundCircuitEstablished` gated. **In-flight cap** (8) enforced on Direct/LocalSeeder path with sliding-window drain on response. **Direct failure cooldown**: `mark_direct_failed()` degrades to Relay on LAN drop (30s cooldown). **Control topic subscription** for all TransferPath variants (seeder cascade readiness). **Platform mDNS permissions**: Android `CHANGE_WIFI_MULTICAST_STATE` + `MulticastLock` in `IntrovertService`; macOS `com.apple.security.network.multicast` entitlement + `NSLocalNetworkUsageDescription` + `NSBonjourServices`; iOS `_p2p._udp` added. **File transfers stable** across same-network, cross-network, and VPN scenarios. | No changes | No changes | No changes |
| **v55 (0.29.0)** | **2026-07-06** | **Recovery, Drive, VPN & Notifications** | **VPN Tunnel Fix**: `ws://` port 80 fallback when VPN detected (bypasses TLS blocking). **VPN Bootstrap Isolation**: Tunnel-only on VPN. **VPN Relay Fix**: Removed `relay_reservations.clear()`. **ListenerClosed Fix**: Full multiaddr. **In-flight Limits**: relay=4/direct=8. **Anchor Relay Strategy**. **Undelivered Retry**. **Telemetry Pipeline**: 30-min interval. **9-Field Bridge**. | **Drive Rebuild**: 612-line folder manager with grouping, minimized view, Introvert Explained, list/grid toggle, multi-select, batch ops, breadcrumb, storage bar. **Messenger tabs** (6 web messengers). **FIFA themes**. **Notification Hardening**: 3-min cooldown (native + Dart), foreground suppression, sound-only when app open. **android libc++_shared.so** bundled. | **FFI**: 134 exports, 0 mismatches. **Drive FFI**: `drive_add_file_with_folder`, `drive_update_folder`. **Storage**: `folder` column, `drive_folders` table. **Backup**: `dd_mm_yy_time` naming. **Economy daemon** restored. | **IQR Anti-Gaming Filter**. **Deployed** to Alibaba RBN. **Full recovery** from GitHub source + v54 networking. |
| v53 (0.23.0) | 2026-07-03 | Beta Stability | **FCM push fix** (`message_type`→`msg_type`). **Connection limit** 102→204. **Step 1 reconnect** (dials disconnected RBNs). **Group ACK** (>=1 not >=total). | **Messenger WebView** — login CSS fix, scroll fix. **CAMERA permission**. **Firebase key** regenerated. | Beta stability release. Cross-network verified. |
| **v51 (0.21.4)** | **2026-07-01** | **Cross-Network Success & Sync Integrity** | **CROSS-NETWORK FILE TRANSFERS WORKING** on VPN and mobile data. **Chat list sorting fixed** — uses `MAX(timestamp)` instead of `MAX(id)` for chronological ordering. **Mailbox drain dedup** — `message_exists()` check + `[FILE]:` filter + cleared-chat timestamp guard. **Cleared chat protection** — `cleared_chats` table tracks clear timestamps; `should_skip_mailbox_message()` prevents re-delivery of pre-clear messages. **Proactive mailbox drain** on chat clear via `ClearMailboxForPeer` command. | **File transfer timestamps** — `FileTransferBubble` now displays HH:MM below status text. **Create/Join Group dialogs fixed** — uses parent widget context instead of invalidated bottom sheet context. | **`cleared_chats` table** — prevents mailbox from re-delivering old messages after chat clear. **`message_exists()`** — O(1) dedup check for mailbox-drained messages. **`cleanup_cleared_chats()`** — prunes entries older than 7 days. | **Stable release** — cross-network file transfers verified working on VPN and mobile data. |
| **v47 (0.21.0)** | **2026-07-01** | **Token Economy v3.1 & FFI Hardening** | Fixed Android NDK cross-compilation (darwin paths, cc-rs env). Verified aarch64-apple-ios, aarch64-apple-ios-sim, aarch64-linux-android, x86_64-linux-android all compile clean. | **FFIDailyState** `#[repr(C)]` struct + `get_current_rewards_state` extern "C" bridge. **Dart FFI bindings** in `rewards_bridge.dart` (struct, typedefs, loader, 16 unit tests). **Package renamed** `introvert_tests` → `introvert` (46 occurrences, 20 files). | **SQLCipher hardening**: HKDF-SHA256 key derivation, explicit SQLCipher 4 PRAGMAs, corruption probe, key zeroization via `zeroize`. **`daily_reward_records` table**: consolidated social/infra/containers/uptime with anti-farming container gate (N≤3). | **Stage 1**: 13 activity types (3 web view primitives), dynamic 15k edge cap (86,400s gate), 3-container sandgate, 4-decimal truncation. **Stage 2**: RBN readiness model (648 uptime + 512 data check + 500 unique handshakes = 1,660 max infra). **Stage 3**: Macro pool clearing tests (RBN dilution, user/edge 3:1 ratio, prestige, annual decay). **DAILY_REWARDS_SYSTEM.md** v3.1.0 spec (247 lines). | | No changes | **Sovereign Earnings → Sovereign Wallet** (merged, expandable notice). **CLAW tab → Settings** expander. **Messenger tabs** (6 web messengers, max 3 active). **Intelligent theme colors** (HSL). **FIFA themes** (18). **Optimistic file placeholders**. **Edit all themes**. **Terms of Use boot gate**. **Legal section merged**. **Points display** (not INTR). **Same-phone QR guide**. **iOS crash fix** (SharedPreferences disclaimer). | **SignedRewardEnvelope** (Ed25519). **Prestige tier on-chain verification**. **Rust-side tier computation**. **Lease validation** (real balance/timeout). **Escrow/RPC fixes**. | No changes |

## Key Networking Parameters (v34 = working baseline)

| Parameter | v34 (working) | v35/v36 (broken group chat) | v37 (restored) |
|-----------|---------------|----------------------------|----------------|
| Gossipsub heartbeat | **10s** | 30s | **10s** ✓ |
| Gossipsub max_transmit_size | **unlimited** | 1MB | **unlimited** ✓ |
| Request-response max | **10MB** | 2MB | **10MB** ✓ |
| Relay max_circuit_bytes | **1GB** | 100MB | **1GB** ✓ |
| Relay max_circuit_duration | **1 hour** | 30 min | **1 hour** ✓ |
| Relay max_reservations | **8192** | 256 | **8192** ✓ |
| Relay max_circuits | **4096** | 100 | **4096** ✓ |

## Lessons Learned

- v35 security hardening broke group chat by reducing gossipsub heartbeat (3x slower propagation) and capping transmit size (1MB silently drops messages)
- Direct P2P pipeline is locked — never modify the file transfer path
- FCM replaces polling in v34 — heartbeat 300s is for regular devices, anchor nodes keep 30s
- Always maintain this changelog at every stable backup to save debugging time
- GroupAction must NOT be Noise-encrypted — it's already AES-256-GCM encrypted with group secret. Double-encryption causes silent delivery failures when Noise session state is out of sync
- Gossipsub `propagation_source` is the RELAY peer, not the original author. Use `message.source.unwrap_or(propagation_source)` for membership verification
- `ApproveGroupJoin` must send `GroupInvite` (not just `GroupManifest`) — `GroupManifest` no longer carries the secret after security hardening
- Group creation must use `StoreInMailbox` (not `ForwardMeshSignaling`) for reliable invite delivery — fire-and-forget loses invites if direct delivery fails momentarily
- RBN self-relay guard: never construct relay paths through yourself — causes infinite OFFLINE loop
- Claude and Gemini AI were instrumental in debugging the group chat cascade — 10+ interrelated bugs fixed across multiple sessions
- **ListenerClosed auto-recovery** is vital for relay resilience: if a circuit listener closes due to a transient drop, immediately clearing records and registering a fresh listener ensures the node stays reachable over the RBN relay.
- UI elements containing multi-line variable data panels (like contact settings dialogs) must be wrapped in `SingleChildScrollView` to prevent layout boundaries from cracking on compact mobile viewports.
- **Overlapping Cupertino dialog transitions** on iOS throw assertion failures (`_dependents.isEmpty`) if a new route is pushed while a previous pop transition or keyboard collapse is still active. Consolidating sequential inputs and result views into a single `StatefulWidget` dialog with internal state machine transitions prevents overlapping Navigator actions and duplicate key collisions.
- **Asynchronous FFI database writes** require a slight timing delay (e.g. 600ms) before querying them on the Dart UI layer to prevent query timing races where stale data is returned before the transaction completes and flushes to disk.
- **PageView Keep-Alive FAB Hero Tag Collisions:** When using `AutomaticKeepAliveClientMixin` inside a `PageView` layout, multiple tabs/pages containing a `FloatingActionButton` are kept alive simultaneously. To prevent duplicate hero tag crashes (`multiple heroes share the same tag within a subtree`), all keep-alive tab FABs must explicitly set `heroTag: null`.

## Backup 07_26_0824 (2026-07-06 08:24)
- Git: main @ f62bc59
- Machine: devs-Mac-mini.local

## Backup 06_07_26_0827 (2026-07-06 08:27)
- Git: main @ f62bc59
- Machine: devs-Mac-mini.local

## Backup 06_07_26_0830 (2026-07-06 08:30)
- Git: main @ f62bc59
- Machine: devs-Mac-mini.local

## Backup 06_07_26_0834 (2026-07-06 08:34)
- Git: main @ f62bc59
- Machine: devs-Mac-mini.local

## Backup 06_07_26_0852 (2026-07-06 08:52)
- Git: main @ f62bc59
- Machine: devs-Mac-mini.local

## Backup 06_07_26_0950 (2026-07-06 09:50)
- Git: main @ f62bc59
- Machine: devs-Mac-mini.local

## Backup 06_07_26_1019 (2026-07-06 10:19)
- Git: main @ 2d44868
- Machine: devs-Mac-mini.local

## Backup 06_07_26_1109 (2026-07-06 11:09)
- Git: main @ 2239bd5
- Machine: devs-Mac-mini.local

## Backup 06_07_26_1435 (2026-07-06 14:35)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 06_07_26_1725 (2026-07-06 17:25)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 06_07_26_1939 (2026-07-06 19:39)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 06_07_26_1947 (2026-07-06 19:47)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 06_07_26_1954 (2026-07-06 19:54)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 07_07_26_0611 (2026-07-07 06:11)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 07_07_26_0636 (2026-07-07 06:36)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 07_07_26_0652 (2026-07-07 06:52)
- Git: main @ 6b4398c
- Machine: devs-Mac-mini.local

## Backup 07_07_26_1243 (2026-07-07 12:43)
- Git: main @ 1ed4e60
- Machine: devs-Mac-mini.local

## Backup 08_07_26_1830 (2026-07-08 18:30)
- Git: main @ 72a5880
- Machine: devs-Mac-mini.local

## Backup 08_07_26_1946 (2026-07-08 19:46)
- Git: main @ 9802c2a
- Machine: devs-Mac-mini.local

## Backup 08_07_26_2154 (2026-07-08 21:54)
- Git: main @ 9802c2a
- Machine: devs-Mac-mini.local

## Backup 09_07_26_0428 (2026-07-09 04:28)
- Git: main @ 7c8639f
- Machine: devs-Mac-mini.local

## Backup 09_07_26_0523 (2026-07-09 05:23)
- Git: main @ a6f18dd
- Machine: devs-Mac-mini.local

## Backup 09_07_26_0548 (2026-07-09 05:48)
- Git: main @ a6f18dd
- Machine: devs-Mac-mini.local

## Backup 09_07_26_0606 (2026-07-09 06:06)
- Git: main @ 95cf389
- Machine: devs-Mac-mini.local

## Backup 09_07_26_0614 (2026-07-09 06:14)
- Git: main @ 24b75ab
- Machine: devs-Mac-mini.local

## Backup 14_07_26_1707 (2026-07-14 17:07)
- Git: main @ 80c5445
- Machine: devs-Mac-mini.local

## v0.34.0 — Cross-Network File Transfer & VPN Stability (2026-07-14)

### Networking
- **Gossipsub file transfer fallback** — File chunks and requests routed through per-transfer gossipsub topics (`file-transfer-{transfer_id}`) instead of request_response. Works for both direct and relayed connections.
- **Gossipsub handler fix** — `file-transfer-*` topics bypass group membership check in gossipsub message handler.
- **Initial chunk requests** — Receiver sends chunk requests immediately when `IncomingTransfer` is created (no longer waits for stall watchdog).
- **RBN auto-subscribe** — RBN daemon subscribes to `file-transfer-*` gossipsub topics on first message, enabling cross-network file transfer relay.
- **Select-loop fix** — Command channel prioritized over swarm events in `tokio::select!` with `biased;` keyword. Prevents `HandleIncomingPayload` starvation.
- **VPN tunnel stability** — Increased tunnel stale thresholds: VPN 300s, MOBILE/WiFi 300s. VPN detection no longer resets working tunnels.
- **Mobile data fix** — Tunnel kept active on mobile data (type=2). Carriers often block direct connections to RBN.
- **RBN mailbox architecture preserved** — Jul 9 backup restored with mailbox system intact.

### Android Stability
- **Foreground service type** — `startForegroundCompat()` with `FOREGROUND_SERVICE_TYPE_SPECIAL_USE` for API 29+.
- **Background FGS exception** — `ForegroundServiceStartNotAllowedException` catch for API 31+.
- **Battery optimization removed** — `requestBatteryOptimizationExemption()` removed (Google Play policy).
- **FFI panic safety** — `ffi_catch!` macro wrapping all 170 extern "C" functions.
- **Safe unwrap** — Replaced risky `.unwrap()` on `get_group()` with safe match.

### Token
- **V2 migration re-applied** — All client + RBN code updated with V2 mint `FhKJjqpsCbymrk4Ntv5jFyZihHsAkW4Fb4fuJYBniydP`.
- **RBN contact removed** — DB cleanup + registry filter prevents RBN from appearing as user contact.

### Infrastructure
- **RBN deployed** — `introvertd` with mailbox + V2 + file-transfer gossipsub on Alibaba (47.89.252.80).
- **Economy daemon** — Disabled (treasury needs SOL funding for circuit breaker).

### Additional Fixes (2026-07-14 session)
- **Networking architecture redesign** — 3-tier progression: Direct P2P → Relay → VPN Tunnel
- **VPN detection removed** — `connectivity_plus` VPN detection had false positives. Removed from `connectivity_listener.dart` and `SetConnectivityType` handler.
- **TLS→plaintext tunnel fallback** — Tunnel tries `wss://443` first, falls back to `ws://80` on failure.
- **Relay circuit stability** — Don't remove `relay_reservations` on `ListenerClosed`/`ListenerError`.
- **File transfer pacing** — 256KB chunks, 8 in-flight, 500ms initial delay (~10x throughput).
- **Relay reservation timer** — Reduced from 30s to 10s for faster relay establishment.
- **Command drain loop** — `try_recv()` drain before `tokio::select!` prevents HandleIncomingPayload starvation.
- **Expert consultation document** — `Docs/NETWORK_ARCHITECTURE_EXPERT_CONSULTATION.md` created.

## Backup 14_07_26_1926 (2026-07-14 19:26)
- Git: main @ 80c5445
- Machine: devs-Mac-mini.local

## Backup 15_07_26_0553 (2026-07-15 05:53)
- Git: main @ 80c5445
- Machine: devs-Mac-mini.local

## Backup 16_07_26_1541 (2026-07-16 15:41)
- Git: main @ d11ecc7
- Machine: devs-Mac-mini.local

## Backup 16_07_26_1652 (2026-07-16 16:52)
- Git: main @ d5a7c9c
- Machine: devs-Mac-mini.local

## Backup 16_07_26_2026 (2026-07-16 20:26)
- Git: main @ 521d315
- Machine: devs-Mac-mini.local

## Backup 17_07_26_0422 (2026-07-17 04:22)
- Git: main @ 521d315
- Machine: devs-Mac-mini.local

## Backup 17_07_26_1535 (2026-07-17 15:35)
- Git: main @ 67e7334
- Machine: devs-Mac-mini.local

## Backup 18_07_26_1727 (2026-07-18 17:27)
- Git: main @ 2d591a0
- Machine: devs-Mac-mini.local

## Backup 18_07_26_1744 (2026-07-18 17:44)
- Git: main @ 2d591a0
- Machine: devs-Mac-mini.local

## Backup 18_07_26_1927 (2026-07-18 19:27)
- Git: main @ 2d591a0
- Machine: devs-Mac-mini.local

## Backup 19_07_26_1217 (2026-07-19 12:17)
- Git: main @ 07aedda
- Machine: devs-Mac-mini.local

## v0.36.0 — Economy Fixes & Referral System (2026-07-22)

### Economy Phase 1: Mint Address Unification
- V2 mint `FhKJjqpsCbymrk4Ntv5jFyZihHsAkW4Fb4fuJYBniydP` canonical across all code
- Runtime + compile-time assertions prevent drift between 3 code locations
- V1 confirmed stale (1,686 residual INTR)

### Economy Phase 2: Server-Side Prestige Tier Verification
- RPC-derived tier overrides client-claimed tier at epoch close
- RPC failure fallback defaults to tier 0 (not client value)
- Min-hold-duration snapshot gate closes flash-fund exploit
- Growth-recovery test confirms no permanent ratchet

### Economy Phase 3: Pool-Cap Overshoot Fix
- Prestige multiplier applied BEFORE normalization (weight-then-normalize)
- sum(payouts) <= pool cap verified in all cases (all-tier-4, mixed, all-tier-0)

### Economy Phase 4: Epoch Reconciliation
- `ON CONFLICT (epoch_id, solana_wallet)` with full-row replace
- Keep-highest logic: higher total_points wins, loser's data fully discarded
- Duplicate-wallet test confirms correct behavior

### Referral System
- **25/75 pool split** — 25% of edge/user pool reserved for referrals (4,109.50 INTR/day Year 1)
- **3-in-7 eligibility gate** — wallet must be active 3 distinct calendar days in 7 days
- **Peer-subset exclusion** — referred wallets with peers fully contained in referrer's peers are excluded
- **Tier bands** — 1-2 new referrals = Catalyst (2x), 3+ = Pulsar (3x)
- **No-stacking effective multiplier** — `max(balance_mult, referral_mult)`, not product
- **Pro-rata pool overflow** — if bonuses exceed referral pool, all scaled down proportionally
- **Double-claim guard extended** — `(peer_id, epoch_id, claim_type)` key supports DailySettlement + ReferralBonus per wallet per epoch
- **RBN to client sync** — TelemetryAck carries referral status fields, client mirrors to local tables
- **Flutter UI** — referral status card in Settings > Points (distribution_in_progress, tier_active, none)

### Technical Details
- 21 RBN economy tests + 5 Solana daemon tests, all passing
- `subtle` pinned to 2.4.1 for solana-zk-token-sdk compatibility
- TTL purge skipped in test_mode (test data uses 2009 timestamp)
- JSON wire format confirmed (serde_json), no `deny_unknown_fields` — old clients safe

### Deferred (not blocking)
- Bounded-concurrency RPC batching (serial acceptable at <50 wallets)
- Referral anti-farm hardening (rolling 30-day cap)
- `distribution_in_progress` UI state doesn't reflect on-chain confirmation (cosmetic)

## v0.35.0 — Ghost Message Fix & Mailbox Sync Hardening (2026-07-19)

### Ghost Message Fix
- **Persisted pull-attempt state** — `_pullRequested`/`_pullRequestedAt` moved from ephemeral widget-scoped sets to SharedPreferences, surviving chat close/reopen and app restart
- **Retry ceiling** — max 5 attempts per transfer (matching Rust `pending_file_chunks` cap), with exponential backoff (30s → 2m → 10m → 30m → stop)
- **Terminal UI state** — failed/expired transfers render as greyed-out "Failed to download — tap to retry" bubble; tap resets counter and retries once
- **Incremental merge in `_loadMessages`** — replaced `_messages.clear(); _messages.addAll(loaded)` with diff-based update-in-place; prevents re-triggering pull eligibility for all historical transfers on every reload
- **Targeted `[FILE]:` update** — `[FILE]:` events now update the specific transfer entry in place instead of triggering a full `_loadMessages()` reload

### Status 5 (Failed/Expired)
- **New terminal status code 5** — marks file transfers that have exhausted retries or exceeded 7-day TTL
- **`mark_file_transfer_failed`** — raw SQL update, preserves status 1 (delivered) and 2 (read)
- **`sweep_expired_file_transfers`** — periodic sweep (piggybacked on status_check_interval) marks incomplete file messages older than 7 days as failed; excludes completed transfers
- **`complete_file_transfer_recovery`** — transitions status 5 → 1 on successful file completion; preserves status 2 (read)
- **Chunk-retry bridge** — when `increment_chunk_retry` hits 5-attempt cap, parent message is now marked as status 5

### Backfill Flag (is_backfill)
- **Wire-level flag** — `ChatMessage` and `SyncMessage` payloads carry `is_backfill: bool` (serde default false)
- **MailboxDrained handler** — marks all drained messages as `is_backfill = true`
- **ChatSyncResponse handler** — marks all sync messages as `is_backfill = true`
- **Event format versioned** — Event 2 dispatch uses `[0x01][timestamp][...][is_backfill_byte]` format; legacy format auto-detected
- **Flutter parser** — detects version byte, extracts backfill flag; backfill messages skip read receipts and scroll-to-bottom

### Read Receipt Gating
- **Backfill suppression** — read receipts only sent for live (non-backfill) messages
- **`_markMessagesAsRead` bulk fix** — persisted `last_read_receipt_{peerId}` timestamp via SharedPreferences; only sends receipts for messages newer than last receipt batch
- **Local state clear preserved** — `updateMessageStatusForPeer(peerId, 0)` still runs unconditionally on chat open

### Drain Efficiency
- **`drain_in_progress` per-anchor** — `HashSet<PeerId>` prevents concurrent drain requests to same RBN; cleared on response or `OutboundFailure`
- **`last_empty_drain` per-anchor** — `HashMap<PeerId, Instant>` tracks empty drain responses; fast-poll skip only fires when ALL connected anchors had empty drains recently
- **Batch size 4 → 8** — `fetch_mailbox_payloads` LIMIT increased (libp2p max confirmed at 10MB)
- **7-day file transfer sweep** — expired incomplete transfers marked as failed in periodic cleanup

### Cleared-Chat Race Fix (P1)
- **Clear-guard re-check on queue pop** — `should_skip_mailbox_message` re-checked when dequeuing from `handle_signaling_payload` queue, catching `delete_chat` calls between initial guard and dispatch
- **`ClearPendingMessages` command** — new FFI function `introvert_network_clear_pending_messages` clears outbound `pending_messages` buffer for a peer; called after `deleteChat` in Flutter

### Sync Timeout
- **`sync_in_progress` timeout 60s → 120s** — extended for large history syncs
- **Recursive sync guard** — time-based cap (120s) prevents infinite recursive `SyncChatMessages` loops

### Deferred
- **P4 (auto-recovery on sync)** — `ChatSyncResponse` still drops `[FILE]:` messages. Infrastructure for status 4 removed (zero callers). Deferral documented at drop point with §5.2 shared-retry-ceiling requirement noted.

## Backup 19_07_26_1430 (2026-07-19 14:30)
- Git: main @ cbd5f0f (uncommitted: ghost message fix + mailbox hardening)
- Machine: devs-Mac-mini.local

## Backup 20_07_26_1428 (2026-07-20 14:28)
- Git: main @ cbd5f0f
- Machine: devs-Mac-mini.local

## Backup 20_07_26_1445 (2026-07-20 14:45)
- Git: main @ cbd5f0f
- Machine: devs-Mac-mini.local

## Backup 20_07_26_1723 (2026-07-20 17:23)
- Git: main @ f4ad546
- Machine: devs-Mac-mini.local

## Backup 21_07_26_0555 (2026-07-21 05:55)
- Git: main @ 81a0c21
- Machine: devs-Mac-mini.local

## Backup 21_07_26_1747 (2026-07-21 17:47)
- Git: main @ 6eda1d0
- Machine: devs-Mac-mini.local

## Backup 22_07_26_1053 (2026-07-22 10:53)
- Git: main @ a8b2597
- Machine: devs-Mac-mini.local

## Backup 23_07_26_0348 (2026-07-23 03:48)
- Git: main @ fa0b6e0
- Machine: devs-Mac-mini.local

## Backup 23_07_26_0524 (2026-07-23 05:24)
- Git: main @ fa0b6e0
- Machine: devs-Mac-mini.local
