# FlipVault — Canonical Understanding Document

*Prepared by the lead engineer for owner review prior to implementation. This document synthesizes five expert dimension analyses (math, Anchor architecture, VRF, security, edge cases) and an adversarial math verdict into one reference. Where the spec is silent, the gap is named explicitly and routed to **Open Decisions for the Owner** rather than filled in by assumption.*

---

## 1. Mental Model in Plain English

FlipVault is a **single shared constant-product AMM** that the program trades **against itself** on behalf of four vaults. There is **no SPL token** and **no user-facing buy/sell**. The "game token" is purely virtual — a `u64` number (`r_tok`) living inside one global curve. The only real asset that ever moves as lamports is **SOL**.

**The curve (global, one instance):**
- `r_sol` — real lamports held in a Reserve PDA (the house bankroll)
- `r_tok` — a purely virtual token reserve (never a real balance)
- `k = r_sol * r_tok` — the constant product

**The four vaults.** Each vault has **two tranche slots**. At any instant, one slot is **SOL-denominated** and the other is **TOKEN-denominated** (either may be empty). A tranche stores `{asset: SOL|TOKEN, amount, total_shares}`. Per-user balances are **non-transferable Position PDAs** keyed by `(user, vault, tranche-slot) -> shares`.

**What users actually do.** Users only ever:
- `deposit(vault, amount)` — add SOL into whichever slot of that vault is *currently* SOL-denominated, receiving pro-rata shares.
- `withdraw(vault, shares)` — burn shares and take SOL out, but **only from a SOL-denominated tranche**.

**The gamble (core mechanic).** Once per round (~30s), `close_round` draws VRF, picks `vault = rand % 4`, and **flips** that one vault: it sells the vault's TOKEN tranche back into the curve for SOL, buys virtual tokens with the vault's SOL tranche, then **swaps the two slots' asset flags**. Shares are never touched, so each slot's holders keep their pro-rata fraction — but their *denomination* changed, at curve prices.

**Trading happens ONLY via flips.** This is the correct reading and a load-bearing invariant: there is no instruction that lets a user trade the curve directly. The only curve interaction is internal to `flip()`. Consequence — a depositor who lands in a SOL tranche that then gets flipped is converted to TOKEN at the current curve price **with no opt-out**, and is **locked** until (a) their vault is selected again by VRF in some future round **and** (b) that flip swaps their slot back to SOL. That wait is **unbounded** and resolves at an **unknown future price**. This illiquidity is the deliberate bet, not a bug. An implementer must not "helpfully" add a TOKEN-to-SOL redeem path; doing so would break the bankroll invariant.

---

## 2. State & Account Architecture

A single program with a small fixed set of singletons plus unbounded tiny Position accounts. **Only SOL moves as real lamports**, and it lives in exactly two kinds of account: the Reserve PDA and the four Vault PDAs.

### Account inventory and seeds

| Account | Seeds | Holds lamports? | Purpose |
|---|---|---|---|
| **Config / Curve** | `[b"config"]` | No (rent only) | Global curve + round state machine |
| **Reserve PDA** | `[b"reserve"]` | **Yes — this is `r_sol`** | House bankroll |
| **Vault PDA ×4** | `[b"vault", &[vault_id]]`, `vault_id ∈ 0..4` | **Yes — its SOL tranche** | Per-vault tranche state + SOL-tranche lamports |
| **Position PDA** | `[b"position", owner, &[vault_id], &[slot_index]]` | No (rent only) | Non-transferable user shares |

### Config / Curve state (store only the truth)
`authority`, `r_tok (u128)`, `k (u128)`, `round_secs`, `last_settled_ts`, the **pending-VRF state machine** (phase enum, committed `randomness_account` pubkey, `commit_slot`/`commit_time`, `selected_vault: Option<u8>`), and bumps. **Recommendation: do NOT store `r_sol` here.** Treat the Reserve PDA's spendable lamports as the single source of truth for `r_sol`; a duplicated field will drift from the manual-mutation path on any rent top-up, stray airdrop, or sol_out/lamport-delta mismatch. *(Owner decision: cached `r_sol` for compute vs. derived — see §6.)*

