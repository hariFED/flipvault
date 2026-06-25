# FlipVault

A fully on-chain, VRF-driven game on **Solana** — with an **all-Rust full-stack app** (frontend, keeper, and indexer) modeled after ORE.

One shared constant-product bonding curve backs **four vaults**. Every ~30 seconds, **ORAO VRF** picks one vault at random and **flips** it — converting its SOL tranche into a virtual-token tranche (or back) at the current curve price. Only **SOL** ever moves as real lamports; the "token" is purely virtual (a `u64` inside the curve). Withdrawals pay a **10% fee** to a treasury.

> **Live on devnet:** program `EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H`

## The game in one breath

- **Deposit** SOL into a vault's **SOL tranche** → receive pro-rata **shares**.
- Each round, VRF flips one vault: its SOL tranche becomes a **TOKEN tranche** (locked) at the current price; its TOKEN tranche becomes SOL.
- If your vault flips while you're on the SOL side, you're **locked in the token side** until that vault is re-selected and flips back — at an unknown future price. That's the gamble.
- **Withdraw** from a SOL tranche anytime (10% fee). `reserve + Σ(SOL-tranche lamports)` is conserved by flips (proven in tests).
- There is **no user buy/sell** — trading only happens via flips.

## Architecture (Rust everywhere)

```
                       SOLANA DEVNET
        program EkfN5v…rV4H  +  ORAO VRF (VRFzZoJ…Dr7y)
   ┌──────────────┬────────────────────┬────────────────────┐
   │ sign + send  │ read state         │ commit / settle    │
   ▼              ▼                    ▼                    
┌──────────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ FRONTEND  Dioxus/WASM│  │ INDEXER + API     │  │ KEEPER            │
│  • wallet-adapter    │◄─┤  Postgres + Axum  │  │  every ~30s:      │
│  • live vault grid   │  │  rounds / history │  │  commit → VRF →   │
│  • deposit/withdraw  │  └──────────────────┘  │  settle (flip)    │
└──────────┬───────────┘          ▲             └────────┬─────────┘
           │   build ix / decode  │  decode               │ build ix
           └──────────► flipvault-sdk ◄────────────────────┘
                       (ids · PDAs · ix builders · borsh decoders)
```

- **On-chain program** (`flipvault/`) — Anchor/Rust. The rules: curve math, the round state machine, VRF vault selection, the fee. Source of truth.
- **`flipvault-sdk`** (`app/crates/flipvault-sdk/`) — shared ABI: program/PDA ids, instruction builders, borsh account decoders. Compiles **native + wasm32**; used by all three app pieces.
- **Keeper** (`app/keeper/`) — native Rust service, the heartbeat. Recovers stuck rounds.
- **Indexer + API** (`app/indexer/`) — ingests on-chain state into Postgres, serves an Axum REST API.
- **Frontend** (`app/frontend/`) — Dioxus → WASM. Wallet connect, live grid, round countdown, deposit/withdraw. No JS framework.

## Repo layout

```
flipsol/
  flipvault/                 # Anchor program + TypeScript client scripts
    programs/flipvault/      #   the on-chain program (+ curve-math unit tests)
    scripts/                 #   initialize, deposit, withdraw, round, status, settle, ...
  app/                       # the Rust full-stack app (cargo workspace)
    crates/flipvault-sdk/    #   shared ABI (native + wasm)
    keeper/                  #   round-driver service
    indexer/                 #   ingester + Axum API
    frontend/                #   Dioxus/WASM dashboard (its own workspace)
  docs/                      # design, decisions, plans, runbooks
  Dockerfile  docker-compose.yml  dev.ps1   # pinned dev toolchain
```

## Prerequisites

- **Docker Desktop** (WSL2 backend on Windows). Everything builds and runs inside a pinned container — no host Rust/Solana/Node required.
- A **Solana wallet** (Phantom) set to **Devnet** for the browser app.

## Setup

```powershell
# Build the toolchain image: Rust 1.92, Agave/Solana, Anchor 0.31, Node 22, Dioxus CLI.
# First build is slow — it compiles Anchor and the Dioxus CLI from source.
./dev.ps1 build
./dev.ps1 up
./dev.ps1 shell        # open a bash shell inside the container
```

