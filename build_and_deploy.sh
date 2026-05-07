#!/bin/bash
# ==============================================================================
# Introvert Sovereign Master Plan v2.0 - Global RBN Deploy
# ==============================================================================

set -euo pipefail

# --- User & Environment Configuration ---
ALIBABA_USER="root"
ALIBABA_HOST="47.89.252.80"
ALIBABA_PATH="/opt/introvert"
SSH_KEY_PATH="/home/dev/.ssh/introvert_deploy_key" 

# UNIVERSAL REVISIONS: NDK Sync
NDK_PATH="/home/dev/Android/Sdk/ndk/28.2.13676358"
TOOLCHAIN="$NDK_PATH/toolchains/llvm/prebuilt/linux-x86_64/bin"

echo "🛡️ Initiating Phase 4.1: Global RBN Hardening..."

# 1. Build Linux RBN (x86_64-unknown-linux-gnu)
echo "📦 Compiling Hardened Linux Daemon & Shared Library..."
# Includes Phase 2.1 libp2p 0.53 relay transport resolutions
cargo build --release --target x86_64-unknown-linux-gnu --bin introvertd
# Also build the shared library for Linux desktop testing
cargo build --release --target x86_64-unknown-linux-gnu --lib
mkdir -p linux/flutter/ephemeral
cp target/x86_64-unknown-linux-gnu/release/libintrovert.so linux/flutter/ephemeral/
# Ensure the root target/release also has a copy for legacy CMake paths
mkdir -p target/release
cp target/x86_64-unknown-linux-gnu/release/libintrovert.so target/release/libintrovert.so


# 2. Build Android Sovereign Client (aarch64)
echo "📱 Compiling Android Native Core (v2.0 Persistence)..."
# Supports Phase 1.2 SQLCipher persistence and Noise IK caching
NDK_PLATFORM="29"
export ANDROID_ABI="arm64-v8a"
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
cargo ndk --target aarch64-linux-android --platform $NDK_PLATFORM build --release

# 3. Strip & Optimize (Payload Reduction)
echo "✂️ Optimizing binaries with llvm-strip..."
LINUX_BIN="target/x86_64-unknown-linux-gnu/release/introvertd"
ANDROID_LIB="target/aarch64-linux-android/release/libintrovert.so"

strip "$LINUX_BIN"
"$TOOLCHAIN/llvm-strip" "$ANDROID_LIB"

# 4. Global RBN Deployment
echo "☁️ Updating Alibaba Cloud RBN Entry Point..."

# Phase 1.2: SQLCipher backup routine and data preservation
ssh -i "$SSH_KEY_PATH" "${ALIBABA_USER}@${ALIBABA_HOST}" << EOF
    mkdir -p ${ALIBABA_PATH}/bin
    mkdir -p ${ALIBABA_PATH}/data
    sudo systemctl stop introvertd || true
    
    # EMERGENCY RECOVERY: Detect SQLCipher key mismatch from previous failed runs
    if sudo journalctl -u introvertd -n 100 --no-pager | grep -q "file is not a database"; then
        echo "   🚨 Detected SQLCipher key mismatch. Moving unrecoverable database..."
        if [ -f "${ALIBABA_PATH}/data/introvert.db" ]; then
            mv "${ALIBABA_PATH}/data/introvert.db" "${ALIBABA_PATH}/data/mismatch_$(date +%F_%H%M%S).db"
        fi
    fi

    # Ensure a persistent master seed exists for stable PeerID
    if [ ! -f "${ALIBABA_PATH}/data/introvert.seed" ]; then
        echo "   🌱 Generating new RBN Master Seed..."
        dd if=/dev/urandom of=${ALIBABA_PATH}/data/introvert.seed bs=1 count=32 status=none
        if [ -f "${ALIBABA_PATH}/data/introvert.db" ]; then
             echo "   ⚠️ Seed missing but database exists. Moving old database to prevent key mismatch..."
             mv "${ALIBABA_PATH}/data/introvert.db" "${ALIBABA_PATH}/data/unrecoverable_$(date +%F_%H%M%S).db"
        fi
    fi

    # Preservation of verified P2P identity records and reward logs
    if [ -f "${ALIBABA_PATH}/data/introvert.db" ]; then
        echo "   Backing up SQLCipher database..."
        cp "${ALIBABA_PATH}/data/introvert.db" "${ALIBABA_PATH}/data/backup_$(date +%F_%H%M%S).db"
    fi
EOF

echo "   Uploading v2.0 Hardened Binary & Service Config..."
scp -i "$SSH_KEY_PATH" "$LINUX_BIN" "${ALIBABA_USER}@${ALIBABA_HOST}:${ALIBABA_PATH}/bin/introvertd"
scp -i "$SSH_KEY_PATH" introvertd.service "${ALIBABA_USER}@${ALIBABA_HOST}:/etc/systemd/system/introvertd.service"

echo "   Restarting RBN with 1M Connection Limit & 5-min Liveness Check..."
ssh -i "$SSH_KEY_PATH" "${ALIBABA_USER}@${ALIBABA_HOST}" << EOF
    chmod +x ${ALIBABA_PATH}/bin/introvertd
    sudo systemctl daemon-reload
    # Launch with Phase 4.1 churn management and scale parameters
    sudo systemctl start introvertd
    sleep 2
    if ! sudo systemctl is-active --quiet introvertd; then
        echo "   ❌ Error: introvertd failed to start. Recent logs:"
        sudo journalctl -u introvertd -n 20 --no-pager
        exit 1
    fi
    sudo systemctl status introvertd --no-pager
EOF

echo "✅ Global RBN Update Complete! Ready for end-user APK distribution."