### Vault state
Per slot `{asset: u8, amount: u64, total_shares: u64}` ×2, plus `bump` and `vault_id`. **`amount` is an authoritative in-state counter, never `account.lamports()`** (this is what defeats the donation/inflation attack — see §5). The vault's spendable lamports must reconcile to its SOL-tranche `amount` net of the rent floor.

### Position state
`owner`, `vault_id`, `slot_index`, `shares (u64)`, `bump`. **Keyed by physical slot index (0/1), NOT by asset flag.** This is mandatory: the asset flag flips while the share ledger persists, so keying by asset would corrupt ledgers on every flip and is impossible for a stable PDA seed. Non-transferability is structural — there is no instruction that reassigns `owner`, and every mutator asserts `signer == position.owner` (enforced by the `owner` seed). The depositing user pays position rent on first deposit.

### The load-bearing lamport-movement mechanic
Reserve and Vault PDAs are **program-owned, data-carrying accounts**. The System Program **cannot** debit them (`system_program::transfer` only debits zero-data, System-owned accounts). Therefore:

- **Reserve ↔ Vault (both flip legs) and the withdraw payout (Vault → user):** move lamports by **direct mutation** under `try_borrow_mut_lamports`, with `checked_sub`/`checked_add`, asserting the debited account stays `>= rent_floor`:
  ```
  **from = from.checked_sub(amount).ok_or(Underflow)?;   // assert from - amount >= rent_floor
  **to   = to.checked_add(amount).ok_or(Overflow)?;
  ```
- **Deposit inflow (user → Vault):** the **opposite** case — the user wallet is System-owned, so use a **CPI `system_program::transfer`** from the signer into the Vault (crediting a program account is allowed; debiting the user requires their signature).

A single `move_lamports(from, to, amount, from_rent_floor)` helper should encapsulate the direct-mutation path. **State must be mutated and the round marked settled BEFORE any lamport moves** (state-before-transfer ordering) to block any re-entrant double-flip.

### Rent and the `r_sol` floor
Every PDA must stay rent-exempt. The curve's logical `r_sol` is `reserve.lamports - reserve_rent_floor` (spendable bankroll above rent). The spec's "guard against `r_sol -> 0`" therefore concretely means **"never let `reserve.lamports` fall to its rent floor,"** enforced as a hard on-chain check on every sell leg, not merely a test assertion.

---

## 3. The Math, Verified

### Buy / Sell (used ONLY inside `flip`)
- **Buy** (`dx` SOL in): `tokens_out = r_tok - k/(r_sol+dx)`; then `r_sol += dx`, `r_tok = k/r_sol`.
- **Sell** (`dy` tok in): `sol_out = r_sol - k/(r_tok+dy)`; then `r_tok += dy`, `r_sol = k/r_tok`.

These are standard `x*y=k`, fee-free. A clean round-trip returns the input exactly (verified numerically).

### Flip (sequential, sell-first) on a vault with SOL-tranche `S`, TOKEN-tranche `T`
1. **Sell `T`:** `sol_out = r_sol - k/(r_tok+T)`; `r_tok += T`; `r_sol -= sol_out`; move `sol_out` **reserve → vault**. The ex-TOKEN tranche now holds `sol_out` lamports (becomes SOL side).
2. **Buy with `S`:** `tok_out = r_tok - k/(r_sol+S)`; `r_sol += S`; `r_tok -= tok_out`; move `S` **vault → reserve**. The ex-SOL tranche now holds `tok_out` tokens (becomes TOKEN side).
3. **Swap the two slots' asset flags.** Shares untouched.

Note: sell-first **decreases `r_sol`** before the buy leg, which **maximizes reserve stress within a single flip**. This is the spec's fixed choice; implementers must not reorder, and it is the reason the `r_sol` floor guard interacts with flip ordering.

### The conservation invariant — IT HOLDS (proven exact)

Let `Q = reserve_lamports + Σ(SOL-tranche lamports across all 4 vaults)`. **The adversarial verdict confirms `Q` is invariant under flips, exactly, for ANY rounding.**

