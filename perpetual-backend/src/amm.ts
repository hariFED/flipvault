// ─────────────────────────────────────────────────────────────────────────
// Pure pump.fun-style constant-product bonding-curve math (§6, §7).
//
// THIS MODULE IS PURE: no DB, no cache, no I/O. Everything is BigInt (base
// units) so it is deterministic and unit-testable in isolation. Floats are
// never used for money. Token outputs round DOWN; results <= 0 are rejected.
//
// Invariant: k = virtualSolReserves * virtualTokenReserves, held constant
// across a swap. Fees are taken OUT separately and do not enter the reserves.
// ─────────────────────────────────────────────────────────────────────────

export const BPS_DENOMINATOR = 10_000n;

// The mutable curve state the math operates on. `real*` reserves track the
// actual SOL held / tokens left to sell (used for graduation + auditing); the
// `virtual*` reserves drive price.
export interface CurveReserves {
  virtualSolReserves: bigint; // lamports
  virtualTokenReserves: bigint; // token base units
  realSolReserves: bigint; // lamports
  realTokenReserves: bigint; // token base units
  feeBps: number;
}

export interface BuyResult {
  tokensOut: bigint; // tokens credited to the buyer (rounded down)
  fee: bigint; // lamports taken as fee
  solInNet: bigint; // lamports that actually entered the curve
  reserves: CurveReserves; // new reserves after the buy
}

export interface SellResult {
  solOut: bigint; // lamports paid to the seller (after fee)
  solOutGross: bigint; // lamports removed from the curve before fee
  fee: bigint; // lamports taken as fee
  reserves: CurveReserves; // new reserves after the sell
}

export class AmmError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AmmError";
  }
}

// k = vSol * vTok — the constant product.
export function invariant(r: CurveReserves): bigint {
  return r.virtualSolReserves * r.virtualTokenReserves;
}

// Spot price = vSol / vTok, in lamports per token base unit. Returned as a
// string so callers can persist it as Decimal without float rounding.
export function spotPrice(r: CurveReserves, decimals = 18): string {
  if (r.virtualTokenReserves <= 0n) throw new AmmError("zero token reserves");
  return divToDecimalString(r.virtualSolReserves, r.virtualTokenReserves, decimals);
}

// ── BUY: amountSolIn (lamports) -> tokensOut (base units) ────────────────────
// 1. fee = amountSolIn * feeBps / 10000
// 2. solInNet = amountSolIn - fee
// 3. newVSol = vSol + solInNet
// 4. newVTok = k / newVSol           (round UP so we never over-credit tokens)
// 5. tokensOut = vTok - newVTok
// 6. commit reserves
export function applyBuy(r: CurveReserves, amountSolIn: bigint): BuyResult {
  if (amountSolIn <= 0n) throw new AmmError("buy amount must be > 0");

  const fee = (amountSolIn * BigInt(r.feeBps)) / BPS_DENOMINATOR;
  const solInNet = amountSolIn - fee;
  if (solInNet <= 0n) throw new AmmError("buy amount too small after fee");

  const k = invariant(r);
  const newVSol = r.virtualSolReserves + solInNet;
  // Round the new token reserve UP (ceil) so tokensOut rounds DOWN — the curve
  // never gives out more tokens than the invariant strictly allows.
  const newVTok = ceilDiv(k, newVSol);
  const tokensOut = r.virtualTokenReserves - newVTok;
  if (tokensOut <= 0n) throw new AmmError("buy yields zero tokens");

  const reserves: CurveReserves = {
    ...r,
    virtualSolReserves: newVSol,
    virtualTokenReserves: newVTok,
    realSolReserves: r.realSolReserves + solInNet,
    realTokenReserves: r.realTokenReserves - tokensOut,
  };

  return { tokensOut, fee, solInNet, reserves };
}

