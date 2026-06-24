# FlipVault — Locked Decisions

Tracks the owner's resolutions to the open decisions in `FlipVault-understanding.md` §6.
Target: **devnet** first.

## ✅ Locked

| # | Decision | Choice |
|---|----------|--------|
| Trust model (#17) | Admin powers after launch | **Fully immutable.** No admin can drain the reserve, change curve params, or upgrade. Upgrade authority burned. Founder can never reclaim the seed SOL. |
| Front-run / round lock (#1) | Deposits/withdrawals during a settling round | **Lock vaults while settling.** From randomness request → flip complete, deposits & withdrawals on (at least) the affected scope are frozen. Eligibility snapshotted at request time. |
| House edge (#4) | Protocol fee | **10% fee on withdrawals only** (configurable bps, default 1000). On `withdraw`, 90% of the SOL payout goes to the user, 10% to a **Treasury PDA**. Deposits and flips are fee-free. Devnet starting value. |
| Standalone buy/sell (#12) | User trading | **None.** Trading happens only via flips. TOKEN-tranche lock with no early exit is the deliberate, permanent mechanic. (Confirmed by owner.) |
| VRF provider (#6) | Randomness source | **ORAO VRF.** `orao-solana-vrf` crate via CPI. `commit_round` issues the `RequestV2`; `settle_round` reads `fulfilled_randomness()` then flips. Provider isolated behind the commit/settle boundary so it's swappable. Has devnet. |
| Oracle-stuck recovery (#7) | If VRF stalls | **`recover_round` instruction**, callable after a timeout: re-request randomness (retry) first; cancel the round (no flip, advance clock) as the fallback. Required for any provider. |

### Immutability ↔ fee compatibility
"Fully immutable" governs the **reserve, curve, and vault funds** — no instruction may touch those outside the defined deposit/withdraw/flip mechanics. A **Treasury PDA** that accumulates the defined fee, swept by a fixed treasury authority set at `initialize`, is *not* an admin power over user funds and does not break immutability. The treasury authority can only move treasury lamports, never reserve/vault lamports.

## ⏳ Pending owner confirmation

_None — all blocking decisions are resolved. Ready to scaffold once the Docker toolchain finishes building._

## Adopted engineering defaults (recommended answers I'll implement unless told otherwise)

- **#2 Accounting:** `tranche.amount` and `S` from in-state counters, never `account.lamports()`. `r_sol` derived from `reserve.lamports − rent_floor` (no duplicated field).
- **#3 Rounding:** shares mint DOWN, withdraw DOWN, `sol_out` payout rounds DOWN (`ceil_div` on the quotient) clamped to `[0, r_sol]`, `tok_out` DOWN. All curve math in **`u128`**.
- **#5 Reserve floor:** hard `min_reserve`; a flip whose sell leg would breach it is clamped/rejected — never panic. Vault withdrawals always payable from the segregated vault PDA regardless of reserve solvency.
- **#8 Rent semantics:** `r_sol` = spendable bankroll above the rent-exempt minimum. Canonical curve update = recompute `floor(k/independent)`.
- **#9 Genesis layout:** all 4 vaults init `slot0 = {SOL, 0, 0}`, `slot1 = {TOKEN, 0, 0}`. `deposit` resolves "current SOL tranche" by scanning the two slots' asset flag.
- **#10 Position keying:** keyed by physical slot index (0/1), never asset flag. A user may hold both slots of one vault.
- **#11 Decimals:** define `r_tok` scale; bound `seed_sol`/`init_r_tok` so `k` fits `u128` with margin. Genesis example to be set.
- **#13 Cadence:** `round_secs` = minimum spacing (`now ≥ last_settled_ts + round_secs`); late call = single flip, no catch-up.
- **#14 Reselection:** same vault may be picked on consecutive rounds (simplest, unbiased).
- **#15 Empty-vault flip:** accepted wasted round (no re-roll).
- **#16 Withdraw-after-flip:** allowed from a just-flipped SOL tranche; a full withdraw resets the tranche to `(0,0)` and re-arms the 1:1 mint path.

> These defaults are documented so they're visible; say the word to change any.
