//! FlipVault Path-B — the confidential flip circuit (Arcis).
//!
//! This is the MPC-side logic: it runs on secret-shared data inside the Arcium cluster, so no
//! node and no observer sees a box's balance, position, P&L, the curve price, or the treasury.
//!
//! It mirrors `path-b/spikes/curve-precision/src/lib.rs::arcis_model` LINE-FOR-LINE. That spike
//! proves (20M random inputs, 0 mismatches) this integer math equals the deployed transparent
//! curve EXACTLY. The empirical M0a exit gate is: `arcium build` this, run it on devnet, decrypt
//! the output, and confirm it still matches the transparent curve.
//!
//! Numeric model (per docs.arcium.com/developers/arcis): everything is `u128` with truncating
//! integer `/` and `%`. NO `f64` — Arcis only emulates floats as fixed-point (52 frac bits,
//! range [-2^75, 2^75), silently clamped), which would introduce drift. Integers are exact.
//!
//! Confidentiality split:
//!   * `Enc<Mxe, Curve>`     — shared bonding-curve reserves; only the cluster can decrypt.
//!   * `Enc<Mxe, u128>`      — treasury (accrued fees); encrypted so fee size can't leak amounts.
//!   * `Enc<Shared, BoxState>` — per-box state; the owning player (and the MXE) can decrypt.
//!   * `k`, `fee_bps`        — PUBLIC scalars (k is derivable; fee_bps is public config).
//!
//! NOTE: the Cargo.toml + arcis dependency version for this crate come from the `arcium init`
//! scaffold (source of truth for the pinned toolchain) — this file is dropped into that crate.
//! If `arcium build` flags an API detail (e.g. tuple-return shape, `owner` accessor), fix it
//! here; the arithmetic is settled.

use arcis::*;

#[encrypted]
mod circuits {
    use arcis::*;

    /// Shared bonding-curve state (Enc<Mxe>). `r_sol` tracks reserve lamports; `r_tok` the
    /// virtual token reserve. The product constant `k` is passed in as a public arg.
    pub struct Curve {
        pub r_sol: u128,
        pub r_tok: u128,
    }

    /// Per-box state (Enc<Shared>). A box holds value in exactly ONE denomination at a time.
    pub struct BoxState {
        pub sol: u128,        // lamports held while waiting (0 while in perp)
        pub perp: u128,       // virtual tokens held while flipped (0 while in sol)
        pub in_perp: bool,    // true => currently holding perp
        pub cost_basis: u128, // lamports spent to enter the current perp position
    }

    // --- Arcis-subset helpers (guarded division; Arcis evaluates BOTH branches of if/else,
    //     so a not-taken branch must never divide by zero — clamp the divisor to >= 1). ---

    fn safe(b: u128) -> u128 {
        if b == 0 {
            1
        } else {
            b
        }
    }

    fn guarded_ceil_div(a: u128, b: u128) -> u128 {
        let d = safe(b);
        a / d + if a % d != 0 { 1 } else { 0 }
    }

    fn guarded_floor_div(a: u128, b: u128) -> u128 {
        a / safe(b)
    }

    /// Sell `dy` perp into the curve -> (sol_out, new_r_sol, new_r_tok). House-safe (rounds down).
    fn sell(r_sol: u128, r_tok: u128, k: u128, dy: u128) -> (u128, u128, u128) {
        let denom = r_tok + dy;
        let cand_r_sol = guarded_ceil_div(k, denom);
        let degenerate = (dy == 0) || (cand_r_sol >= r_sol);
        let new_r_sol = if degenerate { r_sol } else { cand_r_sol };
        let new_r_tok = if degenerate { r_tok } else { denom };
        let sol_out = r_sol - new_r_sol;
        (sol_out, new_r_sol, new_r_tok)
    }

    /// Buy perp with `dx` lamports -> (tok_out, new_r_sol, new_r_tok). House-safe (rounds down).
    fn buy(r_sol: u128, r_tok: u128, k: u128, dx: u128) -> (u128, u128, u128) {
        let denom = r_sol + dx;
        let raw = guarded_floor_div(k, denom);
        let new_r_tok_when_buy = if raw <= r_tok { raw } else { r_tok };
        let buying = dx != 0;
        let new_r_sol = if buying { denom } else { r_sol };
        let new_r_tok = if buying { new_r_tok_when_buy } else { r_tok };
        let tok_out = r_tok - new_r_tok;
        (tok_out, new_r_sol, new_r_tok)
    }

