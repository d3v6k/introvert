use anchor_lang::prelude::*;

declare_id!("RBNRegXy4vQszN2Cg8gqf91mYyL24p8cT32d1mY1111");

#[program]
pub mod introvert_registry {
    use super::*;

    pub fn register_rbn(ctx: Context<RegisterRbn>, peer_id: String, multiaddresses: String, node_name: String) -> Result<()> {
        let registry_entry = &mut ctx.accounts.registry_entry;
        registry_entry.operator = ctx.accounts.operator.key();
        registry_entry.peer_id = peer_id;
        registry_entry.multiaddresses = multiaddresses;
        registry_entry.node_name = node_name;
        registry_entry.is_active = true;
        registry_entry.last_registered = Clock::get()?.unix_timestamp;
        registry_entry.stake_amount = 0; // Staking requirement is bypassed for now
        Ok(())
    }

    pub fn update_rbn(ctx: Context<UpdateRbn>, peer_id: String, multiaddresses: String, node_name: String, is_active: bool) -> Result<()> {
        let registry_entry = &mut ctx.accounts.registry_entry;
        registry_entry.peer_id = peer_id;
        registry_entry.multiaddresses = multiaddresses;
        registry_entry.node_name = node_name;
        registry_entry.is_active = is_active;
        registry_entry.last_registered = Clock::get()?.unix_timestamp;
        Ok(())
    }
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
    pub operator: Signer<'info>,
}

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
