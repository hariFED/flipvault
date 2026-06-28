# perpetual-backend — pump.fun-style bonding-curve AMM vault

A vault backend where users deposit SOL into a single token whose price is
governed by a **pump.fun-style constant-product bonding curve**. Deposits
autobuy into the curve; "picks" liquidate a random holder back to SOL; everyone
can withdraw their realized SOL.

Built on **Express** (TypeScript), **PostgreSQL** (Prisma, system of record) and
**Valkey** (Redis-compatible, hot read path).

## How it works

- **Deposit = autobuy.** Depositing SOL immediately swaps SOL → token on the
  curve and adds the wallet to the game. Re-deposits add to the existing
  position.
- **Pick = settle to SOL.** A pick selects a **random eligible wallet**
  (`ACTIVE`, `tokenBalance > 0`) via `crypto.randomInt` and fully liquidates it:
  any residual free SOL is bought in first (SOL → token), then the **full token
  balance** is sold (token → SOL). The realized SOL becomes the wallet's
  withdrawable balance.
- **Withdraw** draws from that realized free SOL (1:1).

### Withdrawable is derived, never stored (the key design choice)

The only price-moving shared state is the **curve reserves** (a single row). A
trade updates just (a) the curve and (b) the trading user's balance — **O(1)**.
Every other user's `withdrawableSol` is computed **at read time** from the
cached curve state as a **full sell-out quote** (the SOL they'd get selling
their entire token balance now) plus realized free SOL. No per-user value is
rewritten on price moves.

> Because each quote assumes that user sells first, the **sum** of all users'
> withdrawable amounts can exceed the real SOL in the vault. This is intended
> pump.fun behavior — left **uncapped** (no pro-rating).

## One-time setup

```bash
npm install
docker compose up -d          # start Postgres + Valkey
npx prisma migrate dev        # create tables from prisma/schema.prisma
npm run seed                  # initialize the singleton bonding curve at genesis
```

## Daily use

```bash
npm run dev                   # start API on http://localhost:3000 (auto-reloads)
npm test                      # unit tests (AMM math, withdrawable, selection)
npm run typecheck             # tsc --noEmit
npm run db:studio             # Prisma Studio GUI
docker compose down           # stop databases (add -v to delete all data)
```

## Money units

All amounts are integer **base units** — never floats:

- SOL → **lamports** (1 SOL = 1e9 lamports)
- tokens → **base units** (6 decimals, 1 token = 1e6)

Request/response money fields are **strings** of integer base units. Prices are
decimal strings of **lamports per token base unit**.

## API

Action endpoints (state mutations are atomic — Valkey curve lock + Postgres
`SELECT … FOR UPDATE` inside one transaction):

```bash
# Deposit -> autobuy. txSignature is the idempotency key.
curl -X POST localhost:3000/deposit -H 'Content-Type: application/json' \
  -d '{"walletAddress":"alice","amountSol":"2000000000","txSignature":"sig-1"}'

# Pick -> select a random eligible wallet and settle it to SOL.
curl -X POST localhost:3000/pick -H 'Content-Type: application/json' -d '{}'

# Withdraw realized SOL (creates a PENDING withdrawal; on-chain send stubbed).
curl -X POST localhost:3000/withdraw -H 'Content-Type: application/json' \
  -d '{"walletAddress":"alice","amountSol":"1000000000"}'
```

Read endpoints (served from Valkey where possible):

```bash
curl localhost:3000/price/<mint>                       # price + reserves
curl localhost:3000/user/<wallet>                      # balances + derived withdrawableSol
curl localhost:3000/trades/<mint>/recent?count=50      # recent buys/sells feed
curl "localhost:3000/candles/<mint>?timeframe=1m"      # OHLCV (scaffolded, §10)
curl localhost:3000/health
```

Solana + Privy seams (stubs returning 501 — see §11):

```
POST /privy/wallet              # TODO: Privy embedded-wallet creation
POST /solana/deposit-webhook    # TODO: on-chain deposit detection -> deposit()
POST /solana/withdraw-execute   # TODO: build/sign/send withdraw tx
```

## Layout

- `prisma/schema.prisma` — data model: `User`, `BondingCurveState` (singleton),
  `Trade`, `Deposit`, `Withdrawal`, `Candle`
- `src/config.ts` — env-driven config + pump.fun genesis constants
- `src/amm.ts` — **pure** bonding-curve math (BigInt; buy/sell/withdrawable) + `src/amm.test.ts`
- `src/cache.ts` — Valkey client, curve/user hashes, trades stream, distributed lock
- `src/repo.ts` — Prisma ↔ cache repository (write-through, row-lock helpers)
- `src/services/trade.ts` — atomic deposit autobuy, withdraw, shared buy/sell legs
- `src/services/pick.ts` — random pick + two-leg settlement + optional scheduler
- `src/services/selection.ts` — pure eligibility/random-selection + `pick.test.ts`
- `src/services/candle.ts` — OHLCV aggregation **scaffolding** (§10)
- `src/routes/` — `actions.ts`, `reads.ts`, `chain.ts` (stubs)
- `src/index.ts` — Express wiring + error handler + graceful shutdown
- `src/seed.ts` — seed the singleton curve at genesis

## Configuration

See `.env`. Curve genesis constants, fee, graduation threshold, the active
mint, and pick cadence (`PICK_AUTO_ENABLED` / `PICK_INTERVAL_MS`) are all
overridable.

## Scope

Fully implemented against Postgres/Valkey: the AMM, deposit→autobuy, random
pick→settle, withdraw, write-through caching, atomic curve updates (incl.
coherent two-leg settlements), and the recent-trades feed.

Scaffolded / stubbed (commented for later): live OHLCV aggregation, real Solana
program calls, on-chain deposit detection, Privy SDK calls, and DEX migration on
graduation.
