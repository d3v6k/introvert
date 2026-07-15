---
name: build-verify
description: "Multi-platform build and verification pipeline. Chains Rust compilation check, Flutter analysis, native library builds, and Flutter app builds across macOS/Android/iOS/Linux. Use after code changes to verify everything compiles before committing or deploying."
---

# Build & Verify Pipeline

Run after any code change to verify compilation across all targets. Reports errors at each stage and stops early on failure.

## Prerequisites

- Rust toolchain installed
- Flutter SDK installed
- For Linux builds: `dev@thinkpad.local` SSH access
- For iOS: Xcode + iOS SDK

## Steps

### 1. Rust Compilation Check

```bash
# Client library (main src/)
cd /Users/dev/Development/introvert && cargo check --lib 2>&1 | grep -E "^error" | head -10

# RBN daemon (for_linux/)
cd /Users/dev/Development/introvert/for_linux && cargo check 2>&1 | grep "^error" | head -10
```

If either reports errors → fix before proceeding.

### 2. Flutter Analysis

```bash
cd /Users/dev/Development/introvert && flutter analyze 2>&1 | grep -E "error.*lib/(views|src|blueprint)" | head -10
```

If errors reported → fix before proceeding.

### 3. Native Library Builds (per platform)

Use Makefile targets:

```bash
cd /Users/dev/Development/introvert

# macOS native library
make mac 2>&1 | tail -5

# Android native libraries (arm64 + x64)
make android 2>&1 | tail -5

# iOS static libraries (device + simulator)
make ios 2>&1 | tail -5
```

### 4. Flutter App Builds

```bash
cd /Users/dev/Development/introvert

# macOS app
flutter build macos --release 2>&1 | tail -5

# Android APK
flutter build apk --release 2>&1 | tail -5

# Android split APK (smaller per-arch)
flutter build apk --split-per-abi --release 2>&1 | tail -5
```

### 5. Linux Build (remote, optional)

Requires SSH to thinkpad:

```bash
ssh dev@thinkpad.local "export PATH=/home/dev/flutter/bin:\$HOME/.cargo/bin:\$PATH && cd ~/introvert && cargo build --release --lib 2>&1 | tail -2 && flutter build linux --release 2>&1 | tail -3"
```

### 6. iOS Build (optional)

```bash
cd /Users/dev/Development/introvert
IPHONEOS_DEPLOYMENT_TARGET=13.0 cargo build --release --target aarch64-apple-ios 2>&1 | tail -3
IPHONEOS_DEPLOYMENT_TARGET=13.0 cargo build --release --target aarch64-apple-ios-sim 2>&1 | tail -3
mkdir -p ios/libs
cp target/aarch64-apple-ios/release/libintrovert.a ios/libs/libintrovert_device.a
cp target/aarch64-apple-ios-sim/release/libintrovert.a ios/libs/libintrovert_simulator.a
```

## Quick Commands

| Scope | Command |
|-------|---------|
| Check only (fast) | `cargo check --lib && flutter analyze` |
| macOS only | `make mac && flutter build macos --release` |
| Android only | `make android && flutter build apk --release` |
| Full local | Steps 1–4 |
| Full + remote | Steps 1–5 |

## Rules

- Always state which build is required after code changes: Rust → `make all && flutter run`, Dart only → `flutter run`, docs only → no rebuild.
- Never compile on the Alibaba RBN server (1GB RAM constraint).
- Stop on first error — do not continue builds if compilation check fails.
