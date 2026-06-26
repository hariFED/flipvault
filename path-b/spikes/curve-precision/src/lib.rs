//! FlipVault Path-B — Milestone 0a precision spike.
//!
//! GOAL: prove the constant-product flip math can run inside an Arcis MPC circuit with
//! **exactly** the same numeric result as the deployed transparent on-chain curve — i.e. that
//! Path-B inherits Path-A's precision rather than introducing fixed-point drift (risk #5).
//!
//! WHY THIS IS A REAL PROOF (and not a tautology): the Arcis language (per docs.arcium.com)
//! supports `u8..u128`, signed ints, and **truncating integer `/` and `%`** on regular integer
//! types. It does NOT need `f64` (which Arcis only emulates as fixed-point, 52 frac bits, range
//! [-2^75, 2^75), silently clamped — the precision trap). So the curve ports as INTEGER math.
//! This module contains:
//!   * `transparent` — the deployed reference (verbatim algorithm from
//!     flipvault/programs/flipvault/src/curve.rs).
//!   * `arcis_model` — the SAME math rewritten under the Arcis subset constraints:
//!       - u128 only, no f64;
//!       - division guarded against a zero divisor (Arcis evaluates *both* branches of an
//!         `if/else`, so a not-taken branch's division still runs — docs say guard it);
//!       - branch selection modeled as oblivious `select` (Arcis compiles `if/else` to
//!         "evaluate both, select" — numerically identical to a normal `if`).
//! The differential tests assert `arcis_model == transparent` bit-for-bit across millions of
//! random inputs + edge cases. If the constrained rewrite were wrong, the test fails.
//!
//! The matching Arcis circuit source lives in `path-b/encrypted-ixs/src/lib.rs`; it mirrors
//! `arcis_model` line-for-line. The empirical step (compile with `arcium build`, run on
//! Arcium devnet, diff the decrypted output against `transparent`) is M0a's exit gate; this
//! crate is the design-level proof that runs today with zero external toolchain.

#![allow(clippy::needless_range_loop)]

// ============================================================================
// transparent — the deployed reference curve (algorithm identical to curve.rs)
// ============================================================================
pub mod transparent {
    /// `ceil(a / b)` without the `a + b - 1` overflow hazard.
    #[inline]
    pub fn ceil_div(a: u128, b: u128) -> u128 {
        a / b + if a % b != 0 { 1 } else { 0 }
    }

    /// Sell `dy` virtual tokens into the curve. Payout rounds DOWN (house-safe).
    /// Returns (sol_out, new_r_sol, new_r_tok).
    pub fn sell(r_sol: u128, r_tok: u128, k: u128, dy: u128) -> (u128, u128, u128) {
        if dy == 0 {
            return (0, r_sol, r_tok);
        }
        let denom = r_tok + dy;
        let new_r_sol = ceil_div(k, denom);
        if new_r_sol >= r_sol {
            return (0, r_sol, r_tok); // degenerate: no payout, no change
        }
        (r_sol - new_r_sol, new_r_sol, denom)
    }

    /// Buy virtual tokens with `dx` real lamports. Token credit rounds DOWN (house-safe).
    /// Returns (tok_out, new_r_sol, new_r_tok).
    pub fn buy(r_sol: u128, r_tok: u128, k: u128, dx: u128) -> (u128, u128, u128) {
        if dx == 0 {
            return (0, r_sol, r_tok);
        }
        let denom = r_sol + dx;
        let raw = k / denom; // floor
        let new_r_tok = if raw > r_tok { r_tok } else { raw };
        (r_tok - new_r_tok, denom, new_r_tok)
    }
}

// ============================================================================
// arcis_model — the SAME math under Arcis-subset constraints (mirrors the circuit)
// ============================================================================
//
// Every construct here maps 1:1 to something `path-b/encrypted-ixs/src/lib.rs` does:
//   `select_u128`/`select_bool` -> Arcis `if cond { a } else { b }` (both branches run)
//   `guarded_div` / `guarded_ceil_div` -> divisor clamped to >=1 before `/` (docs §best-practices)
//   no `Vec`, no `while`, u128 everywhere.
pub mod arcis_model {
    /// Oblivious select: in Arcis this is `if cond { a } else { b }` — both arms are evaluated
    /// in MPC and the condition only picks the result. Numerically identical to a plain `if`.
    #[inline]
    pub fn select_u128(cond: bool, a: u128, b: u128) -> u128 {
        if cond {
            a
        } else {
            b
        }
    }

