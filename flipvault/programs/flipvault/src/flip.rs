//! Core flip execution. Sells a vault's TOKEN tranche into the shared curve, buys with its
//! SOL tranche, swaps the tranche asset flags. Conserves Q by construction: every lamport
//! that leaves the reserve enters the vault (and vice-versa) as the same integer.

use crate::curve;
use crate::error::FlipError;
use crate::state::*;
use crate::util::{move_lamports_out, rent_floor};
use anchor_lang::prelude::*;

/// Outcome of attempting a flip on the selected vault.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlipResult {
    /// Curve moved, lamports shuttled, flags swapped.
    Flipped,
    /// Skipped: the sell leg would drive the reserve below `min_reserve` (round still advances).
    SkippedReserveFloor,
    /// Skipped: both tranches empty — flags swapped, nothing to move.
    SkippedEmpty,
}

/// Execute the sell-first flip on `vault`.
///
/// `reserve_ai` / `vault_ai` are the lamport-holding PDAs (cloned `AccountInfo`s so the
/// caller can still hold `&mut Config` / `&mut Vault` to the deserialized data). The curve's
/// `r_sol` is derived here from the reserve's spendable lamports; only `r_tok` is persisted
/// in `config`.
pub fn execute_flip<'info>(
    config: &mut Config,
    vault: &mut Vault,
    reserve_ai: &AccountInfo<'info>,
    vault_ai: &AccountInfo<'info>,
) -> Result<FlipResult> {
    let reserve_floor = rent_floor(reserve_ai.data_len())?;
    let vault_floor = rent_floor(vault_ai.data_len())?;

    let sol_slot = vault.sol_slot().ok_or(FlipError::CurveMath)?;
    let tok_slot = 1 - sol_slot;
    let s_u64 = vault.tranches[sol_slot].amount;
    let t_u64 = vault.tranches[tok_slot].amount;

    if s_u64 == 0 && t_u64 == 0 {
        // Roles still alternate; nothing moves.
        swap_flags(vault, sol_slot, tok_slot, 0, 0);
        return Ok(FlipResult::SkippedEmpty);
    }

    // Derived spendable reserve == curve r_sol.
    let r_sol = (reserve_ai.lamports())
        .checked_sub(reserve_floor)
        .ok_or(FlipError::ReserveFloor)? as u128;

    let f = curve::flip(r_sol, config.r_tok, config.k, s_u64 as u128, t_u64 as u128)
        .map_err(FlipError::from)?;

    // Guard the post-sell reserve low point against the configured floor.
    if f.post_sell_r_sol < config.min_reserve as u128 {
        return Ok(FlipResult::SkippedReserveFloor);
    }

    let sol_out = u64::try_from(f.sol_out).map_err(|_| error!(FlipError::Overflow))?;
    let tok_out = u64::try_from(f.tok_out).map_err(|_| error!(FlipError::Overflow))?;

    // Sell-first real lamport movements: reserve -> vault (sol_out), then vault -> reserve (s).
    if sol_out > 0 {
        move_lamports_out(reserve_ai, vault_ai, sol_out, reserve_floor)?;
    }
    if s_u64 > 0 {
        move_lamports_out(vault_ai, reserve_ai, s_u64, vault_floor)?;
    }

    // r_sol is tracked implicitly by the reserve's lamports; persist only the virtual reserve.
    config.r_tok = f.new_r_tok;

    swap_flags(vault, sol_slot, tok_slot, sol_out, tok_out);
    Ok(FlipResult::Flipped)
}

/// ex-TOKEN slot becomes SOL holding `sol_out`; ex-SOL slot becomes TOKEN holding `tok_out`.
/// Each slot's `total_shares` is intentionally left untouched.
fn swap_flags(vault: &mut Vault, sol_slot: usize, tok_slot: usize, sol_out: u64, tok_out: u64) {
    vault.tranches[tok_slot].asset = Asset::Sol;
    vault.tranches[tok_slot].amount = sol_out;
    vault.tranches[sol_slot].asset = Asset::Token;
    vault.tranches[sol_slot].amount = tok_out;
}
