#!/usr/bin/env bash
# ==============================================================================
# Introvert RBN Compilation & Deployment Orchestrator
# ==============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

BUILD_MACHINE="dev@thinkpad.local"
RBN_IP="47.89.252.80"
RBN_USER="root"
RBN_BIN_PATH="/opt/introvert/bin/introvertd"
LOCAL_BIN="./introvertd"

echo -e "${BLUE}🛡️  Step 1/6: Syncing updated source files to build machine ($BUILD_MACHINE)...${NC}"
if scp -r for_linux/src/ for_linux/Cargo.toml for_linux/Cargo.lock "$BUILD_MACHINE":~/introvert/for_linux/; then
    echo -e "${GREEN}✅ Source sync successful.${NC}"
else
    echo -e "${RED}❌ Error: Failed to copy source files to build machine.${NC}"
    exit 1
fi

echo -e "${BLUE}🛡️  Step 2/6: Compiling optimized release binary on build machine...${NC}"
if ssh "$BUILD_MACHINE" 'export PATH=$HOME/.cargo/bin:$PATH && cd ~/introvert/for_linux && cargo build --release --bin introvertd'; then
    echo -e "${GREEN}✅ Compilation on build machine successful.${NC}"
else
    echo -e "${RED}❌ Error: Compilation failed on build machine.${NC}"
    exit 1
fi

echo -e "${BLUE}🛡️  Step 3/6: Copying compiled binary back to local workspace...${NC}"
if scp "$BUILD_MACHINE":~/introvert/for_linux/target/release/introvertd "$LOCAL_BIN"; then
    echo -e "${GREEN}✅ Copy successful. Local location: $LOCAL_BIN${NC}"
else
    echo -e "${RED}❌ Error: Failed to copy binary back from build machine.${NC}"
    exit 1
fi

echo -e "${BLUE}🛡️  Step 4/6: Stopping introvertd daemon on RBN ($RBN_IP) [Password required]...${NC}"
if ssh "${RBN_USER}@${RBN_IP}" "systemctl stop introvertd"; then
    echo -e "${GREEN}✅ Daemon stopped successfully.${NC}"
else
    echo -e "${RED}❌ Error: Failed to stop daemon on RBN.${NC}"
    exit 1
fi

echo -e "${BLUE}🛡️  Step 5/6: Deploying binary, config, and service file to RBN ($RBN_IP) [Password required]...${NC}"
if scp "$LOCAL_BIN" "${RBN_USER}@${RBN_IP}:${RBN_BIN_PATH}" && \
   ssh "${RBN_USER}@${RBN_IP}" "mkdir -p /opt/introvert/config" && \
   scp for_linux/introvertd.service "${RBN_USER}@${RBN_IP}:/etc/systemd/system/introvertd.service"; then
    echo -e "${GREEN}✅ Binary, Firebase config, and service file uploaded successfully.${NC}"
else
    echo -e "${RED}❌ Error: Failed to upload binary, config, or service file to RBN.${NC}"
    exit 1
fi

# Upload APNs key if present
if [ -f firebase/apns-key.p8 ]; then
    echo -e "${BLUE}📤 Uploading APNs key to RBN...${NC}"
    if scp firebase/apns-key.p8 "${RBN_USER}@${RBN_IP}:/opt/introvert/config/apns-key.p8"; then
        echo -e "${GREEN}✅ APNs key uploaded.${NC}"
    else
        echo -e "${RED}⚠️  Failed to upload APNs key — iOS push will not work.${NC}"
    fi
else
    echo -e "${BLUE}ℹ️  No firebase/apns-key.p8 found — skipping APNs key upload.${NC}"
fi

echo -e "${BLUE}🛡️  Step 6/6: Reloading daemon and starting introvertd on RBN [Password required]...${NC}"
if ssh "${RBN_USER}@${RBN_IP}" "systemctl daemon-reload && systemctl start introvertd"; then
    echo -e "${GREEN}✅ Service restarted.${NC}"
else
    echo -e "${RED}❌ Error: Failed to start service on RBN.${NC}"
    exit 1
fi

echo -e "${BLUE}🔍 Verifying active daemon status on RBN [Password required]...${NC}"
if ssh "${RBN_USER}@${RBN_IP}" "systemctl is-active introvertd" >/dev/null 2>&1; then
    echo -e "${GREEN}✨ Verification SUCCESS: introvertd daemon is ACTIVE on RBN!${NC}"
else
    echo -e "${RED}❌ Verification FAILED: introvertd daemon is NOT active on RBN. Please check systemctl logs on the server.${NC}"
    exit 1
fi
