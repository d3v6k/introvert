# Stable Version Creation Process

This document describes how stable versions of Introvert are created, backed up, and documented. Follow this process exactly when saving a new stable version.

---

## Overview

Stable versions are snapshot backups of the entire codebase saved to an external drive. Each version is a self-contained directory that can be used to rebuild the app from scratch without access to the git repository.

**Backup Location:** `/Volumes/512-SSD-External/introvert back up/`
**Naming Convention:** `stable_v{N}/` (directory) or `stable_v{N}.zip` (compressed)
**Current Latest:** v28

---

## Step-by-Step Process

### 1. Determine Version Number

Check the backup location for the latest stable version:
```bash
ls -d /Volumes/512-SSD-External/introvert\ back\ up/stable_v*/
```
The next version is `N = latest + 1`.

### 2. Create Backup Directory

```bash
mkdir -p "/Volumes/512-SSD-External/introvert back up/stable_v{N}"
```

### 3. Copy Source Files

#### Dart Source (lib/)
Copy ALL of these directories/files:
```
lib/main.dart
lib/blueprint_ui.dart
lib/theme/app_theme.dart
lib/views/theme_mockup_grid.dart
lib/views/chat_screen.dart
lib/views/group_chat_screen.dart
lib/views/call_screen.dart
lib/views/group_call_screen.dart
lib/views/profile_screen.dart
lib/views/contact_screen.dart
lib/views/wallet_dashboard.dart
lib/src/ui/main_shell.dart
lib/src/ui/notes_tab.dart
lib/src/ui/drive_tab.dart
lib/src/ui/connection_diagnostics_overlay.dart
lib/src/ui/onboarding_screen.dart
lib/src/ui/video_player.dart
lib/src/ui/update_service.dart
lib/src/ui/widgets/rewards_hud.dart
lib/src/ui/widgets/security_shield.dart
lib/src/ui/widgets/network_optimization_button.dart
lib/src/ui/widgets/sovereign_avatar.dart
lib/src/ui/widgets/file_transfer_bubble.dart
lib/src/ui/widgets/image_stack_bubble.dart
lib/src/ui/widgets/call_widgets.dart
lib/src/services/webrtc_call_service.dart
lib/src/services/group_call_service.dart
lib/src/services/background_sync_service.dart
lib/src/services/network_quality_service.dart
lib/src/repository/sync_repository.dart
lib/src/native/introvert_client.dart
lib/src/native/alert_service.dart
lib/src/native/identity_manager.dart
```

#### Rust Source (src/)
Copy ALL of these:
```
src/lib.rs
src/main.rs
src/storage.rs
src/identity.rs
src/network/mod.rs
src/network/behaviour.rs
src/network/config.rs
src/network/noise_session.rs
src/network/wormhole.rs
src/economy/mod.rs
src/economy/solana.rs
src/media/mod.rs
Cargo.toml
Cargo.lock
```

#### Android/Kotlin
```
android/app/src/main/kotlin/chat/introvert/app/IntrovertService.kt
android/app/src/main/kotlin/chat/introvert/app/MainActivity.kt
android/app/build.gradle.kts
android/build.gradle.kts
android/app/src/main/AndroidManifest.xml
android/gradle.properties
```

#### Configuration
```
pubspec.yaml
pubspec.lock
analysis_options.yaml
```

#### Build Scripts
```
scripts/build_android.sh
deploy_local_rbn.sh
deploy_rbn.sh
introvertd.service
Makefile
```

#### RBN Code (for_linux/)
Copy the ENTIRE directory:
```
for_linux/Cargo.toml
for_linux/Cargo.lock
for_linux/build_linux.sh
for_linux/introvertd.service
for_linux/src/main.rs
for_linux/src/lib.rs
for_linux/src/storage.rs
for_linux/src/identity.rs
for_linux/src/network/mod.rs
for_linux/src/network/behaviour.rs
for_linux/src/network/config.rs
for_linux/src/network/noise_session.rs
for_linux/src/network/wormhole.rs
```

#### Documentation
Copy ALL files from Docs/:
```
Docs/CHANGELOG.md
Docs/INTROVERT_MASTER_PLAN.md
Docs/ARCHITECTURE_BLUEPRINT.md
Docs/BUILD_&_DEPLOYMENT_GUIDE.md
Docs/FILE_TRANSFER_PROTOCOL.md
Docs/INTROVERT_ECONOMY_BLUEPRINT.md
Docs/MESH_STRESS_TEST_REPORT_*.md
Docs/MODULE_REFERENCE.md
Docs/NETWORKING_&_SIGNALING.md
Docs/PROTOCOL_SPECIFICATION.md
Docs/REBUILD_GUIDE.md
Docs/SECURITY_&_ENCRYPTION.md
Docs/UI_COMPONENT_MANIFEST.md
Docs/RELEASE_NOTES_v*.md
```

Also copy:
```
README.md
GEMINI.md
INTROVERT_MASTER_PLAN.md
```

#### Tests
```
tests/*.rs (all Rust test files)
```

#### Stable Backup Copies (*.stable)
Copy all `.stable` files from the project root — these are point-in-time snapshots:
```
*.stable (all files with this extension)
```

### 4. Create Release Notes

Create `RELEASE_NOTES_v{N}.md` in the backup directory with:
1. Version number and date
2. Summary of all changes (new features, bug fixes, breaking changes)
3. Complete file manifest (new files, modified files)
4. Build from scratch guide (prerequisites, step-by-step for each platform)
5. Architecture reference (FFI bridge, event system, database, network stack)
6. RBN deployment guide

### 5. Zip the Backup (Optional but Recommended)

```bash
cd "/Volumes/512-SSD-External/introvert back up"
zip -r stable_v{N}.zip stable_v{N}/
```

### 6. Update Version Numbers

Update version in:
- `pubspec.yaml` → `version: 0.{X}.0`
- `Cargo.toml` → `version = "0.{X}.0"`

### 7. Commit to Git

```bash
git add -A
git commit -m "v{N}: <summary of changes>"
git tag -a v{N} -m "Stable release v{N}"
```

---

## File Naming Conventions

| Pattern | Meaning |
|---------|---------|
| `stable_v{N}/` | Directory with all source files for version N |
| `stable_v{N}.zip` | Compressed archive of the above |
| `*.stable` | Point-in-time backup of a specific file |
| `RELEASE_NOTES_v{N}.md` | Release notes for version N |
| `CHANGELOG.md` | Cumulative changelog (semantic versioning) |

---

## Version Numbering

- **Sequential** (v22, v25, v28): Used for backup directories and release notes files
- **Semantic** (0.1.0, 0.4.0, 0.5.0): Used in pubspec.yaml, Cargo.toml, CHANGELOG.md
- Both systems run in parallel — a backup can be "stable_v28" while the code version is "0.5.0"

---

## What Makes a Version "Stable"

A version is considered stable when:
1. All known critical bugs are fixed
2. The app builds successfully on all target platforms (Android, iOS, macOS, Linux)
3. Core functionality works: messaging, file transfer, group chat, calls
4. Documentation is up to date
5. No regressions from previous stable version

---

## Memory Notes

This process was established on 2026-06-16. Key facts:
- External backup drive: `/Volumes/512-SSD-External/introvert back up/`
- Latest stable as of 2026-06-16: v28 (version 0.5.0)
- The `.stable` files in the project are NOT the same as the `stable_v{N}/` directories on the external drive
- `.stable` files are in-repo point-in-time copies; `stable_v{N}/` are complete external backups
