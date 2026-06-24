//! On-chain account mirrors. Field order/types match the program exactly so we can
//! borsh-decode raw account data (after skipping the 8-byte Anchor discriminator).
use borsh::{BorshDeserialize, BorshSerialize};
use solana_pubkey::Pubkey;

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundPhase {
    Idle,
    Pending,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asset {
    Sol,
    Token,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug)]
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

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct Config {
    pub treasury_authority: Pubkey,
    pub r_tok: u128,
    pub k: u128,
    pub round_secs: i64,
    pub last_settled_ts: i64,
    pub fee_bps: u16,
    pub min_reserve: u64,
    pub phase: RoundPhase,
    pub round_seed: [u8; 32],
    pub commit_slot: u64,
    pub commit_ts: i64,
    pub selected_vault: u8,
    pub bump: u8,
    pub reserve_bump: u8,
    pub treasury_bump: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct Vault {
    pub vault_id: u8,
    pub tranches: [Tranche; 2],
    pub bump: u8,
}

impl Vault {
    pub fn sol_slot(&self) -> Option<usize> {
        self.tranches.iter().position(|t| t.is_sol())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct Position {
    pub owner: Pubkey,
    pub vault_id: u8,
    pub slot: u8,
    pub shares: u64,
    pub bump: u8,
}

/// Decode an Anchor account: skip the 8-byte discriminator, then borsh-deserialize.
pub fn decode<T: BorshDeserialize>(account_data: &[u8]) -> std::io::Result<T> {
    if account_data.len() < 8 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "account too small for discriminator",
        ));
    }
    T::try_from_slice(&account_data[8..])
}
