# Introvert Stable Release v29 Notes & RBN Deployment Guide

## 1. Version Release Notes
**Version:** 0.6.0 | **Date:** 2026-06-18 | **Stable:** v29

This release adds voice memo recording, custom theme wallpapers, message forwarding, reply privately, and several critical bug fixes across Android and macOS platforms.

### New Features
* **Voice Memo Recording**: Record voice memos in AAC-LC format directly in chat. Audio is encoded at 44.1kHz, 128kbps. Files auto-named `voice_memo_{duration}s.m4a` and copied to app documents.
* **Custom Theme Wallpapers**: Set a wallpaper image for any custom theme. Images are auto-resized to 720px wide at JPEG quality 80 to prevent memory issues. Adjustable opacity slider (0-100%).
* **Message Forwarding**: Forward messages to any contact or group from both 1:1 and group chat contexts via long-press menu.
* **Reply Privately**: In group chats, reply to a message sender in a 1:1 chat. Only visible when a direct 1:1 connection exists. Original message is quoted in the reply using markdown blockquote syntax. Uses custom user-arrow icon.
* **flutter_svg Dependency**: Added for custom Reply Privately icon rendering.

### Bug Fixes
* **Android Voice Memo Crash**: Fixed `FileNotFoundException` caused by empty string path passed to `MediaMuxer`. Now uses auto-generated temp path.
* **macOS Voice Memo Playback**: Fixed `AVPlayerItem.Status.failed` by switching from `UrlSource` to `DeviceFileSource`.
* **1 Jan 1970 Date Separator**: Fixed file transfers with null/zero `start_time_ms` displaying epoch dates. Now falls back to current timestamp.
* **Custom Theme Wallpaper Not Showing**: `SovereignWallpaper` converted from `StatelessWidget` to `StatefulWidget` that listens to `AppTheme` `ChangeNotifier`.
* **Edit Theme Wallpaper Loss**: Theme edit flow now always calls `setTheme()` after saving, not just when name matches.

### Architecture Changes
* `ThemeConfig` now includes `wallpaperPath` (nullable String) and `wallpaperOpacity` (double, default 0.3). Backward compatible with existing themes.
* `FileTransferProgress.fromJson` guards against null/zero `start_time_ms` values.
* Wallpaper images stored in `app_documents/wallpapers/` directory, auto-resized on pick.

---

## 2. RBN (Root Bootstrap Node) Compilation Guide
The production RBN daemon (`introvertd`) manages client discovery and DHT bootstrap operations. Due to the 1GB RAM limitation on the Alibaba RBN host, compiling directly on the RBN server will cause Out-Of-Memory (OOM) compiler crashes.

**Rules for Compilation**:
* Always cross-compile the RBN binary using the `deploy_local_rbn.sh` script or on a build machine with >2GB RAM.
* Build native ELF (Linux) binaries using the `for_linux/` source tree.

### Compilation Steps (Local Cross-Compilation):
1. Install cross-compilers:
   ```bash
   brew install zig
   cargo install cargo-zigbuild
   ```
2. Build and deploy:
   ```bash
   ./deploy_local_rbn.sh
   ```

### Compilation Steps (On Build Machine with >2GB RAM):
1. Sync source files to target build machine:
   ```bash
   scp -r for_linux/src/ for_linux/Cargo.toml for_linux/Cargo.lock dev@buildmachine.local:~/introvert/for_linux/
   ```
2. Build release binary:
   ```bash
   ssh dev@buildmachine.local "export PATH=\$HOME/.cargo/bin:\$PATH && cd ~/introvert/for_linux && cargo build --release --bin introvertd"
   ```

---

## 3. RBN Service Daemon Update & Deployment Guide
To safely deploy the updated daemon on the production RBN server without losing state:

1. Stop the running service:
   ```bash
   ssh root@47.89.252.80 "systemctl stop introvertd"
   ```
2. Deploy compiled binary:
   ```bash
   scp target/x86_64-unknown-linux-gnu/release/introvertd root@47.89.252.80:/opt/introvert/bin/introvertd
   ```
3. Reload service configs and restart:
   ```bash
   ssh root@47.89.252.80 "systemctl daemon-reload && systemctl start introvertd"
   ```
4. Verify daemon logs:
   ```bash
   ssh root@47.89.252.80 "journalctl -u introvertd -n 100 -f"
   ```

---

## 4. Complete File Manifest

### New Files
* `assets/images/reply_privately.svg` - Custom Reply Privately icon

### Modified Files
* `lib/theme/app_theme.dart` - Added wallpaperPath, wallpaperOpacity to ThemeConfig
* `lib/blueprint_ui.dart` - SovereignWallpaper converted to StatefulWidget with wallpaper support
* `lib/views/chat_screen.dart` - Voice memo recording, Forward in toolbar, audio file path fix
* `lib/views/group_chat_screen.dart` - Reply Privately, Forward in toolbar, voice memo recording
* `lib/views/chat_features.dart` - DeviceFileSource for voice memo playback
* `lib/src/ui/custom_theme_creator.dart` - Wallpaper picker, resize, opacity slider
* `lib/src/ui/main_shell.dart` - Edit theme flow always applies saved theme
* `lib/src/native/introvert_client.dart` - FileTransferProgress timestamp guard
* `lib/src/ui/widgets/file_transfer_bubble.dart` - (no changes, reference only)
* `pubspec.yaml` - Version 0.6.0, flutter_svg dependency
* `Cargo.toml` - Version 0.6.0
* `Docs/CHANGELOG.md` - v0.6.0 entry
* `Docs/STABLE_VERSION_PROCESS.md` - Updated latest version reference

---

## 5. Build from Scratch

### Prerequisites
* Flutter SDK >= 3.3.0
* Rust toolchain (stable)
* Android SDK (API 33+)
* Xcode (for iOS/macOS)
* `cargo-zigbuild` (for cross-compilation)

### Android Build
```bash
flutter build apk --release
```

### macOS Build
```bash
flutter build macos --release
```

### RBN Binary
```bash
./deploy_local_rbn.sh
```

---

## 6. Architecture Reference

### FFI Bridge
Flutter ↔ Rust via dart:ffi. Key functions in `lib/src/native/introvert_client.dart` call into `libintrovert.dylib` (macOS) or `libintrovert.so` (Linux).

### Event System
Event types: 10=Swarm, 12=Transfer, 21=Sync, 23=Reload, 33=Kademlia, 39=Typing.

### Database
SQLite via Rust FFI. Tables: messages, contacts, groups, group_members, call_history, drive_files.

### Network Stack
libp2p with Kademlia DHT, GossipSub mesh, Noise encryption, QUIC/TCP transports, mDNS local discovery, WebSocket tunneling, relay circuit v2.
