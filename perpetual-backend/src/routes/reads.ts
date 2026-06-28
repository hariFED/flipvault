// Read endpoints (§8). Served from the Valkey hot path where possible.
import { Router } from "express";
import { sendJson } from "../json.js";
import { config } from "../config.js";
import { prisma } from "../db.js";
import { readRecentTrades } from "../cache.js";
import { getCurveCache, getUserBalances } from "../repo.js";
import { withdrawableSol } from "../amm.js";
import { getCandles, TIMEFRAMES, type Timeframe } from "../services/candle.js";

export const readsRouter = Router();

// GET /price/:mint — current price + curve reserves (from Valkey).
readsRouter.get("/price/:mint", async (req, res, next) => {
  try {
    const c = await getCurveCache(req.params.mint);
    sendJson(res, 200, {
      mint: req.params.mint,
      price: c.price, // lamports per token base unit
      reserves: {
        virtualSolReserves: c.vSol,
        virtualTokenReserves: c.vTok,
        realSolReserves: c.rSol,
        realTokenReserves: c.rTok,
      },
      feeBps: c.feeBps,
      complete: c.complete,
    });
  } catch (e) {
    next(e);
  }
});

// GET /user/:wallet — balances + computed (derived) withdrawableSol + recent trades.
readsRouter.get("/user/:wallet", async (req, res, next) => {
  try {
    const wallet = req.params.wallet;
    const balances = await getUserBalances(wallet);
    if (!balances) return sendJson(res, 404, { error: "user not found" });

    // withdrawableSol is DERIVED at read time from the cached curve (§3, §7):
    // full sell-out quote of the token balance + realized free SOL.
    const curve = await getCurveCache();
    const withdrawable = withdrawableSol(
      {
        virtualSolReserves: curve.vSol,
        virtualTokenReserves: curve.vTok,
        realSolReserves: curve.rSol,
        realTokenReserves: curve.rTok,
        feeBps: curve.feeBps,
      },
      balances.solBalance,
      balances.tokenBalance
    );

    const trades = await prisma.trade.findMany({
      where: { walletAddress: wallet },
      orderBy: { createdAt: "desc" },
      take: 20,
    });

    sendJson(res, 200, {
      walletAddress: wallet,
      status: balances.status,
      solBalance: balances.solBalance, // realized, withdrawable 1:1
      tokenBalance: balances.tokenBalance,
      withdrawableSol: withdrawable, // live full sell-out quote + free SOL
      recentTrades: trades,
    });
  } catch (e) {
    next(e);
  }
});

// GET /trades/:mint/recent — recent buys/sells feed (from the Valkey stream).
readsRouter.get("/trades/:mint/recent", async (req, res, next) => {
  try {
    const count = Math.min(Number(req.query.count ?? 50) || 50, 200);
    const trades = await readRecentTrades(req.params.mint, count);
    sendJson(res, 200, { mint: req.params.mint, trades });
  } catch (e) {
    next(e);
  }
});

// GET /candles/:mint?timeframe= — OHLCV (scaffolded per §10).
readsRouter.get("/candles/:mint", async (req, res, next) => {
  try {
    const tf = (req.query.timeframe ?? "1m") as Timeframe;
    if (!TIMEFRAMES.includes(tf)) {
      return sendJson(res, 400, { error: `timeframe must be one of ${TIMEFRAMES.join(", ")}` });
    }
    const candles = await getCandles(req.params.mint, tf);
    // NOTE: live OHLCV aggregation is scaffolded only (§10) — returns [] for now.
    sendJson(res, 200, { mint: req.params.mint, timeframe: tf, candles, note: "aggregation scaffolded (§10)" });
  } catch (e) {
    next(e);
  }
});

// GET /candles (default mint) convenience.
readsRouter.get("/candles", async (req, res, next) => {
  try {
    const tf = (req.query.timeframe ?? "1m") as Timeframe;
    if (!TIMEFRAMES.includes(tf)) {
      return sendJson(res, 400, { error: `timeframe must be one of ${TIMEFRAMES.join(", ")}` });
    }
    const candles = await getCandles(config.tokenMint, tf);
    sendJson(res, 200, { mint: config.tokenMint, timeframe: tf, candles, note: "aggregation scaffolded (§10)" });
  } catch (e) {
    next(e);
  }
});
