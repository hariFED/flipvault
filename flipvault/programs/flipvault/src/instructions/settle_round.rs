use crate::constants::*;
use crate::error::FlipError;
use crate::flip::execute_flip;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::AccountDeserialize;
use orao_solana_vrf::state::RandomnessAccountData;
use orao_solana_vrf::RANDOMNESS_ACCOUNT_SEED;

/// Settle a pending round: read the fulfilled randomness, pick `vault = rand % 4`, flip it,
/// and unlock the vaults. Permissionless and deterministic. The randomness account is pinned
/// to the seed stored at commit, so no substitution is possible.
#[derive(Accounts)]
pub struct SettleRound<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [RESERVE_SEED], bump = config.reserve_bump)]
    pub reserve: Account<'info, Reserve>,

    #[account(mut, seeds = [VAULT_SEED, &[0u8]], bump = vault0.bump)]
    pub vault0: Account<'info, Vault>,
    #[account(mut, seeds = [VAULT_SEED, &[1u8]], bump = vault1.bump)]
    pub vault1: Account<'info, Vault>,
    #[account(mut, seeds = [VAULT_SEED, &[2u8]], bump = vault2.bump)]
    pub vault2: Account<'info, Vault>,
    #[account(mut, seeds = [VAULT_SEED, &[3u8]], bump = vault3.bump)]
    pub vault3: Account<'info, Vault>,

    /// CHECK: ORAO randomness PDA, pinned to the round's committed seed.
    #[account(
        seeds = [RANDOMNESS_ACCOUNT_SEED, &config.round_seed],
        bump,
        seeds::program = orao_solana_vrf::ID,
    )]
    pub random: AccountInfo<'info>,
}

pub fn handler(ctx: Context<SettleRound>) -> Result<()> {
    require!(
        ctx.accounts.config.phase == RoundPhase::Pending,
        FlipError::NoPendingRound
    );

    // Extract the fulfilled randomness; errors (still pending) leave the round Pending.
    let rand_byte = {
        let acc = &ctx.accounts.random;
        require!(!acc.data_is_empty(), FlipError::RandomnessNotResolved);
        let data = RandomnessAccountData::try_deserialize(&mut &acc.data.borrow()[..])
            .map_err(|_| error!(FlipError::RandomnessNotResolved))?;
        *data
            .fulfilled_randomness()
            .ok_or(FlipError::RandomnessNotResolved)?
            .first()
            .ok_or(FlipError::RandomnessNotResolved)?
    };
    // 256 % 4 == 0, so a single byte mod 4 is exactly uniform for 4 vaults.
    let selected = (rand_byte % NUM_VAULTS) as usize;
    let now = Clock::get()?.unix_timestamp;

    // Clone lamport-holding AccountInfos up front so we can still hold &mut to the data.
    let reserve_ai = ctx.accounts.reserve.to_account_info();
    let v0 = ctx.accounts.vault0.to_account_info();
    let v1 = ctx.accounts.vault1.to_account_info();
    let v2 = ctx.accounts.vault2.to_account_info();
    let v3 = ctx.accounts.vault3.to_account_info();

    let _result = match selected {
        0 => execute_flip(&mut ctx.accounts.config, &mut ctx.accounts.vault0, &reserve_ai, &v0)?,
        1 => execute_flip(&mut ctx.accounts.config, &mut ctx.accounts.vault1, &reserve_ai, &v1)?,
        2 => execute_flip(&mut ctx.accounts.config, &mut ctx.accounts.vault2, &reserve_ai, &v2)?,
        _ => execute_flip(&mut ctx.accounts.config, &mut ctx.accounts.vault3, &reserve_ai, &v3)?,
    };

    let cfg = &mut ctx.accounts.config;
    cfg.phase = RoundPhase::Idle; // unlocks vaults
    cfg.last_settled_ts = now;
    cfg.selected_vault = selected as u8;
    Ok(())
}
