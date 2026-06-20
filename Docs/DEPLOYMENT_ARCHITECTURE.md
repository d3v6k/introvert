# Deployment Architecture

## Overview

Introvert's deployment model is fundamentally different from traditional applications: there are no central servers to deploy. Instead, the network consists of:

1. **User Nodes** — Mobile and desktop clients
2. **Root Bootstrap Nodes (RBNs)** — Network anchors
3. **Anchor Nodes** — Optional mailbox storage

## Network Topology

### Production Architecture
```
                    ┌─────────────────────────────┐
                    │     Alibaba Cloud RBN        │
                    │     47.89.252.80:443         │
                    │     (Primary Bootstrap)      │
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────┼──────────────┐
                    │              │              │
              ┌─────▼─────┐ ┌─────▼─────┐ ┌─────▼─────┐
              │  Asia RBN  │ │  EU RBN   │ │  US RBN   │
              │  (Future)  │ │  (Future) │ │  (Future) │
              └─────┬─────┘ └─────┬─────┘ └─────┬─────┘
                    │              │              │
    ┌───────────────┼──────────────┼──────────────┼───────────────┐
    │               │              │              │               │
┌───▼───┐       ┌───▼───┐     ┌───▼───┐     ┌───▼───┐       ┌───▼───┐
│Mobile │       │Desktop│     │Mobile │     │Desktop│       │Mobile │
│Node   │       │Node   │     │Node   │     │Node   │       │Node   │
└───────┘       └───────┘     └───────┘     └───────┘       └───────┘
```

## Component Specifications

### Root Bootstrap Node (RBN)

#### Hardware Requirements
- **CPU:** 2+ vCPU
- **RAM:** 4GB minimum (8GB recommended)
- **Storage:** 20GB SSD
- **Network:** 1Gbps, unmetered
- **Bandwidth:** 100GB+ monthly

#### Software Requirements
- **OS:** Ubuntu 22.04 LTS or Debian 12
- **Runtime:** `introvertd` binary
- **Service:** systemd
- **Firewall:** Port 443 (TCP/UDP) open

#### Configuration
```bash
# Install binary
mkdir -p /opt/introvert/bin
cp introvertd /opt/introvert/bin/
chmod +x /opt/introvert/bin/introvertd

# Create data directory
mkdir -p /opt/introvert/data

# Create seed file
openssl rand -hex 32 > /opt/introvert/data/introvert.seed

# Create systemd service
cat > /etc/systemd/system/introvertd.service << 'EOF'
[Unit]
Description=Introvert Root Bootstrap Node (RBN) Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=/opt/introvert
ExecStart=/opt/introvert/bin/introvertd \
    --data-dir /opt/introvert/data \
    --relay \
    --port 443
Environment="RUST_LOG=info"
Restart=always
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

# Enable and start
systemctl daemon-reload
systemctl enable introvertd
systemctl start introvertd
```

#### Monitoring
```bash
# Check status
systemctl status introvertd

# View logs
journalctl -u introvertd -f

# Check connections
ss -tlnp | grep 443
```

### Intro-Claw on RBN

The RBN runs Intro-Claw's FCM push notification module (`for_linux/src/fcm.rs`). Additional deployment requirements:

- **Firebase service account:** Place at `/opt/introvert/config/firebase-service-account.json`
- **FCM v1 API:** Direct integration (no third-party bridge)
- **Config keys:** `intro_claw_active` and `intro_claw_ai_mode` in the `economy_meta` table control Intro-Claw features on the RBN

### User Nodes

#### Android
- **Minimum SDK:** flutter.minSdkVersion
- **Target SDK:** flutter.targetSdkVersion
- **NDK:** v28.2.13676358
- **Permissions:** INTERNET, CAMERA, MICROPHONE, LOCATION, NOTIFICATIONS

#### iOS
- **Minimum iOS:** 13.0
- **Entitlements:** Local Network, Bonjour, Camera, Microphone
- **Background Modes:** Audio, VoIP

#### macOS
- **Minimum macOS:** 10.15
- **Sandbox:** Enabled (with path resolution)
- **Entitlements:** Network, Camera, Microphone

## Deployment Scripts

### deploy_local_rbn.sh
Cross-compiles and deploys RBN from macOS:
```bash
# Prerequisites
brew install zig
cargo install cargo-zigbuild

# Build
cargo zigbuild --target x86_64-unknown-linux-gnu --release --bin introvertd

# Deploy
./deploy_local_rbn.sh
```

### deploy_rbn.sh
Remote build and deployment:
```bash
# Sync source to build machine
scp -r for_linux/ dev@buildmachine:~/introvert/

# Build on remote machine
ssh dev@buildmachine "cd ~/introvert/for_linux && cargo build --release"

# Deploy to production
./deploy_rbn.sh
```

### scripts/build_android.sh
Android cross-compilation:
```bash
# Auto-detects NDK
# Builds for arm64-v8a and x86_64
# Copies to android/app/src/main/jniLibs/
./scripts/build_android.sh
```

## Scaling Considerations

### RBN Scaling
- **Horizontal:** Add more RBNs in different regions
- **Vertical:** Increase RAM/CPU for higher connection limits
- **Geographic:** Deploy in Asia, EU, US for global coverage

### Connection Limits
- **Current:** 1,000,000 concurrent connections per RBN
- **Memory:** ~100MB per 10,000 connections
- **CPU:** ~5% per 10,000 connections

### Bandwidth
- **Per Connection:** ~1-10 KB/s average
- **Per RBN:** ~1 Gbps for 100,000 connections
- **Monthly:** ~10TB for 100,000 active users

## High Availability

### RBN Redundancy
- Deploy 3+ RBNs in different regions
- Client nodes try multiple bootstrap nodes
- Automatic failover if primary RBN is down

### Data Persistence
- RBN state stored in `/opt/introvert/data/`
- Backup seed file securely
- Database is ephemeral (rebuilt on restart)

## Security Hardening

### RBN Security
```bash
# Firewall
ufw allow 22/tcp    # SSH (restrict to your IP)
ufw allow 443/tcp   # libp2p TCP
ufw allow 443/udp   # libp2p QUIC
ufw enable

# SSH hardening
# Disable password auth
# Use key-based authentication
# Restrict to specific IPs

# Automatic updates
unattended-upgrades
```

### Monitoring
```bash
# System monitoring
htop
iotop
nethogs

# Connection monitoring
ss -s
netstat -an | grep 443 | wc -l

# Log monitoring
journalctl -u introvertd --since "1 hour ago"
```

## Backup Procedures

### Seed Backup
```bash
# Backup seed file
cp /opt/introvert/data/introvert.seed /secure/backup/

# Verify backup
cat /secure/backup/introvert.seed | wc -c  # Should be 65 (64 hex + newline)
```

### Configuration Backup
```bash
# Backup systemd service
cp /etc/systemd/system/introvertd.service /secure/backup/

# Backup entire config
tar -czf /secure/backup/introvert-config-$(date +%Y%m%d).tar.gz \
    /opt/introvert/ \
    /etc/systemd/system/introvertd.service
```

## Disaster Recovery

### RBN Recovery
1. Provision new server
2. Install `introvertd` binary
3. Restore seed file from backup
4. Start service
5. Update bootstrap list if IP changed

### Client Recovery
1. Reinstall app
2. Enter seed phrase
3. Identity restored automatically
4. Contacts and history synced from mesh
