#!/bin/bash
# ==============================================================================
# Introvert Sovereign - RBN Port 443 Finalizer
# ==============================================================================

set -euo pipefail

# --- User & Environment Configuration ---
ALIBABA_USER="root"
ALIBABA_HOST="47.89.252.80"
ALIBABA_PATH="/opt/introvert"
SSH_KEY_PATH="/Users/dev/.ssh/id_ed25519" 

# Hardened SSH options with maximum persistence
SSH_OPTS="-o IPQoS=none -o GSSAPIAuthentication=no -o ConnectTimeout=60 -o ServerAliveInterval=15 -o ServerAliveCountMax=120 -o ControlMaster=no -o PubkeyAuthentication=no -o PreferredAuthentications=password -o TCPKeepAlive=yes"

echo "🛡️  Finalizing Global RBN Hardening (Port 443)..."

# 1. Prepare Source Bundle
echo "📦 Step 1: Bundling source code..."
tar -czf introvert_rbn_update.tar.gz src Cargo.toml Cargo.lock

# 2. Upload to Alibaba
echo "📤 Step 2: Syncing to Alibaba Cloud (Manual Password Required)..."
if scp $SSH_OPTS introvert_rbn_update.tar.gz "${ALIBABA_USER}@${ALIBABA_HOST}:${ALIBABA_PATH}/"; then
    rm introvert_rbn_update.tar.gz
    echo "   ✅ Source sync successful."
else
    echo "   ❌ Error: Source transfer failed."
    rm -f introvert_rbn_update.tar.gz
    exit 1
fi

# 3. Remote Build and Restart
echo "🚀 Step 3: Remote Build & Restart (Manual Password Required)..."
ssh $SSH_OPTS "${ALIBABA_USER}@${ALIBABA_HOST}" << EOF
    set -e
    echo "   🛠  Setting up Build Environment..."
    mkdir -p ${ALIBABA_PATH}/bin ${ALIBABA_PATH}/data
    
    if command -v apt-get &> /dev/null; then
        echo "   📦 Refreshing system packages..."
        apt-get update -y > /dev/null
        apt-get install -y build-essential pkg-config libssl-dev tar > /dev/null
    fi

    if ! command -v cargo &> /dev/null; then
        echo "   📥 Installing Rust toolchain..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source \$HOME/.cargo/env
    fi
    
    echo "   🔧 Verifying toolchain..."
    \$HOME/.cargo/bin/rustup toolchain install stable --profile minimal > /dev/null
    \$HOME/.cargo/bin/rustup default stable
    source \$HOME/.cargo/env
    
    echo "   📦 Compiling Hardened Daemon (Port 443)..."
    cd ${ALIBABA_PATH}
    tar -xzf introvert_rbn_update.tar.gz
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
    echo "   📡 Listening on:"
    netstat -tulpn | grep introvertd
EOF

echo "✨ Global RBN Update Complete! Your mesh is now live on Port 443."
