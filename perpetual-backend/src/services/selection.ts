// Pure wallet-selection logic for picks (§8, §13.7). No DB/cache imports so it
// stays trivially unit-testable in isolation.
import { randomInt } from "node:crypto";

export interface Selectable {
  status: string; // "ACTIVE" | "INACTIVE"
  tokenBalance: bigint;
}

// Eligibility predicate (§8): status ACTIVE and a positive token balance.
export function isEligible(u: Selectable): boolean {
  return u.status === "ACTIVE" && u.tokenBalance > 0n;
}

// Choose one eligible element uniformly at random using a CSPRNG
// (crypto.randomInt — fair and unpredictable). Returns null if none eligible.
export function selectRandomEligible<T extends Selectable>(candidates: T[]): T | null {
  const eligible = candidates.filter(isEligible);
  if (eligible.length === 0) return null;
  return eligible[randomInt(eligible.length)];
}
