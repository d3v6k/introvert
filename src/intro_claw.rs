//! Intro-Claw: Local automation engine for Introvert
//!
//! Runs deterministic, rule-based maintenance tasks on timers.
//! Zero network calls in Offline mode. All logic is local.

use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use libp2p::PeerId;
use crate::storage::StorageService;

/// Context passed to IntroClaw on each tick cycle
pub struct ClawTickContext {
    pub battery_pct: i32,
    pub is_background: bool,
    pub connected_peers: Vec<String>,
    pub mdns_discovered: Vec<String>,
    pub active_transfer_hashes: Vec<String>,
}

/// Core automation engine orchestrating all intro-claw modules
pub struct IntroClawService {
    storage: Arc<StorageService>,
    is_active: bool,
    is_relayed_map: Arc<RwLock<HashMap<PeerId, bool>>>,

    // Sub-module state
    battery_throttler: BatteryThrottler,
    db_pruner: DatabasePruner,
    media_manager: MediaLifecycleManager,
    conn_optimizer: ConnectionOptimizer,
    message_batcher: MessageBatcher,
    prefetcher: PredictivePrefetcher,
    sync_prioritizer: SyncPrioritizer,
    duplicate_suppressor: DuplicateSuppressor,
    health_scorer: ConnectionHealthScorer,
    storage_quotas: StorageQuotaManager,
    adaptive_chunker: AdaptiveChunkSizer,

    // Tick counter for diagnostics
    tick_count: u64,
}

// ============================================================
// Sub-module: Battery-Saver Network Throttling
// ============================================================

const BATTERY_LOW_THRESHOLD: i32 = 20;
const BATTERY_CRITICAL_THRESHOLD: i32 = 10;
const NORMAL_MAILBOX_INTERVAL: u64 = 120;
const THROTTLED_MAILBOX_INTERVAL: u64 = 600;
const THROTTLED_HEARTBEAT_INTERVAL: u64 = 120;
const THROTTLED_CONTACT_REFRESH: u64 = 600;

pub struct BatteryThrottler {
    pub current_battery_pct: i32,
    pub is_background: bool,
    pub connected_peer_count: usize,
}

impl BatteryThrottler {
    pub fn new() -> Self {
        Self { current_battery_pct: 100, is_background: false, connected_peer_count: 0 }
    }

    pub fn should_throttle(&self) -> bool {
        self.current_battery_pct <= BATTERY_LOW_THRESHOLD || self.is_background
    }

    pub fn should_emergency_throttle(&self) -> bool {
        self.current_battery_pct <= BATTERY_CRITICAL_THRESHOLD
    }

    pub fn get_recommended_mailbox_interval(&self) -> u64 {
        if self.should_emergency_throttle() { THROTTLED_MAILBOX_INTERVAL * 2 }
        else if self.should_throttle() { THROTTLED_MAILBOX_INTERVAL }
        else { NORMAL_MAILBOX_INTERVAL }
    }

    pub fn get_recommended_heartbeat_interval(&self) -> u64 {
        if self.should_throttle() { THROTTLED_HEARTBEAT_INTERVAL } else { 30 }
    }

    pub fn get_recommended_contact_refresh(&self) -> u64 {
        if self.should_throttle() { THROTTLED_CONTACT_REFRESH } else { 120 }
    }

    pub fn get_recommended_max_connections(&self) -> u32 {
        if self.should_emergency_throttle() { 16 }
        else if self.should_throttle() { 32 }
        else { 1024 }
    }
}

// ============================================================
// Sub-module: Database Pruning & Cache Cleaning
// ============================================================

const SESSION_CACHE_MAX_AGE_SECS: u64 = 86400;
const CRYPTO_SESSION_MAX_AGE_SECS: u64 = 604800;

pub struct DatabasePruner {
    last_prune: std::time::Instant,
}

impl DatabasePruner {
    pub fn new() -> Self { Self { last_prune: std::time::Instant::now() } }

    pub fn should_run(&self) -> bool {
        self.last_prune.elapsed() >= std::time::Duration::from_secs(3600)
    }
}

// ============================================================
// Sub-module: Media Lifecycle & Storage Management
// ============================================================

pub struct MediaLifecycleManager {
    last_cleanup: std::time::Instant,
}

impl MediaLifecycleManager {
    pub fn new() -> Self { Self { last_cleanup: std::time::Instant::now() } }

    pub fn should_run(&self) -> bool {
        self.last_cleanup.elapsed() >= std::time::Duration::from_secs(1800)
    }
}

// ============================================================
// Sub-module: Connection Optimization
// ============================================================

pub struct ConnectionOptimizer {
    last_optimize: std::time::Instant,
}

impl ConnectionOptimizer {
    pub fn new() -> Self { Self { last_optimize: std::time::Instant::now() } }

    pub fn should_run(&self) -> bool {
        self.last_optimize.elapsed() >= std::time::Duration::from_secs(300)
    }
}

// ============================================================
// Sub-module: Smart Message Batching
// ============================================================

