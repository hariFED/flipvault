// ─────────────────────────────────────────────────────────────────────────
// FlipVault backend — Express app wiring (§8).
//
// A vault backed by a single pump.fun-style bonding-curve AMM:
//   deposit -> autobuy (SOL->token)  •  pick -> settle (token->SOL)  •  withdraw
//
// Postgres (Prisma) = source of truth. Valkey = hot read path. The curve state
// is the only shared price-moving record, so per-user withdrawable amounts are
// DERIVED at read time, never rewritten on every trade (§3).
// ─────────────────────────────────────────────────────────────────────────
import express from "express";
import { prisma } from "./db.js";
import { valkey } from "./cache.js";
import { config } from "./config.js";
import { actionsRouter } from "./routes/actions.js";
import { readsRouter } from "./routes/reads.js";
import { chainRouter } from "./routes/chain.js";
import { errorHandler } from "./errors.js";
import { startPickScheduler, stopPickScheduler } from "./services/pick.js";

const app = express();
app.use(express.json());

// ── Health check ────────────────────────────────────────────────────────────
app.get("/health", async (_req, res, next) => {
  try {
    const pong = await valkey.ping();
    await prisma.$queryRaw`SELECT 1`;
    res.json({ ok: true, valkey: pong, postgres: "up", mint: config.tokenMint });
  } catch (e) {
    next(e);
  }
});

// ── Routes ────────────────────────────────────────────────────────────────
app.use(actionsRouter); // POST /deposit, /pick, /withdraw
app.use(readsRouter); //  GET  /price/:mint, /user/:wallet, /trades/:mint/recent, /candles
app.use(chainRouter); //   Solana + Privy stubs (§11)

// Final error handler.
app.use(errorHandler);

const server = app.listen(config.port, () => {
  console.log(`✓ FlipVault API on http://localhost:${config.port}`);
  // Optional scheduled picks (§8) — enable with PICK_AUTO_ENABLED=true.
  startPickScheduler();
});

// Graceful shutdown.
async function shutdown() {
  stopPickScheduler();
  server.close();
  await prisma.$disconnect();
  valkey.disconnect();
  process.exit(0);
}
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
