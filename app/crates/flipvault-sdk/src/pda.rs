use crate::ids::*;
use solana_pubkey::Pubkey;

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG_SEED], &PROGRAM_ID)
}
pub fn reserve_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[RESERVE_SEED], &PROGRAM_ID)
}
pub fn treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TREASURY_SEED], &PROGRAM_ID)
}
pub fn vault_pda(vault_id: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, &[vault_id]], &PROGRAM_ID)
}
pub fn position_pda(owner: &Pubkey, vault_id: u8, slot: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[POSITION_SEED, owner.as_ref(), &[vault_id], &[slot]],
        &PROGRAM_ID,
    )
}

pub fn orao_network_state() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ORAO_CONFIG_SEED], &ORAO_VRF_ID)
}
pub fn orao_randomness(seed: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ORAO_RANDOMNESS_SEED, seed], &ORAO_VRF_ID)
}
