// ─────────────────────────────────────────────────────────────────────────
// Solana program + Privy integration — STUBS AND COMMENTS ONLY (§11).
//
// None of this calls the chain or Privy yet. The AMM/accounting logic works
// end-to-end against Postgres/Valkey without any of these. These handlers exist
// so the integration seams are visible and named.
// ─────────────────────────────────────────────────────────────────────────
import { Router } from "express";
import { sendJson } from "../json.js";

export const chainRouter = Router();

// POST /privy/wallet — create/link an embedded wallet for a user.
chainRouter.post("/privy/wallet", async (_req, res) => {
  // TODO: integrate Privy — generate or link the embedded wallet via the Privy
  // SDK, persist the resulting walletAddress + privyUserId on the User row.
  sendJson(res, 501, {
    ok: false,
    stub: "privy-wallet-creation",
    message: "TODO: integrate Privy embedded-wallet creation (§11)",
  });
});

// POST /solana/deposit-webhook — on-chain deposit detection entry point.
chainRouter.post("/solana/deposit-webhook", async (_req, res) => {
  // TODO: integrate Solana program — a Helius/Geyser webhook or program-log
  // subscription detects a vault deposit, then calls the SAME internal logic
  // that POST /deposit uses (credit + add to game + autobuy). Verify the
  // signature, de-dupe via txSignature (idempotency), then invoke deposit().
  sendJson(res, 501, {
    ok: false,
    stub: "solana-deposit-detection",
    message: "TODO: integrate Solana deposit listener -> deposit() (§11)",
  });
});

// POST /solana/withdraw-execute — build/sign/send the on-chain withdraw tx.
chainRouter.post("/solana/withdraw-execute", async (_req, res) => {
  // TODO: integrate Solana program — for a PENDING Withdrawal, build, sign, and
  // send the withdraw transaction via the vault program, then flip the
  // Withdrawal status to CONFIRMED (or FAILED) on confirmation, storing the
  // txSignature for reconciliation.
  sendJson(res, 501, {
    ok: false,
    stub: "solana-withdraw-execution",
    message: "TODO: integrate Solana withdraw execution (§11)",
  });
});
