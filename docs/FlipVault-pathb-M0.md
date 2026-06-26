# FlipVault Path-B (Private Perpetual) — Milestone 0 (de-risk spike)

> Branch: `path-b-perpetual`. This doc is the source of truth for the Path-B decisions and the
> M0 spike. Feasibility background: [FlipVault-arcium-research.md](FlipVault-arcium-research.md).
> **File-by-file build plan (M1→M4, incl. frontend connection):**
> [FlipVault-pathb-backend-blueprint.md](FlipVault-pathb-backend-blueprint.md).

## What Path-B is (locked)

A **per-player box** game with **confidential internal state**, built on **Arcium MPC**.

- Every player funds their own **box** (a PDA) with SOL. There are no shared 4-vaults anymore —
  both Path-B and Path-C are per-player boxes; the only difference is Path-B adds privacy.
- Each ~30s round, a **public, verifiable VRF** (ORAO/Switchboard) picks ONE box. That box is
  **flipped** at the current bonding-curve price: SOL→perp if it was waiting, or perp→SOL if it
  was flipped (realizing P&L). A 10% fee is taken on the SOL leg.
- A flipped box **waits** until it is selected again to convert back — at an unknown future
  price. That wait is the gamble (same core loop as Path-A, now per-player).

### Privacy scope (decided 2026-06-25, with the owner)

Arcium gives us **"private contents, public doors":**

| Hidden (inside Arcium MXE) | Public (on-chain) |
|---|---|
| Each box's SOL/perp balance | Box identities (PDAs) in a registry |
| Position (waiting vs flipped) | **VRF selection + proof** (which box, the randomness) |
| Realized / unrealized P&L | The custody vault address |
| Bonding-curve reserves & price | The *fact + timing* of a deposit/withdraw |
| Treasury (accrued fees) | **Deposit / withdraw amounts** (custody boundary) |

The one thing we **cannot** hide today is **deposit/withdraw amounts** — Solana's Token-2022
Confidential Transfers have been disabled since the June 2025 ZK-ElGamal bug, and C-SPL hasn't
shipped. This is a Solana-wide limitation, not an Arcium one. When it returns, amount-hiding is
a switch-on (Milestone 5). The owner accepted this ("Use Arcium now / partial privacy").

## Architecture (the public/confidential split)

```
EVERY ~30s (soft cadence — selection public, settlement async):

  1. PUBLIC SELECTION (on-chain, ORAO/Switchboard VRF)
       selected_index = rand % active_box_count          <-- PLAINTEXT, verifiable
       emit BoxSelected{round, index, box}                   (NOT Arcium ArcisRNG — must stay
                                                                publicly auditable as fair)
                          |
                          v  selected box pubkey (public)
  2. QUEUE CONFIDENTIAL FLIP (tx 1: CPI queue_computation)
       args: public [selected box, k, fee_bps]
             Enc<Mxe, Curve>      (cluster-only)
             Enc<Mxe, u128>       treasury (cluster-only)
             Enc<Shared, BoxState> (player + cluster)
       box.pending = true   (lock)
                          |
                          v  OFF-CHAIN MPC (Cerberus — secure if >=1 honest node)
  3. flip_box CIRCUIT (Arcis, on secret shares)  -- see path-b/encrypted-ixs/src/lib.rs
       compute both legs (buy & sell), select on secret in_perp, fee -> encrypted treasury
                          |
                          v
  4. CALLBACK (tx 2: #[arcium_callback])
       verify_output (BLS) -> write new curve ct + treasury ct + box ct
       GUARD: reject if curve nonce changed since queue (stale-callback guard)
       box.pending = false; emit FlipSettled{round}   (no amounts)
```