    /// Divisor guard: Arcis evaluates both branches, so a not-taken division must not divide by
    /// zero. Clamp the divisor to >=1. In every call below the real divisor is already > 0
    /// (reserves never reach zero — proven by the conservation test), so this never alters a
    /// real result; it only makes the not-taken branch safe.
    #[inline]
    fn safe(b: u128) -> u128 {
        if b == 0 {
            1
        } else {
            b
        }
    }

    #[inline]
    pub fn guarded_ceil_div(a: u128, b: u128) -> u128 {
        let d = safe(b);
        a / d + if a % d != 0 { 1 } else { 0 }
    }

    #[inline]
    pub fn guarded_floor_div(a: u128, b: u128) -> u128 {
        a / safe(b)
    }

    /// Sell, written in the constrained subset. Uses oblivious selects instead of early returns
    /// (an MPC circuit has no early `return`; it computes the value then selects).
    pub fn sell(r_sol: u128, r_tok: u128, k: u128, dy: u128) -> (u128, u128, u128) {
        let denom = r_tok + dy; // dy>=0 so denom>=r_tok>0
        let cand_r_sol = guarded_ceil_div(k, denom);
        // "degenerate" = selling yields nothing: dy==0 OR ceil_div didn't lower the reserve.
        let degenerate = (dy == 0) || (cand_r_sol >= r_sol);
        let new_r_sol = select_u128(degenerate, r_sol, cand_r_sol);
        let new_r_tok = select_u128(degenerate, r_tok, denom);
        // r_sol - new_r_sol is safe: when !degenerate, cand_r_sol < r_sol; when degenerate, equal.
        let sol_out = r_sol - new_r_sol;
        (sol_out, new_r_sol, new_r_tok)
    }

    /// Buy, written in the constrained subset.
    pub fn buy(r_sol: u128, r_tok: u128, k: u128, dx: u128) -> (u128, u128, u128) {
        let denom = r_sol + dx; // dx>=0 so denom>=r_sol>0
        let raw = guarded_floor_div(k, denom);
        let raw_le_rtok = raw <= r_tok;
        let new_r_tok_when_buy = select_u128(raw_le_rtok, raw, r_tok); // min(raw, r_tok)
        // dx==0 => no change at all.
        let buying = dx != 0;
        let new_r_sol = select_u128(buying, denom, r_sol);
        let new_r_tok = select_u128(buying, new_r_tok_when_buy, r_tok);
        let tok_out = r_tok - new_r_tok;
        (tok_out, new_r_sol, new_r_tok)
    }
}

// ============================================================================
// The confidential instruction: per-box flip (the Path-B perpetual primitive)
// ============================================================================
//
// A box holds value in exactly ONE denomination at a time: SOL (waiting) or PERP (flipped).
// When VRF (public, off-circuit) selects this box, the circuit flips it to the other side at
// the current shared-curve price, taking a fee on the SOL leg. This mirrors `flip_box` in
// path-b/encrypted-ixs/src/lib.rs exactly.

