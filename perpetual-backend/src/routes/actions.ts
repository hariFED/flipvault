// Action endpoints (§8): POST /deposit, /pick, /withdraw.
// Handlers are thin — validate input, call the service, serialize the result.
import { Router } from "express";
import { sendJson } from "../json.js";
import { depositSchema, pickSchema, withdrawSchema } from "../validation.js";
import { deposit, withdraw, TradeError } from "../services/trade.js";
import { pick } from "../services/pick.js";

export const actionsRouter = Router();

// POST /deposit — adds the wallet to the game and immediately autobuys
// (SOL -> token). Re-deposits add to the existing position (§2.1).
actionsRouter.post("/deposit", async (req, res, next) => {
  try {
    const input = depositSchema.parse(req.body);
    // TODO: integrate Solana program — on-chain deposit detection (Helius/Geyser
    // or program-log subscription) will call this same logic with the real
    // amount + txSignature (§11). For now both come from the request body.
    const result = await deposit(input);
    sendJson(res, result.idempotentReplay ? 200 : 201, { ok: true, deposit: result });
  } catch (e) {
    next(e);
  }
});

// POST /pick — select a RANDOM eligible wallet and settle it back to SOL (§8).
actionsRouter.post("/pick", async (req, res, next) => {
  try {
    const { mint } = pickSchema.parse(req.body ?? {});
    const result = await pick(mint);
    sendJson(res, 200, { ok: true, pick: result });
  } catch (e) {
    next(e);
  }
});

// POST /withdraw — debit realized free SOL and create a PENDING withdrawal.
actionsRouter.post("/withdraw", async (req, res, next) => {
  try {
    const input = withdrawSchema.parse(req.body);
    const result = await withdraw(input);
    sendJson(res, 201, { ok: true, withdrawal: result });
  } catch (e) {
    next(e);
  }
});

export { TradeError };
