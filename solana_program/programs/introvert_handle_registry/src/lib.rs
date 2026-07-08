use anchor_lang::prelude::*;
use ed25519_dalek::Verifier;
use std::str::FromStr;

declare_id!("FeQNoPnPvvaPKo2Hg4u1c2beSx9xWhQgEs1qVyTjSvrW");

/// Maximum length for the ip_address field (IP:Port or libp2p multiaddress)
const MAX_IP_ADDRESS_LEN: usize = 64;

/// Immutable protocol Master Authority — the only pubkey that can promote
/// entries to community RBN status. Hardcoded to prevent governance drift.
const MASTER_AUTHORITY: &str = "UUN1zBzL5g2TNGtHCVtJPUZNmMKfFrW8odwLPb4jFxk";

/// Minimum IP address length (e.g., "1.2.3.4:80")
const MIN_IP_ADDRESS_LEN: usize = 9;

/// 48-hour expiry window for onboarding intents (prevents state bloat)
const INTENT_EXPIRY_SECONDS: i64 = 172_800;

/// Validates that an ip_address string contains only safe characters:
/// alphanumeric, '.', ':', '/', '-', '[', ']', and '%' (for IPv6 scope).
/// Rejects injection payloads, shell metacharacters, and Unicode tricks.
fn is_valid_ip_address(s: &str) -> bool {
    if s.len() < MIN_IP_ADDRESS_LEN || s.len() > MAX_IP_ADDRESS_LEN {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric()
        || c == '.' || c == ':' || c == '/' || c == '-'
        || c == '[' || c == ']' || c == '%')
}

#[program]
pub mod introvert_handle_registry {
    use super::*;

