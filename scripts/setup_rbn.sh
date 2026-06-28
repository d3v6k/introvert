#!/bin/bash
# ==============================================================================
# Introvert Sovereign - RBN Automated Installer Script
# ==============================================================================
# Target: Ubuntu/Debian, macOS, and WSL2
# Designed for high-reliability, step-by-step verification, and user friendliness.

set -euo pipefail

# Style definitions for clean terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# GitHub Repository Placement (Placeholder - to be confirmed later)
GITHUB_REPO="https://github.com/mahadevbk/introvert"

# Helper: print success message
success() {
    echo -e "${GREEN}✅ SUCCESS: $1${NC}"
}

# Helper: print failure and exit/pause
failure() {
    echo -e "${RED}❌ ERROR: $1${NC}"
    echo -e "${YELLOW}Troubleshooting Tip: $2${NC}"
    exit 1
}

# Helper: wait for user acknowledgment
press_enter() {
    echo -e "${CYAN}👉 Press [ENTER] to proceed...${NC}"
    read -r
}

clear
echo -e "${BLUE}================================================================${NC}"
echo -e "${BLUE}🛡️  Introvert Root Bootstrap Node (RBN) Installer 🛡️${NC}"
echo -e "${BLUE}================================================================${NC}"
echo -e ""
echo -e "${BOLD}${YELLOW}📋 PRE-REQUISITES & SYSTEM REQUIREMENTS:${NC}"
echo -e "  * ${BOLD}OS:${NORMAL} Linux (Ubuntu/Debian recommended), macOS, or WSL2"
echo -e "  * ${BOLD}RAM:${NORMAL} Minimum 1GB RAM (Recommended 2GB+)"
echo -e "  * ${BOLD}Storage:${NORMAL} 10GB free space"
echo -e "  * ${BOLD}Network:${NORMAL} Minimum 10 Mbps symmetric (Recommended 50 Mbps+)"
echo -e "  * ${BOLD}Network Ports:${NORMAL} Port forwarding for TCP port 443 must be open"
echo -e "  * ${BOLD}Wallet Fees:${NORMAL} Needs Solana Devnet SOL for registry sync"
echo -e ""
echo -e "${BLUE}================================================================${NC}"
read -p "Do you meet the requirements and wish to begin the installer? (y/N): " START_CHOICE
START_CHOICE=${START_CHOICE:-n}

if [[ ! "$START_CHOICE" =~ ^[yY]$ ]]; then
    echo -e "${RED}❌ Installation cancelled by user.${NC}"
    exit 0
fi

# ==============================================================================
# STEP 1: AUTOMATED AUDIT & PRE-CHECKS
# ==============================================================================
echo -e "\n${CYAN}🔍 STEP 1: Running system pre-checks and capability audit...${NC}"

OS_TYPE=$(uname -s)
echo -e "  * Operating System: ${GREEN}$OS_TYPE${NC}"

# 1. RAM Audit
if [ "$OS_TYPE" = "Linux" ]; then
    TOTAL_RAM_KB=$(grep MemTotal /proc/meminfo | awk '{print $2}')
    TOTAL_RAM_GB=$((TOTAL_RAM_KB / 1024 / 1024))
elif [ "$OS_TYPE" = "Darwin" ]; then
    TOTAL_RAM_BYTES=$(sysctl -n hw.memsize)
    TOTAL_RAM_GB=$((TOTAL_RAM_BYTES / 1024 / 1024 / 1024))
else
    TOTAL_RAM_GB=0
fi

if [ "$TOTAL_RAM_GB" -lt 1 ]; then
    echo -e "  * RAM Audit: ${RED}FAILED ($TOTAL_RAM_GB GB detected)${NC}"
    read -p "Your memory is below the 1GB minimum. Force installation anyway? (y/N): " FORCE_RAM
    if [[ ! "$FORCE_RAM" =~ ^[yY]$ ]]; then
        failure "Insufficient RAM." "Try allocating more memory to your WSL2 container or virtual machine."
    fi
else
    echo -e "  * RAM Audit: ${GREEN}PASSED ($TOTAL_RAM_GB GB detected)${NC}"
fi

