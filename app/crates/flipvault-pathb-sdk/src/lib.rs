//! FlipVault Path-B shared SDK: ids, PDAs, discriminators, and account decoders.
//! WASM-safe (frontend) and native (keeper/indexer) — the single ABI source of truth for Path-B.
//!
//! NOTE: instruction builders for the MPC queue calls (deposit/withdraw/queue_flip) require the
//! full Arcium account set (mxe/mempool/execpool/computation/comp_def/cluster/fee/clock/sign-pda)
//! and are added alongside the keeper/frontend write-path once a live MXE exists (the deferred
//! piece). The read-path below — PDAs + borsh decoders — works against public on-chain state now.
pub mod disc;
pub mod ids;
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
        let names = [
            "initialize", "seed_curve", "seed_treasury", "register_box", "deposit", "withdraw",
            "queue_flip", "init_curve_comp_def", "init_flip_box_comp_def",
        ];
        let mut seen = std::collections::HashSet::new();
        for n in names {
            let d = disc::discriminator(n);
            assert_eq!(d.len(), 8);
            assert!(seen.insert(d), "duplicate discriminator for {n}");
        }
    }

    #[test]
    fn comp_def_offsets_distinct() {
        let a = disc::comp_def_offset("flip_box");
        let b = disc::comp_def_offset("credit_box");
        let c = disc::comp_def_offset("debit_box");
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn pdas_derive() {
        let owner = Pubkey::new_from_array([3u8; 32]);
        assert_ne!(pda::config_pda().0, pda::curve_pda().0);
        assert_ne!(pda::vault_pda().0, pda::treasury_pda().0);
        // box PDA is owner-specific
        let other = Pubkey::new_from_array([4u8; 32]);
        assert_ne!(pda::box_pda(&owner).0, pda::box_pda(&other).0);
    }

    #[test]
    fn config_borsh_roundtrip() {
        let cfg = state::PathBConfig {
            bump: 254,
            treasury_authority: Pubkey::new_from_array([9u8; 32]),
            k: 1_000_000_000_000_000_000,
            fee_bps: 1000,
            active_box_count: 7,
            curve_version: 3,
            curve_ready: true,
            treasury_ready: false,
        };
        let mut data = vec![0u8; 8]; // 8-byte discriminator
        cfg.serialize(&mut data).unwrap();
        let decoded: state::PathBConfig = state::decode(&data).unwrap();
        assert_eq!(decoded.k, cfg.k);
        assert_eq!(decoded.active_box_count, 7);
        assert_eq!(decoded.curve_ready, true);
        assert_eq!(decoded.treasury_authority, cfg.treasury_authority);
    }

    #[test]
    fn player_box_borsh_roundtrip() {
        let bx = state::PlayerBox {
            bump: 255,
            ct: [[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]],
            nonce: 123456789,
            enc_pubkey: [7u8; 32],
            owner: Pubkey::new_from_array([5u8; 32]),
            index: 2,
            pending: true,
            curve_version_at_queue: 3,
            pending_withdraw: 500_000_000,
        };
        let mut data = vec![0u8; 8];
        bx.serialize(&mut data).unwrap();
        let decoded: state::PlayerBox = state::decode(&data).unwrap();
        assert_eq!(decoded.ct[2], [3u8; 32]);
        assert_eq!(decoded.pending, true);
        assert_eq!(decoded.pending_withdraw, 500_000_000);
        assert_eq!(decoded.owner, bx.owner);
    }
}
