//! Constant-product bonding-curve math for FlipVault.
//!
//! This module is deliberately free of Anchor/Solana dependencies so it can be
//! unit-tested with plain `cargo test`. Everything is `u128`: the product
//! `k = r_sol * r_tok` overflows `u64` (e.g. 1e11 * 1e11 = 1e22 ≈ 73 bits), so
//! narrowing must only happen at the boundary when crediting real `u64` lamports.
//!
//! Invariants enforced here (see docs/FlipVault-understanding.md §3):
//!   * `k` is the fixed genesis constant; we never re-derive it from drifted reserves.
//!   * `r_sol` always tracks the reserve's spendable lamports: every leg moves it by
//!     exactly the real lamport amount that changes hands.
//!   * Real lamport payout (`sol_out`) rounds DOWN (house-safe) via `ceil_div` on the
//!     dependent reserve; the virtual token credit (`tok_out`) also rounds DOWN.
//!   * Empty inputs are explicit no-ops; degenerate/over-drift states clamp instead of
//!     underflowing.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveError {
    DivByZero,
    Overflow,
}

/// Ceil division `ceil(a / b)` without the `a + b - 1` overflow hazard.
#[inline]
pub fn ceil_div(a: u128, b: u128) -> Result<u128, CurveError> {
    if b == 0 {
        return Err(CurveError::DivByZero);
    }
    Ok(a / b + if a % b != 0 { 1 } else { 0 })
}

/// Outcome of selling virtual tokens into the curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SellOut {
    /// Real lamports paid out of the reserve (rounded DOWN, house-safe).
    pub sol_out: u128,
    pub new_r_sol: u128,
    pub new_r_tok: u128,
}

/// Outcome of buying virtual tokens with real lamports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyOut {
    /// Virtual tokens credited (rounded DOWN, house-safe).
    pub tok_out: u128,
    pub new_r_sol: u128,
    pub new_r_tok: u128,
}

/// Outcome of a full sell-first flip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlipOut {
    /// Lamports moved reserve -> vault (received by the ex-TOKEN tranche).
    pub sol_out: u128,
    /// Virtual tokens credited to the ex-SOL tranche.
    pub tok_out: u128,
    /// Reserve `r_sol` at its lowest point (after the sell leg, before the buy leg).
    /// Callers enforce `min_reserve` against this value.
    pub post_sell_r_sol: u128,
    pub new_r_sol: u128,
    pub new_r_tok: u128,
}

/// Sell `dy` virtual tokens into the curve.
///
/// `r_tok` increases by exactly `dy`; the dependent reserve becomes
/// `ceil_div(k, r_tok + dy)`, so the payout `sol_out = r_sol - new_r_sol` rounds DOWN.
/// If the curve says the sale yields no payout (drift/drain), it is a clean no-op.
pub fn sell(r_sol: u128, r_tok: u128, k: u128, dy: u128) -> Result<SellOut, CurveError> {
    if dy == 0 {
        return Ok(SellOut { sol_out: 0, new_r_sol: r_sol, new_r_tok: r_tok });
    }
    let denom = r_tok.checked_add(dy).ok_or(CurveError::Overflow)?;
    let new_r_sol = ceil_div(k, denom)?; // house-safe (ceil) dependent reserve
    if new_r_sol >= r_sol {
        // Degenerate: selling yields <= 0 lamports. No payout, no curve change.
        return Ok(SellOut { sol_out: 0, new_r_sol: r_sol, new_r_tok: r_tok });
    }
    Ok(SellOut {
        sol_out: r_sol - new_r_sol,
        new_r_sol,
        new_r_tok: denom,
    })
}

/// Buy virtual tokens with `dx` real lamports.
///
/// `r_sol` increases by exactly `dx` (the lamports that moved in); the dependent
/// virtual reserve becomes `floor(k / (r_sol + dx))`, clamped so `tok_out >= 0`.
pub fn buy(r_sol: u128, r_tok: u128, k: u128, dx: u128) -> Result<BuyOut, CurveError> {
    if dx == 0 {
        return Ok(BuyOut { tok_out: 0, new_r_sol: r_sol, new_r_tok: r_tok });
    }
    let denom = r_sol.checked_add(dx).ok_or(CurveError::Overflow)?;
    let raw = denom_div(k, denom)?; // floor(k / denom)
    let new_r_tok = if raw > r_tok { r_tok } else { raw };
    Ok(BuyOut {
        tok_out: r_tok - new_r_tok,
        new_r_sol: denom,
        new_r_tok,
    })
}

#[inline]
fn denom_div(a: u128, b: u128) -> Result<u128, CurveError> {
    if b == 0 {
        return Err(CurveError::DivByZero);
    }
    Ok(a / b)
}

