use crate::constants::*;
use crate::error::FlipError;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub founder: Signer<'info>,

    #[account(
        init,
        payer = founder,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,

    /// House bankroll. Funded with `seed_sol` on top of its rent-exempt minimum.
    #[account(
        init,
        payer = founder,
        space = 8 + Reserve::INIT_SPACE,
        seeds = [RESERVE_SEED],
        bump
    )]
    pub reserve: Account<'info, Reserve>,

    #[account(
        init,
        payer = founder,
        space = 8 + Treasury::INIT_SPACE,
        seeds = [TREASURY_SEED],
        bump
    )]
    pub treasury: Account<'info, Treasury>,

    #[account(init, payer = founder, space = 8 + Vault::INIT_SPACE, seeds = [VAULT_SEED, &[0u8]], bump)]
    pub vault0: Account<'info, Vault>,
    #[account(init, payer = founder, space = 8 + Vault::INIT_SPACE, seeds = [VAULT_SEED, &[1u8]], bump)]
    pub vault1: Account<'info, Vault>,
    #[account(init, payer = founder, space = 8 + Vault::INIT_SPACE, seeds = [VAULT_SEED, &[2u8]], bump)]
    pub vault2: Account<'info, Vault>,
    #[account(init, payer = founder, space = 8 + Vault::INIT_SPACE, seeds = [VAULT_SEED, &[3u8]], bump)]
    pub vault3: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

fn set_vault(v: &mut Account<'_, Vault>, id: u8, bump: u8) {
    v.vault_id = id;
    // Genesis layout: slot 0 = SOL, slot 1 = TOKEN, both empty.
    v.tranches = [
        Tranche { asset: Asset::Sol, amount: 0, total_shares: 0 },
        Tranche { asset: Asset::Token, amount: 0, total_shares: 0 },
    ];
    v.bump = bump;
}

pub fn handler(
    ctx: Context<Initialize>,
    seed_sol: u64,
    init_r_tok: u64,
    round_secs: i64,
    fee_bps: u16,
    min_reserve: u64,
    treasury_authority: Pubkey,
) -> Result<()> {
    require!(seed_sol > 0 && init_r_tok > 0, FlipError::InvalidParams);
    require!(round_secs > 0, FlipError::InvalidParams);
    require!(fee_bps <= MAX_FEE_BPS, FlipError::InvalidParams);
    // The configured floor must leave room for the seed bankroll above it.
    require!((min_reserve as u128) < (seed_sol as u128), FlipError::InvalidParams);

    let k = (seed_sol as u128)
        .checked_mul(init_r_tok as u128)
        .ok_or(FlipError::Overflow)?;

    // Fund the reserve with `seed_sol` (founder is System-owned, so a CPI transfer is correct).
    let cpi = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        anchor_lang::system_program::Transfer {
            from: ctx.accounts.founder.to_account_info(),
            to: ctx.accounts.reserve.to_account_info(),
        },
    );
    anchor_lang::system_program::transfer(cpi, seed_sol)?;

    let now = Clock::get()?.unix_timestamp;
    let config = &mut ctx.accounts.config;
    config.treasury_authority = treasury_authority;
    config.r_tok = init_r_tok as u128;
    config.k = k;
    config.round_secs = round_secs;
    config.last_settled_ts = now;
    config.fee_bps = fee_bps;
    config.min_reserve = min_reserve;
    config.phase = RoundPhase::Idle;
    config.round_seed = [0u8; 32];
    config.commit_slot = 0;
    config.commit_ts = 0;
    config.selected_vault = NO_VAULT;
    config.bump = ctx.bumps.config;
    config.reserve_bump = ctx.bumps.reserve;
    config.treasury_bump = ctx.bumps.treasury;

    ctx.accounts.reserve.bump = ctx.bumps.reserve;
    ctx.accounts.treasury.bump = ctx.bumps.treasury;

    set_vault(&mut ctx.accounts.vault0, 0, ctx.bumps.vault0);
    set_vault(&mut ctx.accounts.vault1, 1, ctx.bumps.vault1);
    set_vault(&mut ctx.accounts.vault2, 2, ctx.bumps.vault2);
    set_vault(&mut ctx.accounts.vault3, 3, ctx.bumps.vault3);

    Ok(())
}
