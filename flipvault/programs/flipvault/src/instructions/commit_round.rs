use crate::constants::*;
use crate::error::FlipError;
use crate::state::*;
use anchor_lang::prelude::*;
use orao_solana_vrf::program::OraoVrf;
use orao_solana_vrf::state::NetworkState;
use orao_solana_vrf::{CONFIG_ACCOUNT_SEED, RANDOMNESS_ACCOUNT_SEED};

/// Open a round: time-guarded, requests fresh ORAO randomness, and locks the vaults.
/// Permissionless — any keeper may call it (and pays the ORAO fee + request rent).
#[derive(Accounts)]
#[instruction(force: [u8; 32])]
pub struct CommitRound<'info> {
    #[account(mut)]
    pub keeper: Signer<'info>,

    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    /// CHECK: ORAO randomness request PDA — created by the CPI below (must be fresh).
    #[account(
        mut,
        seeds = [RANDOMNESS_ACCOUNT_SEED, &force],
        bump,
        seeds::program = orao_solana_vrf::ID,
    )]
    pub random: AccountInfo<'info>,

    /// CHECK: ORAO treasury; ORAO validates it against its own network config.
    #[account(mut)]
    pub orao_treasury: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [CONFIG_ACCOUNT_SEED],
        bump,
        seeds::program = orao_solana_vrf::ID,
    )]
    pub network_state: Account<'info, NetworkState>,

    pub vrf: Program<'info, OraoVrf>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CommitRound>, force: [u8; 32]) -> Result<()> {
    // ORAO rejects a zero seed; reject early with a clear error.
    require!(force != [0u8; 32], FlipError::InvalidParams);

    let now = Clock::get()?.unix_timestamp;
    {
        let cfg = &ctx.accounts.config;
        require!(cfg.phase == RoundPhase::Idle, FlipError::RoundPending);
        let next = cfg
            .last_settled_ts
            .checked_add(cfg.round_secs)
            .ok_or(FlipError::Overflow)?;
        require!(now >= next, FlipError::RoundTooSoon);
    }

    // request_v2 inits the request PDA (fails if it already exists), so the round is always
    // bound to a fresh, not-yet-fulfilled randomness account.
    let cpi_accounts = orao_solana_vrf::cpi::accounts::RequestV2 {
        payer: ctx.accounts.keeper.to_account_info(),
        network_state: ctx.accounts.network_state.to_account_info(),
        treasury: ctx.accounts.orao_treasury.to_account_info(),
        request: ctx.accounts.random.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
    };
    orao_solana_vrf::cpi::request_v2(
        CpiContext::new(ctx.accounts.vrf.to_account_info(), cpi_accounts),
        force,
    )?;

    let slot = Clock::get()?.slot;
    let cfg = &mut ctx.accounts.config;
    cfg.phase = RoundPhase::Pending; // locks deposits/withdrawals
    cfg.round_seed = force;
    cfg.commit_slot = slot;
    cfg.commit_ts = now;
    cfg.selected_vault = NO_VAULT;
    Ok(())
}