// ── SELL: amountTokIn (base units) -> solOut (lamports) ──────────────────────
// 1. newVTok = vTok + amountTokIn
// 2. newVSol = k / newVTok            (round UP so solOutGross rounds DOWN)
// 3. solOutGross = vSol - newVSol
// 4. fee = solOutGross * feeBps / 10000
// 5. solOut = solOutGross - fee
// 6. commit reserves
export function applySell(r: CurveReserves, amountTokIn: bigint): SellResult {
  if (amountTokIn <= 0n) throw new AmmError("sell amount must be > 0");

  const k = invariant(r);
  const newVTok = r.virtualTokenReserves + amountTokIn;
  const newVSol = ceilDiv(k, newVTok); // ceil -> solOutGross rounds down
  const solOutGross = r.virtualSolReserves - newVSol;
  if (solOutGross <= 0n) throw new AmmError("sell yields zero SOL");

  const fee = (solOutGross * BigInt(r.feeBps)) / BPS_DENOMINATOR;
  const solOut = solOutGross - fee;
  if (solOut <= 0n) throw new AmmError("sell yields zero SOL after fee");

  const reserves: CurveReserves = {
    ...r,
    virtualSolReserves: newVSol,
    virtualTokenReserves: newVTok,
    realSolReserves: r.realSolReserves - solOutGross,
    realTokenReserves: r.realTokenReserves + amountTokIn,
  };

  return { solOut, solOutGross, fee, reserves };
}

// ── Withdrawable SOL (§7): full curve sell-out quote ─────────────────────────
// The SOL a user would actually receive selling their ENTIRE token balance
// through the curve at the current state, plus their realized free SOL.
//
//   1. newVTok = vTok + tokenBalance
//   2. newVSol = k / newVTok
//   3. grossSolOut = vSol - newVSol
//   4. fromTokens = grossSolOut * (1 - feeBps/10000)
//   5. total = solBalance + fromTokens
//
// NOTE (§7): this is a per-user quote that assumes the user sells FIRST, so the
// sum of all users' withdrawable amounts can exceed the real SOL in the vault.
// This is intended pump.fun behavior — NO capping or pro-rating is applied.
export function withdrawableSol(
  r: CurveReserves,
  solBalance: bigint,
  tokenBalance: bigint
): bigint {
  if (tokenBalance <= 0n) return solBalance;

  const k = invariant(r);
  const newVTok = r.virtualTokenReserves + tokenBalance;
  const newVSol = ceilDiv(k, newVTok);
  const grossSolOut = r.virtualSolReserves - newVSol;
  if (grossSolOut <= 0n) return solBalance;

  const fee = (grossSolOut * BigInt(r.feeBps)) / BPS_DENOMINATOR;
  const fromTokens = grossSolOut - fee;
  return solBalance + (fromTokens > 0n ? fromTokens : 0n);
}

// Has the curve graduated? (real SOL reserves reached the threshold.)
export function hasGraduated(r: CurveReserves, graduationLamports: bigint): boolean {
  return r.realSolReserves >= graduationLamports;
}

// ── BigInt helpers ───────────────────────────────────────────────────────────

// Ceiling division for positive bigints.
export function ceilDiv(a: bigint, b: bigint): bigint {
  if (b <= 0n) throw new AmmError("division by zero");
  if (a <= 0n) return 0n;
  return (a + b - 1n) / b;
}

// Render a/b as a fixed-precision decimal string (no float). Used for prices.
export function divToDecimalString(a: bigint, b: bigint, decimals: number): string {
  if (b === 0n) throw new AmmError("division by zero");
  const neg = a < 0n !== b < 0n;
  const aa = a < 0n ? -a : a;
  const bb = b < 0n ? -b : b;
  const intPart = aa / bb;
  let rem = aa % bb;
  if (decimals <= 0) return `${neg && intPart !== 0n ? "-" : ""}${intPart}`;
  let frac = "";
  for (let i = 0; i < decimals; i++) {
    rem *= 10n;
    frac += (rem / bb).toString();
    rem %= bb;
  }
  const sign = neg && (intPart !== 0n || rem !== 0n || /[1-9]/.test(frac)) ? "-" : "";
  return `${sign}${intPart}.${frac}`;
}
