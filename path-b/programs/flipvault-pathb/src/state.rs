//! On-chain accounts for FlipVault Path-B.
//!
//! Encrypted state is stored as raw `[[u8;32]; N]` ciphertext + a `u128` nonce (the pattern the
//! Arcium v0.11.1 examples use — there is no `MXEEncryptedStruct` account type). For `Shared`
//! state (the player box) we also store the x25519 `enc_pubkey` the box is sealed to.
//!
//! CRITICAL: the `ct` array sits at byte offset 9 in every encrypted account (8 discriminator +
//! 1 `bump`), so `ArgBuilder.account(key, 9, 32*N)` reads exactly the ciphertext. Keep `bump`
//! first and `ct` second; never reorder.

use anchor_lang::prelude::*;
use crate::constants::*;

/// Global singleton. Curve constant `k` and `fee_bps` are PUBLIC (fed to the circuit as
/// plaintext); the curve *reserves* and treasury are encrypted in their own accounts.
#[account]
#[derive(InitSpace)]
pub struct PathBConfig {
    pub bump: u8,
    pub treasury_authority: Pubkey,
    pub k: u128,
    pub fee_bps: u16,
    pub active_box_count: u32,
    /// Bumped on every committed flip callback — the shared-curve stale-callback guard.
    pub curve_version: u64,
    /// Genesis bookkeeping: the curve/treasury ciphertext is minted asynchronously by init circuits.
    pub curve_ready: bool,
    pub treasury_ready: bool,
}

/// Shared bonding-curve reserves, `Enc<Mxe, Curve{r_sol, r_tok}>`.
#[account]
#[derive(InitSpace)]
pub struct CurveState {
    pub bump: u8,
    pub ct: [[u8; 32]; CURVE_SCALARS], // offset 9, len 64
    pub nonce: u128,
}

/// Accrued fees, `Enc<Mxe, u128>`.
#[account]
#[derive(InitSpace)]
pub struct TreasuryState {
    pub bump: u8,
    pub ct: [[u8; 32]; TREASURY_SCALARS], // offset 9, len 32
    pub nonce: u128,
}

/// Per-player box, `Enc<Shared, BoxState{sol, perp, in_perp, cost_basis}>`.
#[account]
#[derive(InitSpace)]
pub struct PlayerBox {
    pub bump: u8,
    pub ct: [[u8; 32]; BOX_SCALARS], // offset 9, len 128
    pub nonce: u128,
    /// The x25519 public key this box's Shared ciphertext is sealed to (the owner can re-derive
    /// the shared secret and decrypt; the MXE decrypts with its half).
    pub enc_pubkey: [u8; 32],
    pub owner: Pubkey,
    pub index: u32,
    /// Locked true while any computation on this box is in flight.
    pub pending: bool,
    /// Snapshot of config.curve_version taken at queue time (stale-callback guard).
    pub curve_version_at_queue: u64,
    /// Requested withdraw amount, remembered across the debit_box round-trip (callbacks get no
    /// instruction args). The callback pays this out from the vault iff the circuit reveals ok=true.
    pub pending_withdraw: u64,
}

/// Public SOL custody vault — a program-owned PDA holding ALL real lamports (the encrypted box
/// balances sum to this, minus reserve/treasury, but that's only verifiable inside the MXE).
/// Deposits transfer SOL in (amount public); withdrawals move SOL out by direct lamport mutation
/// after the `debit_box` circuit confirms a sufficient encrypted balance.
#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub bump: u8,
}

// Byte offsets/lengths for ArgBuilder.account(...). 8 (discriminator) + 1 (bump) = 9.
pub const CURVE_CT_OFFSET: u32 = 9;
pub const CURVE_CT_LEN: u32 = 32 * CURVE_SCALARS as u32;
pub const TREASURY_CT_OFFSET: u32 = 9;
pub const TREASURY_CT_LEN: u32 = 32 * TREASURY_SCALARS as u32;
pub const BOX_CT_OFFSET: u32 = 9;
pub const BOX_CT_LEN: u32 = 32 * BOX_SCALARS as u32;
