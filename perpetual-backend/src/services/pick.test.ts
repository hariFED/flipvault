// Unit tests for random eligible-wallet selection (§12).
import { test } from "node:test";
import assert from "node:assert/strict";
import { isEligible, selectRandomEligible } from "./selection.js";

type Candidate = { status: "ACTIVE" | "INACTIVE"; tokenBalance: bigint; tag: string };

test("isEligible requires ACTIVE status and a positive token balance", () => {
  assert.equal(isEligible({ status: "ACTIVE", tokenBalance: 1n }), true);
  assert.equal(isEligible({ status: "ACTIVE", tokenBalance: 0n }), false);
  assert.equal(isEligible({ status: "INACTIVE", tokenBalance: 5n }), false);
});

test("selectRandomEligible filters out ineligible wallets", () => {
  const candidates: Candidate[] = [
    { status: "ACTIVE", tokenBalance: 0n, tag: "no-tokens" },
    { status: "INACTIVE", tokenBalance: 100n, tag: "inactive" },
    { status: "ACTIVE", tokenBalance: 50n, tag: "ok-a" },
    { status: "ACTIVE", tokenBalance: 999n, tag: "ok-b" },
  ];
  const ok = new Set(["ok-a", "ok-b"]);
  // Run many draws — every pick must be one of the eligible wallets.
  for (let i = 0; i < 500; i++) {
    const chosen = selectRandomEligible(candidates);
    assert.ok(chosen, "should pick someone");
    assert.ok(ok.has(chosen!.tag), `picked ineligible wallet: ${chosen!.tag}`);
  }
});

test("selectRandomEligible returns null when no one is eligible", () => {
  const candidates: Candidate[] = [
    { status: "ACTIVE", tokenBalance: 0n, tag: "a" },
    { status: "INACTIVE", tokenBalance: 10n, tag: "b" },
  ];
  assert.equal(selectRandomEligible(candidates), null);
  assert.equal(selectRandomEligible([]), null);
});

test("selectRandomEligible can reach every eligible wallet (no dead options)", () => {
  const candidates: Candidate[] = [
    { status: "ACTIVE", tokenBalance: 1n, tag: "a" },
    { status: "ACTIVE", tokenBalance: 1n, tag: "b" },
    { status: "ACTIVE", tokenBalance: 1n, tag: "c" },
  ];
  const seen = new Set<string>();
  for (let i = 0; i < 1000; i++) seen.add(selectRandomEligible(candidates)!.tag);
  assert.deepEqual([...seen].sort(), ["a", "b", "c"]);
});
