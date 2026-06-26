use solana_pubkey::{pubkey, Pubkey};

/// FlipVault Path-B program (devnet placeholder = scaffold keypair; re-synced on deploy).
pub const PROGRAM_ID: Pubkey = pubkey!("BH5GgPvyxUYLFHFMZ77g4DrY6fNpfu2u5XBFDn8E8xyr");
/// System program (all-zero pubkey).
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

// Path-B PDA seeds (must match programs/flipvault-pathb/src/constants.rs).
pub const CONFIG_SEED: &[u8] = b"config";
pub const CURVE_SEED: &[u8] = b"curve";
pub const TREASURY_SEED: &[u8] = b"treasury";
pub const BOX_SEED: &[u8] = b"box";
pub const REGISTRY_SEED: &[u8] = b"registry";
pub const VAULT_SEED: &[u8] = b"vault";

/// Basis-points denominator (10_000 = 100%); fee_bps = 1000 = 10%.
pub const BPS_DENOM: u64 = 10_000;
/// Registry cap (v1 soak).
pub const MAX_BOXES: u32 = 256;
/// Sentinel "no box selected".
pub const NO_BOX: u32 = u32::MAX;

// Encrypted-scalar counts per blob (must match the circuit `to_arcis()` boundary / the IDL).
pub const CURVE_SCALARS: usize = 2; // r_sol, r_tok
pub const TREASURY_SCALARS: usize = 1;
pub const BOX_SCALARS: usize = 4; // sol, perp, in_perp, cost_basis
