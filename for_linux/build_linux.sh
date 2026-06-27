#!/usr/bin/env bash
# ==============================================================================
# Introvert RBN Build Script (Native Linux)
# ==============================================================================

set -euo pipefail

echo "🛡️  Initiating Introvert RBN Native Build..."

# 1. Install System Dependencies (Ubuntu/Debian)
if command -v apt-get &> /dev/null; then
    echo "📦 Checking system dependencies..."
    sudo apt-get update -y
    sudo apt-get install -y build-essential pkg-config libssl-dev tar
fi

# 2. Install/Update Rust
if ! command -v cargo &> /dev/null; then
    echo "📥 Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
else
    echo "✅ Rust is already installed."
fi

# 3. Build Hardened Daemon
echo "📦 Compiling Hardened Daemon (Release Mode)..."
cargo build --release --bin introvertd

# 4. Success Output
BIN_PATH="target/release/introvertd"
if [ -f "$BIN_PATH" ]; then
    echo "✅ Compilation Successful!"
    echo "📍 Binary Location: $(pwd)/$BIN_PATH"
    echo ""
    echo "🚀 DEPLOYMENT INSTRUCTIONS:"
    echo "1. Upload the binary to your RBN server:"
    echo "   scp $BIN_PATH root@47.89.252.80:/opt/introvert/bin/introvertd"
    echo ""
    echo "2. Upload the service file:"
    echo "   scp introvertd.service root@47.89.252.80:/etc/systemd/system/introvertd.service"
    echo ""
    echo "3. Log into the RBN server and restart:"
    echo "   ssh root@47.89.252.80 'systemctl daemon-reload && systemctl restart introvertd'"
else
    echo "❌ Error: Compilation failed. Check the logs above."
    exit 1
fi
