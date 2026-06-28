// Unit tests for the pure AMM module (§12). Run with: npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  applyBuy,
  applySell,
  withdrawableSol,
  invariant,
  spotPrice,
  hasGraduated,
  ceilDiv,
  divToDecimalString,
  AmmError,
  type CurveReserves,
} from "./amm.js";

const LAMPORTS = 1_000_000_000n;
const TOK = 1_000_000n;

// Fresh pump.fun-style genesis reserves for each test.
function genesis(): CurveReserves {
  return {
    virtualSolReserves: 30n * LAMPORTS,
    virtualTokenReserves: 1_073_000_000n * TOK,
    realSolReserves: 0n,
    realTokenReserves: 793_100_000n * TOK,
    feeBps: 100,
  };
}

test("buy: charges 1% fee and credits tokens", () => {
  const r = genesis();
  const res = applyBuy(r, 1n * LAMPORTS);
  assert.equal(res.fee, 10_000_000n); // 1% of 1 SOL
  assert.equal(res.solInNet, 990_000_000n);
  assert.ok(res.tokensOut > 0n, "should receive tokens");
  // real reserves move by the net amounts
  assert.equal(res.reserves.realSolReserves, res.solInNet);
  assert.equal(res.reserves.realTokenReserves, r.realTokenReserves - res.tokensOut);
});

test("buy: never over-credits tokens (invariant does not decrease)", () => {
  const r = genesis();
  const res = applyBuy(r, 5n * LAMPORTS);
  // Because newVTok is rounded up, the post-trade product is >= the original k.
  assert.ok(invariant(res.reserves) >= invariant(r), "k must not shrink on buy");
});

test("sell: charges 1% fee and pays SOL", () => {
  const r = genesis();
  const sell = applySell(r, 1_000_000n * TOK); // sell 1M tokens
  assert.ok(sell.solOutGross > 0n);
  assert.equal(sell.fee, (sell.solOutGross * 100n) / 10_000n);
  assert.equal(sell.solOut, sell.solOutGross - sell.fee);
  assert.equal(sell.reserves.realTokenReserves, r.realTokenReserves + 1_000_000n * TOK);
});

test("buy then sell same tokens: round trip loses both fees (price moves against you)", () => {
  const r = genesis();
  const buy = applyBuy(r, 1n * LAMPORTS);
  const sell = applySell(buy.reserves, buy.tokensOut);
  // You put in 1 SOL, you must get strictly less back after two 1% fees + slippage.
  assert.ok(sell.solOut < 1n * LAMPORTS, "round trip must be lossy");
  // Specifically less than the net-of-one-fee buy-in.
  assert.ok(sell.solOut < buy.solInNet);
});

test("price increases after a buy and decreases after a sell", () => {
  const r = genesis();
  const p0 = spotPrice(r);
  const buy = applyBuy(r, 10n * LAMPORTS);
  const p1 = spotPrice(buy.reserves);
  assert.ok(Number(p1) > Number(p0), "buy should raise price");
  const sell = applySell(buy.reserves, buy.tokensOut);
  const p2 = spotPrice(sell.reserves);
  assert.ok(Number(p2) < Number(p1), "sell should lower price");
});

test("withdrawable: zero tokens returns just the free SOL", () => {
  const r = genesis();
  assert.equal(withdrawableSol(r, 5n * LAMPORTS, 0n), 5n * LAMPORTS);
});

test("withdrawable: equals the full sell-out quote of the held tokens", () => {
  const r = genesis();
  const buy = applyBuy(r, 2n * LAMPORTS);
  // After buying, quote selling the whole balance back at the post-buy state.
  const quote = withdrawableSol(buy.reserves, 0n, buy.tokensOut);
  const sell = applySell(buy.reserves, buy.tokensOut);
  assert.equal(quote, sell.solOut, "withdrawable must match an actual full sell");
});

test("withdrawable: adds realized free SOL on top of token quote", () => {
  const r = genesis();
  const buy = applyBuy(r, 2n * LAMPORTS);
  const withFree = withdrawableSol(buy.reserves, 7n * LAMPORTS, buy.tokensOut);
  const withoutFree = withdrawableSol(buy.reserves, 0n, buy.tokensOut);
  assert.equal(withFree - withoutFree, 7n * LAMPORTS);
});

test("two-leg pick ordering: SOL->token then full token->SOL settles to SOL", () => {
  // Wallet holds residual free SOL + tokens. Pick buys in first, then sells all.
  let r = genesis();
  const startTokens = 500_000n * TOK;
  const residualSol = 1n * LAMPORTS;

  // Leg 1: residual SOL -> token
  const leg1 = applyBuy(r, residualSol);
  r = leg1.reserves;
  const totalTokens = startTokens + leg1.tokensOut;

  // Leg 2: full token balance -> SOL
  const leg2 = applySell(r, totalTokens);
  r = leg2.reserves;

  assert.ok(leg2.solOut > 0n, "settlement yields SOL");
  // The wallet ends holding only SOL (its realized withdrawable).
  assert.equal(leg2.reserves.realTokenReserves, r.realTokenReserves);
});

test("graduation flips at the SOL threshold", () => {
  const r = genesis();
  assert.equal(hasGraduated(r, 85n * LAMPORTS), false);
  r.realSolReserves = 85n * LAMPORTS;
  assert.equal(hasGraduated(r, 85n * LAMPORTS), true);
});

test("rejects non-positive amounts", () => {
  const r = genesis();
  assert.throws(() => applyBuy(r, 0n), AmmError);
  assert.throws(() => applyBuy(r, -1n), AmmError);
  assert.throws(() => applySell(r, 0n), AmmError);
  assert.throws(() => applySell(r, -5n), AmmError);
});

test("rejects a sell that yields zero SOL out", () => {
  // A 1-base-unit token sell on genesis reserves is too small to move SOL.
  const r = genesis();
  assert.throws(() => applySell(r, 1n), AmmError);
});

test("ceilDiv and divToDecimalString behave", () => {
  assert.equal(ceilDiv(10n, 3n), 4n);
  assert.equal(ceilDiv(9n, 3n), 3n);
  assert.equal(ceilDiv(0n, 3n), 0n);
  assert.equal(divToDecimalString(1n, 2n, 2), "0.50");
  assert.equal(divToDecimalString(30n * LAMPORTS, 1_073_000_000n * TOK, 18).startsWith("0.0000"), true);
});
