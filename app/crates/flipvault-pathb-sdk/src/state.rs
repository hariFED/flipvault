//! On-chain account mirrors for Path-B. Field order/types match
//! programs/flipvault-pathb/src/state.rs EXACTLY so raw account data borsh-decodes after the
//! 8-byte Anchor discriminator. Encrypted blobs are opaque `[[u8;32]; N]` ciphertext (only the
//! owning player, via the MXE shared secret, can decrypt — not this SDK).
use borsh::{BorshDeserialize, BorshSerialize};
use solana_pubkey::Pubkey;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct PathBConfig {
    pub bump: u8,
    pub treasury_authority: Pubkey,
    pub k: u128,
    pub fee_bps: u16,
    pub active_box_count: u32,
    pub curve_version: u64,
    pub curve_ready: bool,
    pub treasury_ready: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct CurveState {
    pub bump: u8,
    pub ct: [[u8; 32]; 2],
    pub nonce: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct TreasuryState {
    pub bump: u8,
    pub ct: [[u8; 32]; 1],
    pub nonce: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct PlayerBox {
    pub bump: u8,
    pub ct: [[u8; 32]; 4],
    pub nonce: u128,
    pub enc_pubkey: [u8; 32],
    pub owner: Pubkey,
    pub index: u32,
    pub pending: bool,
    pub curve_version_at_queue: u64,
    pub pending_withdraw: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct Vault {
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