# 2. Port 443 Audit
if command -v netstat &> /dev/null; then
    if sudo netstat -tuln | grep -q ":443 "; then
        echo -e "  * Port 443 Check: ${YELLOW}OCCUPIED${NC}"
        echo -e "    Warning: Another process is listening on Port 443 (e.g. Nginx, Apache, or Docker)."
        read -p "Do you want to continue? (You must stop that service before running introvertd) (y/N): " FORCE_PORT
        if [[ ! "$FORCE_PORT" =~ ^[yY]$ ]]; then
            failure "Port 443 occupied." "Stop Nginx/Apache using 'sudo systemctl stop nginx' and run installer again."
        fi
    else
        echo -e "  * Port 443 Check: ${GREEN}FREE${NC}"
    fi
else
    echo -e "  * Port 443 Check: ${GREEN}SKIPPED (net-tools not present)${NC}"
fi

# 3. Superuser Privileges check (Linux-only restriction)
if [ "$OS_TYPE" = "Linux" ] && [ "$EUID" -ne 0 ]; then
    failure "Installer must be run as root." "Launch the script with sudo: 'sudo bash setup_rbn.sh'"
fi

success "System audit complete."
press_enter

# ==============================================================================
# STEP 2: INSTALL SYSTEM DEPENDENCIES
# ==============================================================================
echo -e "\n${CYAN}🔄 STEP 2: Installing system compilation toolchains & tools...${NC}"

if [ "$OS_TYPE" = "Linux" ]; then
    apt-get update -y > /dev/null
    apt-get install -y curl build-essential pkg-config libssl-dev tar net-tools git python3 qrencode xxd > /dev/null
elif [ "$OS_TYPE" = "Darwin" ]; then
    if ! command -v brew &> /dev/null; then
        echo -e "${YELLOW}⚠️  Homebrew is missing. Please install it or manually set up git, python3, openssl, and qrencode.${NC}"
    else
        brew install git python3 openssl qrencode > /dev/null || true
    fi
fi

# Verification Checks
for cmd in git python3 openssl; do
    if ! command -v "$cmd" &> /dev/null; then
        failure "Command '$cmd' could not be resolved after installer run." "Install it manually via your package manager."
    fi
done

success "System dependencies successfully verified."
press_enter

# ==============================================================================
# STEP 3: RUST & CARGO COMPILER INSTALLATION
# ==============================================================================
echo -e "\n${CYAN}🛠️  STEP 3: Setting up Rust compilation toolchain...${NC}"

if ! command -v cargo &> /dev/null; then
    echo -e "  * Installing Rust & Cargo via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y > /dev/null
    source "$HOME/.cargo/env" || true
fi

# Export path just in case shell has not refreshed env variables
export PATH="$HOME/.cargo/bin:$PATH"

# Verification
if ! command -v cargo &> /dev/null; then
    failure "Rust/Cargo installation could not be verified." "Run 'source \$HOME/.cargo/env' and verify manual installation."
fi

CARGO_VER=$(cargo --version | awk '{print $2}')
success "Rust compiler is ready (version $CARGO_VER)."
press_enter

# ==============================================================================
# STEP 4: DIRECTORY SETUP & IDENTITY SEED GENERATION
# ==============================================================================
echo -e "\n${CYAN}🔑 STEP 4: Creating directories and configuring Master Seed...${NC}"

RBN_DIR="/opt/introvert"
RBN_DATA_DIR="$RBN_DIR/data"
RBN_BIN_DIR="$RBN_DIR/bin"
RBN_SEED_FILE="$RBN_DATA_DIR/introvert.seed"

if [ "$OS_TYPE" = "Linux" ]; then
    mkdir -p "$RBN_DIR" "$RBN_DATA_DIR" "$RBN_BIN_DIR"
    chmod 700 "$RBN_DIR"
else
    RBN_DIR="$HOME/.introvert"
    RBN_DATA_DIR="$RBN_DIR/data"
    RBN_BIN_DIR="$RBN_DIR/bin"
    RBN_SEED_FILE="$RBN_DATA_DIR/introvert.seed"
    mkdir -p "$RBN_DIR" "$RBN_DATA_DIR" "$RBN_BIN_DIR"
fi

if [ -f "$RBN_SEED_FILE" ]; then
    echo -e "${YELLOW}⚠️  Existing master seed found at $RBN_SEED_FILE.${NC}"
    read -p "Do you want to overwrite it and generate a new identity? (y/N): " OVERWRITE
    OVERWRITE=${OVERWRITE:-n}
else
    OVERWRITE="y"
fi

