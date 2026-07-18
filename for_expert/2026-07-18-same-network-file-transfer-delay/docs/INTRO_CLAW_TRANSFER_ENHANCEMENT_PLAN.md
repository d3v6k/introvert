# IntroClaw — File Transfer Intelligence Enhancement Plan

## Current IntroClaw Capabilities (What Exists Today)

| Module | Does | Feeds Transfer? |
|--------|------|----------------|
| `AdaptiveChunkSizer` | Picks chunk size & pipeline depth from throughput history | ✅ `get_optimal_pipeline_depth()` called in stall watchdog |
| `BandwidthMonitor` | Per-peer rolling speed samples | ✅ `record_throughput()` called on chunk receipt |
| `ConnectionHealthScorer` | Per-peer health score [0–1] from success/fail | Partially — `record_transfer_success/failure()` exist but not always called |
| `PeerReconnectionScorer` | Counts disconnects per peer per hour | ✅ Used to pre-establish relays for unstable peers |
| `ConnectionPreWarmer` | Proactively dials frequent contacts | Partially — only considers contacts, not active transfer seeders |
| `ConnectionStateCycler` | Escalates: direct → tunnel → VPN configs → offline | ✅ Active on zero-peer events |
| `NodeFileProactiveCacher` | Node: caches files for offline group members | Only in node mode |
| `ClawTickContext.active_transfer_hashes` | List of in-progress transfer IDs | ⚠️ Only used for prefetch gating — no smart relay actions |

## What IntroClaw Does NOT Currently Do for File Transfers

1. **No relay circuit pre-warming for ACTIVE seeder peers** — `ConnectionPreWarmer` only considers contacts, never active transfer seeders
2. **No transfer-aware reconnection cycling** — `ConnectionStateCycler` escalates for zero-peer events, not for stalled transfers
3. **No in-flight limit adaptation** — relay congestion is not fed back to `AdaptiveChunkSizer`
4. **No network-type-aware chunk tuning** — `connectivity_type` is available in `ClawTickContext` but AdaptiveChunkSizer ignores it
5. **No stall prediction** — IntroClaw doesn't see `last_update` timestamps or stall state of transfers
6. **No relay circuit health scoring** — the relay server itself is never health-scored
7. **No multi-hop transfer orchestration** — peers with active transfers aren't coordinated for proximity routing
8. **No transfer ETA or progress signaling** — Claw doesn't know % completion, so it can't back off on low-priority vs high-priority transfers

---

## Proposed Enhancements

### Enhancement 1 — Transfer-Aware ClawTickContext (Foundation)

**What:** Enrich `ClawTickContext` with real transfer state so all modules can reason about it.

**Add to `ClawTickContext`:**
```rust
pub struct ClawTickContext {
    // ... existing fields ...
    
    /// Peer IDs of active seeders we're pulling from
    pub active_seeder_peers: Vec<String>,
    /// Peer IDs of peers we are currently seeding to  
    pub active_receiver_peers: Vec<String>,
    /// Stalled transfers: (transfer_id, seeder_peer_id, seconds_since_last_chunk)
    pub stalled_transfers: Vec<(String, String, u64)>,
    /// Bytes-per-second per peer (measured over last 10 chunks)
    pub peer_throughput_bps: HashMap<String, f64>,
    /// Number of pending_messages per peer
    pub pending_message_count: HashMap<String, usize>,
    /// Current relay reservation peer IDs
    pub relay_peers: Vec<String>,
    /// Whether a circuit is currently established (not just reserved)
    pub has_relay_circuit: bool,
}
```

**Where to populate:** In the `intro_claw_interval` tick handler in `network/mod.rs` (around L1230).

---

### Enhancement 2 — Transfer-Aware Pre-Warming (Highest Impact)

**What:** When a file transfer is active, IntroClaw proactively dials the seeder peer via relay and maintains the circuit. This eliminates the 8s stall → re-dial latency.

**New module in `intro_claw.rs`: `TransferCircuitPrewarmer`**

