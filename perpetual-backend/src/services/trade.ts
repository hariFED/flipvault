// ─────────────────────────────────────────────────────────────────────────
// Trade engine (§8, §9). Deposit autobuy, withdraw, and the shared atomic
// buy/sell primitives the pick service builds on.
//
// CONCURRENCY (§9): every curve mutation runs inside `withCurveLock`, which
//   (a) holds the Valkey lock lock:curve:{mint},
//   (b) opens a Postgres transaction and SELECTs the curve row FOR UPDATE.
// Reserves are read, computed, and written within that single unit so two
// trades can never compute against stale state. Cache + feed writes happen
// AFTER the transaction commits.
//
// PERFORMANCE (§3): a trade updates only (a) the global curve and (b) the
// trading user's balance — O(1). Every other user's withdrawable amount is
// derived at read time from the cached curve, never rewritten here.
// ─────────────────────────────────────────────────────────────────────────
import type { BondingCurveState, TradeSource, User } from "@prisma/client";
import { prisma } from "../db.js";
import { config } from "../config.js";
import { acquireCurveLock, pushTrade, type TradeFeedEntry } from "../cache.js";
import {
  getOrCreateUser,
  lockCurveRow,
  refreshUserCache,
  saveCurve,
  toReserves,
  type Tx,
} from "../repo.js";
import {
  AmmError,
  applyBuy,
  applySell,
  hasGraduated,
  spotPrice,
  type CurveReserves,
} from "../amm.js";
import { feedCandle } from "./candle.js";

export class TradeError extends Error {
  constructor(message: string, public status = 400) {
    super(message);
    this.name = "TradeError";
  }
}

// Side effects to run AFTER the transaction commits (cache + feed).
interface PostCommit {
  feedEntries: TradeFeedEntry[];
  users: Array<Pick<User, "walletAddress" | "solBalance" | "tokenBalance" | "status">>;
  candlePoints: Array<{ price: string; volumeSol: bigint; volumeToken: bigint }>;
}

function newPostCommit(): PostCommit {
  return { feedEntries: [], users: [], candlePoints: [] };
}

async function flushPostCommit(mint: string, pc: PostCommit): Promise<void> {
  for (const u of pc.users) await refreshUserCache(u);
  for (const e of pc.feedEntries) await pushTrade(mint, e);
  const at = new Date();
  for (const c of pc.candlePoints) {
    await feedCandle({ mint, price: c.price, volumeSol: c.volumeSol, volumeToken: c.volumeToken, at });
  }
}

// Run `fn` with the curve lock held and the curve row locked FOR UPDATE inside
// a transaction. Post-commit side effects are flushed once the tx succeeds.
export async function withCurveLock<T>(
  mint: string,
  fn: (ctx: { tx: Tx; row: BondingCurveState; pc: PostCommit }) => Promise<T>
): Promise<T> {
  const release = await acquireCurveLock(mint);
  const pc = newPostCommit();
  try {
    const result = await prisma.$transaction(async (tx) => {
      const row = await lockCurveRow(tx, mint);
      return fn({ tx, row, pc });
    });
    await flushPostCommit(mint, pc);
    return result;
  } finally {
    await release();
  }
}

// ── Shared leg primitives (assume curve already locked, inside a tx) ─────────

// Apply a BUY leg: amountSol -> tokens for `user`. Mutates curve `row` snapshot
// in place via the returned reserves, records a Trade, accumulates side effects.
async function buyLeg(
  ctx: { tx: Tx; pc: PostCommit },
  reserves: CurveReserves,
  user: User,
  mint: string,
  amountSol: bigint,
  source: TradeSource,
  txSignature?: string
): Promise<{ reserves: CurveReserves; tokensOut: bigint }> {
  const res = applyBuy(reserves, amountSol);
  const price = spotPrice(res.reserves);

  await ctx.tx.trade.create({
    data: {
      walletAddress: user.walletAddress,
      userId: user.id,
      tokenMint: mint,
      side: "BUY",
      solAmount: amountSol,
      tokenAmount: res.tokensOut,
      price,
      feePaid: res.fee,
      curveSolAfter: res.reserves.virtualSolReserves,
      curveTokenAfter: res.reserves.virtualTokenReserves,
      source,
      txSignature: txSignature ?? null,
    },
  });

  ctx.pc.feedEntries.push({
    side: "BUY",
    solAmount: amountSol.toString(),
    tokenAmount: res.tokensOut.toString(),
    price,
    source,
    wallet: user.walletAddress,
  });
  ctx.pc.candlePoints.push({ price, volumeSol: amountSol, volumeToken: res.tokensOut });

  return { reserves: res.reserves, tokensOut: res.tokensOut };
}

