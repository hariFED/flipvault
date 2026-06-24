//! Instruction builders. Each assembles the Anchor 8-byte discriminator + borsh-serialized
//! args, with AccountMetas in the exact order/flags of the on-chain `#[derive(Accounts)]`.
use crate::{disc::discriminator, ids::*, pda::*};
use borsh::BorshSerialize;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

fn build(name: &str, args: &impl BorshSerialize, accounts: Vec<AccountMeta>) -> Instruction {
    let mut data = discriminator(name).to_vec();
    args.serialize(&mut data).expect("borsh serialize");
    Instruction { program_id: PROGRAM_ID, accounts, data }
}

#[derive(BorshSerialize)]
struct InitializeArgs {
    seed_sol: u64,
    init_r_tok: u64,
    round_secs: i64,
    fee_bps: u16,
    min_reserve: u64,
    treasury_authority: Pubkey,
}

#[allow(clippy::too_many_arguments)]
pub fn initialize(
    founder: &Pubkey,
    seed_sol: u64,
    init_r_tok: u64,
    round_secs: i64,
    fee_bps: u16,
    min_reserve: u64,
    treasury_authority: Pubkey,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*founder, true),
        AccountMeta::new(config_pda().0, false),
        AccountMeta::new(reserve_pda().0, false),
        AccountMeta::new(treasury_pda().0, false),
        AccountMeta::new(vault_pda(0).0, false),
        AccountMeta::new(vault_pda(1).0, false),
        AccountMeta::new(vault_pda(2).0, false),
        AccountMeta::new(vault_pda(3).0, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ];
    build(
        "initialize",
        &InitializeArgs { seed_sol, init_r_tok, round_secs, fee_bps, min_reserve, treasury_authority },
        accounts,
    )
}

#[derive(BorshSerialize)]
struct DepositArgs { vault_id: u8, slot: u8, amount: u64 }

pub fn deposit(user: &Pubkey, vault_id: u8, slot: u8, amount: u64) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*user, true),
        AccountMeta::new_readonly(config_pda().0, false),
        AccountMeta::new(vault_pda(vault_id).0, false),
        AccountMeta::new(position_pda(user, vault_id, slot).0, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ];
    build("deposit", &DepositArgs { vault_id, slot, amount }, accounts)
}

#[derive(BorshSerialize)]
struct WithdrawArgs { vault_id: u8, slot: u8, shares: u64 }

pub fn withdraw(user: &Pubkey, vault_id: u8, slot: u8, shares: u64) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*user, true),
        AccountMeta::new_readonly(config_pda().0, false),
        AccountMeta::new(vault_pda(vault_id).0, false),
        AccountMeta::new(position_pda(user, vault_id, slot).0, false),
        AccountMeta::new(treasury_pda().0, false),
    ];
    build("withdraw", &WithdrawArgs { vault_id, slot, shares }, accounts)
}

#[derive(BorshSerialize)]
struct CommitArgs { force: [u8; 32] }

/// `orao_treasury` is read from ORAO's NetworkState.config.treasury (caller fetches it).
pub fn commit_round(keeper: &Pubkey, force: [u8; 32], orao_treasury: &Pubkey) -> Instruction {
    let accounts = vec![
        AccountMeta::new(*keeper, true),
        AccountMeta::new(config_pda().0, false),
        AccountMeta::new(orao_randomness(&force).0, false),
        AccountMeta::new(*orao_treasury, false),
        AccountMeta::new(orao_network_state().0, false),
        AccountMeta::new_readonly(ORAO_VRF_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ];
    build("commit_round", &CommitArgs { force }, accounts)
}

#[derive(BorshSerialize)]
struct NoArgs {}

/// `round_seed` is Config.round_seed stored at commit (caller fetches it).
pub fn settle_round(round_seed: &[u8; 32]) -> Instruction {
    let accounts = vec![
        AccountMeta::new(config_pda().0, false),
        AccountMeta::new(reserve_pda().0, false),
        AccountMeta::new(vault_pda(0).0, false),
        AccountMeta::new(vault_pda(1).0, false),
        AccountMeta::new(vault_pda(2).0, false),
        AccountMeta::new(vault_pda(3).0, false),
        AccountMeta::new_readonly(orao_randomness(round_seed).0, false),
    ];
    build("settle_round", &NoArgs {}, accounts)
}

pub fn recover_round() -> Instruction {
    let accounts = vec![AccountMeta::new(config_pda().0, false)];
    build("recover_round", &NoArgs {}, accounts)
}

#[derive(BorshSerialize)]
struct SweepArgs { amount: u64 }

pub fn sweep_treasury(authority: &Pubkey, recipient: &Pubkey, amount: u64) -> Instruction {
    let accounts = vec![
        AccountMeta::new_readonly(*authority, true),
        AccountMeta::new_readonly(config_pda().0, false),
        AccountMeta::new(treasury_pda().0, false),
        AccountMeta::new(*recipient, false),
    ];
    build("sweep_treasury", &SweepArgs { amount }, accounts)
}
