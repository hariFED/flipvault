I have everything I need to synthesize this. The research and adversarial verdicts are comprehensive and current. Let me produce the report.

# FlipVault Path B (Private Perpetual) on Arcium — Feasibility Report

## 1. Go / No-Go Verdict

**Verdict: GO for a DEVNET build NOW. CONDITIONAL-GO for a Mainnet-Alpha launch with explicit "alpha infra" risk disclosure and capped exposure. NO-GO (yet) for trustless, amount-hidden SOL custody at production scale.**

Confidence: **High** that Path B is buildable and demonstrable on devnet today. **Medium** that it survives a real-money mainnet launch in the near term. **Medium-to-low** on the one piece you should NOT promise yet: hiding deposit/withdraw *amounts*.

The three load-bearing questions resolve like this:

- **Is Arcium usable today to build a confidential Anchor app end-to-end?** — **YES, confirmed and un-refuted.** The toolchain is openly installable (no waitlist), the Hello World lifecycle works, and the `arcium-hq/examples` repo contains real (non-stub) Arcis confidential code and was pushed within a day of this research [v-usable; docs.arcium.com/developers/installation; github.com/arcium-hq/examples]. Devnet (cluster offset 456) is the correct, supported target right now.
- **Does a confidential flip fit a 30s tick?** — **PARTIAL → effectively YES for one flip per 30s, but with no SLA.** Production data from Umbra (a live Arcium app) puts a single confidential op at "a few seconds" end-to-end; per-request crypto overhead is ~1s and now cached away [v-latency; sdk.umbraprivacy.com/concepts/how-umbra-works; arcium.com/articles/shared-rescue-keys-caching-optimization]. One flip every 30s sits far under the network's aggregate throughput. **What "partial" blocks:** there is no published latency guarantee, mainnet is permissioned-alpha with no liveness SLA, and a *naive per-player-per-tick* design would exceed the network's ~2.3 computations/sec aggregate capacity. The loop must be redesigned as one batched computation per tick (see §3–4).
- **Can you hide deposit/withdraw amounts today?** — **PARTIAL → effectively NO for the near term.** Token-2022 Confidential Transfers depend on the ZK ElGamal Proof program, which was **disabled on mainnet after a June 2025 forged-proof bug** and, as of this research, is in a re-enablement window via Agave 4.0 but **not confirmed activated** [v-custody; solana.com/news/post-mortem-june-25-2025; github.com/solana-program/token-2022/issues/657]. Arcium's C-SPL bridge is roadmap/devnet-stage and **absent from current dev docs**. **What "partial" blocks:** you cannot promise amount-hidden SOL custody as a launch feature. You *can* hide internal balances/positions/P&L/curve state inside the MXE today.

**Bottom line:** Build and soak Path B on Arcium devnet now. Ship internal-state privacy (balances, positions, P&L, curve) as the core deliverable. Treat amount-hidden custody as a later phase gated on external infra. Keep the existing fully-on-chain FlipVault as a permanent fallback mode.

---

## 2. Arcium in Plain Terms

**What it is:** Arcium is a Solana-native decentralized confidential-computing network. Instead of a smart contract running math on plaintext, the math runs across a *cluster* of MPC nodes ("Arx nodes") that operate on *secret-shared* (encrypted) data — no single node, nor any observer, sees the plaintext. The framing is "mechanism public, data private," which is exactly Path B's requirement.