```rust
pub struct TransferCircuitPrewarmer {
    /// Seeder peers we have proactively dialed: peer_id -> last_dial_at
    prewarmed_seeders: HashMap<String, Instant>,
    /// Cooldown between redials (5s)
    dial_cooldown: Duration,
}

impl TransferCircuitPrewarmer {
    pub fn get_seeders_to_prewarm(
        &mut self,
        seeder_peers: &[String],
        connected_peers: &[String],
        stalled_transfers: &[(String, String, u64)],
    ) -> Vec<String> {
        let connected_set: HashSet<&String> = connected_peers.iter().collect();
        let mut targets = Vec::new();
        
        for peer in seeder_peers {
            // Already connected — no action needed
            if connected_set.contains(peer) { continue; }
            
            // Check cooldown
            if let Some(last_dial) = self.prewarmed_seeders.get(peer) {
                if last_dial.elapsed() < self.dial_cooldown { continue; }
            }
            
            self.prewarmed_seeders.insert(peer.clone(), Instant::now());
            targets.push(peer.clone());
        }
        
        // Also re-dial stalled seeders regardless of cooldown
        for (_, stalled_peer, stall_secs) in stalled_transfers {
            if *stall_secs > 5 && !connected_set.contains(stalled_peer) {
                if !targets.contains(stalled_peer) {
                    targets.push(stalled_peer.clone());
                }
            }
        }
        
        targets
    }
}
```

**Effect:** `ClawActions.pre_establish_relays` gets populated with seeder peers. `execute_claw_actions` in `network/mod.rs` calls `dial_relay_path(peer, true)` for each. Result: circuit is alive BEFORE the stall watchdog fires.

---

### Enhancement 3 — Network-Adaptive Transfer Policy

**What:** IntroClaw adjusts in-flight limits, chunk sizes, and pacing based on:
- Current network type (WiFi vs cellular vs VPN)
- Live RTT (from `peer_throughput_bps` data)  
- Battery level
- Whether a VoIP call is active

**New method on `IntroClawService`: `get_transfer_policy(peer_id, ctx) -> TransferPolicy`**

```rust
pub struct TransferPolicy {
    /// Recommended chunk size in bytes
    pub chunk_size: u32,
    /// Recommended pipeline depth (parallel in-flight requests)
    pub pipeline_depth: u32,
    /// Milliseconds between chunk requests (pacing)
    pub pacing_delay_ms: u64,
    /// Whether to use push or pull mode
    pub prefer_push: bool,
    /// In-flight request limit
    pub inflight_limit: u32,
}

impl IntroClawService {
    pub fn get_transfer_policy(&self, peer_id: &str, ctx: &ClawTickContext) -> TransferPolicy {
        let throughput = ctx.peer_throughput_bps.get(peer_id).copied().unwrap_or(0.0);
        let is_relayed = !ctx.active_seeder_peers.contains(&peer_id.to_string()) 
            || ctx.relay_peers.contains(&peer_id.to_string());
        let call_active = self.voip_monitor.is_call_active();
        
        // VoIP priority — collapse pipeline to prevent media contention
        if call_active {
            return TransferPolicy { chunk_size: 64*1024, pipeline_depth: 2, 
                pacing_delay_ms: 100, prefer_push: false, inflight_limit: 2 };
        }
        
        // Battery-aware caps
        if self.battery_throttler.should_emergency_throttle() {
            return TransferPolicy { chunk_size: 64*1024, pipeline_depth: 2,
                pacing_delay_ms: 200, prefer_push: false, inflight_limit: 2 };
        }
        
        match ctx.connectivity_type {
            // WiFi — full speed
            1 | 3 => {
                let (chunk, depth, inflight) = if throughput > 5_000_000.0 {
                    (512*1024, 16, 12) // >5MB/s: 512KB chunks, 16 pipeline
                } else if throughput > 1_000_000.0 {
                    (256*1024, 8, 8)   // >1MB/s: 256KB chunks, 8 pipeline
                } else {
                    (128*1024, 4, 4)   // slower: 128KB chunks
                };
                TransferPolicy { chunk_size: chunk, pipeline_depth: depth,
                    pacing_delay_ms: if is_relayed { 20 } else { 5 },
                    prefer_push: !is_relayed, inflight_limit: inflight }
            }
            // Cellular — conservative
            2 => {
                TransferPolicy { chunk_size: 128*1024, pipeline_depth: 4,
                    pacing_delay_ms: 50, prefer_push: false, inflight_limit: 4 }
            }
            // VPN (5) — needs careful tuning (VPN often has MTU issues)
            5 => {
                TransferPolicy { chunk_size: 64*1024, pipeline_depth: 4,
                    pacing_delay_ms: 30, prefer_push: false, inflight_limit: 4 }
            }
            // Unknown — safe defaults
            _ => {
                TransferPolicy { chunk_size: 256*1024, pipeline_depth: 8,
                    pacing_delay_ms: 20, prefer_push: !is_relayed, inflight_limit: 8 }
            }
        }
    }
}
```

