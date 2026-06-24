use anchor_lang::prelude::*;

/// Which asset a tranche is currently denominated in.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum Asset {
    Sol,
    Token,
}

impl Default for Asset {
    fn default() -> Self {
        Asset::Sol
    }
}

/// One tranche slot. `amount` is an authoritative in-state counter — NEVER read from the
/// account's raw lamport balance (that is what defeats the donation/inflation attack).
/// When `asset == Sol`, `amount` is lamports; when `asset == Token`, it is virtual tokens.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, InitSpace, Debug)]
pub struct Tranche {
    pub asset: Asset,
    pub amount: u64,
    pub total_shares: u64,
}

impl Tranche {
    pub fn is_sol(&self) -> bool {
        self.asset == Asset::Sol
    }
}

/// One of four vaults. Holds its SOL-tranche's lamports physically; stores both tranche
/// slots. Asset flags swap on flip while each slot's share ledger persists, so Position
/// PDAs key off the physical slot index (0/1), never the asset flag.
#[account]
#[derive(InitSpace, Debug)]
pub struct Vault {
    pub vault_id: u8,
    pub tranches: [Tranche; 2],
    pub bump: u8,
}

impl Vault {
    /// Index of the slot currently denominated in SOL, if any.
    pub fn sol_slot(&self) -> Option<usize> {
        self.tranches.iter().position(|t| t.is_sol())
    }
}