    /// The confidential per-box flip. VRF selection happens publicly OFF-circuit; this only
    /// applies the flip to the already-chosen box. Fee is taken on the SOL leg and folded into
    /// the encrypted treasury so its size never leaks.
    #[instruction]
    pub fn flip_box(
        curve_ctxt: Enc<Mxe, Curve>,
        treasury_ctxt: Enc<Mxe, u128>,
        box_ctxt: Enc<Shared, BoxState>,
        k: u128,
        fee_bps: u128,
    ) -> (Enc<Mxe, Curve>, Enc<Mxe, u128>, Enc<Shared, BoxState>) {
        let curve = curve_ctxt.to_arcis();
        let treasury = treasury_ctxt.to_arcis();
        let bx = box_ctxt.to_arcis();

        let fee_denom: u128 = 10_000;

        // SELL leg (box holds perp): perp -> sol, fee on sol received.
        let (gross_sol, sell_r_sol, sell_r_tok) = sell(curve.r_sol, curve.r_tok, k, bx.perp);
        let sell_fee = gross_sol * fee_bps / fee_denom;
        let sell_net_sol = gross_sol - sell_fee;

        // BUY leg (box holds sol): fee on sol spent, then sol -> perp.
        let buy_fee = bx.sol * fee_bps / fee_denom;
        let buy_spend = bx.sol - buy_fee;
        let (tok_out, buy_r_sol, buy_r_tok) = buy(curve.r_sol, curve.r_tok, k, buy_spend);

        // Oblivious select on the secret `in_perp` flag (both legs already computed above).
        let c = bx.in_perp;
        let new_curve = Curve {
            r_sol: if c { sell_r_sol } else { buy_r_sol },
            r_tok: if c { sell_r_tok } else { buy_r_tok },
        };
        let new_box = BoxState {
            sol: if c { sell_net_sol } else { 0 },
            perp: if c { 0 } else { tok_out },
            in_perp: !c,
            cost_basis: if c { 0 } else { buy_spend },
        };
        let fee = if c { sell_fee } else { buy_fee };
        let new_treasury = treasury + fee;

        (
            curve_ctxt.owner.from_arcis(new_curve),
            treasury_ctxt.owner.from_arcis(new_treasury),
            box_ctxt.owner.from_arcis(new_box),
        )
    }

    // ========================================================================
    // Custody circuits — because internal balances are ENCRYPTED, a deposit /
    // withdraw cannot add/subtract a plaintext amount to a ciphertext on-chain.
    // It must happen inside the MXE. These are tiny (add/sub/compare) and reuse
    // the same queue->callback scaffolding as flip_box. The `amount` is PUBLIC
    // (the deposit/withdraw amount is visible at the custody-vault boundary in
    // v1), so it is a plaintext circuit argument.
    // ========================================================================

    /// Credit a public deposit `amount` into a box's encrypted SOL balance.
    /// Only queued on-chain when the box is on the SOL side (!in_perp) and not pending.
    #[instruction]
    pub fn credit_box(box_ctxt: Enc<Shared, BoxState>, amount: u128) -> Enc<Shared, BoxState> {
        let bx = box_ctxt.to_arcis();
        let new_box = BoxState {
            sol: bx.sol + amount,
            perp: bx.perp,
            in_perp: bx.in_perp,
            cost_basis: bx.cost_basis,
        };
        box_ctxt.owner.from_arcis(new_box)
    }

    /// Debit a public withdraw `amount` from a box's encrypted SOL balance.
    /// Returns the updated box and a PUBLIC `ok` flag: true iff the balance was
    /// sufficient (so the on-chain program knows whether to release SOL from the
    /// vault). Revealing `ok` leaks only success/failure — not the balance — and
    /// the withdraw amount is already public at the custody boundary.
    #[instruction]
    pub fn debit_box(box_ctxt: Enc<Shared, BoxState>, amount: u128) -> (Enc<Shared, BoxState>, bool) {
        let bx = box_ctxt.to_arcis();
        let ok = bx.sol >= amount;
        let new_sol = if ok { bx.sol - amount } else { bx.sol };
        let new_box = BoxState {
            sol: new_sol,
            perp: bx.perp,
            in_perp: bx.in_perp,
            cost_basis: bx.cost_basis,
        };
        (box_ctxt.owner.from_arcis(new_box), ok.reveal())
    }

    /// Pay a public `amount` out of the encrypted treasury (fee sweep). Returns the
    /// updated treasury and a PUBLIC `ok` flag (sufficient balance) gating the on-chain
    /// vault->recipient transfer. Authority-checked on-chain.
    #[instruction]
    pub fn pay_treasury(treasury_ctxt: Enc<Mxe, u128>, amount: u128) -> (Enc<Mxe, u128>, bool) {
        let treasury = treasury_ctxt.to_arcis();
        let ok = treasury >= amount;
        let new_treasury = if ok { treasury - amount } else { treasury };
        (treasury_ctxt.owner.from_arcis(new_treasury), ok.reveal())
    }

    // ========================================================================
    // Genesis circuits — Enc<Mxe,T> state can't be produced by the client (only the
    // cluster holds the Mxe key), so the initial encrypted curve + treasury are minted
    // by these tiny circuits. (Shared box state IS client-producible via the shared
    // secret, so boxes don't need an init circuit.)  Public genesis values come in as
    // plaintext args; Mxe::get().from_arcis(..) seals them to the cluster.
    // ========================================================================

    /// Mint the genesis encrypted curve from public reserve values.
    #[instruction]
    pub fn init_curve(r_sol: u128, r_tok: u128) -> Enc<Mxe, Curve> {
        Mxe::get().from_arcis(Curve { r_sol, r_tok })
    }

    /// Mint the genesis encrypted treasury (zero).
    #[instruction]
    pub fn init_treasury() -> Enc<Mxe, u128> {
        Mxe::get().from_arcis(0u128)
    }
}
