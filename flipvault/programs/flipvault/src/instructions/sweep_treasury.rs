use crate::constants::*;
use crate::error::FlipError;
use crate::state::*;
use crate::util::{move_lamports_out, rent_floor};
use anchor_lang::prelude::*;

/// Sweep accrued withdrawal fees out of the Treasury PDA. Restricted to the
/// `treasury_authority` set at init; it can only touch the treasury, never the reserve or
/// vaults — so this is compatible with the program being immutable over user funds.
#[derive(Accounts)]
pub struct SweepTreasury<'info> {
    pub authority: Signer<'info>,

    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.treasury_authority == authority.key() @ FlipError::Unauthorized,
    )]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [TREASURY_SEED], bump = config.treasury_bump)]
    pub treasury: Account<'info, Treasury>,

    /// CHECK: fee recipient; only ever credited.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<SweepTreasury>, amount: u64) -> Result<()> {
    let treasury_ai = ctx.accounts.treasury.to_account_info();
    let floor = rent_floor(treasury_ai.data_len())?;
    move_lamports_out(
        &treasury_ai,
        &ctx.accounts.recipient.to_account_info(),
        amount,
        floor,
    )?;
    Ok(())
}