**The dev model (it's Anchor + an extra crate):**
- You keep a normal Anchor program but swap `#[program]` for `#[arcium_program]`.
- You write the confidential logic in **Arcis** (a Rust DSL) inside an `encrypted-ixs/` crate. Functions in an `#[encrypted]` module marked `#[instruction]` compile to MPC circuits.
- Each confidential operation is **asynchronous and two-transaction**: your program CPIs `queue_computation(...)` (tx 1), the cluster runs the circuit off-chain, then invokes your `#[arcium_callback]` handler with the result (tx 2). You verify a BLS signature on the output (`verify_output(...)`) before persisting it.
- Encrypted state lives in **ordinary Anchor accounts** as ciphertext byte arrays (`[[u8;32]; N]` + a `u128` nonce). Two ownership types: `Enc<Shared, T>` (client + MXE can decrypt — for per-player data the player reads) and `Enc<Mxe, T>` (only the cluster can decrypt — for shared protocol state like the curve).
- Toolchain: `arcup` installs the CLI; `arcium init / build / test / localnet / deploy`. `arcium localnet` spins up a full local validator + MPC nodes + callback server. Client encryption uses x25519 ECDH + a Rescue cipher, documented for the TS SDK `@arcium-hq/client`.

**Current status (mid-2026):**
- Public testnet since May 2025; **Mainnet Alpha live since Feb 2026** — but explicitly **permissioned** ("a curated set of compute nodes... focused on reliability and usability rather than full decentralization") [Messari; The Block]. "Run a node" / "Stake $ARX" are still "coming soon."
- 1M+ confidential computations / ~200k/day by June 2026 (~80% from a single app, ZINC) — real usage, but a young network with thin headroom [solanacompass.com].
- $ARX token launched ~June 22 2026 (~$400M FDV). Well-backed (Coinbase Ventures, Greenfield; angels include Anatoly Yakovenko, Mert Mumtaz).
- MPC backends: **Cerberus** (dishonest-majority, secure if ≥1 honest node, MAC-authenticated shares, aborts on cheating) and **Manticore** (honest-but-curious + trusted dealer, faster). **Use Cerberus** for money-handling math.
- **Audit posture is "claimed but unverified"** in this research — request the actual reports before trusting real funds.
- **Windows caveat:** the toolchain is macOS/Linux only. This dev box is Windows 10 → **use WSL2.**

---

## 3. Recommended Path-B Architecture

The clean split is: **selection and custody boundaries are public; the flip math and internal state are confidential.**

### What stays PUBLIC on-chain (unchanged from Path A)
- **Box identities** — pseudonymous box PDAs in a public registry. No balances revealed.
- **VRF selection + proof** — keep your existing on-chain VRF (ORAO or Switchboard) picking one box index from the public roster. The proof, the round, and the selected box pubkey are all on-chain-verifiable. **Do NOT use Arcium's internal ArcisRNG for selection** — it's hidden in MPC and is only tamper-*evident* (BLS), not publicly *auditable as fair*. That would silently break your "selection stays publicly verifiable" requirement [vrf research].
- **Custody vault** — the SOL deposit/withdraw boundary. The vault address and the *fact* a deposit/withdraw happened are public regardless of any crypto.
- **Public events** — `BoxSelected{round, index, box}` and `FlipSettled{round}`. These leak that a flip happened, not the amounts.

### What runs CONFIDENTIALLY in Arcium
- **Shared bonding-curve state** as `Enc<Mxe, Curve{sol_reserve, perp_reserve}>` — only the cluster can decrypt. This is the single serialization point for all flips.
- **Per-box state** as `Enc<Shared, Box{sol, perp, in_perp, cost_basis}>` — one PDA per box (NOT one giant array; Arcis has no `Vec`/dynamic indexing, and the ~1232-byte callback output cap forbids re-emitting all N boxes). Sealed to the player so they (and only they) can read their own balance/P&L.
- **The flip math** — constant-product buy/sell, 10% fee, P&L realization, balance + curve update — all branches computed obliviously so the buy-vs-sell path leaks nothing.

### Per-tick data flow (text diagram)

```
  EVERY 30s (keeper-driven, but treat as SOFT cadence)
  ┌──────────────────────────────────────────────────────────────────┐
  │ 1. PUBLIC SELECTION (on-chain, verifiable)                         │
  │    keeper -> VRF (ORAO/Switchboard) -> randomness + proof          │
  │    selected_index = rand % active_box_count   <-- PUBLIC           │
  │    emit BoxSelected{round, selected_index, box_pubkey}             │
  └───────────────────────────────┬──────────────────────────────────┘
                                   │  selected_index is PLAINTEXT
                                   v  (fine — already public)
  ┌──────────────────────────────────────────────────────────────────┐
  │ 2. QUEUE CONFIDENTIAL FLIP (tx 1: CPI queue_computation)           │
  │    args = [ public: selected_index, fee_bps ]                      │
  │           [ Enc<Mxe, Curve>      ciphertext + nonce ]              │
  │           [ Enc<Shared, Box>     ciphertext + nonce + pubkey ]     │
  │    set box.pending = true   (lock; refuse re-select/withdraw)      │
  └───────────────────────────────┬──────────────────────────────────┘
                                   │
                                   v  OFF-CHAIN MPC (Cerberus cluster)
  ┌──────────────────────────────────────────────────────────────────┐
  │ 3. CONFIDENTIAL FLIP CIRCUIT (Arcis, runs on secret shares)        │
  │    if box.in_perp { SELL perp->sol, realize P&L, -10% fee }        │
  │    else           { BUY  sol->perp, -10% fee, set cost_basis }     │
  │    update curve reserves (x*y=k). Re-encrypt:                      │
  │      curve -> Enc<Mxe>, box -> Enc<Shared(player)>                 │
  └───────────────────────────────┬──────────────────────────────────┘
                                   │
                                   v  (cluster invokes your callback)
  ┌──────────────────────────────────────────────────────────────────┐
  │ 4. CALLBACK (tx 2: #[arcium_callback])                            │
  │    verify_output(BLS) -> write new curve ct + box ct back          │
  │    GUARD: reject if curve nonce changed since queue (stale guard)  │
  │    box.pending = false; emit FlipSettled{round}  (no amounts)      │
  └──────────────────────────────────────────────────────────────────┘

  PLAYER READS OWN STATE (off the hot path, anytime):
    player derives x25519 shared secret with MXE -> RescueCipher.decrypt(box.ct)
    -> sees ONLY their own balance / position / P&L. Others cannot.
```

### Custody-amount privacy: the honest ceiling
- **What's achievable today:** internal balances/positions/P&L/curve are fully hidden inside the MXE.
- **What is NOT achievable today as a launch feature:** hiding the *amount* of SOL entering/leaving the vault. On-chain SOL transfers expose the amount. True amount-hiding needs either (a) Token-2022 Confidential Transfers (currently disabled on mainnet, re-enable unconfirmed) or (b) a shielded pool (Umbra-style) or Arcium C-SPL (roadmap/devnet, not in dev docs) [v-custody; custody research].
- **Recommended default (works now):** user sends plain SOL to the program vault (amount visible at the custody boundary — accept this), and the MXE credits an *encrypted internal balance*. Withdraw reverses it. This delivers "balances/positions/P&L private, selection public" without depending on any disabled/unshipped infra. Add amount-hiding as an optional later upgrade.
- **Inevitable leaks even with a shielded pool:** vault address, the fact and timing/slot of a deposit/withdraw, the fee-payer identity, and an anonymity set bounded by *concurrent depositor count* — not by the crypto. Do not promise unconditional anonymity.

---

## 4. The 30s-Tick Latency Reality

**Does one flip fit in 30s? Yes — comfortably, but without a guarantee.**

- A single confidential op is empirically "a few seconds" in production (Umbra) [v-latency; sdk.umbraprivacy.com]. Realistic end-to-end budget for one small fixed-point flip circuit: ~3–12s = MPC round (sub-second to ~2s) + two Solana confirmations (~1–3s each) + cached DH key (~0 after first use).
- The flip is a *small, fixed-shape* circuit (a few muls, one division, one comparison), which is the cheap end of Arcium's cost spectrum and fits Arcis constraints (no `Vec`/loops, ~1232-byte output cap).

**The real risk is NOT single-flip latency — it is throughput-under-load and the absence of an SLA:**

1. **Never submit one computation per player per tick.** Network aggregate is ~2.3 comp/sec; N players → N/30 comp/sec, which dies around N≈50–100. **The design must be ONE batched computation per tick** that mutates only the selected box + the shared curve. Throughput then becomes O(1) in N. This is already baked into §3.
2. **No liveness SLA on permissioned alpha.** Arcium's own dark-pool demo had nodes crash and back up under load. A stuck node stalls the tick.
3. **The shared curve is a hard serialization point.** Every flip read-modify-writes the same `Enc<Mxe, Curve>` ciphertext, so flips must be serial. One-box-per-30s naturally enforces this — but add a curve-nonce stale-guard in the callback so a late/duplicate callback can't clobber newer state.

**How the loop changes — treat 30s as a SOFT cadence, decouple selection from settlement:**

- **Selection** runs on its own public 30s tick and never blocks on Arcium.
- **The flip** is an async job keyed to the selected box. Do not block the next tick on its finalization; the on-chain callback ordering serializes curve updates so a late flip still applies correctly.
- **Lock + timeout + retry:** set `box.pending` on queue; if not finalized within ~25–28s, resubmit; on repeated failure, "skip round" or roll selection into the next tick. The game must never deadlock on one un-acked computation. Set the computation's `valid_before` to the tick deadline so stale work auto-expires.
- **If measured p99 exceeds the window** on devnet/alpha, lengthen the tick (45–90s) or pipeline (pre-queue tick N+1 while N finalizes). **Treat the tick interval as tunable until you have measured numbers** — every published latency figure traces back to vendor/marketing sources; there is no independent benchmark [v-latency caveats].

---

## 5. Confidential Custody — What's Hideable vs What Leaks

| Aspect | Hideable today? | Mechanism | Notes |
|---|---|---|---|
| Per-box SOL/PERP balance | **Yes** | `Enc<Shared,T>` in MXE | Player reads own via re-encryption |
| Position / in-perp flag | **Yes** | MXE state | |
| Realized/unrealized P&L | **Yes** | MXE state | |
| Bonding-curve reserves/price | **Yes** | `Enc<Mxe,T>` | Only cluster decrypts |
| Deposit/withdraw **amount** | **No (near-term)** | needs Token-2022 CT or shielded pool / C-SPL | CT disabled on mainnet; C-SPL not in dev docs |
| Vault/program address | **No** | inherent | Public always |
| Fact + timing of deposit/withdraw | **No** | inherent | A tx hit the vault at slot S |
| Linkage deposit↔withdraw | Only with a shielded pool | Umbra-style note/nullifier | Bounded by anonymity-set size |
| Funding-wallet history | **No** (user opsec) | — | Recommend fresh/relayer fee-payer |

**Key facts that gate amount-hiding:**
- **Token-2022 Confidential Transfers hide transfer/resting amounts, NOT deposit/withdraw amounts** — `Deposit` (public→confidential) and `Withdraw` (confidential→public) both move a *plaintext* u64 [custody research; solana-program.com/docs/confidential-balances]. So even when re-enabled, CT alone does not hide the on/off-ramp.
- **CT is currently disabled on mainnet AND devnet** since the June 2025 ZK-ElGamal forged-proof bug. Re-enable is bundled into Agave 4.0 (mainnet-beta recommended 2026-05-18) but **the feature-gate activation epoch is unconfirmed** and issue #657 is still open [v-custody]. **Verify `explorer.solana.com/feature-gates` and issue #657 before designing around it.**
- **Arcium C-SPL** (the productized Token-2022↔MPC bridge) was roadmapped to devnet in "Phase 2" and is **absent from `docs.arcium.com/llms.txt`** — not shipped as documented dev API [v-custody].
- **Do not mix Token-2022 ElGamal ciphertexts with Arcium MPC secret-shares** — there's no primitive to compute on one from the other without a decrypt-and-recommit bridge every tick. Keep all value-bearing state in ONE trust domain (Arcium).

**Recommended stance for the owner-facing copy:** "Your balance, position, P&L, and the curve are private. Selection is public and provably fair. Deposit/withdraw *amounts* are public at the vault boundary in v1; amount-hidden custody is a planned upgrade pending Solana confidential-transfer re-enablement and Arcium C-SPL maturity."

---

## 6. Alternatives — Is Arcium the Right Tool?

**Yes. Arcium is the best — and effectively the only — Solana-native fit for true Path B**, because Path B's defining need is *encrypted shared state + general confidential computation over multiple parties' encrypted data* (one shared private curve, many private boxes, flip math on ciphertext). Nothing else on Solana does that today.

| Option | Fit for Path B | Why |
|---|---|---|
| **Arcium (MPC)** | **Best / only** | Encrypted shared state + programmable confidential math; live mainnet-alpha; real DeFi/game analogs (Poker, Darklake AMM, dark-pool, sealed-bid auctions) |
| Token-2022 Confidential Transfers | No (alone) | Only hides amounts/balances of a token you hold; no programmable curve math; ZK proof program disabled on mainnet 2025; usage ~zero |
| Light Protocol | No | Pivoted to ZK state *compression*, not confidentiality |
| Elusiv / shielded pools (Privacy Cash, Hush, Umbra) | Partial | Private *transfers* only, no programmable shared state; Umbra useful as a custody-privacy *reference*, not for the curve |
| TEE (MagicBlock PER, AWS Nitro) | Interim only | Fastest + always-on, but hardware-trust based; 2026 research actively erodes TEE guarantees |
| FHE (Zama/Fhenix) | No (now) | Right long-term shape but EVM-centric, largely testnet, slower than MPC |

**Recommendation:** Adopt Arcium for the confidential flip math + private balances. Keep your existing public VRF (ORAO/Switchboard) for selection. Keep the fully-on-chain transparent FlipVault as a permanent opt-out/fallback mode. If time-to-ship dominates and you can accept hardware trust, a MagicBlock TEE rollup is a simpler *interim* path — but it is a strictly weaker trust model, so not for the long-term money path.

---

## 7. Risk Register

| # | Risk | Severity | Fallback / Mitigation |
|---|---|---|---|
| 1 | **MPC cluster liveness gates the loop** — permissioned alpha, no SLA, observed crashes under load | High | Keeper timeout + `valid_before` expiry + retry; "skip round" policy so the game never deadlocks; keep transparent FlipVault as fallback mode; gate mainnet-money on Arcium decentralization (staking/slashing GA) |
| 2 | **Per-player-per-tick design exceeds network throughput** (~2.3 comp/s aggregate) | Critical | Architect ONE batched computation per tick (selected box + shared curve only); O(1) in N. Already in §3 |
| 3 | **Amount-hidden custody depends on disabled/unshipped infra** (Token-2022 CT disabled; C-SPL not in docs) | High | Ship internal-state privacy first; public deposit/withdraw amounts in v1; verify feature-gate + #657 before promising amount-hiding; design MXE-internal-balance as default |
| 4 | **Shared curve ciphertext race** — stale callback clobbers newer state | High | One-box-per-tick serializes; add curve-nonce/version stale-guard in callback (reject if changed since queue) |
| 5 | **f64/fixed-point precision drift** vs the public integer curve; range clamp at [-2^75, 2^75) | High | Prototype exact formula in Arcis at MILESTONE 0; prefer scaled-integer + `field_division` over general f64 division; pick lamport scaling to stay in range; reconcile conservation via periodic reveal/audit |
| 6 | **No verified audit reports for Arcium/Cerberus** | Medium | Request actual reports before real funds; independent review of dishonest-majority + abort model against your threat model |
| 7 | **Latency has no SLA; all numbers are vendor-sourced** | Medium | Benchmark p50/p95/p99 on devnet under simulated load before committing cadence; treat 30s as tunable |
| 8 | **Rust/WASM client crypto path unverified** — Dioxus/WASM frontend + Rust keeper, but documented SDK is TS (`@arcium-hq/client`) | Medium | Spike early: either wasm-bindgen interop to the TS package, or reimplement x25519 + Rescue in Rust/WASM. Affects architecture — resolve at MILESTONE 0 |
| 9 | **Wrong randomness surface** — using ArcisRNG for selection breaks public verifiability | Medium | Keep public VRF (ORAO/Switchboard) for selection; feed only `selected_index` (public plaintext) into the circuit |
| 10 | **Metadata leakage** — which box flipped, timing, treasury growth can deanonymize | Medium | Keep treasury balance encrypted; avoid amount-correlated events; consider fixed-size callbacks; threat-model the public timing channel |
| 11 | **Cerberus is security-with-abort** — a malicious/offline node can DoS a flip | Low | Make the loop restart-safe; an aborted flip re-queues deterministically without state corruption |
| 12 | **Fast-moving 0.x SDK** (v0.5.1 already mandated `verify_output`, breaking v0.4) | Medium | Pin SDK versions; isolate Arcium behind an adapter; track migration guides |

---

## 8. Phased Build Plan

**Environment prerequisite:** set up WSL2/Linux first (Windows not supported by the toolchain).

### MILESTONE 0 — De-risk spike (proves the riskiest assumptions FIRST)
Goal: prove the flip *math* and the *latency/precision* before building any game scaffolding. Do this on `arcium localnet` then devnet (offset 456).
- **0a. Curve-math-in-Arcis spike (riskiest):** implement the constant-product buy/sell + 10% fee + P&L in an Arcis `#[instruction]`, operating on `Enc<Mxe, Curve>` + `Enc<Shared, Box>`. **Verify numeric precision against the existing transparent Rust implementation** across realistic lamport-scale inputs. Decide f64-fixed-point vs scaled-integer here. (Risks #5)
- **0b. End-to-end latency benchmark:** queue→MPC→callback→finalization, hundreds of runs, capture p50/p95/p99 on devnet under simulated concurrent load. Confirm one flip < tick. (Risks #7, #1)
- **0c. Client crypto path:** confirm x25519 + RescueCipher encrypt/decrypt works from Rust/WASM (interop or reimpl). (Risk #8)
- **Exit gate:** circuit matches transparent math within tolerance; p99 < chosen tick; a Rust/WASM client can encrypt inputs and decrypt own state.

### MILESTONE 1 — Confidential flip, single box, devnet
- Adapt the Anchor program to `#[arcium_program]`; implement init-comp-def / queue / callback for `flip_box`.
- Persistent `Enc<Mxe, Curve>` + one `Enc<Shared, Box>`. Wire `box.pending` lock, BLS `verify_output`, curve-nonce stale-guard.
- Manual trigger (no VRF yet). Prove read-modify-write of encrypted state round-trips correctly.

### MILESTONE 2 — Public VRF selection + confidential settlement
- Re-attach your existing ORAO/Switchboard VRF for public selection over the box roster.
- Keeper: select (public, 30s) → queue confidential flip (async) → settle on callback. Implement timeout/retry/skip-round; selection decoupled from settlement.
- Player frontend reads own box via re-encryption.

### MILESTONE 3 — Multi-box scale + soak
- One PDA per box; many boxes; shared curve. Soak the 30s loop for days on devnet under load. Watch for queue backlog, aborts, stale-callback races. Tune tick interval from measured p99.
- Model recurring cost: ~2,880 computations/day at one-per-30s; capture per-computation SOL cost from devnet; sanity-check against the 10% treasury fee.

### MILESTONE 4 — Internal-state-privacy production candidate
- Public deposit/withdraw (amounts visible at vault), encrypted internal balances. Treasury balance encrypted.
- Obtain Arcium audit reports; review trust model. Decide Cerberus parameters/recovery-set-size.
- **Mainnet-Alpha launch with explicit "alpha infrastructure" disclosure, capped per-box deposits, and the transparent fallback mode available.**

### MILESTONE 5 (later, gated on external infra) — Amount-hidden custody
- Only after verifying ZK-ElGamal re-enable is live (feature-gate + issue #657 closed) OR Arcium C-SPL ships to devnet with documented EncryptedTokenAccount deposit APIs.
- Integrate a shielded-pool deposit/withdraw (Umbra-style or own Arcium pool) for amount + linkage privacy. Document anonymity-set limits.

---

## 9. Open Decisions for the Owner

1. **Privacy scope for v1:** Ship "internal state private, custody amounts public" (achievable now), or block launch on amount-hidden custody (gated on external infra, indefinite)? *Recommendation: ship the former; treat the latter as M5.*
2. **Tick cadence:** Hold 30s, or make it adaptive/longer (45–90s) based on M0 p99? *Recommendation: keep 30s for public selection; make settlement async; decide final number from measured data.*
3. **Mainnet-money gate:** What conditions must Arcium meet before real funds at scale — full decentralization (staking/slashing GA), confirmed audits, deposit caps? Define the threshold now.
4. **Trust model acceptance:** Are you comfortable with Cerberus dishonest-majority (privacy holds if ≥1 honest node) on a *permissioned* alpha cluster? This is an operator-trust dependency you don't control.
5. **Fallback policy:** Keep the transparent FlipVault as a permanent opt-out mode, or as emergency-only? *Recommendation: permanent opt-out — it's also your liveness backstop.*
6. **Frontend crypto path:** Commit to wasm-bindgen interop with the TS SDK, or invest in a Rust-native x25519+Rescue implementation? Decide at M0.
7. **Audit budget:** Will you fund an independent review of the Path-B program + circuit before mainnet money, given Arcium's own audit reports are unverified in this research?
8. **Cost ceiling:** What monthly SOL burn for continuous confidential computation is acceptable, and does the 10% treasury fee cover it at expected volume? (Per-computation cost is unpublished — measure at M3.)

---

*Sources are cited inline by verdict key (v-usable, v-latency, v-custody) and primary reference. Load-bearing primary sources: docs.arcium.com (installation, hello-world, deployment, limitations, computation-lifecycle), github.com/arcium-hq/examples, sdk.umbraprivacy.com, solana.com/news/post-mortem-june-25-2025, github.com/solana-program/token-2022/issues/657, solanacompass.com (1M computations). Flagged as stale/unconfirmed: exact alpha node count and decentralization date, per-computation cost, named audit firms, C-SPL ship status, ZK-ElGamal mainnet feature-gate activation. Re-verify these directly before each milestone.*