    /// Initialize the global handle registry PDA (one-time, treasury-funded)
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let registry = &mut ctx.accounts.registry;
        registry.authority = ctx.accounts.authority.key();
        registry.total_handles = 0;
        msg!("Handle registry initialized by {}", registry.authority);
        Ok(())
    }

    /// Claim a handle. Treasury pays rent. Claimant signs to prove ownership.
    ///
    /// SECURITY: `verified` and `is_community_rbn` are ALWAYS set to false.
    /// Only the Master Authority can promote an entry via `register_community_rbn`.
    pub fn claim_handle(
        ctx: Context<ClaimHandle>,
        handle: String,
        peer_id: String,
        timestamp: i64,
    ) -> Result<()> {
        require!(handle.starts_with("i@"), HandleError::InvalidFormat);
        require!(handle.len() <= 64, HandleError::HandleTooLong);
        require!(peer_id.len() <= 64, HandleError::PeerIdTooLong);

        let entry = &mut ctx.accounts.handle_entry;
        require!(entry.owner_wallet == Pubkey::default(), HandleError::AlreadyClaimed);

        entry.handle = handle.clone();
        entry.peer_id = peer_id;
        entry.owner_wallet = ctx.accounts.claimant.key();
        entry.timestamp = timestamp;
        entry.verified = false;
        entry.is_community_rbn = false;
        entry.ip_address = String::new();

        let registry = &mut ctx.accounts.registry;
        registry.total_handles += 1;

        emit!(HandleClaimed {
            handle,
            owner: ctx.accounts.claimant.key(),
            timestamp,
        });

        Ok(())
    }

    /// Update routing metadata for an existing handle entry.
    /// Only the entry owner can call this. Does NOT touch `is_community_rbn`
    /// or `verified` — those are structurally separated and only writable
    /// by the Master Authority via `register_community_rbn`.
    pub fn update_rbn_routing(
        ctx: Context<UpdateRBNRouting>,
        new_ip_address: String,
    ) -> Result<()> {
        require!(
            new_ip_address.len() <= MAX_IP_ADDRESS_LEN,
            HandleError::IpAddressTooLong
        );
        require!(
            is_valid_ip_address(&new_ip_address),
            HandleError::InvalidIpAddress
        );

        let entry = &mut ctx.accounts.handle_mapping;

        require!(
            entry.owner_wallet == ctx.accounts.owner_wallet.key(),
            HandleError::Unauthorized
        );

        // Realloc if the new value is longer than the current allocation.
        let current_data_len = entry.to_account_info().data_len();
        let required_len = 8 // discriminator
            + 4 + entry.handle.len()
            + 4 + entry.peer_id.len()
            + 32 // owner_wallet
            + 8  // timestamp
            + 1  // verified
            + 1  // is_community_rbn
            + 4 + new_ip_address.len();

        if required_len > current_data_len {
            let new_space = required_len;
            let rent = Rent::get()?;
            let new_minimum_balance = rent.minimum_balance(new_space);
            let current_lamports = entry.to_account_info().lamports();
            if new_minimum_balance > current_lamports {
                let lamports_to_add = new_minimum_balance - current_lamports;
                anchor_lang::system_program::transfer(
                    CpiContext::new(
                        ctx.accounts.system_program.to_account_info(),
                        anchor_lang::system_program::Transfer {
                            from: ctx.accounts.owner_wallet.to_account_info(),
                            to: entry.to_account_info(),
                        },
                    ),
                    lamports_to_add,
                )?;
            }
            entry.to_account_info().realloc(new_space, false)?;
        }

        entry.ip_address = new_ip_address;

        msg!(
            "RBN routing updated for {}: ip_address={}",
            entry.handle,
            entry.ip_address
        );

        Ok(())
    }

    /// Submit an onboarding intent to become a community RBN.
    ///
    /// The operator pays rent for a temporary PDA. The Swarm Marshal polls
    /// these PDAs, test-dials, and if verified, calls `register_community_rbn`
    /// which closes the PDA and returns rent. Intents expire after 48 hours.
    pub fn submit_onboarding_intent(
        ctx: Context<SubmitIntent>,
        peer_id: String,
        ip_address: String,
        signature_bytes: [u8; 64],
    ) -> Result<()> {
        let handle_entry = &ctx.accounts.handle_entry;
        require!(
            handle_entry.owner_wallet != Pubkey::default(),
            HandleError::UnclaimedHandle
        );
        require!(
            handle_entry.owner_wallet == ctx.accounts.operator.key(),
            HandleError::Unauthorized
        );
        require!(
            peer_id == handle_entry.peer_id,
            HandleError::PeerIdMismatch
        );
        require!(
            ip_address.len() <= MAX_IP_ADDRESS_LEN,
            HandleError::IpAddressTooLong
        );
        require!(
            is_valid_ip_address(&ip_address),
            HandleError::InvalidIpAddress
        );

        // Capture on-chain clock for expiry enforcement
        let clock = Clock::get()?;

        let intent = &mut ctx.accounts.onboarding_intent;
        intent.handle_entry = ctx.accounts.handle_entry.key();
        intent.peer_id = peer_id;
        intent.ip_address = ip_address;
        intent.signature_bytes = signature_bytes;
        intent.target_wallet = ctx.accounts.operator.key();
        intent.created_at = clock.unix_timestamp;

        emit!(OnboardingIntentSubmitted {
            handle_entry: intent.handle_entry,
            peer_id: intent.peer_id.clone(),
            ip_address: intent.ip_address.clone(),
            target_wallet: intent.target_wallet,
        });

        msg!(
            "Onboarding intent submitted: handle_entry={}, peer_id={}, ip={}, expires_at={}",
            intent.handle_entry,
            intent.peer_id,
            intent.ip_address,
            intent.created_at + INTENT_EXPIRY_SECONDS
        );

        Ok(())
    }

    /// Register or promote a handle entry as a community RBN.
    ///
    /// SECURITY: Four independent controls:
    /// 1. Authority constraint — only Master Authority can invoke.
    /// 2. Handle state guard — entry must be claimed.
    /// 3. Parameter binding — peer_id must match on-chain entry.
    /// 4. Ed25519 signature proof — scoped to owner||handle||peer_id||ip.
    ///
    /// When `onboarding_intent` is provided, it is closed via Anchor's `close`
    /// directive and rent is returned to `operator`.
    pub fn register_community_rbn<'info>(
        ctx: Context<'_, '_, '_, 'info, RegisterRBN<'info>>,
        handle: String,
        peer_id: String,
        ip_address: String,
        signature_bytes: [u8; 64],
    ) -> Result<()> {
        // 1. Authority gate
        let master = Pubkey::from_str(MASTER_AUTHORITY)
            .map_err(|_| error!(HandleError::InvalidMasterAuthority))?;
        require_keys_eq!(
            ctx.accounts.authority.key(),
            master,
            HandleError::Unauthorized
        );

        // 2. State guard
        let entry = &ctx.accounts.handle_entry;
        require!(
            entry.owner_wallet != Pubkey::default(),
            HandleError::UnclaimedHandle
        );

        // 3. Parameter binding
        require!(
            peer_id == entry.peer_id,
            HandleError::PeerIdMismatch
        );

        // 4. Input validation
        require!(handle.starts_with("i@"), HandleError::InvalidFormat);
        require!(handle.len() <= 64, HandleError::HandleTooLong);
        require!(peer_id.len() <= 64, HandleError::PeerIdTooLong);
        require!(
            ip_address.len() <= MAX_IP_ADDRESS_LEN,
            HandleError::IpAddressTooLong
        );
        require!(
            is_valid_ip_address(&ip_address),
            HandleError::InvalidIpAddress
        );

        // 5. Ed25519 signature verification
        let peer_id_decoded = bs58_decode_peer_id(&entry.peer_id)
            .ok_or(error!(HandleError::InvalidPeerIdFormat))?;
        require!(
            peer_id_decoded.len() == 34
                && peer_id_decoded[0] == 0x00
                && peer_id_decoded[1] == 0x24,
            HandleError::InvalidPeerIdFormat
        );
        let ed25519_pubkey: [u8; 32] = peer_id_decoded[2..34]
            .try_into()
            .map_err(|_| error!(HandleError::InvalidPeerIdFormat))?;

        let mut verification_msg = Vec::new();
        verification_msg.extend_from_slice(&entry.owner_wallet.to_bytes());
        verification_msg.extend_from_slice(handle.as_bytes());
        verification_msg.extend_from_slice(peer_id.as_bytes());
        verification_msg.extend_from_slice(ip_address.as_bytes());

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&ed25519_pubkey)
            .map_err(|_| error!(HandleError::InvalidPeerIdFormat))?;
        let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);
        verifying_key.verify(&verification_msg, &signature)
            .map_err(|_| error!(HandleError::InvalidPeerSignature))?;

        // 6. Promote the entry
        let entry = &mut ctx.accounts.handle_entry;
        entry.verified = true;
        entry.is_community_rbn = true;
        entry.ip_address = ip_address.clone();

        emit!(CommunityRBNRegistered {
            handle: handle.clone(),
            peer_id: peer_id.clone(),
            ip_address: ip_address.clone(),
            authority: ctx.accounts.authority.key(),
        });

        msg!(
            "Community RBN registered: handle={}, peer_id={}, ip={}",
            handle,
            peer_id,
            ip_address
        );

        // 7. Intent PDA cleanup is handled automatically by Anchor's `close`
        //    directive on the `onboarding_intent` account in RegisterRBN.
        //    When the Option is Some, Anchor closes the account and transfers
        //    lamports to `operator` atomically.

        Ok(())
    }

    /// Cancel a pending onboarding intent and reclaim the rent deposit.
    /// Only the handle owner can call this.
    pub fn cancel_onboarding_intent(_ctx: Context<CancelOnboardingIntent>) -> Result<()> {
        msg!("Onboarding intent cancelled. Rent returned to operator.");
        Ok(())
    }

    /// Permissionlessly close an expired onboarding intent (>48 hours).
    /// Returns locked rent to the original operator wallet. Anyone can call
    /// this to clean up stale state and prevent account bloat.
    pub fn cancel_expired_intent(ctx: Context<CancelExpiredIntent>) -> Result<()> {
        // Enforce 48-hour expiry in the instruction body
        let clock = Clock::get()?;
        let intent = &ctx.accounts.onboarding_intent;
        require!(
            clock.unix_timestamp > intent.created_at + INTENT_EXPIRY_SECONDS,
            HandleError::IntentNotExpired
        );
        msg!(
            "Expired onboarding intent cleaned up (created_at={}, now={}). Rent returned to {}",
            intent.created_at, clock.unix_timestamp, ctx.accounts.target_wallet.key()
        );
        Ok(())
    }
}

