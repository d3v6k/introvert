#!/bin/bash
# ==============================================================================
# Introvert Sovereign Master Plan v2.0 - Global RBN Deploy
# ==============================================================================

set -euo pipefail

# --- User & Environment Configuration ---
ALIBABA_USER="root"
ALIBABA_HOST="47.89.252.80"
ALIBABA_PATH="/opt/introvert"
SSH_KEY_PATH="/Users/dev/.ssh/id_ed25519" 

# UNIVERSAL REVISIONS: NDK Sync
NDK_PATH="/Users/dev/Library/Android/sdk/ndk/28.2.13676358"
TOOLCHAIN="$NDK_PATH/toolchains/llvm/prebuilt/darwin-x86_64/bin"

echo "🛡️ Initiating Phase 4.1: Global RBN Hardening..."

# 1. Build Linux RBN (x86_64-unknown-linux-gnu)
echo "📦 Cross-compilation of SQLCipher via zigbuild on macOS is unstable."
echo "💡 Strategy Shift: We will build the Android library locally and build the RBN binary directly ON the server."
SKIP_LINUX_BUILD=true
LINUX_BIN="target/x86_64-unknown-linux-gnu/release/introvertd"

# 2. Build Android Sovereign Client (aarch64)
echo "📱 Compiling Android Native Core..."
NDK_PLATFORM="29"
export ANDROID_ABI="arm64-v8a"
export PATH="$TOOLCHAIN:$PATH"

# --- Pre-build openssl-sys to get headers (Phase 3.2 Build Block Resolution) ---
echo "⚙️  Pre-building openssl-sys for headers..."
# We use the correct target and force openssl-sys build
cargo build --target aarch64-linux-android --release -p openssl-sys

# Find the most recent openssl-sys build directory
OPENSSL_OUT_DIR=$(ls -dt "$(pwd)/target/aarch64-linux-android/release/build/openssl-sys-"*/out 2>/dev/null | head -n 1)

if [ -z "$OPENSSL_OUT_DIR" ]; then
    echo "❌ Error: Could not find openssl-sys output directory."
    exit 1
fi

# Messenger strategy: Create a LOCAL predictable include directory
# This solves issues where compilers fail to parse complex /Volumes/... paths
echo "📁 Localizing OpenSSL headers for stable discovery..."
rm -rf .openssl_android
mkdir -p .openssl_android/include
mkdir -p .openssl_android/lib

cp -r "$OPENSSL_OUT_DIR/openssl-build/install/include/openssl" .openssl_android/include/
cp "$OPENSSL_OUT_DIR/openssl-build/install/lib/"*.a .openssl_android/lib/

export OPENSSL_DIR="$(pwd)/.openssl_android"
export OPENSSL_INCLUDE_DIR="$OPENSSL_DIR/include"
export OPENSSL_LIB_DIR="$OPENSSL_DIR/lib"
export OPENSSL_STATIC=1

echo "📍 Predictable OpenSSL path: $OPENSSL_DIR"

# Set ALL possible include variables for maximum compatibility
export CFLAGS="-I$OPENSSL_INCLUDE_DIR"
export CPPFLAGS="-I$OPENSSL_INCLUDE_DIR"
export C_INCLUDE_PATH="$OPENSSL_INCLUDE_DIR"
export CPLUS_INCLUDE_PATH="$OPENSSL_INCLUDE_DIR"
export LIBRARY_PATH="$OPENSSL_LIB_DIR"

echo "⚙️  Building arm64-v8a Core with SQLCipher support..."
cargo ndk --target aarch64-linux-android --platform $NDK_PLATFORM build --release

# 3. Strip & Optimize (Payload Reduction)
echo "✂️ Optimizing binaries..."
ANDROID_LIB="target/aarch64-linux-android/release/libintrovert.so"
"$TOOLCHAIN/llvm-strip" "$ANDROID_LIB"

if [ "$SKIP_LINUX_BUILD" = false ]; then
    echo "   Stripping Linux binary..."
    strip "$LINUX_BIN"
fi

# Cleanup local OpenSSL headers
rm -rf .openssl_android

# 4. Global RBN Deployment
echo "☁️ Updating Alibaba Cloud RBN Entry Point..."

# FIRST: Check if the server is even reachable on Port 22
echo "🔍 Checking server reachability (SSH Port 22)..."
if ! nc -z -w 5 "$ALIBABA_HOST" 22 2>/dev/null; then
    echo "❌ Error: Cannot reach $ALIBABA_HOST on port 22."
    echo "   Manual Step Required: The binary is ready at $LINUX_BIN"
    echo "   Please upload it via Alibaba Web Console or use a hotspot."
    exit 1