/// Sequential sell-first flip on a vault with SOL-tranche `s` lamports and
/// TOKEN-tranche `t` virtual tokens. Returns the lamports/tokens that change
/// denomination plus the resulting curve state.
pub fn flip(r_sol: u128, r_tok: u128, k: u128, s: u128, t: u128) -> Result<FlipOut, CurveError> {
    let sold = sell(r_sol, r_tok, k, t)?;
    let bought = buy(sold.new_r_sol, sold.new_r_tok, k, s)?;
    Ok(FlipOut {
        sol_out: sold.sol_out,
        tok_out: bought.tok_out,
        post_sell_r_sol: sold.new_r_sol,
        new_r_sol: bought.new_r_sol,
        new_r_tok: bought.new_r_tok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic splitmix64 PRNG (no external crate, reproducible).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    const TOK0: u128 = 100_000_000_000; // 1e11
    const SOL0: u128 = 100_000_000_000; // 100 SOL in lamports
    const K: u128 = SOL0 * TOK0; // 1e22, ~73 bits — overflows u64 on purpose

    #[test]
    fn ceil_div_basic() {
        assert_eq!(ceil_div(10, 3).unwrap(), 4);
        assert_eq!(ceil_div(9, 3).unwrap(), 3);
        assert_eq!(ceil_div(0, 5).unwrap(), 0);
        assert_eq!(ceil_div(u128::MAX, 1).unwrap(), u128::MAX); // no overflow
        assert_eq!(ceil_div(1, 0), Err(CurveError::DivByZero));
    }

    #[test]
    fn empty_inputs_are_noops() {
        let s = sell(SOL0, TOK0, K, 0).unwrap();
        assert_eq!(s, SellOut { sol_out: 0, new_r_sol: SOL0, new_r_tok: TOK0 });
        let b = buy(SOL0, TOK0, K, 0).unwrap();
        assert_eq!(b, BuyOut { tok_out: 0, new_r_sol: SOL0, new_r_tok: TOK0 });
        // A flip of a fully empty vault touches nothing.
        let f = flip(SOL0, TOK0, K, 0, 0).unwrap();
        assert_eq!(f.sol_out, 0);
        assert_eq!(f.tok_out, 0);
        assert_eq!(f.new_r_sol, SOL0);
        assert_eq!(f.new_r_tok, TOK0);
    }

    #[test]
    fn sell_payout_is_house_safe() {
        // After a sell, the retained reserve * denom must be >= k (we never overpay).
        let dy = 12_345_678_900u128;
        let out = sell(SOL0, TOK0, K, dy).unwrap();
        let denom = TOK0 + dy;
        assert!(out.new_r_sol * denom >= K, "reserve must retain at least k/denom");
        // Payout never exceeds the reserve.
        assert!(out.sol_out < SOL0);
        assert_eq!(out.new_r_sol, SOL0 - out.sol_out);
    }

    #[test]
    fn buy_then_sell_roundtrip_does_not_profit_user() {
        // Buying dx then selling the tokens back must not return more than dx (house-safe).
        let dx = 5_000_000_000u128; // 5 SOL
        let b = buy(SOL0, TOK0, K, dx).unwrap();
        let s = sell(b.new_r_sol, b.new_r_tok, K, b.tok_out).unwrap();
        assert!(s.sol_out <= dx, "round trip must not profit the user: {} > {}", s.sol_out, dx);
    }

    #[test]
    fn large_values_do_not_overflow() {
        // ~1e9 SOL reserve, big token side: k ~ 1e29, far past u64 but within u128.
        let r_sol = 1_000_000_000_000_000_000u128; // 1e18
        let r_tok = 100_000_000_000u128; // 1e11
        let k = r_sol * r_tok; // 1e29
        let f = flip(r_sol, r_tok, k, 1_000_000_000, 50_000_000_000).unwrap();
        assert!(f.new_r_sol > 0);
    }

    #[test]
    fn flip_conserves_q_over_many_rounds() {
        // Q = reserve lamports (r_sol) + sum of SOL-tranche lamports across vaults.
        // Flips must leave Q EXACTLY unchanged, for any rounding (double-entry).
        let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
        let mut r_sol = SOL0;
        let mut r_tok = TOK0;

        // 4 vaults, each two tranches: (is_sol, amount). Genesis: slot0 SOL, slot1 TOKEN.
        let mut vaults: [[(bool, u128); 2]; 4] = [[(true, 0), (false, 0)]; 4];

        // Seed deposits into each vault's SOL tranche (raises Q, leaves the curve alone).
        for v in 0..4 {
            let dep = 1_000_000_000u128 + rng.below(50_000_000_000) as u128;
            let slot = if vaults[v][0].0 { 0 } else { 1 };
            vaults[v][slot].1 += dep;
        }

        let q = |r_sol: u128, vaults: &[[(bool, u128); 2]; 4]| -> u128 {
            let mut acc = r_sol;
            for v in vaults {
                for tr in v {
                    if tr.0 {
                        acc += tr.1;
                    }
                }
            }
            acc
        };
        let q_start = q(r_sol, &vaults);

        for _ in 0..100_000 {
            let v = rng.below(4) as usize;
            let sol_slot = if vaults[v][0].0 { 0 } else { 1 };
            let tok_slot = 1 - sol_slot;
            let s = vaults[v][sol_slot].1;
            let t = vaults[v][tok_slot].1;

            let f = flip(r_sol, r_tok, K, s, t).unwrap();
            r_sol = f.new_r_sol;
            r_tok = f.new_r_tok;
            // ex-TOKEN tranche becomes SOL holding sol_out; ex-SOL becomes TOKEN holding tok_out.
            vaults[v][tok_slot] = (true, f.sol_out);
            vaults[v][sol_slot] = (false, f.tok_out);

            assert_eq!(q(r_sol, &vaults), q_start, "Q must be conserved by flips");
            assert!(r_sol > 0, "reserve must never reach zero");
        }

        // Curve product stays within a tiny tolerance of k (it is NOT exactly constant).
        let prod = r_sol * r_tok;
        let drift = if prod > K { prod - K } else { K - prod };
        assert!(drift <= r_sol + r_tok + 2, "k drift must stay bounded: {}", drift);
    }
}
