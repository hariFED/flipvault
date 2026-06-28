// ─────────────────────────────────────────────────────────────────────────
// Repository / cache layer (§5, §12). Keeps Valkey access behind a small set of
// functions and translates between Prisma rows and the pure AMM types.
//
// Read path: try Valkey -> on miss, load from Postgres and backfill the cache.
// Write path: callers mutate Postgres inside a transaction, then call the
// refresh* helpers to write-through the cache.
// ─────────────────────────────────────────────────────────────────────────
import type { BondingCurveState, Prisma, PrismaClient, User } from "@prisma/client";
import { prisma } from "./db.js";
import { config } from "./config.js";
import {
  readCurveCache,
  writeCurveCache,
  readUserCache,
  writeUserCache,
  type CurveCache,
} from "./cache.js";
import { spotPrice, type CurveReserves } from "./amm.js";

// A Prisma transaction client (what `prisma.$transaction(async (tx) => …)` gives).
export type Tx = Prisma.TransactionClient | PrismaClient;

// ── Curve ───────────────────────────────────────────────────────────────────

// Convert a DB row -> the pure AMM reserves type.
export function toReserves(row: BondingCurveState): CurveReserves {
  return {
    virtualSolReserves: row.virtualSolReserves,
    virtualTokenReserves: row.virtualTokenReserves,
    realSolReserves: row.realSolReserves,
    realTokenReserves: row.realTokenReserves,
    feeBps: row.feeBps,
  };
}

function toCurveCache(row: BondingCurveState): CurveCache {
  return {
    vSol: row.virtualSolReserves,
    vTok: row.virtualTokenReserves,
    rSol: row.realSolReserves,
    rTok: row.realTokenReserves,
    feeBps: row.feeBps,
    price: row.lastPrice.toString(),
    complete: row.complete,
  };
}

// Read the curve from cache; on miss load the singleton row and backfill.
// Returns the data reads derive `withdrawableSol` from (§3).
export async function getCurveCache(mint = config.tokenMint): Promise<CurveCache> {
  const hit = await readCurveCache(mint);
  if (hit) return hit;

  const row = await prisma.bondingCurveState.findUnique({ where: { tokenMint: mint } });
  if (!row) throw new Error(`curve not initialized for mint ${mint} — run the seed`);
  const c = toCurveCache(row);
  await writeCurveCache(mint, c);
  return c;
}

// Load the curve row FOR UPDATE inside a transaction (§9 row-lock). Combined
// with the Valkey lock this guarantees no two trades compute against stale
// reserves. Returns the typed row.
export async function lockCurveRow(tx: Tx, mint = config.tokenMint): Promise<BondingCurveState> {
  const rows = await tx.$queryRaw<BondingCurveState[]>`
    SELECT * FROM bonding_curve_state WHERE "tokenMint" = ${mint} FOR UPDATE
  `;
  if (rows.length === 0) throw new Error(`curve not initialized for mint ${mint}`);
  return rows[0];
}

// Persist new reserves + price to Postgres (inside a tx) and write-through cache.
export async function saveCurve(
  tx: Tx,
  id: string,
  mint: string,
  reserves: CurveReserves,
  complete: boolean
): Promise<void> {
  const price = spotPrice(reserves);
  await tx.bondingCurveState.update({
    where: { id },
    data: {
      virtualSolReserves: reserves.virtualSolReserves,
      virtualTokenReserves: reserves.virtualTokenReserves,
      realSolReserves: reserves.realSolReserves,
      realTokenReserves: reserves.realTokenReserves,
      lastPrice: price,
      complete,
    },
  });
  await writeCurveCache(mint, {
    vSol: reserves.virtualSolReserves,
    vTok: reserves.virtualTokenReserves,
    rSol: reserves.realSolReserves,
    rTok: reserves.realTokenReserves,
    feeBps: reserves.feeBps,
    price,
    complete,
  });
}

// ── User ─────────────────────────────────────────────────────────────────────

export async function getOrCreateUser(
  tx: Tx,
  walletAddress: string,
  privyUserId?: string
): Promise<User> {
  const existing = await tx.user.findUnique({ where: { walletAddress } });
  if (existing) return existing;
  // NOTE: Privy wallet creation is stubbed (§11). For now the caller supplies
  // a wallet address; in production this is the Privy-generated embedded wallet.
  return tx.user.create({ data: { walletAddress, privyUserId: privyUserId ?? null } });
}

// Write-through the user balances cache after a DB mutation.
export async function refreshUserCache(u: Pick<User, "walletAddress" | "solBalance" | "tokenBalance" | "status">): Promise<void> {
  await writeUserCache(u.walletAddress, {
    solBalance: u.solBalance,
    tokenBalance: u.tokenBalance,
    status: u.status,
  });
}

// Read a user's balances from cache, falling back to Postgres + backfill.
export async function getUserBalances(
  walletAddress: string
): Promise<{ solBalance: bigint; tokenBalance: bigint; status: string } | null> {
  const hit = await readUserCache(walletAddress);
  if (hit) return hit;

  const row = await prisma.user.findUnique({ where: { walletAddress } });
  if (!row) return null;
  await refreshUserCache(row);
  return { solBalance: row.solBalance, tokenBalance: row.tokenBalance, status: row.status };
}
