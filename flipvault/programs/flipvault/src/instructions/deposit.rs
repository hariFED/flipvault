use crate::constants::*;
use crate::error::FlipError;
use crate::state::*;
use crate::util::mul_div_floor;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(vault_id: u8, slot: u8)]
pub struct Deposit<'info> {
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
        init_if_needed,
        payer = user,
        space = 8 + Position::INIT_SPACE,
        seeds = [POSITION_SEED, user.key().as_ref(), &[vault_id], &[slot]],
        bump,
    )]
    pub position: Account<'info, Position>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Deposit>, vault_id: u8, slot: u8, amount: u64) -> Result<()> {
    // Vaults are locked while a round is settling (blocks the snapshot/front-run window).
    require!(ctx.accounts.config.phase == RoundPhase::Idle, FlipError::RoundPending);
    require!(amount >= MIN_DEPOSIT, FlipError::DepositTooSmall);
    let slot_idx = slot as usize;
    require!(slot_idx < 2, FlipError::InvalidParams);

    // Compute shares against the in-state tranche amount (never the raw balance).
    let shares = {
        let tr = &ctx.accounts.vault.tranches[slot_idx];
        require!(tr.asset == Asset::Sol, FlipError::NotSolTranche);
        if tr.total_shares == 0 {
            amount // first deposit: 1:1
        } else {
            require!(tr.amount > 0, FlipError::CurveMath);
            mul_div_floor(amount, tr.total_shares, tr.amount)?
        }
    };
    require!(shares > 0, FlipError::ZeroShares);

    {
        let tr = &mut ctx.accounts.vault.tranches[slot_idx];
        tr.amount = tr.amount.checked_add(amount).ok_or(FlipError::Overflow)?;
        tr.total_shares = tr.total_shares.checked_add(shares).ok_or(FlipError::Overflow)?;
    }
    {
        let p = &mut ctx.accounts.position;
        p.owner = ctx.accounts.user.key();
        p.vault_id = vault_id;
        p.slot = slot;
        p.shares = p.shares.checked_add(shares).ok_or(FlipError::Overflow)?;
        p.bump = ctx.bumps.position;
    }

    // Move lamports user -> vault (crediting a program account is allowed).
    let cpi = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        anchor_lang::system_program::Transfer {
            from: ctx.accounts.user.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
        },
    );
    anchor_lang::system_program::transfer(cpi, amount)?;

    Ok(())
}
