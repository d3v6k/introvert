#!/usr/bin/env bash
# ==============================================================================
# Introvert RBN Pre-compiled Deploy Script
# ==============================================================================
set -euo pipefail

ALIBABA_USER="root"
ALIBABA_HOST="47.89.252.80"
ALIBABA_PATH="/opt/introvert"
LOCAL_BIN="target/x86_64-unknown-linux-gnu/release/introvertd"

if [ ! -f "$LOCAL_BIN" ]; then
    echo "❌ Error: Pre-compiled binary not found at $LOCAL_BIN"
    echo "Please build it first using: cargo zigbuild --target x86_64-unknown-linux-gnu --release --bin introvertd"
    exit 1
fi

echo "🔍 Checking server reachability..."
if ! nc -z -w 5 "$ALIBABA_HOST" 22 2>/dev/null; then
    echo "❌ Error: Cannot reach $ALIBABA_HOST on port 22."
    exit 1
fi

SSH_OPTS="-o IPQoS=none -o GSSAPIAuthentication=no -o ConnectTimeout=60 -o ServerAliveInterval=15 -o ServerAliveCountMax=120 -o ControlMaster=no -o PubkeyAuthentication=no -o PreferredAuthentications=password -o TCPKeepAlive=yes"

echo "📤 Uploading pre-compiled binary to RBN server (Password Required)..."
cat "$LOCAL_BIN" | ssh $SSH_OPTS "${ALIBABA_USER}@${ALIBABA_HOST}" "mkdir -p ${ALIBABA_PATH}/bin && cat > ${ALIBABA_PATH}/bin/introvertd.new && chmod +x ${ALIBABA_PATH}/bin/introvertd.new"

echo "🚀 Restarting RBN service..."
ssh $SSH_OPTS "${ALIBABA_USER}@${ALIBABA_HOST}" << EOF
    set -e
    mkdir -p ${ALIBABA_PATH}/data
    
    echo "   🛑 Stopping running RBN daemon..."
    systemctl stop introvertd || true
    
    echo "   💾 Replacing binary..."
    mv ${ALIBABA_PATH}/bin/introvertd.new ${ALIBABA_PATH}/bin/introvertd
    
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
EOF

echo "✨ Global RBN Deploy Complete!"
