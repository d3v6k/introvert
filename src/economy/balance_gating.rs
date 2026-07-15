use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// INTR token mint address on Solana mainnet.
const INTR_MINT: &str = "FhKJjqpsCbymrk4Ntv5jFyZihHsAkW4Fb4fuJYBniydP";

/// Token program and ATA program IDs.
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ATA_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// Node tier profile returned by the balance gating service.
/// Maps on-chain INTR holdings to engineering visibility and allocation parameters.
#[derive(Debug, Clone, Copy)]
pub struct NodeTierProfile {
    /// Whether the advanced diagnostic UI should be visible in the client.
    pub advanced_diagnostic_ui_visible: bool,
    /// Allocation multiplier applied to telemetry weight scoring (1.0x - 2.5x).
    pub allocation_multiplier: f32,
    /// Raw tier level (0-4) for internal tracking.
    pub tier: u8,
}

impl Default for NodeTierProfile {
    fn default() -> Self {
        Self {
            advanced_diagnostic_ui_visible: false,
            allocation_multiplier: 1.0,
            tier: 0,
        }
    }
}

/// Solana balance gating service.
/// Queries on-chain INTR token balances to determine node tier profiles.
pub struct BalanceGatingService {
    rpc_client: Arc<RpcClient>,
    intr_mint: Pubkey,
}

impl BalanceGatingService {
    pub fn new(rpc_url: &str) -> Self {
        let intr_mint = Pubkey::from_str(INTR_MINT).expect("valid INTR mint pubkey");
        Self {
            rpc_client: Arc::new(RpcClient::new_with_timeout(
                rpc_url.to_string(),
                std::time::Duration::from_secs(5),
            )),
            intr_mint,
        }
    }

    /// Fetches the INTR token balance for a given owner's Associated Token Account.
    /// Returns balance in nano-INTR (1 INTR = 1,000,000,000 nano-INTR).
    /// Returns 0 if the account doesn't exist (no tokens).
    pub async fn fetch_intr_balance(&self, owner: &Pubkey) -> u64 {
        let token_program = match Pubkey::from_str(TOKEN_PROGRAM) {
            Ok(p) => p,
            Err(_) => return 0,
        };
        let ata_program = match Pubkey::from_str(ATA_PROGRAM) {
            Ok(p) => p,
            Err(_) => return 0,
        };

        let (ata, _) = Pubkey::find_program_address(
            &[&owner.to_bytes(), &token_program.to_bytes(), &self.intr_mint.to_bytes()],
            &ata_program,
        );

        match self.rpc_client.get_account(&ata).await {
            Ok(account) => {
                // SPL Token account: amount is at bytes 64..72 (little-endian u64)
                if account.data.len() >= 72 {
                    let mut amount_bytes = [0u8; 8];
                    amount_bytes.copy_from_slice(&account.data[64..72]);
                    u64::from_le_bytes(amount_bytes)
                } else {
                    0
                }
            }
            Err(_) => 0, // Account doesn't exist yet (no tokens)
        }
    }

    /// Fetches the node's tier profile based on on-chain INTR balance.
    ///
    /// Tier mapping (unified with solana.rs verify_prestige_tier thresholds):
    ///   Balance < 100,000 INTR   → (visible: false, multiplier: 1.0, tier: 0) Citizen
    ///   100,000 - 249,999 INTR   → (visible: true,  multiplier: 1.5, tier: 1) Sentinel
    ///   250,000 - 499,999 INTR   → (visible: true,  multiplier: 1.75, tier: 2) Silver
    ///   500,000 - 999,999 INTR   → (visible: true,  multiplier: 2.0, tier: 3) Gold
    ///   1,000,000+ INTR          → (visible: true,  multiplier: 2.5, tier: 4) Platinum
    ///
    /// Balance is in nano-INTR (9 decimals). Thresholds converted from INTR.
    pub async fn fetch_node_tier_profile(&self, owner: &Pubkey) -> NodeTierProfile {
        let balance_nano = self.fetch_intr_balance(owner).await;
        let balance_intr = balance_nano as f64 / 1_000_000_000.0;

        let profile = if balance_intr >= 1_000_000.0 {
            NodeTierProfile {
                advanced_diagnostic_ui_visible: true,
                allocation_multiplier: 2.5,
                tier: 4, // Platinum
            }
        } else if balance_intr >= 500_000.0 {
            NodeTierProfile {
                advanced_diagnostic_ui_visible: true,
                allocation_multiplier: 2.0,
                tier: 3, // Gold
            }
        } else if balance_intr >= 250_000.0 {
            NodeTierProfile {
                advanced_diagnostic_ui_visible: true,
                allocation_multiplier: 1.75,
                tier: 2, // Silver
            }
        } else if balance_intr >= 100_000.0 {
            NodeTierProfile {
                advanced_diagnostic_ui_visible: true,
                allocation_multiplier: 1.5,
                tier: 1, // Sentinel
            }
        } else {
            NodeTierProfile::default() // Citizen: hidden UI, 1.0x multiplier
        };

        info!(
            "[BalanceGating] Node {} → balance={:.2} INTR, tier={}, visible={}, multiplier={}",
            owner, balance_intr, profile.tier, profile.advanced_diagnostic_ui_visible, profile.allocation_multiplier
        );

        profile
    }
}
