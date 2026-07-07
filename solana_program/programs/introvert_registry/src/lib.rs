use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("RBNRegXy4vQszN2Cg8gqf91mYyL24p8cT32d1mY1111");

/// Minimum stake required to register/activate an RBN node: 2,000,000 INTR
const MIN_STAKE_AMOUNT: u64 = 2_000_000_000_000_000; // 2M INTR in nano-INTR (9 decimals)

/// Unbonding cooldown period: 7 days in seconds
const UNBONDING_COOLDOWN_SECS: i64 = 604_800;

/// Bootstrap RBN operator public key — exempt from staking requirements.
/// This is the primary bootstrap node that seeds the network before staking is viable.
const BOOTSTRAP_RBN_OPERATOR: Pubkey = Pubkey::new_from_array([
    0x12, 0xd3, 0x4b, 0x0e, 0x4a, 0x6e, 0x8f, 0x2c,
    0x1a, 0x5d, 0x7b, 0x9f, 0x3e, 0x8c, 0x2d, 0x6a,
    0x4b, 0x0e, 0x1f, 0x3a, 0x5c, 0x7d, 0x9e, 0x2b,
    0x4a, 0x6c, 0x8d, 0x0f, 0x2e, 0x4a, 0x6b, 0x8c,
]);

/// Returns true if the operator is the bootstrap RBN (exempt from staking).
fn is_bootstrap_rbn(operator: &Pubkey) -> bool {
    *operator == BOOTSTRAP_RBN_OPERATOR
}

#[program]
pub mod introvert_registry {
    use super::*;

    /// Stakes INTR tokens into the program-controlled escrow vault.
    /// Must be called before register_rbn or update_rbn(is_active=true).
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        require!(amount >= MIN_STAKE_AMOUNT, RegistryError::InsufficientStake);

        // Transfer INTR from operator's ATA to the escrow vault PDA
        let cpi_accounts = Transfer {
            from: ctx.accounts.operator_token_account.to_account_info(),
            to: ctx.accounts.escrow_vault.to_account_info(),
            authority: ctx.accounts.operator.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
        );
        token::transfer(cpi_ctx, amount)?;

        // Record the stake amount in the escrow state
        let escrow = &mut ctx.accounts.escrow_state;
        escrow.operator = ctx.accounts.operator.key();
        escrow.staked_amount = escrow.staked_amount.checked_add(amount)
            .ok_or(RegistryError::Overflow)?;
        escrow.last_staked_at = Clock::get()?.unix_timestamp;
        escrow.is_unbonding = false;
        escrow.unbond_initiated_at = 0;

        Ok(())
    }

    /// Initiates unbonding. Stake enters a 7-day cooldown before withdrawal.
    pub fn initiate_unbond(ctx: Context<Unbond>) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow_state;
        require!(!escrow.is_unbonding, RegistryError::AlreadyUnbonding);
        require!(escrow.staked_amount > 0, RegistryError::NoStake);

        escrow.is_unbonding = true;
        escrow.unbond_initiated_at = Clock::get()?.unix_timestamp;

        // Deactivate the RBN registration
        let registry_entry = &mut ctx.accounts.registry_entry;
        registry_entry.is_active = false;

        Ok(())
    }

    /// Withdraws staked INTR after the 7-day unbonding cooldown.
    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        let escrow = &ctx.accounts.escrow_state;
        require!(escrow.is_unbonding, RegistryError::NotUnbonding);

        let now = Clock::get()?.unix_timestamp;
        let elapsed = now.saturating_sub(escrow.unbond_initiated_at);
        require!(elapsed >= UNBONDING_COOLDOWN_SECS, RegistryError::CooldownNotExpired);

        let amount = escrow.staked_amount;
        require!(amount > 0, RegistryError::NoStake);

        // Transfer INTR from escrow vault back to operator
        let seeds = &[b"escrow_vault".as_ref(), &[ctx.bumps.escrow_vault]];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.escrow_vault.to_account_info(),
            to: ctx.accounts.operator_token_account.to_account_info(),
            authority: ctx.accounts.escrow_vault.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount)?;

        // Zero out escrow state
        let escrow = &mut ctx.accounts.escrow_state;
        escrow.staked_amount = 0;
        escrow.is_unbonding = false;
        escrow.unbond_initiated_at = 0;

        Ok(())
    }

    /// Registers a new RBN node on-chain. Requires active stake >= 2M INTR,
    /// unless the operator is the bootstrap RBN identity (exempt from staking).
    pub fn register_rbn(ctx: Context<RegisterRbn>, peer_id: String, multiaddresses: String, node_name: String) -> Result<()> {
        let operator_key = ctx.accounts.operator.key();

        // Bootstrap RBN bypasses staking verification entirely
        if !is_bootstrap_rbn(&operator_key) {
            let escrow = ctx.accounts.escrow_state.as_ref()
                .ok_or(RegistryError::InsufficientStake)?;
            require!(escrow.staked_amount >= MIN_STAKE_AMOUNT, RegistryError::InsufficientStake);
            require!(!escrow.is_unbonding, RegistryError::UnbondingActive);
        }

        let registry_entry = &mut ctx.accounts.registry_entry;
        registry_entry.operator = operator_key;
        registry_entry.peer_id = peer_id;
        registry_entry.multiaddresses = multiaddresses;
        registry_entry.node_name = node_name;
        registry_entry.is_active = true;
        registry_entry.last_registered = Clock::get()?.unix_timestamp;
        registry_entry.stake_amount = if is_bootstrap_rbn(&operator_key) {
            u64::MAX // Sentinel: bootstrap RBN has effectively unlimited stake
        } else {
            ctx.accounts.escrow_state.as_ref().unwrap().staked_amount
        };
        Ok(())
    }

    /// Updates an existing RBN registration. If activating, requires active stake >= 2M INTR
    /// unless the operator is the bootstrap RBN identity.
    pub fn update_rbn(ctx: Context<UpdateRbn>, peer_id: String, multiaddresses: String, node_name: String, is_active: bool) -> Result<()> {
        let operator_key = ctx.accounts.operator.key();

        // If re-activating, verify stake (bootstrap RBN exempt)
        if is_active && !is_bootstrap_rbn(&operator_key) {
            let escrow = ctx.accounts.escrow_state.as_ref()
                .ok_or(RegistryError::InsufficientStake)?;
            require!(escrow.staked_amount >= MIN_STAKE_AMOUNT, RegistryError::InsufficientStake);
            require!(!escrow.is_unbonding, RegistryError::UnbondingActive);
        }

        let registry_entry = &mut ctx.accounts.registry_entry;
        registry_entry.peer_id = peer_id;
        registry_entry.multiaddresses = multiaddresses;
        registry_entry.node_name = node_name;
        registry_entry.is_active = is_active;
        registry_entry.last_registered = Clock::get()?.unix_timestamp;
        if is_active {
            registry_entry.stake_amount = if is_bootstrap_rbn(&operator_key) {
                u64::MAX
            } else {
                ctx.accounts.escrow_state.as_ref().unwrap().staked_amount
            };
        }
        Ok(())
    }
}

