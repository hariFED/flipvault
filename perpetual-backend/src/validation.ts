// Request validation (zod). Money fields arrive as strings of integer base
// units (lamports for SOL) and are validated as positive integers, then the
// route converts them to BigInt. Never parse money as a float (§12).
import { z } from "zod";

// A positive integer expressed as a decimal string (base units). Accepts a
// JS number too for convenience, but rejects anything non-integer / <= 0.
export const positiveBaseUnits = z
  .union([z.string(), z.number()])
  .transform((v) => String(v).trim())
  .refine((s) => /^[0-9]+$/.test(s), { message: "must be a non-negative integer string (base units)" })
  .refine((s) => BigInt(s) > 0n, { message: "must be > 0" })
  .transform((s) => BigInt(s));

const walletAddress = z.string().min(1).max(128);

export const depositSchema = z.object({
  walletAddress,
  // lamports. On-chain deposit detection is stubbed (§11), so the amount and
  // signature are supplied by the caller for now.
  amountSol: positiveBaseUnits,
  txSignature: z.string().min(1).max(200), // idempotency key (§9)
  privyUserId: z.string().min(1).optional(),
  mint: z.string().min(1).optional(),
});

export const withdrawSchema = z.object({
  walletAddress,
  amountSol: positiveBaseUnits, // lamports
});

export const pickSchema = z.object({
  mint: z.string().min(1).optional(),
});

export type DepositInput = z.infer<typeof depositSchema>;
export type WithdrawInput = z.infer<typeof withdrawSchema>;
