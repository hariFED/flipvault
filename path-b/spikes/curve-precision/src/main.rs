//! M0a differential sweep — prints a precision report comparing the Arcis-subset model against
//! the transparent reference curve. This is the human-readable artifact behind the unit tests.
//!
//! Run (in the path-A `dev` container, or any Rust toolchain):
//!     cargo run --release --bin sweep
//!     cargo run --release --bin sweep -- 50000000     # custom iteration count

use curve_precision::{arcis_model, flip_box, transparent, BoxState, Curve};

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
        lo + (self.next() as u128) % (hi - lo)
    }
}

fn main() {
    let iters: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000_000);

    println!("FlipVault Path-B · M0a precision sweep");
    println!("  transparent curve  vs  Arcis-subset model (u128 integer math)");
    println!("  iterations: {iters}");

    let mut rng = Rng(0x5EED_F112_0A11_CE00);
    let mut mismatches: u64 = 0;
    let mut max_diff: i128 = 0;
    let mut checked: u64 = 0;

    for _ in 0..iters {
        let r_sol = rng.range(1_000_000, 1_000_000_000_000);
        let r_tok = rng.range(1_000_000, 1_000_000_000_000);
        let k = r_sol * r_tok;
        let amt = rng.range(0, 2_000_000_000_000);

        let ts = transparent::sell(r_sol, r_tok, k, amt);
        let asx = arcis_model::sell(r_sol, r_tok, k, amt);
        if ts != asx {
            mismatches += 1;
            max_diff = max_diff.max((ts.0 as i128 - asx.0 as i128).abs());
        }
        let tb = transparent::buy(r_sol, r_tok, k, amt);
        let ab = arcis_model::buy(r_sol, r_tok, k, amt);
        if tb != ab {
            mismatches += 1;
            max_diff = max_diff.max((tb.0 as i128 - ab.0 as i128).abs());
        }
        checked += 2;
    }

    // A representative end-to-end flip round-trip for the report.
    let curve = Curve { r_sol: 100_000_000_000, r_tok: 100_000_000_000 };
    let k = curve.r_sol * curve.r_tok;
    let bx = BoxState { sol: 5_000_000_000, perp: 0, in_perp: false, cost_basis: 0 };
    let (c1, b1, r1) = flip_box(curve, bx, k, 1_000);
    let (_c2, b2, r2) = flip_box(c1, b1, k, 1_000);

    println!("\nresults");
    println!("  comparisons : {checked}");
    println!("  mismatches  : {mismatches}");
    println!("  max |diff|  : {max_diff} lamports");
    println!(
        "  verdict     : {}",
        if mismatches == 0 {
            "EXACT — Arcis-subset model reproduces the transparent curve bit-for-bit"
        } else {
            "MISMATCH — investigate the constrained rewrite"
        }
    );

    println!("\nexample per-box flip (deposit 5 SOL, 10% fee):");
    println!("  enter perp : got {} perp tokens, fee {} lamports, cost_basis {} lamports",
        b1.perp, r1.fee_lamports, b1.cost_basis);
    println!("  exit  perp : got {} lamports back, fee {} lamports, realized P&L {} lamports",
        b2.sol, r2.fee_lamports, r2.realized_pnl);

    if mismatches != 0 {
        std::process::exit(1);
    }
}
