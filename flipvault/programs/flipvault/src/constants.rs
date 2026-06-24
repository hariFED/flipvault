// PDA seeds.
pub const CONFIG_SEED: &[u8] = b"config";
pub const RESERVE_SEED: &[u8] = b"reserve";
pub const VAULT_SEED: &[u8] = b"vault";
pub const POSITION_SEED: &[u8] = b"position";
pub const TREASURY_SEED: &[u8] = b"treasury";

/// Number of vaults backed by the shared curve.
pub const NUM_VAULTS: u8 = 4;

/// Tranche slot layout. Genesis: slot 0 is SOL, slot 1 is TOKEN. Asset flags swap on flip;
/// the physical slot index is the stable key for Position PDAs.
pub const SLOT_SOL_GENESIS: usize = 0;
pub const SLOT_TOKEN_GENESIS: usize = 1;

/// Fee basis points denominator (10_000 = 100%).
pub const BPS_DENOM: u64 = 10_000;
/// Safety cap on the configurable fee (20%).
pub const MAX_FEE_BPS: u16 = 2_000;

/// Minimum first/any deposit, guards against dust and zero-share griefing (lamports).
pub const MIN_DEPOSIT: u64 = 1_000;

/// Sentinel for "no vault selected".
pub const NO_VAULT: u8 = 0xFF;

/// How long a `Pending` round may wait for VRF before `recover_round` can cancel it (seconds).
/// Well under ORAO's request expiry; ORAO normally fulfills sub-second.
pub const RECOVER_AFTER_SECS: i64 = 300;
