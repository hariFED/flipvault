use crate::constants::*;
use crate::error::FlipError;
use crate::state::*;
use anchor_lang::prelude::*;

/// Recover a round stuck waiting on VRF. After `RECOVER_AFTER_SECS`, cancel it (no flip) and
/// unlock the vaults; a fresh `commit_round` then retries with a new seed. Permissionless.
#[derive(Accounts)]
pub struct RecoverRound<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,
}

pub fn handler(ctx: Context<RecoverRound>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let cfg = &mut ctx.accounts.config;
    require!(cfg.phase == RoundPhase::Pending, FlipError::NoPendingRound);
    let deadline = cfg
        .commit_ts
        .checked_add(RECOVER_AFTER_SECS)
        .ok_or(FlipError::Overflow)?;
    require!(now >= deadline, FlipError::RecoverTooSoon);

    cfg.phase = RoundPhase::Idle;
    cfg.last_settled_ts = now;
    cfg.selected_vault = NO_VAULT;
    Ok(())
}
