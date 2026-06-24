use anchor_lang::prelude::*;

/// House bankroll PDA. Its spendable lamports (`lamports - rent_floor`) ARE the curve's
/// `r_sol`. Program-owned and data-carrying, so lamports are moved by direct mutation
/// (the System Program cannot debit it), always keeping it rent-exempt.
#[account]
#[derive(InitSpace, Debug)]
pub struct Reserve {
    pub bump: u8,
}
