//! Program error codes for FlipVault Path-B.

use anchor_lang::prelude::*;

#[error_code]
pub enum PathBError {
    #[msg("Invalid initialization parameters")]
    InvalidParams,
    #[msg("Fee exceeds the maximum allowed")]
    FeeTooHigh,
    #[msg("Round is not due yet")]
    RoundTooSoon,
    #[msg("A round is already in progress")]
    RoundPending,
    #[msg("No round is pending")]
    NoPendingRound,
    #[msg("VRF randomness is not yet resolved")]
    RandomnessNotResolved,
    #[msg("Too soon to recover this round")]
    RecoverTooSoon,
    #[msg("Arithmetic overflow")]
    Overflow,
    /// verify_output() rejected the cluster's signed output (BLS check failed / aborted).
    #[msg("The confidential computation was aborted")]
    AbortedComputation,
    /// The shared curve was mutated by a newer committed flip since this flip was queued.
    #[msg("Stale callback: curve version changed since queue")]
    StaleCallback,
    /// The box is locked by an in-flight computation.
    #[msg("Box is locked by a pending computation")]
    BoxPending,
    /// Keeper passed a box whose index doesn't match the VRF-selected index.
    #[msg("Selected box index mismatch")]
    IndexMismatch,
    /// Withdraw debit returned ok=false (insufficient encrypted balance).
    #[msg("Insufficient balance")]
    InsufficientBalance,
    /// Registry is full (v1 cap).
    #[msg("Box registry is full")]
    RegistryFull,
    /// Deposits/withdrawals are only allowed while the box is on the SOL side.
    #[msg("Box is not on the SOL side")]
    NotOnSolSide,
    /// Caller is not the configured treasury authority.
    #[msg("Unauthorized")]
    Unauthorized,
}
