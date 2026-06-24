//! FlipVault shared SDK: ids, PDAs, instruction builders, and account decoders.
//! WASM-safe (frontend) and native (keeper/indexer) — the single ABI source of truth.
pub mod disc;
pub mod ids;
pub mod ix;
pub mod pda;
pub mod state;

pub use ids::*;
pub use solana_instruction::{AccountMeta, Instruction};
pub use solana_pubkey::Pubkey;

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    #[test]
    fn discriminators_are_distinct_and_sized() {
        let names = ["initialize", "deposit", "withdraw", "commit_round", "settle_round", "recover_round", "sweep_treasury"];
        let mut seen = std::collections::HashSet::new();
        for n in names {
            let d = disc::discriminator(n);
            assert_eq!(d.len(), 8);
            assert!(seen.insert(d), "duplicate discriminator for {n}");
        }
    }

    #[test]
    fn deposit_ix_shape() {
        let user = Pubkey::new_from_array([3u8; 32]);
        let ix = ix::deposit(&user, 0, 0, 1_000_000);
        assert_eq!(ix.program_id, PROGRAM_ID);
        assert_eq!(ix.accounts.len(), 5);
        assert!(ix.accounts[0].is_signer && ix.accounts[0].is_writable); // user
        // discriminator(8) + vault_id(1) + slot(1) + amount(8) = 18 bytes
        assert_eq!(ix.data.len(), 18);
        assert_eq!(&ix.data[..8], &disc::discriminator("deposit"));
    }

    #[test]
    fn config_borsh_roundtrip() {
        let cfg = state::Config {
            treasury_authority: Pubkey::new_from_array([9u8; 32]),
            r_tok: 1_000_000_000,
            k: 1_000_000_000_000_000_000,
            round_secs: 30,
            last_settled_ts: 1_782_313_930,
            fee_bps: 1000,
            min_reserve: 1_000_000,
            phase: state::RoundPhase::Idle,
            round_seed: [7u8; 32],
            commit_slot: 0,
            commit_ts: 0,
            selected_vault: NO_VAULT,
            bump: 254,
            reserve_bump: 253,
            treasury_bump: 252,
        };
        // Simulate an account: 8-byte discriminator + borsh body.
        let mut data = vec![0u8; 8];
        cfg.serialize(&mut data).unwrap();
        let decoded: state::Config = state::decode(&data).unwrap();
        assert_eq!(decoded.k, cfg.k);
        assert_eq!(decoded.phase, state::RoundPhase::Idle);
        assert_eq!(decoded.selected_vault, NO_VAULT);
        assert_eq!(decoded.treasury_authority, cfg.treasury_authority);
    }

    #[test]
    fn vault_borsh_roundtrip() {
        let v = state::Vault {
            vault_id: 3,
            tranches: [
                state::Tranche { asset: state::Asset::Token, amount: 90_909_091, total_shares: 100_000_000 },
                state::Tranche { asset: state::Asset::Sol, amount: 0, total_shares: 0 },
            ],
            bump: 255,
        };
        let mut data = vec![0u8; 8];
        v.serialize(&mut data).unwrap();
        let decoded: state::Vault = state::decode(&data).unwrap();
        assert_eq!(decoded.sol_slot(), Some(1));
        assert_eq!(decoded.tranches[0].amount, 90_909_091);
    }
}
