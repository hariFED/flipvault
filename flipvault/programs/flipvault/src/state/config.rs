use anchor_lang::prelude::*;

/// Round state machine. While `Pending`, all vaults are locked (no deposit/withdraw),
/// which both freezes the eligibility snapshot and blocks post-reveal front-running.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum RoundPhase {
    Idle,
    Pending,
}

impl Default for RoundPhase {
    fn default() -> Self {
        RoundPhase::Idle
    }
}

/// Global singleton: the shared curve and the round lifecycle.
///
/// Note: the canonical `r_sol` is the Reserve PDA's spendable lamports
/// (`reserve.lamports - rent_floor`); it is intentionally NOT duplicated here.
#[account]
#[derive(InitSpace, Debug)]
pub struct Config {
    /// May sweep the Treasury only — never the reserve or vaults.
    pub treasury_authority: Pubkey,
    /// Virtual token reserve (u128: pairs with r_sol to form k).
    pub r_tok: u128,
    /// Fixed genesis constant product. Never re-derived from drifted reserves.
    pub k: u128,
    /// Minimum seconds between settled rounds.
    pub round_secs: i64,
    /// Unix timestamp of the last settled (or cancelled) round.
    pub last_settled_ts: i64,
    /// Withdrawal fee in basis points (1000 = 10%).
    pub fee_bps: u16,
    /// Hard floor for spendable reserve `r_sol`; a flip that would breach it is skipped.
    pub min_reserve: u64,
    /// Round phase. `Pending` locks the vaults.
    pub phase: RoundPhase,
    /// The 32-byte ORAO VRF seed ("force") bound to the current pending round. `settle_round`
    /// re-derives the randomness PDA from this, so a different account cannot be substituted.
    pub round_seed: [u8; 32],
    /// Slot at which the current round was committed.
    pub commit_slot: u64,
    /// Timestamp at which the current round was committed (reveal-deadline anchor).
    pub commit_ts: i64,
    /// Vault selected by the last settled round (NO_VAULT when none).
    pub selected_vault: u8,
    pub bump: u8,
    pub reserve_bump: u8,
    pub treasury_bump: u8,
}
