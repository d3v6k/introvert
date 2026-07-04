use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

/// 10-year emission schedule constants (from whitepaper).
/// Year 1 daily user pool: 16,438 INTR/day
/// Year 1 daily RBN pool: 8,219 INTR/day
/// Annual decay: 20% (multiplier 0.8)
const YEAR_1_DAILY_USER_POOL: f64 = 16_438.0;
const YEAR_1_DAILY_RBN_POOL: f64 = 8_219.0;
const ANNUAL_DECAY: f64 = 0.8;
const TGE_DATE: &str = "2026-01-01";

/// Computes the daily emission for a given pool at year t (0-indexed from TGE).
///
/// Formula: E_day(t) = (I_base * 0.8^t) / 365
///
/// Where:
///   I_base = Year 1 daily pool (user or RBN)
///   t = years since TGE (0 = Year 1)
///   0.8 = annual decay multiplier
pub fn compute_daily_emission(base_pool: f64, years_since_tge: u32) -> f64 {
    let decay_factor = ANNUAL_DECAY.powi(years_since_tge as i32);
    (base_pool * decay_factor) / 365.0
}

/// Returns the current year index (0 = Year 1) based on TGE date.
pub fn current_emission_year() -> u32 {
    let tge = chrono::NaiveDate::parse_from_str(TGE_DATE, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let today = chrono::Utc::now().date_naive();
    let days_since_tge = today.signed_duration_since(tge).num_days().max(0) as u32;
    days_since_tge / 365
}

/// A single node's telemetry snapshot for daily allocation computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTelemetryClaim {
    pub peer_id: String,
    pub sol_address: String,
    pub relayed_bytes: u64,
    pub uptime_seconds: u64,
    pub active_containers: u32,
    pub allocation_multiplier: f32, // From BalanceGatingService tier
    pub proof_hash: String,         // SHA-256 of claim data
    pub timestamp: u64,
}

/// Result of a daily allocation computation for a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyAllocation {
    pub peer_id: String,
    pub sol_address: String,
    pub raw_points: f64,
    pub weighted_points: f64,      // raw_points * allocation_multiplier
    pub share_of_pool: f64,        // weighted_points / total_weighted_points
    pub intr_allocated: f64,       // share_of_pool * daily_emission
    pub tier_multiplier: f32,
}

/// Nightly ledger cron engine.
/// Aggregates verified telemetry claims, computes proportional allocations
/// against the 10-year decay emission schedule, and returns finalized allocations.
pub struct LedgerCronEngine;

impl LedgerCronEngine {
    /// Computes daily allocations for all nodes that submitted telemetry claims.
    ///
    /// Algorithm:
    /// 1. Calculate current daily emission pool using decay formula
    /// 2. For each node: raw_points = relayed_bytes + (uptime_seconds * 100) + (active_containers * 1000)
    /// 3. Apply tier multiplier: weighted_points = raw_points * allocation_multiplier
    /// 4. Compute share: share = weighted_points / total_weighted_points
    /// 5. Allocate: intr_allocated = share * daily_emission
    pub fn compute_daily_allocations(claims: &[NodeTelemetryClaim]) -> Vec<DailyAllocation> {
        if claims.is_empty() {
            return Vec::new();
        }

        let year = current_emission_year();
        let daily_emission = compute_daily_emission(YEAR_1_DAILY_USER_POOL, year);

        info!(
            "[LedgerCron] Computing daily allocations: year={}, emission={:.4} INTR, nodes={}",
            year, daily_emission, claims.len()
        );

        // Step 1: Compute raw points for each node
        let raw_points: Vec<f64> = claims
            .iter()
            .map(|c| {
                let byte_score = (c.relayed_bytes as f64) / 1_048_576.0; // MB relayed
                let uptime_score = (c.uptime_seconds as f64) / 3600.0;   // Hours online
                let container_score = (c.active_containers as f64) * 10.0; // Container bonus
                byte_score + uptime_score + container_score
            })
            .collect();

        // Step 2: Apply tier multipliers
        let weighted_points: Vec<f64> = raw_points
            .iter()
            .zip(claims.iter())
            .map(|(raw, claim)| raw * claim.allocation_multiplier as f64)
            .collect();

        let total_weighted: f64 = weighted_points.iter().sum();

        if total_weighted <= 0.0 {
            warn!("[LedgerCron] Total weighted points is zero — no allocations computed");
            return Vec::new();
        }

        // Step 3: Compute proportional allocations
        let allocations: Vec<DailyAllocation> = claims
            .iter()
            .zip(raw_points.iter())
            .zip(weighted_points.iter())
            .map(|((claim, raw), weighted)| {
                let share = weighted / total_weighted;
                let allocated = share * daily_emission;

                DailyAllocation {
                    peer_id: claim.peer_id.clone(),
                    sol_address: claim.sol_address.clone(),
                    raw_points: *raw,
                    weighted_points: *weighted,
                    share_of_pool: share,
                    intr_allocated: allocated,
                    tier_multiplier: claim.allocation_multiplier,
                }
            })
            .collect();

        let total_allocated: f64 = allocations.iter().map(|a| a.intr_allocated).sum();
        info!(
            "[LedgerCron] Allocations computed: {} nodes, {:.4} INTR total allocated (pool: {:.4})",
            allocations.len(), total_allocated, daily_emission
        );

        allocations
    }

    /// Computes the RBN infrastructure pool allocation for a given day.
    /// RBNs receive a separate pool with the same decay schedule.
    pub fn compute_rbn_daily_emission() -> f64 {
        let year = current_emission_year();
        compute_daily_emission(YEAR_1_DAILY_RBN_POOL, year)
    }

    /// Executes the nightly allocation cycle: aggregates claims, computes allocations,
    /// and persists results to storage. Called once per day by the RBN cron scheduler.
    ///
    /// Returns the number of nodes that received allocations.
    pub fn execute_nightly_cycle(
        claims: &[NodeTelemetryClaim],
        storage: &crate::storage::StorageService,
    ) -> usize {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // Check if we already ran today
        if let Ok(existing) = storage.get_allocations_for_cycle(&today) {
            if !existing.is_empty() {
                info!("[LedgerCron] Cycle already completed for {} ({} nodes)", today, existing.len());
                return existing.len();
            }
        }

        // Compute allocations
        let allocations = Self::compute_daily_allocations(claims);
        if allocations.is_empty() {
            warn!("[LedgerCron] No claims to allocate for {}", today);
            return 0;
        }

        // Persist to storage
        match storage.save_daily_allocations(&today, &allocations) {
            Ok(_) => {
                let total: f64 = allocations.iter().map(|a| a.intr_allocated).sum();
                info!(
                    "[LedgerCron] Nightly cycle complete for {}: {} nodes, {:.4} INTR allocated",
                    today, allocations.len(), total
                );
            }
            Err(e) => {
                error!("[LedgerCron] Failed to save allocations for {}: {:?}", today, e);
            }
        }

        allocations.len()
    }
}