if [ "$OVERWRITE" = "y" ] || [ "$OVERWRITE" = "Y" ]; then
    echo -e "Choose seed generation method:"
    echo -e "  1) Generate a new secure random seed (Recommended)"
    echo -e "  2) Paste an existing 32-byte hex seed"
    read -p "Selection (1 or 2): " SEED_CHOICE
    SEED_CHOICE=${SEED_CHOICE:-1}

    if [ "$SEED_CHOICE" -eq 1 ]; then
        python3 -c 'import os; open("'"$RBN_SEED_FILE"'", "wb").write(os.urandom(32))'
    else
        read -p "Paste 64-character Hex Seed: " USER_HEX
        if [ ${#USER_HEX} -ne 64 ]; then
            failure "Invalid hex seed length." "A 32-byte seed must be exactly 64 characters long in hexadecimal."
        fi
        if command -v xxd &> /dev/null; then
            echo "$USER_HEX" | xxd -r -p > "$RBN_SEED_FILE"
        else
            python3 -c "import bytes; open('$RBN_SEED_FILE', 'wb').write(bytes.fromhex('$USER_HEX'))"
        fi
    fi
    chmod 400 "$RBN_SEED_FILE"
fi

# Verification
if [ ! -f "$RBN_SEED_FILE" ] || [ "$(wc -c < "$RBN_SEED_FILE" | tr -d ' ')" -ne 32 ]; then
    failure "Identity seed file check failed." "The seed must be exactly 32 bytes binary."
fi

success "Cryptographic identity seed file created successfully."
press_enter

# ==============================================================================
# STEP 5: SYNC & COMPILE DAEMON
# ==============================================================================
echo -e "\n${CYAN}📥 STEP 5: Downloading source files and compiling RBN daemon...${NC}"

BUILD_DIR="/tmp/introvert-rbn-build"
rm -rf "$BUILD_DIR"

echo -e "  * Cloning codebase from $GITHUB_REPO..."
if ! git clone --depth 1 "$GITHUB_REPO" "$BUILD_DIR" > /dev/null; then
    failure "Failed to clone repository from GitHub." "Check internet connection or verified address link."
fi

cd "$BUILD_DIR/for_linux"

echo -e "  * Compiling optimized release binary (this may take a few minutes)..."
if ! cargo build --release --bin introvertd; then
    failure "Cargo compilation failed." "Check system resources or look for missing dependency libraries."
fi

cp target/release/introvertd "$RBN_BIN_DIR/introvertd"
chmod +x "$RBN_BIN_DIR/introvertd"
rm -rf "$BUILD_DIR"

# Verification
if [ ! -x "$RBN_BIN_DIR/introvertd" ]; then
    failure "Daemon executable is not ready or missing." "Try building manually in the target folder."
fi

success "intovertd daemon built successfully."
press_enter

# ==============================================================================
# STEP 6: REGISTER DAEMON SERVICE
# ==============================================================================
echo -e "\n${CYAN}⚙️  STEP 6: Installing and running background service...${NC}"

if [ "$OS_TYPE" = "Linux" ]; then
    cat << SVC > /etc/systemd/system/introvertd.service
[Unit]
Description=Introvert Root Bootstrap Node (RBN) Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=$RBN_DIR
ExecStart=$RBN_BIN_DIR/introvertd --data-dir $RBN_DATA_DIR --relay --port 443
Environment="RUST_LOG=info"
Restart=always
RestartSec=5
StartLimitIntervalSec=0

[Install]
WantedBy=multi-user.target
SVC

    systemctl daemon-reload
    systemctl enable introvertd > /dev/null
    systemctl restart introvertd

    # Verification
    sleep 3
    if ! systemctl is-active introvertd > /dev/null; then
        echo -e "${RED}⚠️  Warning: introvertd failed to start. Let's dump recent logs:${NC}"
        sudo journalctl -u introvertd -n 30 --no-pager
        failure "Systemd daemon failed startup." "Verify that another service is not using Port 443."
    fi
    success "Systemd background service is running."
else
    echo -e "${YELLOW}⚠️  systemd is only supported on Linux.${NC}"
    echo -e "Execute the daemon manually on macOS:"
    echo -e "  ${CYAN}nohup $RBN_BIN_DIR/introvertd --data-dir $RBN_DATA_DIR --relay --port 443 > $RBN_DIR/introvertd.log 2>&1 &${NC}"
fi
press_enter

# ==============================================================================
# STEP 7: WALLET REVELATION & REGISTRATION
# ==============================================================================
echo -e "\n${CYAN}🚀 STEP 7: Deriving Solana registry address...${NC}"

WALLET_ADDRESS=""
if [ "$OS_TYPE" = "Linux" ]; then
    for i in {1..10}; do
        WALLET_ADDRESS=$(journalctl -u introvertd -n 100 --no-pager | grep -oE "Derived Operator Wallet Address: [1-9A-HJ-NP-Za-km-z]{32,44}" | awk '{print $NF}' | tail -n 1 || true)
        if [ -n "$WALLET_ADDRESS" ]; then
            break
        fi
        sleep 1
    done
else
    WALLET_ADDRESS=$("$RBN_BIN_DIR/introvertd" --data-dir "$RBN_DATA_DIR" --show-wallet | grep -E "Address" -A 1 | tail -n 1 | tr -d ' ' || true)
fi

if [ -z "$WALLET_ADDRESS" ]; then
    echo -e "${YELLOW}⚠️  Warning: Unable to parse derived Solana address automatically.${NC}"
    if [ "$OS_TYPE" = "Linux" ]; then
        echo -e "Please verify system daemon logs: ${CYAN}sudo journalctl -u introvertd -f${NC}"
    fi
else
    echo -e "${BLUE}================================================================${NC}"
    echo -e "${GREEN}🎯 Derived Operator Wallet Address: $WALLET_ADDRESS${NC}"
    echo -e "${BLUE}================================================================${NC}"

    if command -v qrencode &> /dev/null; then
        echo -e "${CYAN}Scan QR Code to transfer Devnet SOL to your RBN Operator Address:${NC}"
        qrencode -t UTF8 "$WALLET_ADDRESS"
    fi

    echo -e "\n${YELLOW}⚠️  Lease Registration Pending fee payment...${NC}"
    echo -e "To activate on-chain registry mapping, fund this wallet with at least ${GREEN}0.05 Devnet SOL${NC}."
    echo -e "Run this command on your machine with Solana CLI:"
    echo -e "  ${CYAN}solana airdrop 1 $WALLET_ADDRESS --url https://api.devnet.solana.com${NC}"
fi

success "Core RBN Server Setup Complete."
press_enter

# ==============================================================================
# STEP 8: HOW TO ACCESS THE OPERATOR WEB DASHBOARD GUI
# ==============================================================================
clear
echo -e "${BLUE}================================================================${NC}"
echo -e "${BOLD}${GREEN}🌐 HOW TO ACCESS THE TELEMETRY WEB DASHBOARD GUI${NC}"
echo -e "${BLUE}================================================================${NC}"
echo -e ""
echo -e "For security, the dashboard server binds ONLY to localhost (${BOLD}127.0.0.1:8080${NORMAL})."
echo -e "It is never exposed directly to the public internet."
echo -e ""
echo -e "${BOLD}${YELLOW}💻 CASE A: Node is Running Locally (Same Computer)${NC}"
echo -e "  1. Open your web browser."
echo -e "  2. Go to: ${CYAN}http://localhost:8080${NC}"
echo -e ""
echo -e "${BOLD}${YELLOW}☁️  CASE B: Node is Running on a Remote VPS (SSH Tunnel Required)${NC}"
echo -e "  To access the dashboard, establish a secure port forward from your local computer."
echo -e ""
echo -e "  ${BOLD}macOS / Linux Terminal:${NC}"
echo -e "    Run the following command in a new terminal window:"
echo -e "    ${CYAN}ssh -N -L 8080:localhost:8080 root@YOUR_SERVER_IP${NC}"
echo -e ""
echo -e "  ${BOLD}Windows (PowerShell / CMD):${NC}"
echo -e "    Open PowerShell and run:"
echo -e "    ${CYAN}ssh -N -L 8080:localhost:8080 root@YOUR_SERVER_IP${NC}"
echo -e ""
echo -e "  ${BOLD}Windows (PuTTY GUI):${NC}"
echo -e "    1. Navigate to: Connection ➔ SSH ➔ Tunnels."
echo -e "    2. Set Source Port to ${BOLD}8080${NORMAL} and Destination to ${BOLD}localhost:8080${NORMAL}."
echo -e "    3. Click ${BOLD}Add${NORMAL}, then open the session and log in."
echo -e ""
echo -e "  Once the tunnel is active, open your local browser and go to:"
echo -e "    👉 ${CYAN}http://localhost:8080${NC}"
echo -e ""
echo -e "${BLUE}================================================================${NC}"
echo -e "${GREEN}✨ Congratulations! Your Introvert RBN Server is established!${NC}"
echo -e "${BLUE}================================================================${NC}"
