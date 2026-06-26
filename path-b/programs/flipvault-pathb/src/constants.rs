//! Seeds, sizes, and fixed parameters for FlipVault Path-B.

use anchor_lang::prelude::*;

// PDA seeds (mirror Path-A's naming so the two programs read familiarly).
#[constant]
pub const CONFIG_SEED: &[u8] = b"config";
#[constant]
pub const CURVE_SEED: &[u8] = b"curve";
#[constant]
pub const TREASURY_SEED: &[u8] = b"treasury";
#[constant]
pub const BOX_SEED: &[u8] = b"box";
#[constant]
pub const REGISTRY_SEED: &[u8] = b"registry";
#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

/// Fee basis-point denominator (10_000 = 100%).
pub const BPS_DENOM: u128 = 10_000;
/// Hard ceiling on the configurable fee (20%).
pub const MAX_FEE_BPS: u16 = 2_000;

/// Sentinel "no box selected" index.
pub const NO_BOX: u32 = u32::MAX;

/// A box can be unstuck (pending lock cleared) only after this many seconds — covers a
/// dropped/aborted MPC computation. Mirrors Path-A's RECOVER_AFTER_SECS intent.
pub const RECOVER_AFTER_SECS: i64 = 300;

/// Registry cap for the v1 soak (dense index space for VRF modulo). Fixed so the on-chain
/// registry account size is bounded.
pub const MAX_BOXES: u32 = 256;

/// Scalar counts per encrypted struct (these MUST match the circuit's `to_arcis()` boundary
/// and the generated IDL: Curve=2, treasury=1, BoxState=4).
pub const CURVE_SCALARS: usize = 2; // r_sol, r_tok
pub const TREASURY_SCALARS: usize = 1; // amount
pub const BOX_SCALARS: usize = 4; // sol, perp, in_perp, cost_basis