// ─── Account Contexts ──────────────────────────────────────────

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(
        init_if_needed,
        payer = operator,
        space = 8 + EscrowState::INIT_SPACE,
        seeds = [b"escrow_state", operator.key().as_ref()],
        bump
    )]
    pub escrow_state: Account<'info, EscrowState>,

    #[account(
        mut,
        seeds = [b"escrow_vault"],
        bump
    )]
    /// CHECK: PDA-owned token account, validated by seeds
    pub escrow_vault: AccountInfo<'info>,

    #[account(mut)]
    pub operator_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub operator: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Unbond<'info> {
    #[account(
        mut,
        seeds = [b"escrow_state", operator.key().as_ref()],
        bump,
        has_one = operator,
    )]
    pub escrow_state: Account<'info, EscrowState>,

    #[account(
        mut,
        seeds = [b"rbn-registry", operator.key().as_ref()],
        bump,
    )]
    pub registry_entry: Account<'info, RbnRegistryEntry>,

    pub operator: Signer<'info>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        seeds = [b"escrow_state", operator.key().as_ref()],
        bump,
        has_one = operator,
    )]
    pub escrow_state: Account<'info, EscrowState>,

    #[account(
        mut,
        seeds = [b"escrow_vault"],
        bump
    )]
    /// CHECK: PDA-owned token account, validated by seeds
    pub escrow_vault: AccountInfo<'info>,

    #[account(mut)]
    pub operator_token_account: Account<'info, TokenAccount>,

    pub operator: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RegisterRbn<'info> {
    #[account(
        init,
        payer = operator,
        space = 8 + 32 + (4 + 64) + (4 + 256) + (4 + 32) + 1 + 8 + 8,
        seeds = [b"rbn-registry", operator.key().as_ref()],
        bump
    )]
    pub registry_entry: Account<'info, RbnRegistryEntry>,

    // Optional: bootstrap RBN can register without an escrow account
    #[account(
        seeds = [b"escrow_state", operator.key().as_ref()],
        bump,
    )]
    pub escrow_state: Option<Account<'info, EscrowState>>,

    #[account(mut)]
    pub operator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateRbn<'info> {
    #[account(
        mut,
        seeds = [b"rbn-registry", operator.key().as_ref()],
        bump
    )]
    pub registry_entry: Account<'info, RbnRegistryEntry>,

    // Optional: bootstrap RBN can update without an escrow account
    #[account(
        seeds = [b"escrow_state", operator.key().as_ref()],
        bump,
    )]
    pub escrow_state: Option<Account<'info, EscrowState>>,

    pub operator: Signer<'info>,
}

// ─── Account Data ──────────────────────────────────────────────

#[account]
pub struct RbnRegistryEntry {
    pub operator: Pubkey,
    pub peer_id: String,
    pub multiaddresses: String,
    pub node_name: String,
    pub is_active: bool,
    pub last_registered: i64,
    pub stake_amount: u64,
}

#[account]
#[derive(InitSpace)]
pub struct EscrowState {
    pub operator: Pubkey,        // 32
    pub staked_amount: u64,      // 8
    pub last_staked_at: i64,     // 8
    pub is_unbonding: bool,      // 1
    pub unbond_initiated_at: i64,// 8
}

// ─── Errors ────────────────────────────────────────────────────

#[error_code]
pub enum RegistryError {
    #[msg("Stake amount below minimum requirement of 2,000,000 INTR")]
    InsufficientStake,
    #[msg("Operator is currently unbonding; cannot activate")]
    UnbondingActive,
    #[msg("Already in unbonding state")]
    AlreadyUnbonding,
    #[msg("No stake to withdraw")]
    NoStake,
    #[msg("Unbonding cooldown period has not expired (7 days)")]
    CooldownNotExpired,
    #[msg("Not in unbonding state")]
    NotUnbonding,
    #[msg("Arithmetic overflow")]
    Overflow,
}
