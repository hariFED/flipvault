use anchor_lang::prelude::*;

/// Accumulates withdrawal fees. Swept only by `Config::treasury_authority`. Holding fees
/// here (separate from the reserve and vaults) keeps the program immutable over user funds
/// while still letting the protocol collect revenue.
#[account]
#[derive(InitSpace, Debug)]
pub struct Treasury {
    pub bump: u8,
}
