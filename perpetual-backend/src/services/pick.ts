// ─────────────────────────────────────────────────────────────────────────
// Pick service (§8, §13.7). The random settlement that drives the sell side.
//
// Select a RANDOM eligible wallet (status ACTIVE, tokenBalance > 0) using
// crypto.randomInt for fair, unpredictable selection, then FULLY liquidate it:
//   leg 1: if it holds residual free SOL, swap that SOL -> token first;
//   leg 2: swap the FULL token balance token -> SOL.
// The realized SOL becomes the wallet's withdrawable solBalance; the wallet
// drops out of the eligible pool (tokenBalance -> 0) until it deposits again.
//
// No partial-liquidation logic — the SOL out is whatever the AMM yields. Both
// legs run inside the SAME locked transaction (§9) so prices are coherent and
// no other trade interleaves between them.
// ─────────────────────────────────────────────────────────────────────────
import type { User } from "@prisma/client";
import { config } from "../config.js";
import { hasGraduated, spotPrice } from "../amm.js";
import { saveCurve, toReserves } from "../repo.js";
import { buyLeg, sellLeg, TradeError, withCurveLock } from "./trade.js";
import { isEligible, selectRandomEligible } from "./selection.js";

// Pure selection helpers live in ./selection.ts (kept import-light for tests).
export { isEligible, selectRandomEligible } from "./selection.js";

export interface PickResult {
  walletAddress: string;
  tokensLiquidated: bigint;
  residualSolBoughtIn: bigint;
  solRealized: bigint;
  solBalance: bigint;
  price: string;
}

export async function pick(mint = config.tokenMint): Promise<PickResult> {
  return withCurveLock(mint, async ({ tx, row, pc }) => {
    if (row.complete) throw new TradeError("curve has graduated; trading is closed", 409);

    // Candidate pool: ACTIVE wallets currently holding tokens.
    const candidates = await tx.user.findMany({
      where: { status: "ACTIVE", tokenBalance: { gt: 0 } },
      select: { id: true, status: true, tokenBalance: true },
    });
    const chosen = selectRandomEligible(candidates);
    if (!chosen) throw new TradeError("no eligible wallets to pick", 409);

    // Lock the chosen wallet row FOR UPDATE and re-read fresh balances.
    const lockedRows = await tx.$queryRaw<User[]>`
      SELECT * FROM users WHERE id = ${chosen.id} FOR UPDATE
    `;
    const user = lockedRows[0];
    if (!user || !isEligible(user)) throw new TradeError("chosen wallet no longer eligible", 409);

    let reserves = toReserves(row);
    let residualSolBoughtIn = 0n;
    let tokenBalance = user.tokenBalance;

    // Leg 1 (ordering rule §2.1): residual free SOL -> token, FIRST.
    if (user.solBalance > 0n) {
      const buy = await buyLeg({ tx, pc }, reserves, user, mint, user.solBalance, "PICK_LIQUIDATION");
      reserves = buy.reserves;
      residualSolBoughtIn = user.solBalance;
      tokenBalance += buy.tokensOut;
    }

    // Leg 2: full token balance -> SOL.
    const sell = await sellLeg({ tx, pc }, reserves, user, mint, tokenBalance, "PICK_LIQUIDATION");
    reserves = sell.reserves;

    // Wallet ends holding only its realized SOL; drops out of the pool.
    const updated = await tx.user.update({
      where: { id: user.id },
      data: { solBalance: sell.solOut, tokenBalance: 0n },
    });
    pc.users.push(updated);

    const complete = hasGraduated(reserves, config.curve.graduationLamports);
    await saveCurve(tx, row.id, mint, reserves, complete);

    return {
      walletAddress: user.walletAddress,
      tokensLiquidated: tokenBalance,
      residualSolBoughtIn,
      solRealized: sell.solOut,
      solBalance: updated.solBalance,
      price: spotPrice(reserves),
    };
  });
}

// ── Pick scheduler (§8: configurable cadence) ────────────────────────────────
let timer: NodeJS.Timeout | null = null;

export function startPickScheduler(): void {
  if (!config.pick.autoEnabled || timer) return;
  console.log(`✓ pick scheduler enabled — every ${config.pick.intervalMs}ms`);
  timer = setInterval(async () => {
    try {
      const r = await pick();
      console.log(`auto-pick: liquidated ${r.walletAddress} for ${r.solRealized} lamports`);
    } catch (e: any) {
      // "no eligible wallets" is expected when the pool is empty — log quietly.
      if (e?.message !== "no eligible wallets to pick") console.error("auto-pick error:", e?.message);
    }
  }, config.pick.intervalMs);
}

export function stopPickScheduler(): void {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}