pub const FEE_DENOM: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Curve {
    pub r_sol: u128, // reserve lamports (also tracks real reserve balance)
    pub r_tok: u128, // virtual token reserve
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxState {
    pub sol: u128,        // lamports held while waiting (0 while in perp)
    pub perp: u128,       // virtual tokens held while flipped (0 while in sol)
    pub in_perp: bool,    // true => currently holding perp tokens
    pub cost_basis: u128, // lamports spent to acquire the current perp position (for P&L)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlipReceipt {
    pub fee_lamports: u128,   // taken on the SOL leg, accrues to treasury
    pub realized_pnl: i128,   // only meaningful on a perp->sol flip (sol received - cost_basis)
}

/// One confidential flip of `bx` against shared `curve`, fee `fee_bps`, public constant `k`.
/// Returns the new curve, new box, and a receipt. Written in the Arcis subset (oblivious).
pub fn flip_box(
    curve: Curve,
    bx: BoxState,
    k: u128,
    fee_bps: u128,
) -> (Curve, BoxState, FlipReceipt) {
    use arcis_model::{buy, select_u128, sell};

    // --- SELL leg (assume box holds perp): perp -> sol, fee on the sol received ---
    let (gross_sol, sell_r_sol, sell_r_tok) = sell(curve.r_sol, curve.r_tok, k, bx.perp);
    let sell_fee = gross_sol * fee_bps / FEE_DENOM;
    let sell_net_sol = gross_sol - sell_fee;

    // --- BUY leg (assume box holds sol): fee on the sol spent, then sol -> perp ---
    let buy_fee = bx.sol * fee_bps / FEE_DENOM;
    let buy_spend = bx.sol - buy_fee;
    let (tok_out, buy_r_sol, buy_r_tok) = buy(curve.r_sol, curve.r_tok, k, buy_spend);

    // --- oblivious select on in_perp ---
    let c = bx.in_perp;
    let new_curve = Curve {
        r_sol: select_u128(c, sell_r_sol, buy_r_sol),
        r_tok: select_u128(c, sell_r_tok, buy_r_tok),
    };
    let new_box = BoxState {
        sol: select_u128(c, sell_net_sol, 0),
        perp: select_u128(c, 0, tok_out),
        in_perp: !c,
        // realize on sell (reset to 0); on buy, cost_basis = lamports actually spent.
        cost_basis: select_u128(c, 0, buy_spend),
    };
    let fee_lamports = select_u128(c, sell_fee, buy_fee);
    // realized P&L only on perp->sol: sol received minus what was paid to enter.
    let realized_pnl = if c {
        sell_net_sol as i128 - bx.cost_basis as i128
    } else {
        0
    };

    (new_curve, new_box, FlipReceipt { fee_lamports, realized_pnl })
}

// ============================================================================
// Custody ops — models of the credit_box / debit_box / pay_treasury circuits.
// Trivial add/sub/compare, but included so the solvency proof below covers the
// WHOLE backend (deposits + withdrawals + flips + sweeps), not just flips.
// ============================================================================

/// Credit a public deposit `amount` into a box's SOL balance.
pub fn credit(bx: BoxState, amount: u128) -> BoxState {
    BoxState { sol: bx.sol + amount, ..bx }
}

/// Debit a public withdraw `amount`. Returns (box, ok) where ok = balance sufficient.
pub fn debit(bx: BoxState, amount: u128) -> (BoxState, bool) {
    let ok = bx.sol >= amount;
    let new_sol = if ok { bx.sol - amount } else { bx.sol };
    (BoxState { sol: new_sol, ..bx }, ok)
}

/// Pay a public `amount` out of the treasury. Returns (treasury, ok).
pub fn pay_treasury(treasury: u128, amount: u128) -> (u128, bool) {
    let ok = treasury >= amount;
    let new_treasury = if ok { treasury - amount } else { treasury };
    (new_treasury, ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic splitmix64 (same generator style as curve.rs — reproducible, no deps).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn range(&mut self, lo: u128, hi: u128) -> u128 {
            // inclusive lo, exclusive hi
            lo + (self.next() as u128) % (hi - lo)
        }
    }

    // ---- the precision claim: arcis_model == transparent, bit-for-bit ----

    #[test]
    fn differential_sell_buy_matches_transparent() {
        let mut rng = Rng(0xF112_0001_u64.wrapping_mul(0x9E37));
        let iters = 2_000_000u64;
        let mut max_sell_diff = 0i128;
        let mut max_buy_diff = 0i128;
        for _ in 0..iters {
            // Realistic lamport-scale ranges: reserves 0.001..1000 SOL-equivalent, k from them.
            let r_sol = rng.range(1_000_000, 1_000_000_000_000); // 1e6 .. 1e12
            let r_tok = rng.range(1_000_000, 1_000_000_000_000);
            let k = r_sol * r_tok; // <= 1e24, ~80 bits, within u128
            let amt = rng.range(0, 2_000_000_000_000); // include 0 and large

            let t_sell = transparent::sell(r_sol, r_tok, k, amt);
            let a_sell = arcis_model::sell(r_sol, r_tok, k, amt);
            assert_eq!(t_sell, a_sell, "sell mismatch r_sol={r_sol} r_tok={r_tok} amt={amt}");
            max_sell_diff = max_sell_diff.max((t_sell.0 as i128 - a_sell.0 as i128).abs());

            let t_buy = transparent::buy(r_sol, r_tok, k, amt);
            let a_buy = arcis_model::buy(r_sol, r_tok, k, amt);
            assert_eq!(t_buy, a_buy, "buy mismatch r_sol={r_sol} r_tok={r_tok} amt={amt}");
            max_buy_diff = max_buy_diff.max((t_buy.0 as i128 - a_buy.0 as i128).abs());
        }
        assert_eq!(max_sell_diff, 0, "sell payout must be EXACT, not approximate");
        assert_eq!(max_buy_diff, 0, "buy credit must be EXACT, not approximate");
    }

    #[test]
    fn edge_cases_match() {
        // Zero amounts, denom==reserve, huge k near u128 headroom.
        let cases: &[(u128, u128, u128)] = &[
            (1_000_000, 1_000_000, 0),
            (1, 1, 0),
            (1_000_000_000_000, 1, 999_999_999_999),
            (1, 1_000_000_000_000, 999_999_999_999),
        ];
        for &(r_sol, r_tok, amt) in cases {
            let k = r_sol * r_tok;
            assert_eq!(transparent::sell(r_sol, r_tok, k, amt), arcis_model::sell(r_sol, r_tok, k, amt));
            assert_eq!(transparent::buy(r_sol, r_tok, k, amt), arcis_model::buy(r_sol, r_tok, k, amt));
        }
        // Large values: ~1e18 reserve * 1e11 = 1e29 (~97 bits) — still u128, no overflow.
        let (r_sol, r_tok) = (1_000_000_000_000_000_000u128, 100_000_000_000u128);
        let k = r_sol * r_tok;
        assert_eq!(
            transparent::sell(r_sol, r_tok, k, 50_000_000_000),
            arcis_model::sell(r_sol, r_tok, k, 50_000_000_000)
        );
    }

    // ---- house-safety carries over to the per-box flip ----

    #[test]
    fn buy_then_sell_does_not_profit_user() {
        // Enter perp with S lamports, immediately flip back: must not receive more than S (before fee).
        let (r_sol, r_tok) = (100_000_000_000u128, 100_000_000_000u128);
        let k = r_sol * r_tok;
        let curve = Curve { r_sol, r_tok };
        let s = 5_000_000_000u128;
        let bx = BoxState { sol: s, perp: 0, in_perp: false, cost_basis: 0 };
        let (c1, b1, _) = flip_box(curve, bx, k, 0); // 0 fee to isolate curve behavior
        assert!(b1.in_perp && b1.perp > 0 && b1.sol == 0);
        let (_c2, b2, _) = flip_box(c1, b1, k, 0);
        assert!(!b2.in_perp);
        assert!(b2.sol <= s, "round-trip must not profit the user: {} > {}", b2.sol, s);
    }

    // ---- the path-B conservation invariant (analog of path-A's 100k-flip test) ----

    #[test]
    fn perpetual_model_conserves_lamports_over_many_flips() {
        // Q = curve.r_sol + Σ(box.sol) + treasury  must be EXACTLY constant across flips.
        // (perp is virtual and not part of Q.) Fees move lamports box->treasury, never destroy them.
        let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
        let r_sol0 = 100_000_000_000u128;
        let r_tok0 = 100_000_000_000u128;
        let k = r_sol0 * r_tok0;
        let mut curve = Curve { r_sol: r_sol0, r_tok: r_tok0 };
        let fee_bps = 1_000u128; // 10%

        const N: usize = 64;
        let mut boxes = [BoxState { sol: 0, perp: 0, in_perp: false, cost_basis: 0 }; N];
        // Seed each box with a SOL deposit (raises Q; curve untouched).
        for b in boxes.iter_mut() {
            b.sol = rng.range(100_000_000, 20_000_000_000); // 0.1..20 SOL
        }
        let mut treasury: u128 = 0;

        let q = |curve: &Curve, boxes: &[BoxState; N], treasury: u128| -> u128 {
            let mut acc = curve.r_sol + treasury;
            for b in boxes {
                acc += b.sol;
            }
            acc
        };
        let q_start = q(&curve, &boxes, treasury);

        for _ in 0..200_000 {
            let i = (rng.next() as usize) % N;
            let (nc, nb, receipt) = flip_box(curve, boxes[i], k, fee_bps);
            curve = nc;
            boxes[i] = nb;
            treasury += receipt.fee_lamports;
            assert_eq!(q(&curve, &boxes, treasury), q_start, "Q (lamports) must be conserved");
            assert!(curve.r_sol > 0, "reserve must never reach zero");
        }
    }

    #[test]
    fn box_holds_single_denomination_invariant() {
        // After any flip a box is all-SOL or all-PERP, never both.
        let (r_sol, r_tok) = (50_000_000_000u128, 50_000_000_000u128);
        let k = r_sol * r_tok;
        let mut curve = Curve { r_sol, r_tok };
        let mut bx = BoxState { sol: 3_000_000_000, perp: 0, in_perp: false, cost_basis: 0 };
        for _ in 0..50 {
            let (nc, nb, _) = flip_box(curve, bx, k, 1_000);
            curve = nc;
            bx = nb;
            assert!(
                (bx.in_perp && bx.sol == 0 && bx.perp > 0)
                    || (!bx.in_perp && bx.perp == 0),
                "box must hold exactly one denomination"
            );
        }
    }

    #[test]
    fn full_system_conserves_solvency() {
        // The whole-backend invariant: the real lamports held in the custody vault always
        // equal exactly what is owed —  vault == Σ(box.sol) + curve.r_sol + treasury — across
        // an arbitrary interleaving of deposits, withdrawals, flips, and treasury sweeps.
        // (perp tokens are virtual and not backed by lamports, so they are not in the sum.)
        // This proves no operation in the Path-B economic model creates or destroys a lamport.
        let mut rng = Rng(0x501F_0B07_DEC0_DE01);
        let r_sol0 = 100_000_000_000u128;
        let r_tok0 = 100_000_000_000u128;
        let k = r_sol0 * r_tok0;
        let mut curve = Curve { r_sol: r_sol0, r_tok: r_tok0 };
        let fee_bps = 1_000u128; // 10%

        const N: usize = 32;
        let mut boxes = [BoxState { sol: 0, perp: 0, in_perp: false, cost_basis: 0 }; N];
        let mut treasury: u128 = 0;
        // Genesis: the curve reserve is backed by real lamports the founder seeded into the vault.
        let mut vault: u128 = r_sol0;

        let owed = |boxes: &[BoxState; N], curve: &Curve, treasury: u128| -> u128 {
            let mut acc = curve.r_sol + treasury;
            for b in boxes {
                acc += b.sol;
            }
            acc
        };
        assert_eq!(vault, owed(&boxes, &curve, treasury), "genesis must be solvent");

        for _ in 0..200_000 {
            let i = (rng.next() as usize) % N;
            match rng.next() % 4 {
                0 => {
                    // deposit — only allowed while waiting on the SOL side
                    if !boxes[i].in_perp {
                        let amt = rng.range(100_000, 5_000_000_000);
                        boxes[i] = credit(boxes[i], amt);
                        vault += amt;
                    }
                }
                1 => {
                    // withdraw — only while on the SOL side; vault pays only if `ok`
                    if !boxes[i].in_perp {
                        let amt = rng.range(100_000, 5_000_000_000);
                        let (nb, ok) = debit(boxes[i], amt);
                        boxes[i] = nb;
                        if ok {
                            vault -= amt;
                        }
                    }
                }
                2 => {
                    // flip — moves encrypted balances only; NO real lamport (vault unchanged)
                    let (nc, nb, receipt) = flip_box(curve, boxes[i], k, fee_bps);
                    curve = nc;
                    boxes[i] = nb;
                    treasury += receipt.fee_lamports;
                }
                3 => {
                    // treasury sweep — vault pays out only if `ok`
                    let amt = rng.range(0, 2_000_000_000);
                    let (nt, ok) = pay_treasury(treasury, amt);
                    treasury = nt;
                    if ok {
                        vault -= amt;
                    }
                }
                _ => unreachable!(),
            }
            assert_eq!(
                vault,
                owed(&boxes, &curve, treasury),
                "vault must always equal Σ box.sol + reserve + treasury"
            );
            assert!(curve.r_sol > 0, "reserve must never reach zero");
        }
    }
}