/// Decode a base58-encoded libp2p PeerId into raw bytes.
fn bs58_decode_peer_id(peer_id: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut result = Vec::new();
    let mut leading_zeros = 0;
    for c in peer_id.bytes() {
        if c == b'1' { leading_zeros += 1; } else { break; }
    }
    for c in peer_id.bytes() {
        let val = match ALPHABET.iter().position(|&a| a == c) {
            Some(v) => v as u8,
            None => return None,
        };
        let mut carry = val as u32;
        for byte in result.iter_mut() {
            carry += (*byte as u32) * 58;
            *byte = (carry & 0xFF) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            result.push((carry & 0xFF) as u8);
            carry >>= 8;
        }
    }
    for _ in 0..leading_zeros { result.push(0); }
    result.reverse();
    Some(result)
}

// ─── Account Contexts ────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + 32 + 8, seeds = [b"handle_registry"], bump)]
    pub registry: Account<'info, HandleRegistry>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(handle: String)]
pub struct ClaimHandle<'info> {
    #[account(mut, seeds = [b"handle_registry"], bump)]
    pub registry: Account<'info, HandleRegistry>,
    #[account(
        init, payer = treasury,
        space = 8 + 4 + 64 + 4 + 64 + 32 + 8 + 1 + 1 + 4 + 64,
        seeds = [b"handle", handle.as_bytes()], bump,
    )]
    pub handle_entry: Account<'info, HandleEntry>,
    #[account(mut)]
    pub treasury: Signer<'info>,
    pub claimant: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(new_ip_address: String)]
