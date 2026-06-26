# FlipVault Path-B — Private Perpetual (Arcium)

Per-player boxes with **confidential internal state** (balances, positions, P&L, curve, treasury)
on **Arcium MPC**, with **public, verifiable VRF selection**. The privacy model, architecture, and
milestone plan are in [`../docs/FlipVault-pathb-M0.md`](../docs/FlipVault-pathb-M0.md).

## Layout

```
path-b/
  encrypted-ixs/src/lib.rs        # the Arcis confidential flip circuit (flip_box)
  spikes/curve-precision/         # M0a: proves the Arcis-subset curve == transparent curve (pure Rust)
  (program/, app/ …)              # added in M1+ (the #[arcium_program] Anchor program, keeper, UI)
```

## Toolchain (separate from Path-A — Arcium pins Solana 3.1.10 / Anchor 1.0.2)

```powershell
docker compose --profile arcium build arcium     # build the Arcium toolchain image (slow first time)
docker compose --profile arcium up -d arcium
docker compose exec arcium bash                   # shell inside; arcium --version
```

## M0a — run the precision proof now (no Arcium toolchain needed)

Pure Rust, zero deps — runs in the existing Path-A `dev` container:

```bash
docker compose exec dev bash -lc \
  "cd /workspace/path-b/spikes/curve-precision && cargo test --release && cargo run --release --bin sweep"
```

Expected: 5 tests pass; sweep reports `0 mismatches · max |diff| 0 lamports · EXACT`.

## M0 remaining (needs the Arcium toolchain)

1. `arcium init` a scaffold; drop `encrypted-ixs/src/lib.rs` in; `arcium build`.
2. Deploy to Arcium devnet (offset 456); run `flip_box`; decrypt output; diff vs the transparent
   curve (M0a empirical exit gate).
3. Benchmark queue→MPC→callback→finalization latency, p50/p95/p99 (M0b).
4. Prove x25519 + Rescue encrypt/decrypt from Rust/WASM (M0c).
```