fi

echo "📁 Preparing server environment..."
echo "🔐 Manual Password Entry Mode..."

# Hardened SSH options
SSH_OPTS="-o IPQoS=none -o GSSAPIAuthentication=no -o ConnectTimeout=30 -o ServerAliveInterval=15 -o ControlMaster=no -o PubkeyAuthentication=no -o PreferredAuthentications=password"

# Try to upload the binary and service file
echo "📤 Beginning Deployment Process..."

# Hardened SSH options with maximum persistence
SSH_OPTS="-o IPQoS=none -o GSSAPIAuthentication=no -o ConnectTimeout=60 -o ServerAliveInterval=15 -o ServerAliveCountMax=120 -o ControlMaster=no -o PubkeyAuthentication=no -o PreferredAuthentications=password -o TCPKeepAlive=yes"

if [ "$SKIP_LINUX_BUILD" = true ]; then
    echo "📤 Step 1: Syncing source code to RBN server..."
    # Create a temporary source bundle
    tar -czf introvert_src.tar.gz src Cargo.toml Cargo.lock
    
    # Messenger strategy: Use 'cat' over SSH for maximum resilience
    if cat introvert_src.tar.gz | ssh $SSH_OPTS "${ALIBABA_USER}@${ALIBABA_HOST}" "mkdir -p ${ALIBABA_PATH} && cat > ${ALIBABA_PATH}/introvert_src.tar.gz"; then
        rm introvert_src.tar.gz
        echo "   ✅ Source sync successful."
    else
        echo "   ❌ Error: Source transfer failed."
        exit 1
    fi
    
    echo "🚀 Step 2: Remote Build & Deployment (Password Required)..."
    ssh $SSH_OPTS "${ALIBABA_USER}@${ALIBABA_HOST}" << EOF
        set -e
        echo "   🛠  Setting up Build Environment..."
        mkdir -p ${ALIBABA_PATH}/bin ${ALIBABA_PATH}/data
        
        # Install dependencies if missing (Removed sudo since we are root)
        if command -v apt-get &> /dev/null; then
            echo "   📦 Refreshing system package list..."
            apt-get update -y
            echo "   📦 Installing build dependencies..."
            apt-get install -y build-essential pkg-config libssl-dev tar
        fi

        if ! command -v cargo &> /dev/null; then
            echo "   📥 Installing Rust toolchain..."
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
            source \$HOME/.cargo/env
        fi
        
        # Ensure toolchain is healthy
        echo "   🔧 Optimizing Rust toolchain..."
        \$HOME/.cargo/bin/rustup toolchain install stable --profile minimal > /dev/null
        \$HOME/.cargo/bin/rustup default stable > /dev/null
        source \$HOME/.cargo/env
        
        echo "   📦 Compiling Hardened Daemon (Port 443)..."
        cd ${ALIBABA_PATH}
        tar -xzf introvert_src.tar.gz
        \$HOME/.cargo/bin/cargo build --release --bin introvertd
        
        echo "   💾 Deploying binary..."
        cp target/release/introvertd ${ALIBABA_PATH}/bin/introvertd
        chmod +x ${ALIBABA_PATH}/bin/introvertd

        echo "   ⚙️  Updating Service Configuration..."
        cat << SVC > /etc/systemd/system/introvertd.service
[Unit]
Description=Introvert Root Bootstrap Node (RBN) Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=${ALIBABA_PATH}
ExecStart=${ALIBABA_PATH}/bin/introvertd --data-dir ${ALIBABA_PATH}/data --relay --port 443
Environment="RUST_LOG=info"
Restart=always
RestartSec=5
StartLimitIntervalSec=0
[Install]
WantedBy=multi-user.target
SVC

        echo "   🔄 Restarting RBN service..."
        systemctl daemon-reload
        systemctl restart introvertd
        
        echo "   ✅ RBN Status:"
        systemctl is-active introvertd
        echo "   PEER_ID: \$(${ALIBABA_PATH}/bin/introvertd --data-dir ${ALIBABA_PATH}/data get-peer-id 2>/dev/null || echo 'Unknown')"
EOF
else
    echo "📤 Uploading Local Hardened Binary..."
    cat "$LINUX_BIN" | ssh $SSH_OPTS "${ALIBABA_USER}@${ALIBABA_HOST}" "mkdir -p ${ALIBABA_PATH}/bin && cat > ${ALIBABA_PATH}/bin/introvertd"
    # ... (rest of local deployment if needed)
fi

echo "✅ Global RBN Update Complete! Ready for end-user APK distribution."
