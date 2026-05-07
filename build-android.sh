#!/usr/bin/env bash
# ==============================================================================
# Introvert Sovereign Master Plan v2.0 - Android Native Compile
# ==============================================================================

set -euo pipefail

# UNIVERSAL REVISIONS: NDK Sync
NDK_PATH="/home/dev/Android/Sdk/ndk/28.2.13676358"
TARGET_DIR="target"
JNI_LIBS_DIR="android/app/src/main/jniLibs"
TOOLCHAIN="$NDK_PATH/toolchains/llvm/prebuilt/linux-x86_64/bin"

echo "🚀 Starting Android Cross-Compilation (aarch64-linux-android)..."

if [ ! -d "$NDK_PATH" ]; then
    echo "❌ Error: Android NDK not found at $NDK_PATH."
    exit 1
fi

# Ensure output directories exist
mkdir -p "$JNI_LIBS_DIR/arm64-v8a"

# Set environment variables for cross-compilation
export ANDROID_NDK=$NDK_PATH
export ANDROID_NDK_ROOT=$NDK_PATH
export PATH="$TOOLCHAIN:$PATH"

# --- 1. Pre-build openssl-sys to get headers ---
echo "⚙️  Pre-building openssl-sys for headers..."
cargo build --target aarch64-linux-android --release -p openssl-sys

# Find the include path (using absolute path for reliability)
OPENSSL_INCLUDE=$(find "$(pwd)/target/aarch64-linux-android" -name "crypto.h" | head -n 1 | xargs dirname | xargs dirname)
if [ -z "$OPENSSL_INCLUDE" ]; then
    echo "❌ Error: Could not find openssl headers."
    exit 1
fi
echo "📍 Found OpenSSL headers at: $OPENSSL_INCLUDE"
export CFLAGS="-I$OPENSSL_INCLUDE"

# --- 2. Build for arm64-v8a (Real hardware) ---
echo "⚙️  Building arm64-v8a Core..."
cargo ndk --target aarch64-linux-android --platform 29 build --release

# --- 2. Strip & Optimize ---
echo "✂️  Stripping debug symbols with llvm-strip..."
"$TOOLCHAIN/llvm-strip" "$TARGET_DIR/aarch64-linux-android/release/libintrovert.so"

# Copy to Android jniLibs
cp "$TARGET_DIR/aarch64-linux-android/release/libintrovert.so" "$JNI_LIBS_DIR/arm64-v8a/libintrovert.so"

echo "✅ Android Compilation Successful! Shared library installed to: $JNI_LIBS_DIR"
