# FlipVault App — Build & Deploy Runbook

The app lives in `app/` (a cargo workspace). Pieces and their status:

| Piece | Path | Status |
|---|---|---|
| Shared SDK | `app/crates/flipvault-sdk` | ✅ compiles native + wasm32, tested |
| Keeper | `app/keeper` | ✅ verified driving live devnet rounds |
| Indexer + API | `app/indexer` | ✅ verified ingesting live devnet → Postgres |
| Frontend (Dioxus/WASM) | `app/frontend` | spike (connect wallet + deposit); browser test is manual |

Version lane (do not mix): **solana-pubkey 4.x / solana-instruction 3.x / borsh 1**, `wallet-adapter 1.4.2`, raw web-sys fetch for RPC. The frontend is its own workspace so its wasm-only deps stay out of the native build.

## Local dev (inside the dev container)

```bash
# native crates
cd /workspace/app
cargo test  -p flipvault-sdk
cargo build -p flipvault-keeper -p flipvault-indexer

# keeper (drives rounds on devnet)
KEEPER_KEYPAIR=/root/.config/solana/devnet.json RPC_URL=https://api.devnet.solana.com \
  ./target/debug/flipvault-keeper

# indexer + API (Postgres is the `postgres` compose service)
DATABASE_URL=postgres://flip:flip@postgres:5432/flipvault RPC_URL=https://api.devnet.solana.com \
  BIND=0.0.0.0:8080 ./target/debug/flipvault-indexer
# API: GET /healthz · /rounds · /vaults/{id}/history

# frontend (Dioxus → WASM)
cd /workspace/app/frontend && dx serve --platform web   # open the printed localhost URL
```

## Containers

```bash
cd app
docker build -f keeper/Dockerfile  -t flipvault-keeper  .
docker build -f indexer/Dockerfile -t flipvault-indexer .
```

## Deployment topology (M5)

| Piece | Target | Notes |
|---|---|---|
| Frontend | Cloudflare Pages / Netlify (static WASM) | `dx bundle --platform web --release` → upload `dist/`; set `RPC_URL`/`PROGRAM_ID` at build |
| Keeper | Fly.io / Railway (1 always-on replica) | secret `KEEPER_KEYPAIR` (fund it!), `RPC_URL`, `ORAO_TREASURY` |
| Indexer + API | Fly.io / Railway | `DATABASE_URL`, `RPC_URL`, `BIND=0.0.0.0:8080` |
| Postgres | Neon / Supabase (managed) | `DATABASE_URL` secret |
| RPC | Helius (devnet free → mainnet paid) | public `api.devnet.solana.com` rate-limits; fine for testing |

**Secrets:** never commit the keeper keypair. On Fly: `fly secrets set KEEPER_KEYPAIR="$(cat devnet.json)"` and read it from env into a temp file, or mount a volume.

## devnet → mainnet (M6)

The code is cluster-agnostic; only config changes:
1. Point `RPC_URL` at a mainnet provider (Helius); fund a **mainnet keeper keypair**.
2. Fresh mainnet Postgres.
3. Run `scripts/initialize.ts` (or an SDK call) on mainnet with the chosen `seed_sol`/`fee_bps`.
4. Vendor/fork `wallet-adapter` to remove single-maintainer risk; add priority-fee + `skip_preflight` tuning to the keeper for landing under load.
5. When satisfied: **burn the upgrade authority** —
   `solana program set-upgrade-authority <PROGRAM_ID> --final --url mainnet-beta --keypair <authority>`
   (irreversible; per locked decision #17).

## Known follow-ups
- Indexer: add deposit/withdraw/position/leaderboard via tx-arg decoding (currently snapshot + rounds only).
- Keeper: priority fees + a keypair-balance watcher (alert/top-up).
- Frontend: full UI (vault grid, round timer, withdraw, positions) on top of the spike; `account_subscribe` websocket for instant flips.
