# RBN Stress Test Plan

> **For agentic workers:** Use compose:execute to implement this plan task-by-task.

**Goal:** Run the stress_tester binary against the live RBN to validate Phase 2 scaling changes under load.

**Architecture:** Compile stress_tester on Ubuntu build machine, run against RBN at 47.89.252.80, monitor logs for performance metrics.

**Tech Stack:** Rust, tokio, libp2p, SSH to dev@thinkpad.local

## Global Constraints

- Build machine: dev@thinkpad.local (Ubuntu, passwordless SSH from Mac Mini)
- RBN: 47.89.252.80 (Alibaba Cloud)
- Stress tester connects to RBN, not to each other
- Monitor for 10+ minutes to observe steady-state behavior

---

### Task 1: Compile Stress Tester on Build Machine

**Files:**
- Source: `for_linux/src/bin/stress_tester.rs`
- Build: `for_linux/Cargo.toml`

- [ ] **Step 1: SSH to build machine and compile**

```bash
ssh dev@thinkpad.local 'export PATH=$HOME/.cargo/bin:$PATH && cd ~/introvert/for_linux && cargo build --release --bin stress_tester 2>&1 | tail -5'
```

Expected: `Finished release [optimized] target(s) in Xm Ys`

- [ ] **Step 2: Verify binary exists**

```bash
ssh dev@thinkpad.local 'ls -la ~/introvert/for_linux/target/release/stress_tester'
```

Expected: Binary exists, ~10-20MB

---

### Task 2: Run Stress Test with 50 Nodes

**Why 50 first:** Validate the test works before scaling to 100+. 50 nodes simulates moderate load.

- [ ] **Step 1: Start stress test in background on build machine**

```bash
ssh dev@thinkpad.local 'nohup ~/introvert/for_linux/target/release/stress_tester 50 > /tmp/stress_test.log 2>&1 & echo "PID: $!"'
```

Expected: Returns PID, stress test starts in background

- [ ] **Step 2: Verify stress test is running**

```bash
ssh dev@thinkpad.local 'ps aux | grep stress_tester | grep -v grep'
```

Expected: Process running with 50 virtual nodes

- [ ] **Step 3: Check initial connection activity**

```bash
ssh dev@thinkpad.local 'tail -20 /tmp/stress_test.log'
```

Expected: Nodes connecting to RBN, no fatal errors

---

### Task 3: Monitor RBN Under Load

**Duration:** 10 minutes minimum

- [ ] **Step 1: Check RBN heartbeat and peer count**

```bash
ssh root@47.89.252.80 "journalctl -u introvertd --no-pager --since '2 min ago' | grep 'Swarm Heartbeat' | tail -5"
```

Expected: Peer count increasing as stress nodes connect

- [ ] **Step 2: Check for errors or panics**

```bash
ssh root@47.89.252.80 "journalctl -u introvertd --no-pager --since '5 min ago' | grep -iE 'error|panic|fatal' | head -10"
```

Expected: No errors (or only pre-existing warnings)

- [ ] **Step 3: Check memory usage**

```bash
ssh root@47.89.252.80 "ps aux | grep introvertd | grep -v grep | awk '{print \"CPU:\", \$3\"%\", \"MEM:\", \$4\"%\", \"RSS:\", \$6\"KB\"}'"
```

Expected: RSS < 500MB, CPU < 50%

- [ ] **Step 4: Check push activity (should be minimal — no offline peers)**

```bash
ssh root@47.89.252.80 "journalctl -u introvertd --no-pager --since '5 min ago' | grep -c 'Triggering Push'"
```

Expected: 0 or very low (stress nodes are all online)

- [ ] **Step 5: Check gossipsub message flow**

```bash
ssh root@47.89.252.80 "journalctl -u introvertd --no-pager --since '5 min ago' | grep -c 'Publishing\|Published'"
```

Expected: Messages flowing through gossipsub

---

### Task 4: Scale to 100 Nodes (If 50 Succeeds)

- [ ] **Step 1: Stop current stress test**

```bash
ssh dev@thinkpad.local 'pkill -f stress_tester'
```

- [ ] **Step 2: Start with 100 nodes**

```bash
ssh dev@thinkpad.local 'nohup ~/introvert/for_linux/target/release/stress_tester 100 > /tmp/stress_test_100.log 2>&1 & echo "PID: $!"'
```

- [ ] **Step 3: Monitor for 10 minutes (same checks as Task 3)**

---

### Task 5: Collect Results and Cleanup

- [ ] **Step 1: Collect RBN metrics**

```bash
ssh root@47.89.252.80 "journalctl -u introvertd --no-pager --since '30 min ago' | grep -c 'Swarm Heartbeat'"
ssh root@47.89.252.80 "ps aux | grep introvertd | grep -v grep | awk '{print \"CPU:\", \$3\"%\", \"MEM:\", \$4\"%\", \"RSS:\", \$6\"KB\"}'"
```

- [ ] **Step 2: Stop stress test**

```bash
ssh dev@thinkpad.local 'pkill -f stress_tester'
```

- [ ] **Step 3: Check for any leaked connections**

```bash
ssh root@47.89.252.80 "ss -tnp | grep introvertd | wc -l"
```

Expected: Connection count drops after stress test stops

---

## Success Criteria

| Metric | Target | Actual |
|--------|--------|--------|
| 50 nodes connected | Yes | |
| 100 nodes connected | Yes | |
| RBN RSS < 500MB at 100 nodes | Yes | |
| RBN CPU < 50% at 100 nodes | Yes | |
| No panics or fatal errors | Yes | |
| Heartbeat log gap < 30s | Yes | |
| Push dedup working (no duplicates) | Yes | |
