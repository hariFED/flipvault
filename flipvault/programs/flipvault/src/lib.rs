pub mod constants;
pub mod curve;
pub mod error;
pub mod flip;
pub mod instructions;
pub mod state;
pub mod util;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H");

#[program]
pub mod flipvault {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        seed_sol: u64,
        init_r_tok: u64,
        round_secs: i64,
        fee_bps: u16,
        min_reserve: u64,
        treasury_authority: Pubkey,
    ) -> Result<()> {
        instructions::initialize::handler(
            ctx,
            seed_sol,
            init_r_tok,
            round_secs,
            fee_bps,
            min_reserve,
            treasury_authority,
        )
    }

    pub fn deposit(ctx: Context<Deposit>, vault_id: u8, slot: u8, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, vault_id, slot, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, vault_id: u8, slot: u8, shares: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, vault_id, slot, shares)
    }

    pub fn sweep_treasury(ctx: Context<SweepTreasury>, amount: u64) -> Result<()> {
        instructions::sweep_treasury::handler(ctx, amount)
    }

    pub fn commit_round(ctx: Context<CommitRound>, force: [u8; 32]) -> Result<()> {
        instructions::commit_round::handler(ctx, force)
    }

    pub fn settle_round(ctx: Context<SettleRound>) -> Result<()> {
        instructions::settle_round::handler(ctx)
    }

    pub fn recover_round(ctx: Context<RecoverRound>) -> Result<()> {
        instructions::recover_round::handler(ctx)
    }
}
