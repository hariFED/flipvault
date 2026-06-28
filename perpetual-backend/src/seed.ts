// Seed script: initialize the SINGLETON bonding-curve state from the pump.fun
// genesis constants (§6) and warm the Valkey cache. Run with: npm run seed
//
// Idempotent: re-running resets the curve to genesis and clears game state.
import { prisma } from "./db.js";
import { valkey, writeCurveCache } from "./cache.js";
import { config } from "./config.js";
import { spotPrice } from "./amm.js";

async function main() {
  const mint = config.tokenMint;
  const g = config.curve;

  // Wipe game state so re-seeding is a clean genesis (dev convenience).
  await prisma.trade.deleteMany();
  await prisma.deposit.deleteMany();
  await prisma.withdrawal.deleteMany();
  await prisma.candle.deleteMany();
  await prisma.user.deleteMany();
  await prisma.bondingCurveState.deleteMany();

  const price = spotPrice({
    virtualSolReserves: g.virtualSolReserves,
    virtualTokenReserves: g.virtualTokenReserves,
    realSolReserves: g.realSolReserves,
    realTokenReserves: g.realTokenReserves,
    feeBps: g.feeBps,
  });

  await prisma.bondingCurveState.create({
    data: {
      tokenMint: mint,
      virtualSolReserves: g.virtualSolReserves,
      virtualTokenReserves: g.virtualTokenReserves,
      realSolReserves: g.realSolReserves,
      realTokenReserves: g.realTokenReserves,
      totalSupply: g.totalSupply,
      feeBps: g.feeBps,
      lastPrice: price,
      complete: false,
    },
  });

  // Warm the hot read path.
  await writeCurveCache(mint, {
    vSol: g.virtualSolReserves,
    vTok: g.virtualTokenReserves,
    rSol: g.realSolReserves,
    rTok: g.realTokenReserves,
    feeBps: g.feeBps,
    price,
    complete: false,
  });
  // Clear any stale per-user / trade-feed cache from a previous run.
  for (const key of await valkey.keys("user:*")) await valkey.del(key);
  await valkey.del(`trades:${mint}`);

  console.log(`✓ seeded bonding curve for mint ${mint}`);
  console.log(`  virtualSolReserves   = ${g.virtualSolReserves} lamports`);
  console.log(`  virtualTokenReserves = ${g.virtualTokenReserves} base units`);
  console.log(`  genesis spot price   = ${price} lamports/token-base-unit`);
}

main()
  .then(async () => {
    await prisma.$disconnect();
    valkey.disconnect();
  })
  .catch(async (e) => {
    console.error(e);
    await prisma.$disconnect();
    valkey.disconnect();
    process.exit(1);
  });
