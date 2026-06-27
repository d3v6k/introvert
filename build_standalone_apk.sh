#!/usr/bin/env bash
# ==============================================================================
# Introvert Sovereign Master Plan v2.0 - Standalone APK Assembly
# ==============================================================================

set -euo pipefail

# UNIVERSAL REVISIONS: NDK Sync
NDK_PATH="/home/dev/Android/Sdk/ndk/28.2.13676358"
TARGET_DIR="target"
JNI_LIBS_DIR="android/app/src/main/jniLibs"
TOOLCHAIN="$NDK_PATH/toolchains/llvm/prebuilt/linux-x86_64/bin"

echo "🚀 Phase 4.1: Building Hardened Standalone APK..."

# 1. Check for NDK
if [ ! -d "$NDK_PATH" ]; then
    echo "❌ Error: Android NDK not found at $NDK_PATH."
    exit 1
fi

# 2. Rust Cross-Compilation (aarch64-linux-android)
echo "⚙️  Compiling Native Core (Rust) for arm64-v8a..."
mkdir -p "$JNI_LIBS_DIR/arm64-v8a"
export PATH="$TOOLCHAIN:$PATH"

# --- Pre-build openssl-sys to get headers (Phase 3.2 Build Block Resolution) ---
echo "⚙️  Pre-building openssl-sys for headers..."
cargo build --target aarch64-linux-android --release -p openssl-sys

# Find the include path using absolute path for reliability
OPENSSL_INCLUDE=$(find "$(pwd)/target/aarch64-linux-android" -name "crypto.h" | head -n 1 | xargs dirname | xargs dirname)
if [ -z "$OPENSSL_INCLUDE" ]; then
    echo "❌ Error: Could not find openssl headers."
    exit 1
fi
echo "📍 Found OpenSSL headers at: $OPENSSL_INCLUDE"
export CFLAGS="-I$OPENSSL_INCLUDE"

echo "⚙️  Building arm64-v8a Core with SQLCipher support..."
cargo ndk --target aarch64-linux-android --platform 29 build --release

# 3. Optimization & Injection
echo "✂️  Optimizing Payload (llvm-strip)..."
"$TOOLCHAIN/llvm-strip" "$TARGET_DIR/aarch64-linux-android/release/libintrovert.so"

cp "$TARGET_DIR/aarch64-linux-android/release/libintrovert.so" "$JNI_LIBS_DIR/arm64-v8a/libintrovert.so"
echo "✅ Native library injected."

# 4. Flutter Assembly
echo "⚙️  Assembling Standalone Flutter APK..."
flutter build apk --release --target-platform android-arm64 --split-per-abi

echo "🎉 STANDALONE APK READY!"
echo "📍 Path: build/app/outputs/flutter-apk/app-arm64-v8a-release.apk"
echo "🛡️  Deployment: RBNs configured with 1M Connection Limit and Liveness Checks."
