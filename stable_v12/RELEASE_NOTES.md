# Release Notes - Stable v12

- **Extreme Reliability Cross-Network File Transfer**: 
    - Reduced relay chunk size to **16KB** and increased pacing to **250ms** for maximum NAT/firewall traversal stability.
    - Implemented **Redundancy Filtering** in the RAM buffer (`pending_messages`) to prevent thundering herd congestion during network dips.
    - Switched to a **2-deep sliding window** for relayed transfers to ensure thin-pipe stability.
- **Seeding Lifecycle Mandates**:
    - **1-to-1 Transfers**: Seeding now explicitly stops once the receiver verifies the file, taking it off the mesh to preserve privacy.
    - **Group Transfers**: Sender and receivers continue seeding to ensure mesh-wide availability for all group members.
- **Strategic Relay Dialing**:
    - Centralized dialing into `dial_relay_path` with a **5s per-peer cooldown**.
    - Restored **Multi-RBN Redundancy**: Now dials all port 443 bootstrap nodes simultaneously to ensure a path is found even if primary RBN is busy.
- **Branding Update**:
    - Differentiated **App Icon** (World Globe) for system integration vs. **Home Logo** (Stylized Text) for in-app UI.
    - Updated icons for Android, iOS, macOS, Windows, and Web.
- **Protocol Hardening**:
    - Strictly excluded all file data from WebRTC on relay connections to utilize the more robust libp2p `request_response` stack.
    - Enabled **Auto-Pull** mode to allow transfers to self-heal and resume automatically if connection conditions change mid-transfer.
    - Reduced `request_response` timeouts to **20s** for 3x faster failure detection and circuit failover.