pub struct MessageBatcher {
    pending_outgoing: Vec<Vec<u8>>,
    is_batching: bool,
}

impl MessageBatcher {
    pub fn new() -> Self { Self { pending_outgoing: Vec::new(), is_batching: false } }

    pub fn should_batch(&self, is_throttled: bool) -> bool { is_throttled }

    pub fn queue(&mut self, payload: Vec<u8>) {
        self.pending_outgoing.push(payload);
    }

    pub fn flush(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_outgoing)
    }

    pub fn has_pending(&self) -> bool { !self.pending_outgoing.is_empty() }
}

// ============================================================
// Sub-module: Predictive File Pre-fetching
// ============================================================

pub struct PredictivePrefetcher {
    prefetch_limit: usize,
}

impl PredictivePrefetcher {
    pub fn new() -> Self { Self { prefetch_limit: 3 } }
}

// ============================================================
// Sub-module: Smart Sync Prioritization
// ============================================================

pub struct SyncPrioritizer;

impl SyncPrioritizer {
    pub fn new() -> Self { Self }
}

// ============================================================
// Sub-module: Duplicate Message Suppression
// ============================================================

pub struct DuplicateSuppressor {
    seen_ids: Vec<String>,
}

impl DuplicateSuppressor {
    pub fn new() -> Self { Self { seen_ids: Vec::new() } }

    pub fn check(&self, msg_id: &str) -> bool {
        self.seen_ids.contains(&msg_id.to_string())
    }

    pub fn mark_seen(&mut self, msg_id: &str) {
        self.seen_ids.push(msg_id.to_string());
    }
}

// ============================================================
// Sub-module: Connection Health Scoring
// ============================================================

pub struct ConnectionHealthScorer {
    scores: HashMap<String, f64>,
}

impl ConnectionHealthScorer {
    pub fn new() -> Self { Self { scores: HashMap::new() } }
}

// ============================================================
// Sub-module: Storage Quota Management
// ============================================================

pub struct StorageQuotaManager;

impl StorageQuotaManager {
    pub fn new() -> Self { Self }
}

// ============================================================
// Sub-module: Adaptive Chunk Sizing
// ============================================================

pub struct AdaptiveChunkSizer {
    observations: HashMap<String, Vec<f64>>,
}

impl AdaptiveChunkSizer {
    pub fn new() -> Self { Self { observations: HashMap::new() } }
}

// ============================================================
// Core Orchestrator
// ============================================================

