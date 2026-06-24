use anchor_lang::prelude::*;

#[error_code]
pub enum FlipError {
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Division by zero")]
    DivByZero,
    #[msg("Curve math error")]
    CurveMath,
    #[msg("Vaults are locked while a round is settling")]
    RoundPending,
    #[msg("No round is currently pending")]
    NoPendingRound,
    #[msg("Round interval has not elapsed yet")]
    RoundTooSoon,
    #[msg("Randomness is not yet resolved")]
    RandomnessNotResolved,
    #[msg("Reveal deadline has not passed yet")]
    RecoverTooSoon,
    #[msg("Deposit below minimum")]
    DepositTooSmall,
    #[msg("Operation would mint or burn zero shares")]
    ZeroShares,
    #[msg("Insufficient shares in position")]
    InsufficientShares,
    #[msg("Tranche is not SOL-denominated; cannot withdraw")]
    NotSolTranche,
    #[msg("Reserve floor would be breached")]
    ReserveFloor,
    #[msg("Invalid vault id")]
    InvalidVault,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Invalid parameters")]
    InvalidParams,
}

impl From<crate::curve::CurveError> for FlipError {
    fn from(e: crate::curve::CurveError) -> Self {
        match e {
            crate::curve::CurveError::DivByZero => FlipError::DivByZero,
            crate::curve::CurveError::Overflow => FlipError::Overflow,
        }
    }
}