pub struct UpdateRBNRouting<'info> {
    #[account(mut, seeds = [b"handle", handle_mapping.handle.as_bytes()], bump)]
    pub handle_mapping: Account<'info, HandleEntry>,
    #[account(mut)]
    pub owner_wallet: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(peer_id: String, ip_address: String, signature_bytes: [u8; 64])]
pub struct SubmitIntent<'info> {
    #[account(seeds = [b"handle", handle_entry.handle.as_bytes()], bump)]
    pub handle_entry: Account<'info, HandleEntry>,
    #[account(
        init, payer = operator,
        // 8 (disc) + 32 (handle_entry) + 4+64 (peer_id) + 4+64 (ip_address)
        // + 64 (signature_bytes) + 32 (target_wallet) + 8 (created_at) = 280
        space = 8 + 32 + 4 + 64 + 4 + 64 + 64 + 32 + 8,
        seeds = [b"onboarding-intent", handle_entry.key().as_ref()], bump,
    )]
    pub onboarding_intent: Account<'info, OnboardingIntent>,
    #[account(mut)]
    pub operator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(handle: String, peer_id: String, ip_address: String, signature_bytes: [u8; 64])]
pub struct RegisterRBN<'info> {
    #[account(mut, seeds = [b"handle", handle.as_bytes()], bump)]
    pub handle_entry: Account<'info, HandleEntry>,
    pub authority: Signer<'info>,
    /// The operator wallet — receives intent PDA rent refund via `close`.
    #[account(mut)]
    pub operator: UncheckedAccount<'info>,
    /// Optional onboarding intent PDA. Closed via Anchor `close` directive.
    #[account(
        mut,
        seeds = [b"onboarding-intent", handle_entry.key().as_ref()],
        bump,
        constraint = onboarding_intent.target_wallet == operator.key()
            @ HandleError::IntentTargetMismatch,
        close = operator,
    )]
    pub onboarding_intent: Option<Account<'info, OnboardingIntent>>,
}