Not on Windows? Use the equivalents: `docker compose build`, `docker compose up -d`, `docker compose exec dev bash`.

> The container keypair at `/root/.config/solana/devnet.json` is used by the keeper and scripts. Generate one with `solana-keygen new -o ~/.config/solana/devnet.json` and fund it via <https://faucet.solana.com> (Devnet).

## Run it

The program is **already deployed on devnet**, so you can run every piece against it without redeploying. Run these **inside the container** (`./dev.ps1 shell`).

### Frontend — the dashboard
```bash
cd /workspace/app/frontend
dx serve --platform web --port 8899 --addr 0.0.0.0
# → open http://localhost:8899
```
In the browser: set **Phantom to Devnet** (Settings → Developer Settings → Testnet Mode → Solana Devnet), fund your wallet, **Connect**, then Deposit / Withdraw. The grid is live; the countdown advances only while the keeper is running.

### Keeper — drives the rounds (the "game loop")
Pays a tiny fee per round, so it needs a funded devnet keypair.
```bash
cd /workspace/app
KEEPER_KEYPAIR=/root/.config/solana/devnet.json \
RPC_URL=https://api.devnet.solana.com \
  cargo run -p flipvault-keeper
# commit → wait for ORAO VRF → settle (flips vault = rand % 4), every ~30s
```

### Indexer + API
```bash
docker compose up -d postgres        # from the host
cd /workspace/app
DATABASE_URL=postgres://flip:flip@postgres:5432/flipvault \
RPC_URL=https://api.devnet.solana.com BIND=0.0.0.0:8080 \
  cargo run -p flipvault-indexer
# → GET http://localhost:8080/healthz · /rounds · /vaults/0/history
```

### On-chain program
```bash
cd /workspace/flipvault
anchor build                         # SBF program + IDL + TS types
cargo test -p flipvault              # curve-math unit tests (incl. 100k-flip invariant)
# integration tests run against a local validator — see docs/FlipVault-devnet-runbook.md
```

### TypeScript client scripts (against devnet)
```bash
cd /workspace/flipvault
export ANCHOR_PROVIDER_URL=https://api.devnet.solana.com
export ANCHOR_WALLET=/root/.config/solana/devnet.json
npx ts-node scripts/status.ts        # inspect config + vaults
npx ts-node scripts/round.ts         # run a single round manually
npx ts-node scripts/settle.ts        # settle a stuck Pending round
```

## Deploy your own (devnet → mainnet)

Full deployment topology (Dockerfiles, Fly.io / Cloudflare Pages / Neon, secrets, the mainnet immutability burn) is in **[docs/FlipVault-app-deploy.md](docs/FlipVault-app-deploy.md)** and **[docs/FlipVault-devnet-runbook.md](docs/FlipVault-devnet-runbook.md)**.

## Docs

| File | What |
|------|------|
| [docs/FlipVault-understanding.md](docs/FlipVault-understanding.md) | Design, the math, and the verified conservation invariant |
| [docs/FlipVault-decisions.md](docs/FlipVault-decisions.md) | Locked product/economic decisions |
| [docs/FlipVault-app-plan.md](docs/FlipVault-app-plan.md) | Full-stack app architecture + build plan |
| [docs/FlipVault-devnet-runbook.md](docs/FlipVault-devnet-runbook.md) | Deploy / initialize / run a round on devnet |
| [docs/FlipVault-app-deploy.md](docs/FlipVault-app-deploy.md) | App build & deploy runbook |

## Status

| Piece | State |
|---|---|
| On-chain program | ✅ deployed + verified on devnet |
| `flipvault-sdk` | ✅ native + wasm32, unit-tested |
| Keeper | ✅ drives live devnet rounds |
| Indexer + API | ✅ ingests live devnet → Postgres |
| Frontend | ✅ live dashboard, real deposits/withdrawals |
| Mainnet | ◻️ code-ready; needs hosting accounts + the irreversible upgrade-authority burn |

## Tech

Anchor 0.31 · Agave/Solana · ORAO VRF · Rust (solana-pubkey 4.x / instruction 3.x lane) · Dioxus 0.7 + `wallet-adapter` (WASM) · Axum + SQLx + Postgres.