Proof (double-entry): Step 1 moves `sol_out` reserve→vault: `ΔQ = (-sol_out) + (+sol_out) = 0`. Step 2 moves `S` vault→reserve: `ΔQ = (+S) + (-S) = 0`. Step 3 is pure relabeling: `ΔQ = 0`. The conservation depends **only** on the credited tranche receiving the *same integer* debited from the reserve (and vice versa) — **never on `sol_out` equalling the exact curve price**, so it is fully rounding-independent. Verified over 200,000 random flips across 4 coupled vaults under both floor- and ceil-rounding: `Q` unchanged exactly. `Q` changes **only** by deposit (+) and withdraw (−).

**Two distinct invariants — do not conflate.** `Q` (the lamport sum) is exact. The **curve product `k = r_sol*r_tok` is a separate invariant** that is only *approximately* constant under floor division. Tests must assert `Q` by **exact integer equality** and `k` only **within tolerance**. Asserting exact `k` equality will fail; asserting only `k` will miss lamport leaks.

### MANDATORY rules (from the math verdict — these are not optional)

1. **`u128` everywhere.** `k = 50e9 * 3e9 = 1.5e20 = 68 bits`, which **overflows `u64`** (max ~1.84e19). Store `k` and `r_tok` as `u128`. Widen `(r_tok+T)`, `(r_sol+S)`, `(r_sol+dx)`, `(r_tok+dy)` to `u128` **before** any multiply/add-then-divide; divide `k` by them in `u128`; narrow the reserve quotient back to `u64` with a checked cast. Never form `r_sol*r_tok` or the dividend in `u64`. (`u256` is **not** required.)

2. **Round the real lamport payout DOWN (house-safe).** The literal spec `sol_out = r_sol - floor(k/(r_tok+T))` rounds the payout **UP** — it bleeds the reserve up to ~1 lamport per sell leg, a systematic one-directional drain (conservation is unaffected, but solvency margin erodes). Use:
   ```
   sol_out = r_sol - ceil_div(k, r_tok+T),  where ceil_div(a,b) = (a + b - 1)/b
   ```
   **and clamp** `sol_out` into `[0, r_sol]` with `checked_sub`. The clamp is load-bearing: under upward `k`-drift `ceil_div` can exceed `r_sol` and underflow `u64` (verified: `-10`).

3. **Round the virtual token credit DOWN.** `tok_out = r_tok - floor(k/(r_sol+S))`, clamped `>= 0`. Fewer tokens credited = conservative for the house.

4. **Canonicalize the curve update.** After each leg, persist the dependent reserve as `floor(k/independent)` using **one** chosen form; never also persist a separately-subtracted value, or `k`-drift compounds across trades. Assert `|r_sol*r_tok - k|` within a small tolerance after every flip.

5. **Hard minimum-reserve floor.** Reject or cap any flip whose sell leg would drive post-sell `r_sol` below a configured `min_reserve`. **A single large `T` can floor `r_sol` to exactly 0** (verified: `k=1e15, T=1e15, denom>k → quotient 0 → r_sol=0`), which then bricks the `r_tok = k/r_sol` recompute (division by zero). The `r_sol -> 0` guard is **mandatory, not precautionary** — this corrects an over-optimistic "asymptotically approaches but never reaches zero" claim.

6. **Empty tranches are EXPLICIT early-returns.** If `T == 0`, skip the sell leg (no curve mutation); if `S == 0`, skip the buy leg. **Do NOT rely on the formula yielding 0** — once `k`-drift exists, `T=0` produces a *phantom nonzero* `sol_out` (verified: drift of 1 → `sol_out = 1` on an empty token tranche).

7. **Guard every division** with `checked_div` + custom errors (never panic): `r_tok+T > 0`, `r_sol+S > 0`, `r_tok > 0` before `r_sol=k/r_tok`, `r_sol > 0` before `r_tok=k/r_sol`, `total_shares > 0` in withdraw. Key the deposit **1:1 empty-mint branch off `total_shares == 0`** consistently.

8. **Genesis exactness.** Set `r_sol = seed_sol` (the exact lamports transferred into the Reserve PDA) and `k = (seed_sol as u128) * (init_r_tok as u128)`. Decide explicitly that `r_sol` tracks **spendable** bankroll (excluding rent) so `reserve -= sol_out` never hits the rent floor. Validate `seed_sol > 0`, `init_r_tok > 0`, and bound them so `k` fits `u128` with margin.