**Custody model (v1, works today):** one public custody vault holds the real SOL. Deposit credits
an **encrypted internal balance** (the box's `sol`); withdraw reverses it. Per-flip, only encrypted
internal balances move — **no real SOL moves per flip**, so no per-flip amount leaks. Real SOL
moves only on deposit/withdraw, where the amount is public (accepted for v1).

## Toolchain (Path-B has its OWN, isolated from Path-A)

Arcium 0.11.1 pins **Solana CLI 3.1.10** and **Anchor 1.0.2**, which clash with Path-A's
Agave 4.0.2 / Anchor 0.31.1. So Path-B gets a separate image and compose service:

- `Dockerfile.arcium` — Ubuntu 22.04 + Rust 1.92 + Solana 3.1.10 + Anchor 1.0.2 + arcup 0.11.1
  (`arcup install` → arcium CLI) + docker CLI/compose plugin (for `arcium localnet`).
- `docker compose --profile arcium build arcium` / `… up -d arcium` / `… exec arcium bash`.
- The docker socket is mounted so `arcium localnet` can spin up MPC nodes (Docker-out-of-Docker).
- **Empirical runs target Arcium devnet (cluster offset 456)** — needs only a funded keypair +
  RPC, so it sidesteps fragile Docker-in-Docker on Windows. localnet is for fast local iteration.

## M0 spikes — status

### 0a — Curve math in Arcis vs transparent Rust  ✅ proven at the design level
**Decision: scaled-integer `u128`, NOT fixed-point f64.** Arcis supports `u8..u128` with truncating
integer `/` and `%`, so the entire curve (`ceil_div(k,denom)`, adds, compares) ports verbatim as
integers. f64 in Arcis is emulated fixed-point (52 frac bits, range `[-2^75, 2^75)`, silently
clamped) — avoided entirely. This **eliminates risk #5** (precision drift): the confidential curve
is numerically EXACT, not approximate.

Proof artifact: `path-b/spikes/curve-precision/` (pure Rust, zero deps, runs in the Path-A `dev`
container). `arcis_model` is the curve rewritten under Arcis-subset constraints (u128 only,
guarded division, oblivious selects), diffed against the deployed `transparent` curve:

```
cargo test --release        # 5 tests pass
cargo run --release --bin sweep
  -> 20,000,000 comparisons · 0 mismatches · max |diff| 0 lamports · EXACT
```

Tests (6, all green) include two whole-model invariants:
- a **200,000-flip lamport-conservation invariant** (`reserve + Σ box-SOL + treasury = const`) — the
  Path-B analog of Path-A's 100k-flip test; and
- a **full-system solvency invariant** over 200,000 *mixed* deposits / withdrawals / flips / sweeps:
  `vault == Σ box-SOL + reserve + treasury` holds at every step — i.e. no operation in the Path-B
  economic model creates or destroys a lamport.

The matching Arcis circuits are in `path-b/encrypted-ixs/src/lib.rs`: `flip_box` plus the custody
circuits (`credit_box`, `debit_box`) and `pay_treasury` — because internal balances are encrypted,
deposits/withdrawals/sweeps must also run as (tiny) MPC computations. All mirror the proven model.
**Exit gate (pending toolchain):** `arcium build` the circuits, run on devnet, decrypt output, confirm
it still equals the transparent curve.

### 0b — End-to-end latency benchmark  ⏳ pending toolchain
Queue→MPC→callback→finalization on Arcium devnet; hundreds of runs; capture p50/p95/p99 under
simulated concurrent load. Confirm one flip < chosen tick. Research expectation: ~3–12s/flip,
no SLA → treat 30s as a SOFT cadence; ONE batched computation per tick (never per-player-per-tick).

### 0c — Rust/WASM client crypto path  ⏳ pending toolchain
Confirm x25519 ECDH + RescueCipher encrypt/decrypt from Rust/WASM (wasm-bindgen interop with
`@arcium-hq/client`, or Rust-native reimpl). Decides the Dioxus frontend crypto architecture.

**M0 exit gate:** circuit matches transparent math within tolerance (target: exactly) on real MPC;
measured p99 < chosen tick; a Rust/WASM client can encrypt inputs and decrypt its own box.

## Open decisions for the owner (from research §9 — my recommendations)

1. **Privacy scope v1** — internal-state-private, custody amounts public. ✅ DECIDED (Arcium now).
2. **Tick cadence** — keep 30s public selection; settlement async; final number from M0b data. *(rec)*
3. **Fee placement** — spike takes the 10% on the SOL leg both directions; revisit when we wire
   real economics. *(open — not yet final)*
4. **Mainnet-money gate** — define the bar (Arcium decentralization/staking GA, audits, deposit
   caps) before real funds. *(open)*
5. **Trust model** — Cerberus dishonest-majority (private if ≥1 honest node) on a *permissioned*
   alpha cluster. Acceptable for devnet/alpha. *(open for mainnet)*
6. **Fallback** — keep the transparent Path-A/Path-C as a permanent opt-out + liveness backstop. *(rec)*
7. **Frontend crypto** — decide at M0c (interop vs Rust-native). *(open — M0c output)*
8. **Cost ceiling** — per-computation SOL cost is unpublished; measure at M0b/soak; check the 10%
   fee covers ~2,880 comp/day. *(open — measured later)*
