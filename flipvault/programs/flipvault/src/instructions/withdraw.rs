use crate::constants::*;
use crate::error::FlipError;
use crate::state::*;
use crate::util::{move_lamports_out, mul_div_floor, rent_floor};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(vault_id: u8, slot: u8)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [VAULT_SEED, &[vault_id]],
        bump = vault.bump,
        constraint = vault.vault_id == vault_id @ FlipError::InvalidVault,
    )]
    pub vault: Account<'info, Vault>,

    #[account(
        mut,
        seeds = [POSITION_SEED, user.key().as_ref(), &[vault_id], &[slot]],
        bump = position.bump,
        constraint = position.owner == user.key() @ FlipError::Unauthorized,
    )]
    pub position: Account<'info, Position>,

    #[account(mut, seeds = [TREASURY_SEED], bump = config.treasury_bump)]
    pub treasury: Account<'info, Treasury>,
}

pub fn handler(ctx: Context<Withdraw>, _vault_id: u8, slot: u8, shares: u64) -> Result<()> {
    require!(ctx.accounts.config.phase == RoundPhase::Idle, FlipError::RoundPending);
    let slot_idx = slot as usize;
    require!(slot_idx < 2, FlipError::InvalidParams);
    require!(shares > 0, FlipError::ZeroShares);

    let fee_bps = ctx.accounts.config.fee_bps;

    let (payout_gross, fee, user_amount) = {
        let tr = &ctx.accounts.vault.tranches[slot_idx];
        // Withdraw is only ever from a SOL-denominated tranche.
        require!(tr.asset == Asset::Sol, FlipError::NotSolTranche);
        require!(tr.total_shares > 0, FlipError::ZeroShares);
        require!(shares <= ctx.accounts.position.shares, FlipError::InsufficientShares);

        let payout_gross = mul_div_floor(shares, tr.amount, tr.total_shares)?;
        require!(payout_gross > 0, FlipError::ZeroShares);
        let fee = mul_div_floor(payout_gross, fee_bps as u64, BPS_DENOM)?;
        let user_amount = payout_gross.checked_sub(fee).ok_or(FlipError::Overflow)?;
        (payout_gross, fee, user_amount)
    };

    // State before transfers (re-entrancy hygiene).
    {
        let tr = &mut ctx.accounts.vault.tranches[slot_idx];
        tr.amount = tr.amount.checked_sub(payout_gross).ok_or(FlipError::Overflow)?;
        tr.total_shares = tr.total_shares.checked_sub(shares).ok_or(FlipError::Overflow)?;
    }
    {
        let p = &mut ctx.accounts.position;
        p.shares = p.shares.checked_sub(shares).ok_or(FlipError::Overflow)?;
    }

    // Pay out of the vault PDA by direct lamport mutation: 90% user, 10% treasury.
    let vault_ai = ctx.accounts.vault.to_account_info();
    let floor = rent_floor(vault_ai.data_len())?;
    move_lamports_out(&vault_ai, &ctx.accounts.user.to_account_info(), user_amount, floor)?;
    move_lamports_out(&vault_ai, &ctx.accounts.treasury.to_account_info(), fee, floor)?;

    Ok(())
}
