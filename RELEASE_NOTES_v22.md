# Introvert Stable Release v22 Notes & RBN Deployment Guide

## 1. Version Release Notes
This release represents a fully functional, highly optimized, and audited version of the Introvert privacy-focused P2P communication system.

### Key Resolved Issues
*   **Direct P2P & Group Chat File Sharing Restored**: Fixed critical pacing bugs and dynamic timing overrides where direct transfers were demoted to slow 250ms relay pacing. Re-established the high-speed 20ms pacing delay for direct transfers, restoring P2P speeds to 70+ Mbps.
*   **Watchdog Recovery & Push Protection**: Added watchdog protections to prevent premature pull triggers on active direct streams, while adding a 10s timeout fallback to transition stalled direct transfers into pull recovery mode cleanly.
*   **FFI Swarm Instance Guard**: Fixed FFI multi-loop duplicates where calling `startNetwork` spawned multiple competing tokio loops, splitting packets and causing high-speed stalls. Added static active swarm tracking to immediately exit if a loop is already running.
*   **Database & Event Isolation**: Fixed chat context file leakage where group files were rendering in 1-on-1 screens. Separated data persistence between `messages` and `group_messages` databases and added `groupId` checks to the UI event listeners.
*   **Layout Overflow Fixes**: Prevented layout overflow errors in `MediaGalleryViewer` by making the indicator dots scrollable via a horizontal `SingleChildScrollView` wrapped in a `ConstrainedBox`, with automatic active-dot centering animations.
*   **Embedded Minimalistic Audio Player**: Implemented an interactive audio player inside `FileTransferBubble` for completed audio files, supporting play, pause, stop, and slider-seeking.

---

## 2. RBN (Root Bootstrap Node) Compilation Guide
The production RBN daemon (`introvertd`) manages client discovery and DHT bootstrap operations. Due to the 1GB RAM limitation on the Alibaba RBN host, compiling directly on the RBN server will cause Out-Of-Memory (OOM) compiler crashes. 

**Rule:** Always compile on a build machine with >2GB RAM (e.g., a local Ubuntu machine or Thinkpad build target).

### Compilation Steps
1. Navigate to the RBN source tree:
   ```bash
   cd for_linux/
   ```
2. Run the build script to compile the native Linux ELF release binary:
   ```bash
   ./build_linux.sh
   ```
   Alternatively, compile using cargo directly:
   ```bash
   cargo build --release --bin introvertd
   ```
3. Locate the compiled Linux ELF binary at `target/release/introvertd`.

---

## 3. RBN Deployment & Update Steps
To update the running daemon on the production RBN host, perform the following steps:

1. **Stop the Running Daemon on RBN**:
   Connect via SSH and stop the `introvertd` systemd service:
   ```bash
   ssh root@47.89.252.80 "systemctl stop introvertd"
   ```
2. **Transfer the Compiled Binary**:
   Secure-copy the compiled ELF binary from your build workspace to the RBN directory:
   ```bash
   scp target/release/introvertd root@47.89.252.80:/opt/introvert/bin/introvertd
   ```
3. **Restart the Service**:
   Reload the systemd daemon config and start the service:
   ```bash
   ssh root@47.89.252.80 "systemctl daemon-reload && systemctl start introvertd"
   ```
4. **Verify Daemon Liveness**:
   Confirm that the service is successfully running and active:
   ```bash
   ssh root@47.89.252.80 "systemctl is-active introvertd"
   ```
