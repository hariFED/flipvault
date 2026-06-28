// Consistent error shape + status codes (§12). Mounted as the final Express
// middleware so every route's thrown error funnels through one place.
import type { ErrorRequestHandler } from "express";
import { ZodError } from "zod";
import { sendJson } from "./json.js";
import { TradeError } from "./services/trade.js";
import { LockError } from "./cache.js";
import { AmmError } from "./amm.js";

export const errorHandler: ErrorRequestHandler = (err, _req, res, _next) => {
  // Validation errors -> 400 with field detail.
  if (err instanceof ZodError) {
    return sendJson(res, 400, {
      ok: false,
      error: "validation_error",
      issues: err.issues.map((i) => ({ path: i.path.join("."), message: i.message })),
    });
  }

  // Domain errors carry their own status.
  if (err instanceof TradeError) {
    return sendJson(res, err.status, { ok: false, error: err.message });
  }

  // AMM guard violations (dust, zero-out, etc.) are bad requests.
  if (err instanceof AmmError) {
    return sendJson(res, 400, { ok: false, error: err.message });
  }

  // Lock contention -> 503 so the caller can retry.
  if (err instanceof LockError) {
    return sendJson(res, 503, { ok: false, error: "curve busy, retry shortly" });
  }

  // Prisma unique-constraint (e.g. duplicate txSignature) -> 409.
  if ((err as { code?: string })?.code === "P2002") {
    return sendJson(res, 409, { ok: false, error: "duplicate (idempotency conflict)" });
  }

  console.error("unhandled error:", err);
  return sendJson(res, 500, { ok: false, error: "internal_error" });
};