#[derive(Accounts)]
pub struct CancelOnboardingIntent<'info> {
    #[account(
        mut,
        seeds = [b"onboarding-intent", handle_entry.key().as_ref()],
        bump,
        close = target_wallet,
    )]
    pub onboarding_intent: Account<'info, OnboardingIntent>,
    #[account(seeds = [b"handle", handle_entry.handle.as_bytes()], bump)]
    pub handle_entry: Account<'info, HandleEntry>,
    #[account(
        mut,
        constraint = target_wallet.key() == handle_entry.owner_wallet
            @ HandleError::Unauthorized,
    )]
    pub target_wallet: Signer<'info>,
}

#[derive(Accounts)]
pub struct CancelExpiredIntent<'info> {
    /// The expired intent PDA. Expiry is validated in the instruction body.
    /// Anchor's `close` directive transfers lamports to `target_wallet`.
    #[account(
        mut,
        seeds = [b"onboarding-intent", handle_entry.key().as_ref()],
        bump,
        close = target_wallet,
    )]
    pub onboarding_intent: Account<'info, OnboardingIntent>,
    #[account(seeds = [b"handle", handle_entry.handle.as_bytes()], bump)]
    pub handle_entry: Account<'info, HandleEntry>,
    /// Rent receiver — the original operator wallet stored in the intent.
    #[account(mut, address = onboarding_intent.target_wallet @ HandleError::Unauthorized)]
    pub target_wallet: AccountInfo<'info>,
}

// ─── State ───────────────────────────────────────────────────────────────────

#[account]
pub struct HandleRegistry {
    pub authority: Pubkey,
    pub total_handles: u64,
}

#[account]
pub struct HandleEntry {
    pub handle: String,
    pub peer_id: String,
    pub owner_wallet: Pubkey,
    pub timestamp: i64,
    pub verified: bool,
    pub is_community_rbn: bool,
    pub ip_address: String,
}

#[account]
pub struct OnboardingIntent {
    pub handle_entry: Pubkey,
    pub peer_id: String,
    pub ip_address: String,
    pub signature_bytes: [u8; 64],
    pub target_wallet: Pubkey,
    /// Unix timestamp when this intent was created. Used for 48-hour expiry.
    pub created_at: i64,
}

// ─── Events ──────────────────────────────────────────────────────────────────

#[event]
pub struct HandleClaimed {
    pub handle: String,
    pub owner: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct CommunityRBNRegistered {
    pub handle: String,
    pub peer_id: String,
    pub ip_address: String,
    pub authority: Pubkey,
}

#[event]
pub struct OnboardingIntentSubmitted {
    pub handle_entry: Pubkey,
    pub peer_id: String,
    pub ip_address: String,
    pub target_wallet: Pubkey,
}

// ─── Errors ──────────────────────────────────────────────────────────────────

#[error_code]
pub enum HandleError {
    #[msg("Handle must start with 'i@'")]
    InvalidFormat,
    #[msg("Handle too long (max 64 chars)")]
    HandleTooLong,
    #[msg("PeerId too long (max 64 chars)")]
    PeerIdTooLong,
    #[msg("Handle already claimed")]
    AlreadyClaimed,
    #[msg("IP address too long (max 64 chars)")]
    IpAddressTooLong,
    #[msg("IP address contains invalid characters")]
    InvalidIpAddress,
    #[msg("Unauthorized: signer is not the entry owner")]
    Unauthorized,
    #[msg("Unauthorized: only Master Authority can register community RBNs")]
    UnauthorizedAuthority,
    #[msg("Invalid Master Authority pubkey")]
    InvalidMasterAuthority,
    #[msg("PeerId is not a valid base58-encoded Ed25519 multihash")]
    InvalidPeerIdFormat,
    #[msg("Ed25519 signature does not match the claimed PeerId")]
    InvalidPeerSignature,
    #[msg("Handle has not been claimed yet — owner_wallet is default")]
    UnclaimedHandle,
    #[msg("PeerId argument does not match on-chain handle entry")]
    PeerIdMismatch,
    #[msg("Onboarding intent target_wallet does not match operator account")]
    IntentTargetMismatch,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("The target onboarding intent has not exceeded its 48-hour operational window")]
    IntentNotExpired,
}
