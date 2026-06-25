# Introvert Version Changelog

_Stable version history with key changes. Updated at every stable backup._

| Version | Date | Codename | Networking | UI/UX | Security | Economy |
|---------|------|----------|-----------|-------|----------|---------|
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

