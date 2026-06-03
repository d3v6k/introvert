# Rebuild Guide: From Zero to Sovereignty

This guide provides a linear, step-by-step process to rebuild the entire Introvert system from source code.

## Step 1: Environment Setup
1.  **Install Rust:** `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.
2.  **Add Targets:** `rustup target add aarch64-linux-android x86_64-linux-android`.
3.  **Install Flutter:** Download from `flutter.dev`, add to PATH, and run `flutter doctor`.
4.  **Install Android NDK:** Via Android Studio SDK Manager (version 25.x recommended).

## Step 2: Native Core Compilation
1.  **Clone Source:** Ensure you have the full repository.
2.  **Configure NDK:** Edit `android/local.properties` to set `ndk.dir`.
3.  **Build macOS Core:**
    ```bash
    make mac
    ```
    *Verification: `libintrovert.dylib` should appear in the root.*
4.  **Build Android Core:**
    ```bash
    make android
    ```
    *Verification: `.so` files should appear in `android/app/src/main/jniLibs/`.*

## Step 3: Flutter UI Setup
1.  **Get Dependencies:**
    ```bash
    flutter pub get
    ```
2.  **Pod Install (macOS only):**
    ```bash
    cd macos && pod install && cd ..
    ```

## Step 4: RBN Infrastructure (Optional but Recommended)
1.  **Server:** Provision a Linux VPS (Port 443 TCP/UDP must be open).
2.  **Compile RBN:**
    ```bash
    cd for_linux
    ./build_linux.sh
    ```
3.  **Deploy:** Follow the `BUILD_&_DEPLOYMENT_GUIDE.md` to start the `introvertd` service.
4.  **Update Bootstrap:** Add your RBN's Multiaddr to `src/network/config.rs`.

## Step 5: Launch & Onboarding
1.  **Run:** `flutter run`.
2.  **Identity:** Select "Create New Identity".
3.  **Mnemonic:** Write down your 12 words. These are the **only** way to recover your account, as Introvert has no "Forgot Password" feature.
4.  **Verification:** Use "Magic Wormhole" to connect to a second device and verify the E2EE handshake.

## Step 6: Security Verification
1.  **Offline Check:** Turn off Wi-Fi on one device. Send a message from the other.
2.  **Mailbox Check:** Turn Wi-Fi back on. The message should be pushed from the Anchor/RBN and render instantly.
3.  **P2P Performance:** Connect both devices to the same network. Send a 10MB video. It should transfer at >5 MB/s using the adaptive 256KB chunking.
