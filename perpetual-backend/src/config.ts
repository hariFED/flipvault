// Central configuration. Curve genesis constants default to pump.fun's
// canonical values but every one is overridable via env (§6, §12).
//
// UNITS (must stay consistent everywhere):
//   SOL    -> lamports,     1 SOL   = 1e9 lamports     (LAMPORTS_PER_SOL)
//   tokens -> base units,   1 token = 1e6 base units   (TOKEN_DECIMALS = 6)
import "dotenv/config";

export const LAMPORTS_PER_SOL = 1_000_000_000n; // 1e9
export const TOKEN_DECIMALS = 6;
export const TOKEN_UNIT = 1_000_000n; // 1e6

// Parse a BigInt from env, falling back to a default. Accepts plain integers.
function bigintEnv(key: string, fallback: bigint): bigint {
  const raw = process.env[key];
  if (raw === undefined || raw.trim() === "") return fallback;
  return BigInt(raw.trim());
}

function intEnv(key: string, fallback: number): number {
  const raw = process.env[key];
  if (raw === undefined || raw.trim() === "") return fallback;
  const n = Number(raw);
  if (!Number.isFinite(n)) throw new Error(`Invalid integer for ${key}: ${raw}`);
  return n;
}

// Helpers to express human amounts as base units.
const sol = (n: bigint) => n * LAMPORTS_PER_SOL;
const tok = (n: bigint) => n * TOKEN_UNIT;

// ── pump.fun canonical genesis constants (§6) ────────────────────────────────
export const config = {
  tokenMint: process.env.TOKEN_MINT ?? "FLIPVLT0000000000000000000000000000000000000",

  curve: {
    // virtualTokenReserves = 1,073,000,000 tokens
    virtualTokenReserves: bigintEnv("CURVE_VIRTUAL_TOKEN_RESERVES", tok(1_073_000_000n)),
    // virtualSolReserves = 30 SOL
    virtualSolReserves: bigintEnv("CURVE_VIRTUAL_SOL_RESERVES", sol(30n)),
    // realTokenReserves = 793,100,000 tokens
    realTokenReserves: bigintEnv("CURVE_REAL_TOKEN_RESERVES", tok(793_100_000n)),
    // realSolReserves starts at 0 (no real SOL has entered yet)
    realSolReserves: bigintEnv("CURVE_REAL_SOL_RESERVES", 0n),
    // totalSupply = 1,000,000,000 tokens
    totalSupply: bigintEnv("CURVE_TOTAL_SUPPLY", tok(1_000_000_000n)),
    // 1% fee
    feeBps: intEnv("CURVE_FEE_BPS", 100),
    // Graduation when real SOL reserves reach ~85 SOL
    graduationLamports: bigintEnv("CURVE_GRADUATION_LAMPORTS", sol(85n)),
  },

  // Pick scheduler: when enabled, fire a pick every PICK_INTERVAL_MS (§8).
  pick: {
    autoEnabled: (process.env.PICK_AUTO_ENABLED ?? "false").toLowerCase() === "true",
    intervalMs: intEnv("PICK_INTERVAL_MS", 30_000),
  },

  port: intEnv("PORT", 3000),
} as const;

export type CurveGenesis = typeof config.curve;