**Where to call:** In `process_outgoing_file` and the stall watchdog `pull_retry_interval`, replace hardcoded values with `intro_claw.get_transfer_policy(peer_id, &ctx)`.

---

### Enhancement 4 — Relay Circuit Health Scorer

**What:** Track relay circuit drops and recoveries per relay peer. Score the relay server's stability. If the relay has dropped >3 times in 60 seconds, mark it as `Unstable` and take action (force mesh refresh, prioritize direct dial).

**New module: `RelayCircuitHealthScorer`**

```rust
pub struct RelayCircuitDrop {
    pub relay_peer_id: String,
    pub dropped_at: Instant,
    pub circuit_age_secs: u64,
}

pub struct RelayCircuitHealthScorer {
    drops: VecDeque<RelayCircuitDrop>,
    drops_window_secs: u64,
    instability_threshold: usize, // drops in window to be considered unstable
}

impl RelayCircuitHealthScorer {
    pub fn record_drop(&mut self, relay_peer_id: &str, circuit_age_secs: u64) {
        // Prune old entries
        let cutoff = Instant::now() - Duration::from_secs(self.drops_window_secs);
        self.drops.retain(|d| d.dropped_at > cutoff);
        self.drops.push_back(RelayCircuitDrop {
            relay_peer_id: relay_peer_id.to_string(),
            dropped_at: Instant::now(),
            circuit_age_secs,
        });
        warn!("[IntroClaw/Relay] Circuit drop recorded for {} (age={}s, total_drops_60s={})",
            relay_peer_id, circuit_age_secs, self.drops.len());
    }
    
    pub fn is_unstable(&self, relay_peer_id: &str) -> bool {
        let recent_drops = self.drops.iter()
            .filter(|d| d.relay_peer_id == relay_peer_id)
            .count();
        recent_drops >= self.instability_threshold
    }
    
    pub fn get_avg_circuit_age_secs(&self, relay_peer_id: &str) -> Option<u64> {
        let relay_drops: Vec<_> = self.drops.iter()
            .filter(|d| d.relay_peer_id == relay_peer_id)
            .collect();
        if relay_drops.is_empty() { return None; }
        Some(relay_drops.iter().map(|d| d.circuit_age_secs).sum::<u64>() / relay_drops.len() as u64)
    }
    
    pub fn should_force_refresh(&self) -> bool {
        // Total drops across all relays in window > threshold
        self.drops.len() >= self.instability_threshold * 2
    }
}
```

**Where to call:** 
- `record_drop()` in `ConnectionClosed` handler when the closed peer is in `relay_reservations`
- `is_unstable()` and `should_force_refresh()` in the `fast_reconnect_interval` handler

---

### Enhancement 5 — Transfer Stall Prediction and Pre-emptive Recovery

**What:** Instead of waiting for the stall watchdog to fire (8s+), IntroClaw detects stalls 3s early by comparing `last_update` age against the rolling throughput of the peer. If a peer was doing 1MB/s but has sent nothing for 3s, that's a stall — pre-emptively re-dial.

**New `stall_predictor` on `IntroClawService`:**