// Apply a SELL leg: tokens -> SOL for `user`.
async function sellLeg(
  ctx: { tx: Tx; pc: PostCommit },
  reserves: CurveReserves,
  user: User,
  mint: string,
  amountTok: bigint,
  source: TradeSource
): Promise<{ reserves: CurveReserves; solOut: bigint }> {
  const res = applySell(reserves, amountTok);
  const price = spotPrice(res.reserves);

  await ctx.tx.trade.create({
    data: {
      walletAddress: user.walletAddress,
      userId: user.id,
      tokenMint: mint,
      side: "SELL",
      solAmount: res.solOut,
      tokenAmount: amountTok,
      price,
      feePaid: res.fee,
      curveSolAfter: res.reserves.virtualSolReserves,
      curveTokenAfter: res.reserves.virtualTokenReserves,
      source,
    },
  });

  ctx.pc.feedEntries.push({
    side: "SELL",
    solAmount: res.solOut.toString(),
    tokenAmount: amountTok.toString(),
    price,
    source,
    wallet: user.walletAddress,
  });
  ctx.pc.candlePoints.push({ price, volumeSol: res.solOut, volumeToken: amountTok });

  return { reserves: res.reserves, solOut: res.solOut };
}

// ── Deposit = autobuy (§2.1, §8) ─────────────────────────────────────────────
export interface DepositResult {
  walletAddress: string;
  amountSol: bigint;
  tokensBought: bigint;
  tokenBalance: bigint;
  price: string;
  idempotentReplay: boolean;
}

export async function deposit(args: {
  walletAddress: string;
  amountSol: bigint;
  txSignature: string;
  privyUserId?: string;
  mint?: string;
}): Promise<DepositResult> {
  const mint = args.mint ?? config.tokenMint;
  if (args.amountSol <= 0n) throw new TradeError("amountSol must be > 0");

  // Idempotency (§9): a deposit with a known txSignature is a no-op replay.
  const prior = await prisma.deposit.findUnique({ where: { txSignature: args.txSignature } });
  if (prior) {
    const user = await prisma.user.findUnique({ where: { walletAddress: args.walletAddress } });
    const curve = await prisma.bondingCurveState.findUnique({ where: { tokenMint: mint } });
    return {
      walletAddress: args.walletAddress,
      amountSol: prior.amountSol,
      tokensBought: 0n,
      tokenBalance: user?.tokenBalance ?? 0n,
      price: curve?.lastPrice.toString() ?? "0",
      idempotentReplay: true,
    };
  }

  return withCurveLock(mint, async ({ tx, row, pc }) => {
    if (row.complete) throw new TradeError("curve has graduated; trading is closed", 409);

    // Create-or-add: a wallet that already holds tokens adds to its position.
    const user = await getOrCreateUser(tx, args.walletAddress, args.privyUserId);

    await tx.deposit.create({
      data: {
        walletAddress: user.walletAddress,
        userId: user.id,
        amountSol: args.amountSol,
        txSignature: args.txSignature,
        status: "CONFIRMED",
      },
    });

    let reserves = toReserves(row);
    const buy = await buyLeg({ tx, pc }, reserves, user, mint, args.amountSol, "DEPOSIT_AUTOBUY", args.txSignature);
    reserves = buy.reserves;

    const updated = await tx.user.update({
      where: { id: user.id },
      data: { tokenBalance: { increment: buy.tokensOut } },
    });

    const complete = hasGraduated(reserves, config.curve.graduationLamports);
    await saveCurve(tx, row.id, mint, reserves, complete);

    pc.users.push(updated);

    return {
      walletAddress: user.walletAddress,
      amountSol: args.amountSol,
      tokensBought: buy.tokensOut,
      tokenBalance: updated.tokenBalance,
      price: spotPrice(reserves),
      idempotentReplay: false,
    };
  });
}

// ── Withdraw (§8) ─────────────────────────────────────────────────────────────
// Debits realized free SOL only. Does NOT touch the curve, so no curve lock is
// needed — but the user row is locked FOR UPDATE to serialize concurrent
// withdrawals. On-chain send is stubbed (§11): withdrawal is left PENDING.
export interface WithdrawResult {
  walletAddress: string;
  withdrawalId: string;
  amountSol: bigint;
  remainingSolBalance: bigint;
  status: string;
}

export async function withdraw(args: {
  walletAddress: string;
  amountSol: bigint;
}): Promise<WithdrawResult> {
  if (args.amountSol <= 0n) throw new TradeError("amountSol must be > 0");

  return prisma.$transaction(async (tx) => {
    const rows = await tx.$queryRaw<User[]>`
      SELECT * FROM users WHERE "walletAddress" = ${args.walletAddress} FOR UPDATE
    `;
    const user = rows[0];
    if (!user) throw new TradeError("user not found", 404);
    if (user.solBalance < args.amountSol) {
      throw new TradeError(
        `insufficient realized SOL: have ${user.solBalance}, requested ${args.amountSol}`,
        400
      );
    }

    const withdrawal = await tx.withdrawal.create({
      data: {
        walletAddress: user.walletAddress,
        userId: user.id,
        amountSol: args.amountSol,
        status: "PENDING", // on-chain send stubbed (§11)
      },
    });

    const updated = await tx.user.update({
      where: { id: user.id },
      data: { solBalance: { decrement: args.amountSol } },
    });
    await refreshUserCache(updated);

    // TODO: integrate Solana program — build/sign/send the withdraw tx, then
    // flip the Withdrawal status to CONFIRMED (or FAILED) on confirmation (§11).

    return {
      walletAddress: user.walletAddress,
      withdrawalId: withdrawal.id,
      amountSol: args.amountSol,
      remainingSolBalance: updated.solBalance,
      status: withdrawal.status,
    };
  });
}

// Re-export the leg primitives for the pick service (same lock/tx contract).
export { buyLeg, sellLeg };
