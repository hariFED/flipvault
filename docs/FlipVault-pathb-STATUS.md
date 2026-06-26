# FlipVault Path-B — Status & Handoff

> Branch `path-b-perpetual`. Per-player confidential perpetual on **Arcium MPC**. This is the
> single source of truth for *where Path-B is*. Design: [pathb-M0](FlipVault-pathb-M0.md) ·
> Build plan: [pathb-backend-blueprint](FlipVault-pathb-backend-blueprint.md) ·
> Arcium patterns: [pathb-arcium-patterns](FlipVault-pathb-arcium-patterns.md).

## TL;DR
The Path-B **backend is built and proven-correct** (math + compile), and the **SDK** is built and
tested. The one thing not yet done is the **live-MPC runtime proof** (running the confidential flip
on real Arcium nodes), because localnet can't run the MPC nodes from *inside* our Docker container
(Docker-out-of-Docker path mismatch on Windows). That last step runs in WSL2-native Arcium or on
devnet — see **Go-live** below.

## Done & verified ✅

| Piece | What | How to re-verify |
|---|---|---|
| **Curve precision (M0a)** | Arcis-subset math == transparent curve, **0 drift** over 20M inputs | `docker compose exec dev bash -lc "cd /workspace/path-b/spikes/curve-precision && cargo run --release --bin sweep"` |
| **Economic solvency** | `vault == Σ box-SOL + reserve + treasury` across 200k mixed deposits/withdrawals/flips/sweeps | `… && cargo test --release` (6 tests) |
| **Circuits compile** | `flip_box`, `credit_box`, `debit_box`, `pay_treasury`, `init_curve`, `init_treasury` compile on real Arcis; generated IDL matches design exactly | `docker compose --profile arcium exec arcium bash -lc "cd /workspace/path-b && arcium build"` |
| **Program builds (SBF)** | Full M1 program: `initialize · seed_curve · seed_treasury · register_box · deposit · withdraw · queue_flip` + all MPC callbacks + comp-def bootstraps. 814 KB `.so` + IDL + TS types | same `arcium build` → `target/deploy/flipvault_pathb.so` |
| **SDK** | `flipvault-pathb-sdk` — ids/PDAs/disc/borsh decoders; native **+ wasm32** | `docker compose exec dev bash -lc "cd /workspace/app && cargo test -p flipvault-pathb-sdk"` (5 tests) |
| **Toolchain** | arcium 0.11.2 · solana 3.1.10 · anchor 1.0.2 · SBF platform-tools — all on **persistent volumes** | `docker compose --profile arcium exec arcium arcium --version` |
| **Validation test** | `tests/flipvault-pathb.ts` — genesis→register→flip→decrypt→assert vs transparent curve (ready to run once MPC is live) | runs under `arcium test` (needs live MPC env) |

## Architecture recap
- **Public on-chain**: box identities, the SOL **custody vault** (real lamports), config (`k`, `fee_bps`),
  and (M2) VRF round/selection. Deposit/withdraw amounts are public at the vault boundary (v1).
- **Confidential (Arcium MXE)**: curve reserves (`Enc<Mxe>`), treasury (`Enc<Mxe>`), and each box's
  `{sol, perp, in_perp, cost_basis}` (`Enc<Shared>`, owner-decryptable). Persisted as `[[u8;32];N]`
  + nonce; fed to circuits via `ArgBuilder.account(pda, offset, len)`; written back in callbacks.
- Per flip: **no real SOL moves** (only encrypted balances + an encrypted treasury) → no per-flip
  amount/position/P&L leak. Stale-callback guard via `config.curve_version`; per-box `pending` lock.

## Deferred / remaining

| Item | Status | Why / Note |
|---|---|---|
| **Live-MPC runtime proof** (M0a-runtime, M0b latency, M0c client crypto) | **deferred** | Localnet MPC nodes can't start from inside the container (DinD path mismatch). The validation test is ready. |
| **VRF auto-selection** (commit_round / select_and_queue_flip / recover_round) | not built | The round loop. Needs ORAO↔anchor-1.0.2 compat check (blueprint §4); can't *run* without live MPC + keeper. |
| **Admin sweep_treasury** | not built | `pay_treasury` circuit exists; the on-chain ix is a follow-up. |
| **Keeper** | not built | Round driver; mirrors path-A keeper, three-phase. |
| **Frontend connection** | SDK ready | Public-state read works now via the SDK. Deposit/withdraw + private box decrypt need a live MXE + the M0c WASM-crypto decision (wasm-bindgen interop vs Rust-native x25519+Rescue). |

## Go-live (running the confidential flip for real)

**Option A — WSL2-native Arcium (recommended; localnet, handles the heavy circuit):**
1. Install the Arcium toolchain in WSL2 directly (not nested in a container) so `arcium localnet`
   manages the MPC node containers with aligned paths. Reuse cached artifacts if possible:
   the SBF platform-tools live in the `arcium-toolcache` docker volume; the arcium CLI is in the image.
2. `cd path-b && arcium test` → runs `tests/flipvault-pathb.ts` → the flip executes on local MPC and the
   decrypted box must equal `buy(...)` from the transparent curve (the M0a runtime gate).

**Option B — Devnet (live cluster, no local nodes):**
1. Fund a devnet keypair (~5 SOL). `arcium deploy --cluster-offset <devnet> …`.
2. Heads-up: `flip_box.arcis` is **16.9 MB** (two oblivious u128 divisions). Uploading it as on-chain
   circuit buffers over a slow link is painful — first **optimize the circuit** (2 divisions → 1 via an
   obliviously-selected denominator; ~halves size + latency), then deploy.

## File map (Path-B)
```
path-b/
  encrypted-ixs/src/lib.rs              # 6 Arcis circuits (flip + custody + genesis)
  programs/flipvault-pathb/src/         # the #[arcium_program]: lib.rs, state.rs, error.rs, constants.rs
  spikes/curve-precision/               # the math proof (sweep + 6 tests)
  tests/flipvault-pathb.ts              # the M0a runtime validation driver
  Anchor.toml Arcium.toml Cargo.toml    # arcium workspace (Solana 3.1.10 / Anchor 1.0.2 lane)
app/crates/flipvault-pathb-sdk/         # the read-path SDK (native + wasm)
Dockerfile.arcium  docker-compose.yml   # the arcium toolchain (seccomp, persistent tool/ledger volumes)
docs/FlipVault-pathb-*.md               # design, blueprint, patterns, this status
```