impl IntroClawService {
    pub fn new(
        storage: Arc<StorageService>,
        is_relayed_map: Arc<RwLock<HashMap<PeerId, bool>>>,
    ) -> Self {
        Self {
            storage,
            is_active: false,
            is_relayed_map,
            battery_throttler: BatteryThrottler::new(),
            db_pruner: DatabasePruner::new(),
            media_manager: MediaLifecycleManager::new(),
            conn_optimizer: ConnectionOptimizer::new(),
            message_batcher: MessageBatcher::new(),
            prefetcher: PredictivePrefetcher::new(),
            sync_prioritizer: SyncPrioritizer::new(),
            duplicate_suppressor: DuplicateSuppressor::new(),
            health_scorer: ConnectionHealthScorer::new(),
            storage_quotas: StorageQuotaManager::new(),
            adaptive_chunker: AdaptiveChunkSizer::new(),
            tick_count: 0,
        }
    }

    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
        if active {
            println!("[IntroClaw] Engine ACTIVATED");
        } else {
            println!("[IntroClaw] Engine DEACTIVATED");
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Called every tick (5 minutes) from NetworkService
    pub fn tick(&mut self, ctx: &ClawTickContext) {
        if !self.is_active { return; }

        self.tick_count += 1;
        println!("[IntroClaw] Tick #{} — battery={}%, bg={}, peers={}, mdns={}",
                 self.tick_count, ctx.battery_pct, ctx.is_background,
                 ctx.connected_peers.len(), ctx.mdns_discovered.len());

        // 1. Battery-saver throttling
        self.battery_throttler.current_battery_pct = ctx.battery_pct;
        self.battery_throttler.is_background = ctx.is_background;
        self.battery_throttler.connected_peer_count = ctx.connected_peers.len();

        // 2. Database pruning (hourly)
        self.run_database_maintenance();

        // 3. Media cleanup (30 min)
        self.run_media_cleanup();

        // 4. Connection optimization (5 min)
        self.run_connection_optimization(ctx);

        // 5. Message batching
        self.run_message_batching();

        // 6. Predictive prefetch
        self.run_predictive_prefetch(ctx);

        // 7. Sync prioritization
        self.run_sync_prioritization();

        // 8. Health scoring
        self.run_health_scoring(ctx);

        // 9. Storage quotas
        self.run_storage_quota_check();

        // 10. Adaptive chunk sizing
        self.run_adaptive_chunking();

        // 11. Duplicate suppression (passive, runs on message write)
    }

    // ---- Module runners ----

    fn run_database_maintenance(&mut self) {
        if !self.db_pruner.should_run() { return; }
        println!("[IntroClaw] Running database maintenance...");
        let _ = self.storage.prune_expired_sessions(SESSION_CACHE_MAX_AGE_SECS);
        let _ = self.storage.prune_expired_crypto_sessions(CRYPTO_SESSION_MAX_AGE_SECS);
        let _ = self.storage.prune_old_mesh_chunks();
        let _ = self.storage.run_pragma_optimize();
        self.db_pruner.last_prune = std::time::Instant::now();
        println!("[IntroClaw] Database maintenance complete");
    }

    fn run_media_cleanup(&mut self) {
        if !self.media_manager.should_run() { return; }
        println!("[IntroClaw] Running media lifecycle cleanup...");
        let active_hashes = self.storage.get_active_drive_hashes();
        let _ = self.storage.cleanup_orphaned_mesh_chunks(&active_hashes);
        self.media_manager.last_cleanup = std::time::Instant::now();
        println!("[IntroClaw] Media cleanup complete");
    }

    fn run_connection_optimization(&mut self, ctx: &ClawTickContext) {
        if !self.conn_optimizer.should_run() { return; }
        if self.battery_throttler.should_emergency_throttle() { return; }

        let battery_ok = !self.battery_throttler.should_throttle();
        for peer_id_str in &ctx.connected_peers {
            let peer_id = peer_id_str.parse::<PeerId>();
            let peer_id = match peer_id {
                Ok(p) => p,
                Err(_) => continue,
            };
            let is_relayed = self.is_relayed_map.read().get(&peer_id).cloned().unwrap_or(false);
            let has_mdns = ctx.mdns_discovered.contains(peer_id_str);

            if is_relayed && has_mdns && battery_ok {
                println!("[IntroClaw] Direct P2P upgrade candidate: {}", peer_id_str);
            }
        }
        self.conn_optimizer.last_optimize = std::time::Instant::now();
    }

    fn run_message_batching(&mut self) {
        // Passive — batching decisions happen when send commands arrive
    }

    fn run_predictive_prefetch(&mut self, _ctx: &ClawTickContext) {
        // Passive — triggered by message receipt
    }

    fn run_sync_prioritization(&mut self) {
        // Passive — triggered by sync commands
    }

    fn run_health_scoring(&mut self, ctx: &ClawTickContext) {
        for peer_id in &ctx.connected_peers {
            self.health_scorer.scores.entry(peer_id.clone())
                .and_modify(|s| *s = (*s * 0.9 + 0.1).min(1.0))
                .or_insert(0.5);
        }
    }

    fn run_storage_quota_check(&mut self) {
        let (drive_bytes, mesh_bytes, total_disk) = self.storage.get_storage_usage();
        let usage_pct = if total_disk > 0 { ((drive_bytes + mesh_bytes) as f64 / total_disk as f64 * 100.0) as i32 } else { 0 };
        if usage_pct > 80 {
            println!("[IntroClaw] Storage at {}% — auto-pruning mesh chunks", usage_pct);
            let active = self.storage.get_active_drive_hashes();
            let _ = self.storage.cleanup_orphaned_mesh_chunks(&active);
        }
    }

    fn run_adaptive_chunking(&mut self) {
        // Passive — called during file transfer to get optimal chunk size
    }

    pub fn get_optimal_chunk_size(&self, peer_id: &str) -> u32 {
        if let Some(throughput) = self.adaptive_chunker.observations.get(peer_id) {
            if let Some(&last) = throughput.last() {
                if last > 10_000_000.0 { return 512 * 1024; }
                if last > 1_000_000.0 { return 256 * 1024; }
                return 64 * 1024;
            }
        }
        256 * 1024 // default
    }

    pub fn get_peer_health(&self, peer_id: &str) -> f64 {
        self.health_scorer.scores.get(peer_id).copied().unwrap_or(0.5)
    }

    pub fn get_storage_usage(&self) -> (u64, u64, u64) {
        self.storage.get_storage_usage()
    }

    pub fn check_duplicate(&self, msg_id: &str) -> bool {
        self.duplicate_suppressor.seen_ids.contains(&msg_id.to_string())
    }

    pub fn mark_seen(&mut self, msg_id: &str) {
        self.duplicate_suppressor.seen_ids.push(msg_id.to_string());
    }

    pub fn flush_batch(&mut self) -> Vec<Vec<u8>> {
        self.message_batcher.flush()
    }

    pub fn queue_batch(&mut self, payload: Vec<u8>) {
        self.message_batcher.queue(payload);
    }

    pub fn should_batch(&self) -> bool {
        self.battery_throttler.should_throttle()
    }
}

// ============================================================
// StorageService extension methods (defined in storage.rs)
// ============================================================
// These methods are added to StorageService in storage.rs:
// - prune_expired_sessions(max_age_secs)
// - prune_expired_crypto_sessions(max_age_secs)
// - run_pragma_optimize()
// - cleanup_orphaned_mesh_chunks(active_hashes)
// - get_active_drive_hashes()
// - get_storage_usage() -> (drive_bytes, mesh_bytes, total_disk)
