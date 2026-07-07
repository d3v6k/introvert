use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

/// Computes the epoch index (days since TGE) for a given date string.
pub fn epoch_index_from_date(date: &str) -> u64 {
    let tge = chrono::NaiveDate::parse_from_str(TGE_DATE, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let epoch_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .unwrap_or(tge);
    epoch_date.signed_duration_since(tge).num_days().max(0) as u64
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
    pub is_rbn: bool,               // Whether this node is an RBN operator
    pub epoch_date: Option<String>, // Calendar date (e.g., "2026-07-07") — None = current epoch
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
    pub pool_type: String,         // "user" or "rbn"
}

/// Late-arriving credit from a historical epoch, computed using that epoch's multiplier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LateCredit {
    pub peer_id: String,
    pub sol_address: String,
    pub epoch_index: u64,
    pub intr_credit: f64,
    pub pool_type: String,
}

/// Incremental delta log: epoch_number -> accumulated points for that epoch.
/// Allows late-arriving telemetry to be slotted into the correct historical epoch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeltaLog {
    /// epoch_index -> total weighted points received for that epoch
    pub epoch_points: BTreeMap<u64, f64>,
    /// epoch_index -> daily emission rate at that epoch (cached for late credit computation)
    pub epoch_emissions: BTreeMap<u64, f64>,
    /// Accumulated late credits to be processed in the next cycle
    pub pending_late_credits: Vec<LateCredit>,
}

impl DeltaLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records points for a given epoch. If the epoch is already closed (not the current one),
    /// computes a late credit using the historical emission rate.
    pub fn record_epoch_points(
        &mut self,
        epoch_index: u64,
        points: f64,
        current_epoch: u64,
        claim: &NodeTelemetryClaim,
        historical_emission: f64,
        total_epoch_points: f64,
    ) {
        // Accumulate points for this epoch
        let entry = self.epoch_points.entry(epoch_index).or_insert(0.0);
        *entry += points;

        // Cache the emission rate for this epoch if not already stored
        self.epoch_emissions.entry(epoch_index).or_insert(historical_emission);

        // If this is a late arrival (epoch is already closed), compute a late credit
        if epoch_index < current_epoch && total_epoch_points > 0.0 && historical_emission > 0.0 {
            let share = points / total_epoch_points;
            let credit = share * historical_emission;

            if credit > 0.0 {
                info!(
                    "[DeltaLog] Late credit computed: peer={}, epoch={}, points={:.1}, credit={:.4} INTR",
                    claim.peer_id, epoch_index, points, credit
                );
                self.pending_late_credits.push(LateCredit {
                    peer_id: claim.peer_id.clone(),
                    sol_address: claim.sol_address.clone(),
                    epoch_index,
                    intr_credit: credit,
                    pool_type: if claim.is_rbn { "rbn".to_string() } else { "user".to_string() },
                });
            }
        }
    }

    /// Drains and returns all pending late credits.
    pub fn drain_late_credits(&mut self) -> Vec<LateCredit> {
        std::mem::take(&mut self.pending_late_credits)
    }
}

/// Nightly ledger cron engine.
/// Aggregates verified telemetry claims, computes proportional allocations
/// against the 10-year decay emission schedule, and returns finalized allocations.
///
/// Supports incremental delta logging: late-arriving telemetry for closed epochs
/// is converted to `LateCredit` adjustments processed in the next active cycle.
pub struct LedgerCronEngine;

impl LedgerCronEngine {
    /// Computes daily allocations for all nodes that submitted telemetry claims.
    ///
    /// Splits claims into user and RBN pools:
    /// - Standard users draw from the 16,438 INTR/day user pool
    /// - RBN operators draw from the 8,219 INTR/day RBN pool
    ///
    /// Algorithm:
    /// 1. Split claims by is_rbn flag
    /// 2. For each pool: compute raw_points, apply tier multipliers, allocate proportionally
    /// 3. Merge results
    pub fn compute_daily_allocations(claims: &[NodeTelemetryClaim]) -> Vec<DailyAllocation> {
        if claims.is_empty() {
            return Vec::new();
        }

        let year = current_emission_year();
        let user_emission = compute_daily_emission(YEAR_1_DAILY_USER_POOL, year);
        let rbn_emission = compute_daily_emission(YEAR_1_DAILY_RBN_POOL, year);

        info!(
            "[LedgerCron] Computing daily allocations: year={}, user_pool={:.4}, rbn_pool={:.4}, total_nodes={}",
            year, user_emission, rbn_emission, claims.len()
        );

        // Split claims by node type
        let (rbn_claims, user_claims): (Vec<_>, Vec<_>) = claims.iter()
            .partition(|c| c.is_rbn);

        let mut allocations = Vec::with_capacity(claims.len());

        // Allocate user pool
        if !user_claims.is_empty() {
            let user_allocs = Self::allocate_pool(&user_claims, user_emission, "user");
            allocations.extend(user_allocs);
        }

        // Allocate RBN pool
        if !rbn_claims.is_empty() {
            let rbn_allocs = Self::allocate_pool(&rbn_claims, rbn_emission, "rbn");
            allocations.extend(rbn_allocs);
        }

        let total_allocated: f64 = allocations.iter().map(|a| a.intr_allocated).sum();
        info!(
            "[LedgerCron] Allocations computed: {} nodes, {:.4} INTR total allocated (user: {:.4}, rbn: {:.4})",
            allocations.len(), total_allocated,
            allocations.iter().filter(|a| a.pool_type == "user").map(|a| a.intr_allocated).sum::<f64>(),
            allocations.iter().filter(|a| a.pool_type == "rbn").map(|a| a.intr_allocated).sum::<f64>(),
        );

        allocations
    }

