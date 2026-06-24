use solana_pubkey::{pubkey, Pubkey};

/// Deployed FlipVault program (devnet + future mainnet share the address).
pub const PROGRAM_ID: Pubkey = pubkey!("EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H");
/// ORAO VRF program (same on devnet/mainnet).
pub const ORAO_VRF_ID: Pubkey = pubkey!("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");
/// System program (all-zero pubkey).
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

// FlipVault PDA seeds.
pub const CONFIG_SEED: &[u8] = b"config";
pub const RESERVE_SEED: &[u8] = b"reserve";
pub const TREASURY_SEED: &[u8] = b"treasury";
pub const VAULT_SEED: &[u8] = b"vault";
pub const POSITION_SEED: &[u8] = b"position";

// ORAO PDA seeds.
pub const ORAO_CONFIG_SEED: &[u8] = b"orao-vrf-network-configuration";
pub const ORAO_RANDOMNESS_SEED: &[u8] = b"orao-vrf-randomness-request";

pub const NUM_VAULTS: u8 = 4;
/// Sentinel for Config.selected_vault when no vault is selected.
pub const NO_VAULT: u8 = 0xFF;
/// Basis-points denominator (10_000 = 100%); fee_bps = 1000 = 10%.
pub const BPS_DENOM: u64 = 10_000;