9. **Exactly one flip per round, sell-first, with `checked_*` on every real lamport move.** Multiple flips in one settle callback would be order-dependent on the shared curve and consensus-critical — forbid it.

---

## 4. VRF Flow

**Critical correction to the spec.** The spec describes a "settle callback" — that mental model is the **deprecated** Switchboard-v2 VRF (a true async oracle CPI back into your program). **Current Switchboard On-Demand has NO program callback.** It is a **2-transaction commit-then-reveal** protocol driven by the client. The oracle writes the value into a Randomness account; it never calls FlipVault back. Building a v2-style callback will not compile against the current API.

**Crate/type:** `switchboard-on-demand` (≈0.13, `anchor` feature), `RandomnessAccountData`. Do **not** use `switchboard-v2`.

### How `close_round` maps to the real pattern

Split the single spec `close_round` into **two instructions**:

- **`commit_round`** — run in the **same tx** as the client's `randomness.commitIx(queue)`, which binds the Randomness account to the current recent slothash (a seed nobody can predict yet). Your instruction: assert `now - last_settled_ts >= round_secs`; assert the previous round is settled-or-expired; store `randomness_account = key`, `commit_slot`, `commit_time`, `state = Pending`. Optionally assert `get_value(clock.slot).is_err()` (anti-peek). **No random value exists yet — never compute the outcome here.**

- **`settle_round`** — run in the **same tx** as the client's `randomness.revealIx()`, ~1+ slot later. Your instruction: assert `randomness_account.key() == round.randomness_account` (identity pin); `RandomnessAccountData::parse(...)`; `let v = data.get_value(clock.slot)?`; `let vault = (v[0] % 4) as usize`; perform the deterministic sell-first flip; set `state = Settled`, `last_settled_ts = now`. **Vault selection and the flip live ONLY here.** Instruction ordering is load-bearing — the Switchboard ix runs first; your ix reads the account it just updated within the same tx.

