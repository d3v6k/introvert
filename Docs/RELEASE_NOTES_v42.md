# Release Notes: Stable v42 — "Reliable Push Wakeups"
**Date:** June 28, 2026
**Version:** `0.17.0`
**Predecessor:** stable_v41 (`0.16.0`)

---

## 1. Executive Summary

Stable v42 ("Reliable Push Wakeups") addresses a critical background synchronization bug and protocol race condition affecting Android clients:
1.  **FCM Token Evaluation on Android (Fixed):** Resolved an issue in the Flutter client where push notifications availability was evaluated solely against Apple APNS tokens. On Android, this caused push notifications to be reported as disabled, triggering a fallback battery-intensive polling loop instead of utilizing push wakeups.
2.  **Identify-Level Push Registration (Fixed):** Shifted the sleep-state push registration logic (`IdentifySleepState`) from the raw libp2p `ConnectionEstablished` event to the `Identify` event handler. This guarantees that client FCM/APNS tokens are only transmitted to the RBN nodes after signaling capability negotiation has successfully completed.
3.  **FFI Binary Compilation Pipeline Integration:** Recompiled client target architectures (`arm64-v8a` and `x86_64`) and packaged them into the Android JNI folders (`android/app/src/main/jniLibs`).
4.  **RBN Server Updates:** Deployed matching `Identify` registration code and restarted the remote RBN daemon (`introvertd`) on Alibaba Cloud.

---

## 2. File Manifest

### Modified Client Core Files
*   [src/network/mod.rs](file:///Users/dev/Development/introvert/src/network/mod.rs)
    *   **Identify Handshake Sync:** Moved push token registration logic from `ConnectionEstablished` to `Identify(identify::Event::Received)`.
*   [lib/src/native/alert_service.dart](file:///Users/dev/Development/introvert/lib/src/native/alert_service.dart)
    *   **FCM Availability:** Corrected token evaluation checks for Android FCM tokens.
*   [lib/src/ui/main_shell.dart](file:///Users/dev/Development/introvert/lib/src/ui/main_shell.dart)
    *   **Background sync startup:** Updated startup check to activate background push sync on Android.
*   [pubspec.yaml](file:///Users/dev/Development/introvert/pubspec.yaml) / [Cargo.toml](file:///Users/dev/Development/introvert/Cargo.toml)
    *   Bumped version to `0.17.0`.

### Modified RBN Daemon Files
*   [for_linux/src/network/mod.rs](file:///Users/dev/Development/introvert/for_linux/src/network/mod.rs)
    *   **Identify Handshake Sync:** Moved token registration from `ConnectionEstablished` to `Identify` to match client-side updates.
*   [for_linux/Cargo.toml](file:///Users/dev/Development/introvert/for_linux/Cargo.toml)
    *   Bumped version to `0.17.0`.

---

## 3. Rebuild From Scratch Guide

### Prerequisites
*   Rust: `rustup target add aarch64-linux-android x86_64-linux-android`
*   Flutter: Dart SDK >=3.3.0
*   Android NDK: Version 28.2.13676358

### Rebuild Core & Flutter UI
1.  **Build macOS Native Library:**
    ```bash
    make mac
    ```
2.  **Build Android `.so` Libraries:**
    ```bash
    ./scripts/build_android.sh
    ```
3.  **Run Application:**
    ```bash
    flutter run -d SM-S908E
    ```

---

## 4. RBN Daemon Compilation & Deployment

To compile and run the RBN daemon for Linux servers:
1.  **Compile and Deploy RBN binary:**
    ```bash
    ./deploy_rbn.sh
    ```
