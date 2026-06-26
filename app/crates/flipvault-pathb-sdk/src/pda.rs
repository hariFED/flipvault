use crate::ids::*;
use solana_pubkey::Pubkey;

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG_SEED], &PROGRAM_ID)
}
pub fn curve_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CURVE_SEED], &PROGRAM_ID)
}
pub fn treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TREASURY_SEED], &PROGRAM_ID)
}
pub fn vault_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED], &PROGRAM_ID)
}
pub fn registry_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[REGISTRY_SEED], &PROGRAM_ID)
}
/// Per-player box PDA: ["box", owner].
pub fn box_pda(owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BOX_SEED, owner.as_ref()], &PROGRAM_ID)
}