```rust
pub struct StallPredictor {
    /// Transfer ID -> (last_chunk_received_at, avg_inter_chunk_interval_ms)
    transfer_timing: HashMap<String, (Instant, u64)>,
}

impl StallPredictor {
    pub fn record_chunk(&mut self, transfer_id: &str, inter_chunk_ms: u64) {
        let entry = self.transfer_timing.entry(transfer_id.to_string())
            .or_insert((Instant::now(), inter_chunk_ms));
        entry.0 = Instant::now();
        // EMA of inter-chunk interval
        entry.1 = (entry.1 * 7 + inter_chunk_ms) / 8;
    }
    
    pub fn is_likely_stalled(&self, transfer_id: &str, stall_multiplier: u64) -> bool {
        if let Some((last_chunk, avg_interval)) = self.transfer_timing.get(transfer_id) {
            let silence = last_chunk.elapsed().as_millis() as u64;
            let threshold = avg_interval * stall_multiplier; // e.g., 5x average interval
            silence > threshold.max(3000) // minimum 3s
        } else {
            false
        }
    }
}
```

**Where to call:** In the `fast_poll_interval` (1s tick), check `is_likely_stalled()` for all active incoming_transfers. If stalled, immediately `heal_peers.push(seeder_id)` — which triggers `dial_relay_path`.

---

### Enhancement 6 — ClawActions: Transfer Control

**Add to `ClawActions`:**
```rust
pub struct ClawActions {
    // ... existing fields ...
    
    /// Seeder peers to proactively pre-connect for active transfers
    pub prewarm_transfer_seeders: Vec<String>,
    /// Transfer IDs to resume (send initial chunk requests for)
    pub resume_stalled_transfers: Vec<(String, String)>, // (transfer_id, seeder_peer_id)
    /// Force specific pending_messages flush for these peer IDs
    pub flush_pending_for_peers: Vec<String>,
    /// Recommended TransferPolicy per peer
    pub transfer_policies: HashMap<String, TransferPolicy>,
}
```

**In `execute_claw_actions`:**
- `prewarm_transfer_seeders` → call `dial_relay_path(peer, true)` for each
- `resume_stalled_transfers` → send `FileChunkRequest` for first missing chunk
- `flush_pending_for_peers` → drain `pending_messages[peer]` immediately via `ForwardMeshSignaling`
- `transfer_policies` → cache in `NetworkService` and consult before each chunk send

---

## Implementation Order (Lowest Risk First)

| Step | Change | Risk | Impact |
|------|--------|------|--------|
| 1 | Enrich `ClawTickContext` with seeder peers, stall data, throughput | Low | Foundation for all others |
| 2 | `TransferCircuitPrewarmer` → `pre_establish_relays` for seeder peers | Low | Eliminates 8s stall |
| 3 | `RelayCircuitHealthScorer` → log drops + predict instability | Low | Diagnostic first |
| 4 | `get_transfer_policy()` → adaptive chunk/pipeline per network type | Medium | Speed boost |
| 5 | `StallPredictor` → pre-emptive re-dial via `heal_peers` | Medium | Reduces stall recovery from 8s to 3s |
| 6 | Relay circuit instability → trigger `ForceMeshRefresh` | Medium | Addresses 13s drop issue |

---

## Where Each Module Plugs In (File Map)

```
src/intro_claw.rs
├── ClawTickContext — add seeder_peers, stalled_transfers, peer_throughput_bps
├── ClawActions — add prewarm_transfer_seeders, resume_stalled_transfers
├── IntroClawService — new fields: relay_circuit_scorer, transfer_prewarmer, stall_predictor
├── tick() step 16.5 — NEW: transfer circuit pre-warming
├── get_transfer_policy() — NEW: adaptive policy per peer/network
└── record_relay_drop() — NEW: public API for network/mod.rs to call

src/network/mod.rs  
├── intro_claw_interval L1230 — enrich ClawTickContext with transfer state
├── execute_claw_actions() — handle prewarm_transfer_seeders, resume_stalled_transfers
├── pull_retry_interval L434 — call get_transfer_policy() for pipeline depth
├── ConnectionClosed handler — call intro_claw.record_relay_drop() for relay peers
└── handle_file_chunk() — call intro_claw.stall_predictor.record_chunk()
```