### Key VRF design points
- **`vault = v[0] % 4` is exactly uniform** (`256 % 4 == 0`, no modulo bias for n=4). Document which byte is folded; derive selection **solely** from oracle output, never from caller-controlled data. If vault count ever leaves a power of two, fold all 32 bytes and use rejection sampling.
- **Make `settle_round` PERMISSIONLESS** (any keeper, time/state-guarded). This kills the **selective-reveal / grinding** attack: because reveal is a normal client tx, a restricted submitter could simulate the outcome off-chain and refuse to broadcast unfavorable reveals, stalling the round. A permissionless settle removes the abort advantage.
- **Do NOT hard-bind `seed_slot == clock.slot - 1`.** That tutorial check is brittle for a 30s round (miss the exact next slot → round bricks). Rely on `get_value(clock.slot)` succeeding plus your own `now - commit_time <= reveal_deadline` guard (well under the oracle's ~1h expiry).
- **Stuck-oracle recovery is REQUIRED.** If the oracle never reveals within ~1h, `get_value` errors forever and the round bricks. Add a `cancel_or_recommit_round` instruction, callable after `reveal_deadline`, that either cancels the round (no flip, advance `last_settled_ts`) or re-commits the same Randomness account to a fresh slothash. *(Spec is silent — Owner decision in §6.)*

---

## 5. Security & Economic Risks (ranked)

1. **[CRITICAL] Post-reveal deposit front-run (VRF→flip atomicity gap).** Between randomness reveal and flip execution, `vault = rand % 4` is public. If a deposit can interleave, an attacker reads the winner and `deposit(winner, large)` to ride a known-favorable flip. — *Mitigation:* consume randomness and flip **atomically** in `settle_round`; **freeze each vault's `(amount, total_shares)` eligibility snapshot at `commit_round` time**; only pre-commit deposits participate. Freeze deposits/withdrawals while a round is `Pending`.

2. **[HIGH] Donation / first-depositor inflation attack (ERC4626).** If `tranche_amount` is read from `account.lamports()`, an attacker deposits 1 lamport (1 share), donates 10 SOL directly to the Vault PDA, and the next depositor's `shares = floor(amount * total_shares / tranche_amount)` rounds to a tiny number — attacker steals ~half. Even with state-based accounting, a donation strands lamports above the tracked amount. — *Mitigation:* **always use in-state `tranche.amount`, never `account.lamports()`**, for all share math and for `S` in the flip; seed **virtual/dead shares** (ERC4626 offset) and enforce a **minimum first deposit**; explicitly sweep or refuse to count donated surplus.

3. **[HIGH] House EV / reserve drain.** All 4 vaults share one curve and flips are fee-free, so an actor pre-positioning across all four SOL tranches can harvest the slippage the reserve pays on every sell leg; combined with the round-DOWN payout fix, the house has **no built-in edge**. — *Mitigation:* decide the **fee/edge model up front** (explicit flip or withdraw haircut to the reserve) **or** document explicitly that the reserve can be depleted and token-tranche holders bear insolvency risk.

4. **[HIGH] Rounding-direction leakage.** Any division that credits shares or pays lamports must round **against the user**; an UP-rounding mint or payout drains value at dust scale. — *Mitigation:* one rounding law (mint DOWN, withdraw DOWN, `sol_out` DOWN per §3); randomized property tests asserting `Q` to the lamport.

5. **[HIGH] Reserve shortfall bricks all rounds.** If a flip's `sol_out` exceeds `reserve.lamports - rent_floor`, the transfer fails and `close_round` panics, **freezing every round**. — *Mitigation:* define non-panic behavior (clamp/partial-flip with invariant-preserving accounting, or revert just that round leaving state intact), keep the reserve rent-exempt always, and enforce `min_reserve` per §3.

6. **[HIGH] VRF grinding / reroll.** If `commit_round` can be re-requested before settle (or an abandoned settle leaves the round re-requestable), an attacker rerolls until favorable. The 30s cadence alone limits frequency, not reroll. — *Mitigation:* one live Randomness account per round, non-re-requestable once committed; idempotent, address-pinned settle; abandoned settle must not reset to re-requestable.

7. **[HIGH] `withdraw` against a mislabeled tranche.** A TOKEN holder must not withdraw. — *Mitigation:* determine "is SOL tranche?" **solely from the current `tranche.asset` flag** updated by the flip; **never cache asset on the Position PDA**. Test: deposit → flipped to TOKEN → withdraw fails; flip back → withdraw succeeds.

8. **[HIGH] Anchor hygiene.** — *Mitigation:* signer/owner checks and seed-derived Position PDA on deposit/withdraw; `address=`-pin Reserve, Vaults, and the VRF account in settle; `checked_*`/`u128` throughout; **state-before-transfer ordering**; confirm no instruction reassigns position ownership and **no path drains the reserve outside flip mechanics**.

9. **[MEDIUM] Founder / admin trust.** Any reserve-drain authority post-init is a rug for locked token holders. — *Mitigation:* make `round_secs`/curve params immutable (or timelock/multisig), guarantee no reserve-drain instruction, burn or multisig the upgrade authority, and document the seeded reserve as permanently locked house collateral.

10. **[MEDIUM] Empty/zero & dust edge cases.** — *Mitigation:* separate `total_shares==0` 1:1 path; reject zero-value deposits/withdraws and deposits that mint 0 shares; guard every divisor; make empty-tranche flip legs clean early-return no-ops so a fresh/idle vault being picked never bricks the round (it is an accepted wasted round).

---

## 6. Open Decisions for the Owner

These are the choices the spec leaves unspecified. **All must be settled before coding** (items 1–8 block the curve/round modules).

1. **VRF→flip atomicity & eligibility snapshot.** Confirm randomness is consumed and the flip executes in **one** instruction, and that flip-eligibility is snapshotted at **commit** time (not reveal). Are deposits/withdrawals **frozen** (per-vault or global) while a round is `Pending`? *(Recommended: atomic settle, snapshot at commit, global freeze while pending.)*

2. **Accounting source.** Confirm `tranche.amount` and `S` come from **in-state counters**, never `account.lamports()`. Is `r_sol` derived from `reserve.lamports - rent_floor` (recommended) or cached in Config?

3. **Rounding policy.** Confirm: shares mint DOWN, withdraw DOWN, `sol_out` rounds the payout DOWN (`ceil_div` on the quotient) and clamped to `[0, r_sol]`, `tok_out` DOWN. All curve math `u128`.

4. **House edge / fee model.** Is there a flip/withdraw fee to the reserve, or is the house edge **none** (pure slippage + rounding, reserve depletable)? This is an economic decision that changes the curve math.

5. **Reserve floor & shortfall behavior.** Concrete `min_reserve` value, and the action when a sell leg would breach it or `sol_out` exceeds available reserve: **reject / clamp / partial-flip** — never panic. Confirm vault SOL-tranche withdrawals remain payable from segregated vault PDAs **regardless** of reserve solvency (user principal never covers a reserve shortfall).

6. **Who calls `close_round` / `settle_round`.** Permissionless keeper (recommended for `settle_round`, to defeat selective-reveal) vs authority-only. Who pays for and triggers the commit/reveal txs, and what happens economically if nobody settles for several rounds.

7. **Oracle-stuck handling.** On non-reveal within ~1h: **cancel** the round (no flip) or **re-commit** the same Randomness account to a fresh slothash? Are deposits made during a stuck round withdrawable while unsettled? *(A recovery instruction is required either way.)*

8. **`r_sol` rent semantics.** Does `r_sol` track spendable bankroll above the rent-exempt minimum (recommended) or total PDA lamports? Pin the **canonical curve update** form (recommend recompute `floor(k/independent)`).

9. **Genesis vault layout.** Confirm all 4 vaults init with `slot0 = {SOL, 0, 0}`, `slot1 = {TOKEN, 0, 0}`. Which physical slot is SOL at genesis, and how does `deposit` resolve "the current SOL tranche" (scan the two slots for the SOL flag).

10. **Position keying.** Confirm Position PDAs are keyed by **physical slot index (0/1)**, never by asset flag. Confirm a user may hold both slots of one vault simultaneously. Confirm `init_if_needed` on first deposit resolves the *current* SOL slot index correctly (footgun: crediting the wrong ledger).

11. **Token decimals / scale.** Define `r_tok` decimals/units and bounds on `seed_sol`/`init_r_tok` so `k` fits `u128` with margin. Provide a concrete genesis example (e.g. `seed_sol = 100 SOL = 1e11`, `init_r_tok = 1e11`, `k = 1e22`, `u128`).

12. **Standalone buy/sell.** Confirm there is **intentionally no** user buy/sell — trading happens only via flips — and the TOKEN-tranche lock with no exit until reselection is the deliberate, permanent mechanic.

13. **Round cadence semantics.** Is `round_secs` a **minimum spacing** (`now >= last_settled_ts + round_secs`) or a fixed schedule? On a late call, single flip with no catch-up (recommended) vs accumulating missed rounds.

14. **Vault reselection.** May the same vault be picked on consecutive rounds (recommended: yes, simplest and unbiased), or must selection exclude the last-picked vault (needs `last_picked` state and `rand % 3`)?

15. **Empty-vault flip.** Confirm an all-empty flip (`S=0, T=0`) is an **accepted wasted round** (consumes the VRF draw and the window) rather than triggering a re-roll. *(Re-rolling biases VRF and complicates settle — recommend accept.)*

16. **Withdraw-after-flip timing.** May holders withdraw from a just-flipped SOL tranche in the same round/slot? Confirm a full withdraw resets the tranche to `(0, 0)` and re-arms the 1:1 mint path.

17. **Admin powers / fund recovery.** Are `round_secs`/curve params mutable post-init (timelock/multisig?), is the upgrade authority burned/multisig, and can the founder ever reclaim `seed_sol`? Any such path must not break the bankroll invariant for locked holders.

---

*Bottom line: the conservation invariant `Q` is mathematically sound and rounding-independent — solvency is robust by construction. The real work is (a) the `u128`/rounding/guard rules in §3, which are mandatory and non-negotiable, (b) re-architecting the VRF path to the real commit-reveal pattern in §4, and (c) the owner resolving the front-run/atomicity, accounting-source, and house-edge questions in §6 before any curve or round code is written.*