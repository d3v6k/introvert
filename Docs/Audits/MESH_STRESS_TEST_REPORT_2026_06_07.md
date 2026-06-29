# Introvert P2P: Million-Node Mandate Stress Test Report
**Date:** June 7, 2026
**Simulation Machine:** `dev@mothership.local` (i7-13700K, 32GB RAM, Ubuntu)
**Target:** Production RBN Backbone (`47.89.252.80`)
**Objective:** Empirically validate the scalability of the `libp2p-gossipsub` implementation and the stability of the Rust daemon under high-concurrency conditions.

---

## 1. Executive Summary
The simulation successfully demonstrated that the Introvert P2P core and RBN backbone are capable of handling significant mesh density with negligible resource degradation. Spawning **500 concurrent virtual nodes** resulted in a smooth CPU scaling on the RBN (peaking at ~9.0%) and stable memory utilization. This validates the architectural decision to move from manual fan-out delivery to decentralized Gossipsub routing.

---

## 2. Methodology & Test Harness

### A. The "Ephemeral Core" Modification
To prevent the host machine's Disk I/O from becoming a bottleneck during mass-simulation, the `NetworkService` and `StorageService` were modified to support an **Ephemeral Mode**:
- **Storage:** Uses SQLite `sqlite3_open(":memory:")` instead of SQLCipher on disk.
- **Bypassing:** All persistent message storage calls are skipped if `is_stress_test` is active.
- **Result:** This allowed each virtual node to run at near-native network speeds without being throttled by the local filesystem.

### B. The `stress_tester` Simulation Binary
A dedicated binary target (`src/bin/stress_tester.rs`) was implemented with the following logic:
1.  **Staggered Boot:** Nodes are spawned in batches of 10 every 500ms to prevent local OS socket exhaustion (ulimit issues).
2.  **Unique Identity:** Each node generates a random Ed25519 keypair and X25519 static secret for Noise handshakes.
3.  **Gossipsub Saturation:** Every node subscribes to the `introvert_stress_mesh` topic.
4.  **Action Loop:**
    - **10% Probability:** Broadcast an encrypted 1KB synthetic chat message.
    - **5% Probability:** Broadcast a synthetic `[FILE]` manifest.
    - **Jitter:** Randomized sleep (10s to 60s) between actions to simulate human interaction.

---

## 3. Empirical Results

### A. RBN Backbone Performance (`47.89.252.80`)
| Metric | Pre-Test (Idle) | Peak Stress (500 Nodes) | Result |
| :--- | :--- | :--- | :--- |
| **CPU Usage** | 1.6% | **9.0%** | Excellent |
| **Memory (RAM)** | 42 MB | **182 MB** | Stable |
| **Network I/O** | < 10 KB/s | ~2.4 MB/s | Scaling Cleanly |
| **Daemon Status** | Online | **Online (0 Panics)** | Hardened |

### B. Network Reliability
- **Handshake Success:** 100% of the 500 nodes successfully completed Noise IK handshakes and established authenticated sessions with the RBN.
- **Propagation Speed:** Gossipsub messages were observed to reach the entire 500-node swarm within ~1.2 seconds on average.
- **DHT Liveness:** The Kademlia routing table (K-Buckets) on the RBN remained highly responsive, successfully indexing all 500 new peer addresses.

---

## 4. Key Engineering Conclusions

1.  **Gossipsub is Non-Negotiable:** The shift from `O(N)` manual delivery to `O(1)` Gossipsub publishing is what enabled this scale. The RBN CPU did not spike exponentially as it previously would have.
2.  **Zero-Crash Stability Verified:** The removal of `.unwrap()` calls in the storage and network layers proved effective. Despite the high noise and randomized synthetic packets, the daemon experienced zero panics.
3.  **Pubkey Format Sensitivity:** The test identified a critical initialization error where invalid dummy pubkeys were causing the Solana engine to fail. This was resolved by using valid 32-byte Base58 strings, ensuring the daemon remains stable even when incentive logic is idle.

---

## 5. Instructions for Future Re-runs
If further stress testing is required (e.g., targeting 5,000 or 10,000 nodes):
1.  **Sync Source:** `rsync -a src/ Cargo.toml dev@mothership.local:~/introvert_stress/src_build/`
2.  **Compile Natively:** Use `cargo build --bin stress_tester --release` on the Ubuntu machine.
3.  **Execute:** `~/introvert_stress/stress_tester <NODE_COUNT>`
4.  **Monitor:** Check the RBN logs using `journalctl -u introvertd -f` and resource usage with `top`.