    /// Allocates from a specific pool (user or RBN) proportionally.
    fn allocate_pool(claims: &[&NodeTelemetryClaim], daily_emission: f64, pool_type: &str) -> Vec<DailyAllocation> {
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
            warn!("[LedgerCron] Total weighted points is zero for {} pool — no allocations", pool_type);
            return Vec::new();
        }

        // Step 3: Compute proportional allocations
        claims
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
                    pool_type: pool_type.to_string(),
                }
            })
            .collect()
    }

    /// Processes incoming claims through the delta log system.
    /// Active epoch claims go to the live allocation pool.
    /// Historical epoch claims produce late credits using the historical emission rate.
    pub fn process_claims_with_delta(
        claims: &[NodeTelemetryClaim],
        delta_log: &mut DeltaLog,
        current_epoch_date: &str,
    ) -> (Vec<DailyAllocation>, Vec<LateCredit>) {
        let current_epoch = epoch_index_from_date(current_epoch_date);
        let year = current_emission_year();
        let user_emission = compute_daily_emission(YEAR_1_DAILY_USER_POOL, year);
        let rbn_emission = compute_daily_emission(YEAR_1_DAILY_RBN_POOL, year);

        let mut active_claims: Vec<NodeTelemetryClaim> = Vec::new();
        let mut late_claims: Vec<NodeTelemetryClaim> = Vec::new();

        // Partition claims into active and late based on epoch_date
        for claim in claims {
            let claim_epoch = claim.epoch_date.as_deref()
                .map(epoch_index_from_date)
                .unwrap_or(current_epoch);
            if claim_epoch >= current_epoch {
                active_claims.push(claim.clone());
            } else {
                late_claims.push(claim.clone());
            }
        }

        // Compute total points per epoch from delta log for late credit denominators
        let epoch_totals: BTreeMap<u64, f64> = delta_log.epoch_points.clone();

        // Register active claims in the delta log
        for claim in &active_claims {
            let raw_points = (claim.relayed_bytes as f64) / 1_048_576.0
                + (claim.uptime_seconds as f64) / 3600.0
                + (claim.active_containers as f64) * 10.0;
            let weighted = raw_points * claim.allocation_multiplier as f64;
            let emission = if claim.is_rbn { rbn_emission } else { user_emission };
            let total = *epoch_totals.get(&current_epoch).unwrap_or(&0.0);

            delta_log.record_epoch_points(
                current_epoch,
                weighted,
                current_epoch,
                claim,
                emission,
                total + weighted,
            );
        }

        // Process late claims: compute historical emission and generate late credits
        for claim in &late_claims {
            let claim_epoch = claim.epoch_date.as_deref()
                .map(epoch_index_from_date)
                .unwrap_or(current_epoch);
            let claim_year = (claim_epoch / 365) as u32;
            let historical_emission = if claim.is_rbn {
                compute_daily_emission(YEAR_1_DAILY_RBN_POOL, claim_year)
            } else {
                compute_daily_emission(YEAR_1_DAILY_USER_POOL, claim_year)
            };

            let raw_points = (claim.relayed_bytes as f64) / 1_048_576.0
                + (claim.uptime_seconds as f64) / 3600.0
                + (claim.active_containers as f64) * 10.0;
            let weighted = raw_points * claim.allocation_multiplier as f64;
            let total = *epoch_totals.get(&claim_epoch).unwrap_or(&weighted);

            delta_log.record_epoch_points(
                claim_epoch,
                weighted,
                current_epoch,
                claim,
                historical_emission,
                total,
            );
        }

        // Compute active allocations
        let allocations = Self::compute_daily_allocations(&active_claims);

        // Drain late credits that accumulated from historical epoch processing
        let late_credits = delta_log.drain_late_credits();

        (allocations, late_credits)
    }

    /// Executes the nightly allocation cycle: aggregates claims, computes allocations,
    /// and persists results to storage. Called once per day by the RBN cron scheduler.
    ///
    /// Returns the number of nodes that received allocations.
    pub fn execute_nightly_cycle(
        claims: &[NodeTelemetryClaim],
        storage: &crate::storage::StorageService,
    ) -> usize {
        let today = crate::economy::daily_rewards::economy_today();

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
