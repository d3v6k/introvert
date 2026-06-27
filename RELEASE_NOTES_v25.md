# Introvert Stable Release v25 Notes & RBN Deployment Guide

## 1. Version Release Notes
This release represents a fully functional, highly optimized, and audited version of the Introvert privacy-focused P2P communication system, featuring robust dialog state management, verified group membership approvals, and complete rebuilding documentation.

### Key Resolved Issues & Features in Version 25
* **Context Deactivation Crash Resolution**: Captured the `ScaffoldMessengerState` instance prior to executing `Navigator.pop(context)`. This avoids inherited widget lookup failures and runtime crashes on the popped context, keeping UI routing perfectly stable.
* **DHT Duplicate Dialog Suppression**: Implemented overlay tracking variables (`_isHandleResolvedDialogOpen`, `_activeConnectionRequestPeerIds`, `_activeGroupInviteIds`) to catch multiple parallel Kademlia search callbacks (Event Type 33) and discard duplicate dialog stack pushes. Reset tracking variables inside the dialog's `.then((_) { ... })` completion callback.
* **INR Verified Quorum Consensus**: DHT resolves handles (`i@handle`) and validates signatures ledger quorum across RBN witnesses.
* **Group Join Approvals/Rejections**: Added support for FFI methods `introvert_group_approve_join` and `introvert_group_reject_join` to handle decentralized group membership requests.
* **Comprehensive Rebuild Documentation**: Expanded all blueprints, FFI registries, database schemas, and event matrices in `Docs/` to make the codebase completely self-contained.

---

## 2. RBN (Root Bootstrap Node) Compilation Guide
The production RBN daemon (`introvertd`) manages client discovery and DHT bootstrap operations. Due to the 1GB RAM limitation on the Alibaba RBN host, compiling directly on the RBN server will cause Out-Of-Memory (OOM) compiler crashes.

**Rules for Compilation**:
* Always cross-compile the RBN binary using the `deploy_local_rbn.sh` script or on a build machine with >2GB RAM.
* Build native ELF (Linux) binaries using the `for_linux/` source tree.

### Compilation Steps (Local Cross-Compilation):
1. Install cross-compilers:
   ```bash
   brew install zig
   cargo install cargo-zigbuild
   ```
2. Build and deploy:
   ```bash
   ./deploy_local_rbn.sh
   ```

### Compilation Steps (On Build Machine with >2GB RAM):
1. Sync source files to target build machine:
   ```bash
   scp -r for_linux/src/ for_linux/Cargo.toml for_linux/Cargo.lock dev@buildmachine.local:~/introvert/for_linux/
   ```
2. Build release binary:
   ```bash
   ssh dev@buildmachine.local "export PATH=\$HOME/.cargo/bin:\$PATH && cd ~/introvert/for_linux && cargo build --release --bin introvertd"
   ```

---

## 3. RBN Service Daemon Update & Deployment Guide
To safely deploy the updated daemon on the production RBN server without losing state:

1. Stop the running service:
   ```bash
   ssh root@47.89.252.80 "systemctl stop introvertd"
   ```
2. Deploy compiled binary:
   ```bash
   scp target/x86_64-unknown-linux-gnu/release/introvertd root@47.89.252.80:/opt/introvert/bin/introvertd
   ```
3. Reload service configs and restart:
   ```bash
   ssh root@47.89.252.80 "systemctl daemon-reload && systemctl start introvertd"
   ```
4. Verify daemon logs:
   ```bash
   ssh root@47.89.252.80 "journalctl -u introvertd -n 100 -f"
   ```
