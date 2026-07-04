# Build & Deployment Guide

## 1. Prerequisites
To rebuild Introvert from scratch, you need the following toolchains:
- **Rust:** `rustup` with `nightly` or `stable` (current uses `1.75+`).
- **Flutter:** `3.19+` (stable channel).
- **Android NDK:** Required for cross-compiling the Rust core for mobile.
- **CMake & LLVM:** Required for the native build process.
- **candle-core:** Required for BERT embeddings in Intro-Claw (included in Cargo.toml).

## 2. Platform Build Targets

### A. macOS (Local Development)
The build process produces a `.dylib` which is then embedded into the Flutter app.
```bash
make mac
```
*Logic: Runs `cargo build --release`, copies `libintrovert.dylib` to the project root and `macos/Flutter/ephemeral/`.*

### B. Android (Cross-Compilation)
We target `aarch64-linux-android` (arm64-v8a) and `x86_64-linux-android`.
```bash
make android
```
*Logic: Executes `scripts/build_android.sh`. This script sets up the NDK environment variables (`CC`, `AR`, `LD`) and compiles for multiple targets using `cargo ndk` or manual target specification.*

## 3. RBN (Root Bootstrap Node) Deployment
RBNs must be compiled as native ELF binaries for Linux (typically Ubuntu/Debian).

### Local Cross-Compilation Flow (Recommended)
You can cross-compile the Linux RBN binary directly on macOS using `cargo-zigbuild` and deploy it using our automation script:

1. **Pre-requisites**: Install Zig:
   ```bash
   brew install zig
   cargo install cargo-zigbuild
   ```
2. **Build & Deploy**:
   ```bash
   ./deploy_local_rbn.sh
   ```
   *This automatically cross-compiles the binary locally to `target/x86_64-unknown-linux-gnu/release/introvertd`, uploads it to the RBN server, handles dynamic stopping of the systemd service to avoid "Text file busy", and restarts the service.*

### Manual/Alternative Build Machine Flow:
1.  **Build Machine**: Use a machine with >2GB RAM (compilation on 1GB nodes will OOM).
2.  **Compilation**:
    ```bash
    cd for_linux
    ./build_linux.sh
    ```
3.  **Transfer**: Upload the produced `introvertd` binary to your server.
4.  **Service Setup**:
    - Move `introvertd.service` to `/etc/systemd/system/`.
    - `systemctl enable introvertd`
    - `systemctl start introvertd`
5.  **Bootstrap Update**: Ensure the `get_bootstrap_nodes()` function in `src/network/config.rs` includes the correct IP/Multiaddr of your new RBN.

## 4. Troubleshooting the Build

### Rust-Dart FFI Mismatch
If you add a new `#[no_mangle]` function in Rust:
1.  Declare the C-signature in `src/lib.rs`.
2.  Update `lib/src/native/introvert_client.dart` with the corresponding `typedef` and `lookup`.
3.  Rebuild the native core (`make mac` or `make android`).

### Android Linker Errors
Ensure your `local.properties` in the `android/` folder contains the correct path to the NDK:
`ndk.dir=/Users/youruser/Library/Android/sdk/ndk/25.x.xxxxxxx`

## 5. Port 443 Conflicts
If you are running an RBN on a server that also hosts a web server (Nginx/Apache), you must:
- Either disable the web server on 443.
- Or use a reverse proxy that supports SNI-based routing (e.g., HAProxy) to distinguish between HTTPS and libp2p traffic.
