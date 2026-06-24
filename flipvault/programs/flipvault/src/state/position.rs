use anchor_lang::prelude::*;

/// Non-transferable per-user share ledger, keyed by (owner, vault_id, slot index).
/// Non-transferability is structural: no instruction reassigns `owner`, and the `owner`
/// seed binds the PDA to a single wallet.
#[account]
#[derive(InitSpace, Debug)]
pub struct Position {
    pub owner: Pubkey,
    pub vault_id: u8,
    /// Physical tranche slot index (0 or 1) — stable across flips.
    pub slot: u8,
    pub shares: u64,
    pub bump: u8,
}
