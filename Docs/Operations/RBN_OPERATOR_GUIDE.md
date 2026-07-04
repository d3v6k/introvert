# RBN Operator Guide

## Overview

This guide is for operators running Root Bootstrap Nodes (RBNs) on the Introvert network. RBNs serve as network anchors, providing:
- Bootstrap discovery for new nodes
- Relay circuit hosting
- Mailbox storage for offline peers
- Handle registry witnessing

## Requirements

### Hardware
- **CPU:** 2+ vCPU (4+ recommended)
- **RAM:** 4GB minimum (8GB recommended)
- **Storage:** 20GB SSD
- **Network:** 1Gbps, unmetered

### Software
- **OS:** Ubuntu 22.04 LTS or Debian 12
- **Binary:** `introvertd` (provided)
- **Service:** systemd

## Installation

### 1. Download Binary
```bash
# From release
wget https://github.com/introvert/introvert/releases/download/v0.1.0/introvertd-linux-x86_64
chmod +x introvertd-linux-x86_64
mv introvertd-linux-x86_64 /opt/introvert/bin/introvertd
```

### 2. Create Directories
```bash
mkdir -p /opt/introvert/bin
mkdir -p /opt/introvert/data
```

### 3. Generate Seed
```bash
openssl rand -hex 32 > /opt/introvert/data/introvert.seed
chmod 600 /opt/introvert/data/introvert.seed
```

**IMPORTANT:** Back up this seed file! It's your node's identity.

### 4. Configure Firewall
```bash
ufw allow 22/tcp    # SSH (restrict to your IP)
ufw allow 443/tcp   # libp2p TCP
ufw allow 443/udp   # libp2p QUIC
ufw enable
```

### 5. Create systemd Service
```bash
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
```

### 6. Start Service
```bash
systemctl daemon-reload
systemctl enable introvertd
systemctl start introvertd
```

## Management

### Status
```bash
systemctl status introvertd
```

### Logs
```bash
# Follow logs
journalctl -u introvertd -f

# Last 100 lines
journalctl -u introvertd -n 100

# Since specific time
journalctl -u introvertd --since "1 hour ago"
```

### Restart
```bash
systemctl restart introvertd
```

### Stop
```bash
systemctl stop introvertd
```

## Monitoring

### Connection Count
```bash
ss -tlnp | grep 443 | wc -l
```

### Bandwidth Usage
```bash
# Install nethogs
apt install nethogs

# Monitor
nethogs
```

### Memory Usage
```bash
# Check process memory
ps aux | grep introvertd

# System memory
free -h
```

### Disk Usage
```bash
df -h /opt/introvert
```

## Configuration Options

### CLI Arguments
```bash
introvertd [OPTIONS]

Options:
  -s, --seed-file <PATH>        Seed file path
  -d, --db-path <PATH>          Database path [default: introvert.db]
  -p, --port <PORT>             Listen port [default: 443]
  -r, --relay                   Enable relay mode
      --max-connections <N>      Max connections [default: 1000000]
      --liveness-check <SECS>   Liveness check interval [default: 300]
      --tunnel-port <PORT>      WebSocket tunnel port [default: 80]
```

### Environment Variables
```bash
RUST_LOG=info          # Logging level
INTROVERT_SEED=...     # Seed from environment
```

## Updating

### 1. Stop Service
```bash
systemctl stop introvertd
```

### 2. Backup Current Binary
```bash
cp /opt/introvert/bin/introvertd /opt/introvert/bin/introvertd.bak
```

### 3. Replace Binary
```bash
cp /path/to/new/introvertd /opt/introvert/bin/introvertd
chmod +x /opt/introvert/bin/introvertd
```

### 4. Start Service
```bash
systemctl start introvertd
```

### 5. Verify
```bash
systemctl status introvertd
journalctl -u introvertd -n 20
```

## Troubleshooting

### Service Won't Start
```bash
# Check logs
journalctl -u introvertd -n 50

# Common issues:
# - Port 443 already in use
# - Seed file missing
# - Permission denied
```

### High Memory Usage
```bash
# Check connections
ss -s

# Restart if needed
systemctl restart introvertd
```

### No Connections
```bash
# Check firewall
ufw status

# Check port listening
ss -tlnp | grep 443

# Check internet connectivity
ping 8.8.8.8
```

### "Text file busy" Error
```bash
# Stop service first
systemctl stop introvertd

# Then replace binary
cp /path/to/new/introvertd /opt/introvert/bin/introvertd

# Start service
systemctl start introvertd
```

## Security Best Practices

### SSH Hardening
```bash
# /etc/ssh/sshd_config
PermitRootLogin prohibit-password
PasswordAuthentication no
AllowUsers your_user
```

### Automatic Updates
```bash
apt install unattended-upgrades
dpkg-reconfigure unattended-upgrades
```

### Log Rotation
```bash
# /etc/logrotate.d/introvertd
/var/log/introvertd/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
}
```

### Monitoring Alerts
```bash
# Install monitoring
apt install prometheus-node-exporter

# Add to crontab
*/5 * * * * /opt/introvert/bin/check_health.sh
```

## Backup Procedures

### Seed Backup (CRITICAL)
```bash
# Backup to secure location
cp /opt/introvert/data/introvert.seed /secure/backup/

# Verify
cat /secure/backup/introvert.seed | wc -c  # Should be 65
```

### Configuration Backup
```bash
# Backup entire config
tar -czf /backup/introvert-$(date +%Y%m%d).tar.gz \
    /opt/introvert/ \
    /etc/systemd/system/introvertd.service
```

### Automated Backup
```bash
# Add to crontab
0 2 * * * /opt/introvert/bin/backup.sh
```

## Scaling

### Adding More RBNs
1. Provision new server
2. Install `introvertd`
3. Generate new seed
4. Start service
5. Add to bootstrap list in `src/network/config.rs`

### Load Balancing
- Deploy RBNs in multiple regions
- Clients automatically discover and connect
- No manual load balancing required

## Support

### Resources
- GitHub Issues: [link]
- Discord: [link]
- Documentation: `Docs/` directory

### Reporting Issues
Include:
- Server specifications
- OS version
- `introvertd` version
- Logs (last 100 lines)
- Connection count
- Memory usage
