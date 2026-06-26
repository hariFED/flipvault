# FlipVault Path-B — Backend Build Blueprint (Arcium MPC Private Perpetual)

> Status: ready to execute the moment the Arcium 0.11.1 toolchain image (`Dockerfile.arcium`) finishes building.
> Source of truth for decisions: `docs/FlipVault-pathb-M0.md`. Feasibility: `docs/FlipVault-arcium-research.md`.
> The confidential circuit (`path-b/encrypted-ixs/src/lib.rs`) and the curve-precision proof (`path-b/spikes/curve-precision/`) are **already written and proven** (20M inputs, 0 mismatches). This document covers everything that wraps them.
>
> **Verdict discipline:** Where research items conflicted with their adversarial verdict, this blueprint follows the verdict. The single biggest unresolved point is the **`queue_computation` arity** (research said 5, 6, and 7 args in different places). It is marked `⚠ TOOLCHAIN-VERIFY` everywhere it appears and resolved by reading the generated IDL/macro output on first `arcium build`.

---

## 1. Arcium 0.11.1 API cheat-sheet (verified signatures we rely on)

### 1.1 Toolchain & scaffold (`high` confidence)

```bash
# install (inside Dockerfile.arcium / WSL2 Linux — NOT native Windows)
curl --proto '=https' --tlsv1.2 -sSfL https://install.arcium.com/ | bash
arcup install 0.11.1          # pins Solana CLI 3.1.10 + Anchor 1.0.2
arcup version                 # verify components

arcium init flipvault-pathb   # generates workspace (see §2.0 for what we keep/replace)
arcium build                  # compiles encrypted-ixs/ -> build/*.arcis + *.hash + IDL, then the program
arcium localnet --skip-keygen # local MPC nodes (Docker-out-of-Docker); fast iteration
arcium test                   # TS tests vs localnet
arcium test --cluster devnet  # TS tests vs devnet (needs cluster offset in Arcium.toml)
```

- **Toolchain pins (confirmed by `Dockerfile.arcium` + M0 doc):** Arcium 0.11.1 → Solana CLI 3.1.10, Anchor **CLI** 1.0.2, `anchor-lang` **crate** ~0.32.x. This is incompatible with Path-A (Agave 4.0.2 / Anchor 0.31.1) → **separate container, separate `Cargo.toml`, never share a workspace.**
- **Crate versions are auto-pinned by `arcium init`** — do **not** hand-pick `arcis = "0.9.6"` (that was the v0.9.6 example repo; verdict-confirmed). Trust whatever versions the scaffold writes for `arcis`, `arcium-anchor`, `arcium-client`.

### 1.2 Confidential-instruction triad (`high` confidence on shape)

Every confidential instruction needs **three** Rust items: a comp-def init, a queue fn, and a callback. Wiring is done by macros that read the `build/*.idarc`/IDL produced by `arcium build`.

```rust
#[arcium_program]
pub mod flipvault_pathb {
    use super::*;

    // (a) one-time comp-def init per circuit
    pub fn init_flip_box_comp_def(ctx: Context<InitFlipBoxCompDef>) -> Result<()> {
        init_comp_def(ctx.accounts, None /* inline */ )?;      // ⚠ name+arity TOOLCHAIN-VERIFY
        Ok(())
    }

    // (b) queue
    pub fn queue_flip_box(ctx: Context<QueueFlipBox>, computation_offset: u64, /* args */) -> Result<()> { … }

    // (c) callback
    #[arcium_callback(encrypted_ix = "flip_box")]
    pub fn flip_box_callback(ctx: Context<FlipBoxCallback>, output: SignedComputationOutputs<FlipBoxOutput>) -> Result<()> { … }
}
```

**`queue_computation` signature — ⚠ TOOLCHAIN-VERIFY (research conflict: 5 vs 6 vs 7 args):**

```rust
// MOST-LIKELY (v0.11.x docs, two high-confidence verdicts):
queue_computation(
    ctx.accounts,
    computation_offset,          // u64, random per call
    args,                        // ArgBuilder::…build()
    vec![FlipBoxCallback::callback_ix(computation_offset, &ctx.accounts.mxe_account, &custom_cbk_accs)?],
    1,                           // num_callback_txs: u32
    0,                           // cu_price_micro: u32 (priority fee)
    0,                           // callback_cu_limit: u32  <-- v0.11.x addition; OMIT if build rejects it
)?;
```
> The coinflip/voting **example repos** (v0.9.6) show only 5–6 args. The first thing the build does is settle this: generate the program, read the macro-expanded signature, drop or keep the trailing `callback_cu_limit`. **Do not block design on it** — it is one trailing `0`.

**`callback_ix()` helper** auto-includes the 6 standard callback accounts; pass custom writable accounts as `&[CallbackAccount { pubkey, is_writable: true }]`. **Order of custom accounts in this slice must match the order in the callback `#[derive(Accounts)]` struct** (verdict-confirmed gotcha).

**Callback + `verify_output` (confirmed against coinflip/voting + our M0 doc):**
```rust
let out = match output.verify_output(&ctx.accounts.cluster_account, &ctx.accounts.computation_account) {
    Ok(o) => o,
    Err(e) => { msg!("flip aborted: {e}"); return Err(PathBError::AbortedComputation.into()); }
};
```
`verify_output` does BLS verification of the cluster's signed output; **mandatory** before trusting any decrypted result.

### 1.3 Account macros (confirmed shapes)

| Macro | Used on | Purpose |
|---|---|---|
| `#[queue_computation_accounts("flip_box", payer)]` | `QueueFlipBox` | wires MXE/mempool/execpool/computation/comp-def/cluster/fee-pool/clock |
| `#[callback_accounts("flip_box")]` | `FlipBoxCallback` | wires arcium_program/comp_def/mxe/computation/cluster/instructions-sysvar + our custom PDAs |
| `#[init_computation_definition_accounts("flip_box", payer)]` | `InitFlipBoxCompDef` | wires mxe/comp_def/LUT/lut_program |
| `derive_mxe_pda!()`, `derive_cluster_pda!(mxe, err)`, `derive_mempool_pda!`, `derive_execpool_pda!`, `derive_comp_pda!(offset, mxe, err)`, `derive_comp_def_pda!(OFFSET)`, `derive_sign_pda!()`, `derive_mxe_lut_pda!(slot)` | account constraints | PDA derivations |
| `comp_def_offset("flip_box") -> u32` | const | `sha256("flip_box")[..4]` LE |
| `circuit_hash!("flip_box")` | offchain comp-def only | embeds `build/flip_box.hash` |

Fixed Arcium accounts (do **not** derive): `ARCIUM_FEE_POOL_ACCOUNT_ADDRESS`, `ARCIUM_CLOCK_ACCOUNT_ADDRESS`.

### 1.4 Encrypted I/O shapes (`high` confidence)

- **Input args** via `ArgBuilder`:
  - `Enc<Shared,T>` field → `.x25519_pubkey(pubkey).plaintext_u128(nonce).<encrypted_*>(ct)…` (order matters).
  - `Enc<Mxe,T>` field → `.plaintext_u128(nonce).<encrypted_*>(ct)…` (no pubkey).
  - Public scalar → `.plaintext_u128(k)`, `.plaintext_u128(fee_bps)`.
  - Pull ciphertext straight from an account → `.account(pubkey, offset_bytes, size_bytes)`.
  - **`encrypted_*` method granularity is ⚠ TOOLCHAIN-VERIFY:** docs/examples show `encrypted_u8`; whether a 32-byte ciphertext block maps to one `encrypted_u128`/`encrypted_u64` call or several `encrypted_u8` calls is resolved by the generated IDL. The circuit's `to_arcis()` boundary fixes the count.
- **Ciphertext on wire/at rest:** each scalar = one `[u8; 32]` RescueCipher block; nonce = `u128`.
- **Output struct (auto-generated from circuit return type):** our circuit returns `(Enc<Mxe,Curve>, Enc<Mxe,u128>, Enc<Shared,BoxState>)`, so the macro emits a nested `FlipBoxOutput { field_0: FlipBoxOutputStruct0 { field_0: MXEEncryptedStruct<2> /*Curve*/, field_1: MXEEncryptedStruct<1> /*treasury*/, field_2: SharedEncryptedStruct<4> /*BoxState*/ } }` (exact nesting depth is ⚠ TOOLCHAIN-VERIFY — read the IDL, mirror the destructure; pattern proven by sealed_bid_auction's `…OutputStruct00`).
  - `MXEEncryptedStruct<N> { nonce: u128, ciphertexts: [[u8;32]; N] }`
  - `SharedEncryptedStruct<N> { encryption_key: [u8;32], nonce: u128, ciphertexts: [[u8;32]; N] }`
  - `LEN` = **scalar count**, not bytes. `Curve{r_sol,r_tok}` → 2. `BoxState{sol,perp,in_perp,cost_basis}` → 4. treasury → 1.
- **Callback payload cap ≈ 1232 bytes.** Our total output ≈ `(2+1+4) scalars × 32B` + nonces + one 32B shared key ≈ **~290 bytes** → comfortably fits. **Confirmed safe.**
- **Nonce:** MXE re-encrypts outputs at `nonce+1`; the **cluster increments**, the client uses the nonce delivered in the callback event (does not increment itself).

### 1.5 Devnet deploy (`high` confidence)

```bash
arcium deploy \
  --cluster-offset 456 \           # devnet (2026 = mainnet)
  --recovery-set-size 4 \          # minimum; CLI prints higher if required
  --keypair-path ~/.config/solana/id.json \
  --rpc-url <Helius|Triton|QuickNode devnet URL>   # default RPC drops txs — use a paid one
# partial: --skip-init (program only) / --skip-deploy (MXE init only)
```
`Arcium.toml`: `[clusters.devnet] offset = 456`. Fund keypair ~2–5 SOL. Per-computation SOL cost is **unpublished** → measured at M0b.

### 1.6 Open API questions (must be answered on first build — not blockers)

1. `queue_computation` final arity (5/6/7) + whether `callback_cu_limit` exists.
2. `init_comp_def` vs `init_computation_def` name + 2-arg vs 3-arg form.
3. `ArgBuilder` `encrypted_*` granularity per 32-byte block.
4. Exact nesting of `FlipBoxOutput` for a 3-tuple return.
5. ORAO 0.6.x compatibility with `anchor-lang` 0.32.x (else Switchboard) — §4.

All five are **read-the-generated-IDL** questions, resolved in the first hour of M1 with zero design impact.

---

## 2. File-by-file backend plan (`path-b/`)

### 2.0 Workspace layout produced/kept after `arcium init`

```
path-b/
  Anchor.toml                         # scaffold; set [programs.devnet] flipvault_pathb = <id>
  Arcium.toml                         # scaffold; add [clusters.devnet] offset = 456
  Cargo.toml                          # [workspace] members = ["programs/*","encrypted-ixs"]
  encrypted-ixs/
    Cargo.toml                        # SCAFFOLD-PINNED arcis version (do not hand-edit)
    src/lib.rs                        # ★ ALREADY WRITTEN — flip_box circuit. Dropped in verbatim.
  programs/flipvault-pathb/
    Cargo.toml                        # arcium-anchor, arcium-client (scaffold-pinned)
    src/lib.rs                        # NEW — #[arcium_program] (§2.1)
    src/state.rs                      # NEW — account layouts (§2.2)
    src/error.rs                      # NEW — PathBError (§2.5)
    src/constants.rs                  # NEW — seeds, NO_BOX, BPS, timeouts
    src/instructions/                 # NEW — one file per ix (§2.3)
      mod.rs initialize.rs register_box.rs deposit.rs withdraw.rs
      commit_round.rs queue_flip.rs flip_callback.rs recover_round.rs sweep_treasury.rs
  tests/flipvault-pathb.ts            # NEW — TS e2e (client crypto + queue + await + decrypt)
  spikes/curve-precision/             # ★ EXISTS — unchanged proof artifact
```
`spikes/` and `encrypted-ixs/src/lib.rs` are **untouched** (already proven). Everything else is new.

### 2.1 `programs/flipvault-pathb/src/lib.rs` — the `#[arcium_program]`

Mirrors Path-A's `flipvault/src/lib.rs` structure (thin dispatch into `instructions::*::handler`). Instruction set:

```rust
declare_id!("<new path-b program id>");

#[arcium_program]
pub mod flipvault_pathb {
    use super::*;

    // comp-def bootstrap (run once after deploy)
    pub fn init_flip_box_comp_def(ctx: Context<InitFlipBoxCompDef>) -> Result<()> { … }

    // custody + registry
    pub fn initialize(ctx, fee_bps: u16, round_secs: i64, min_reserve: u64, treasury_authority: Pubkey, k: u128) -> Result<()>;
    pub fn register_box(ctx, /* enc seed init */) -> Result<()>;   // creates a Box PDA + zero ciphertext
    pub fn deposit(ctx, amount: u64) -> Result<()>;                // public amount -> queue encrypt-credit
    pub fn deposit_callback(...) -> Result<()>;                    // writes updated Box.sol ct
    pub fn withdraw(ctx, amount: u64) -> Result<()>;               // queue verify-and-debit
    pub fn withdraw_callback(...) -> Result<()>;                   // writes Box.sol ct, then pays SOL out

    // round loop
    pub fn commit_round(ctx, force: [u8;32]) -> Result<()>;        // PUBLIC ORAO VRF request
    pub fn select_and_queue_flip(ctx, computation_offset: u64) -> Result<()>;  // read VRF -> pick box -> queue_computation(flip_box)
    #[arcium_callback(encrypted_ix = "flip_box")]
    pub fn flip_box_callback(ctx, output: SignedComputationOutputs<FlipBoxOutput>) -> Result<()>;  // verify + stale-guard + unlock
    pub fn recover_round(ctx) -> Result<()>;                       // timeout cancel (no VRF or stuck flip)

    // treasury
    pub fn sweep_treasury(...) -> Result<()>;                      // queue: decrypt treasury, pay out, re-encrypt remainder
}
```

> **Design note on deposit/withdraw as MPC computations:** because internal balances are `Enc<Shared,BoxState>`, crediting a deposit and debiting a withdraw must happen *inside* the MXE (you cannot add a plaintext `amount` to a ciphertext on-chain). So `deposit`/`withdraw` follow the same **queue → callback** pattern as `flip_box`, but with their own tiny circuits. **This adds two more circuits to `encrypted-ixs/src/lib.rs`** (`credit_box(box, amount_pub) -> box`, `debit_box(box, amount_pub) -> (box, ok_flag_pub)`). These are trivial vs `flip_box` and reuse the exact same scaffolding. (Captured as an M1 scope addition; the M0 doc only enumerated `flip_box`.)

### 2.2 Account / state layouts — `programs/flipvault-pathb/src/state.rs`

```rust
/// Global singleton. Mirrors Path-A Config but curve/treasury are CIPHERTEXT, not plaintext.
#[account]
#[derive(InitSpace)]
pub struct PathBConfig {
    pub treasury_authority: Pubkey,
    pub k: u128,                    // PUBLIC curve constant (passed to circuit as plaintext)
    pub fee_bps: u16,              // PUBLIC
    pub round_secs: i64,
    pub min_reserve: u64,          // public reserve floor (advisory; circuit is house-safe anyway)
    pub last_settled_ts: i64,
    pub phase: RoundPhase,         // Idle | VrfPending | FlipPending  (3-state; Path-A had 2)
    pub round_seed: [u8;32],       // ORAO force bound to current round
    pub commit_ts: i64,
    pub commit_slot: u64,
    pub active_box_count: u32,     // for selected_index = rand % active_box_count
    pub selected_box: Pubkey,      // box chosen this round (PUBLIC)
    pub selected_index: u32,       // PUBLIC selection (auditable vs VRF)
    pub round: u64,                // monotonic round counter
    pub curve_version: u64,        // ★ STALE-GUARD: bumped on every committed flip callback
    pub bump: u8,
}

/// Shared bonding-curve reserves, encrypted to the MXE only.
#[account]
#[derive(InitSpace)]
pub struct CurveState {
    pub ct: [[u8;32]; 2],          // Enc<Mxe,Curve> = {r_sol, r_tok}
    pub nonce: u128,
    pub bump: u8,
}

/// Accrued fees, encrypted to the MXE only.
#[account]
#[derive(InitSpace)]
pub struct TreasuryState {
    pub ct: [[u8;32]; 1],          // Enc<Mxe,u128>
    pub nonce: u128,
    pub bump: u8,
}

/// Per-player box. PDA: ["box", owner]. Position/balance/P&L all live in `ct` (Shared).
#[account]
#[derive(InitSpace)]
pub struct PlayerBox {
    pub owner: Pubkey,             // PUBLIC identity
    pub index: u32,               // dense index used by VRF modulo (registry slot)
    pub ct: [[u8;32]; 4],          // Enc<Shared,BoxState> = {sol, perp, in_perp, cost_basis}
    pub encryption_key: [u8;32],   // x25519 key the box was sealed to (player can re-derive shared secret)
    pub nonce: u128,
    pub pending: bool,             // ★ LOCK: true while ANY computation on this box is in flight
    pub curve_version_at_queue: u64, // ★ STALE-GUARD snapshot taken at queue time
    pub last_round_touched: u64,
    pub bump: u8,
}

/// Dense registry mapping index -> box pubkey, so VRF `rand % active_box_count` resolves a box.
/// Append-only with a free-list for unregister; capped (e.g. 256 for v1 soak — fixed array, Arcis-friendly off-chain).
#[account]
pub struct BoxRegistry {
    pub count: u32,
    pub entries: Vec<Pubkey>,      // on-chain Anchor Vec OK (this is the PUBLIC program, not Arcis)
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum RoundPhase { Idle, VrfPending, FlipPending }
```

**Critical layout rule:** the byte offsets of `PlayerBox.ct`/`nonce`, `CurveState.ct`/`nonce`, `TreasuryState.ct`/`nonce` must be **stable and known**, because `select_and_queue_flip` feeds them to the circuit via `ArgBuilder.account(pubkey, offset, size)`. Document each offset (8-byte discriminator + field order) and pin it with an SDK test (§2.6).

### 2.3 Instruction contexts (exact account lists)

**`InitFlipBoxCompDef`** (`#[init_computation_definition_accounts("flip_box", payer)]`): `payer(signer,mut)`, `mxe_account = derive_mxe_pda!()`, `comp_def_account(mut, unchecked)`, `address_lookup_table = derive_mxe_lut_pda!(mxe.lut_offset_slot)`, `lut_program = LUT_PROGRAM_ID`, `arcium_program`, `system_program`. (Repeat for `init_credit_box_comp_def`, `init_debit_box_comp_def`.)

**`Initialize`**: `founder(signer,mut)`, `config(init, PathBConfig)`, `curve(init, CurveState)`, `treasury(init, TreasuryState)`, `box_registry(init, BoxRegistry)`, `vault = ["vault"] system-owned SOL account`, `system_program`. Seeds genesis curve ciphertext via a one-shot `seed_curve` circuit (or accept a client-encrypted genesis ciphertext, since k is public and genesis reserves are not secret pre-launch).

**`RegisterBox`**: `owner(signer,mut)`, `config`, `box_registry(mut)`, `player_box(init, seeds=["box", owner])`, `system_program`. Writes `owner`, assigns dense `index = registry.count`, appends to registry, stores client-supplied zero-balance `Enc<Shared,BoxState>` ciphertext + `encryption_key` + `nonce`.

**`Deposit`** (queue): `owner(signer,mut)`, `config`, `player_box(mut)`, `curve?`(no), `vault(mut)` (SOL lands here, public), Arcium queue accounts (mempool/execpool/computation/comp_def(credit_box)/cluster/fee_pool/clock/mxe/sign_pda), `arcium_program`, `system_program`. Transfers `amount` lamports `owner -> vault` (public), then `queue_computation(credit_box, args = box_ct + amount_plaintext)`. Sets `player_box.pending = true`.

**`DepositCallback`** / **`WithdrawCallback`** (`#[callback_accounts("credit_box"/"debit_box")]`): standard 6 + `player_box(mut)` + (`vault(mut)` + `owner(mut)` for withdraw payout). Verify, write new `box.ct`/`nonce`, clear `pending`. Withdraw additionally checks a **public `ok` flag** the circuit reveals (sufficient-balance) and only then moves SOL `vault -> owner`.

**`CommitRound`** — *identical to Path-A* (`flipvault/src/instructions/commit_round.rs`): `keeper(signer,mut)`, `config(mut)`, `random` (ORAO request PDA), `orao_treasury`, `network_state`, `vrf` (OraoVrf program), `system_program`. CPI `orao_solana_vrf::cpi::request_v2(force)`. Sets `phase = VrfPending`, `round_seed = force`, `commit_ts/slot`, bumps `round`.

**`SelectAndQueueFlip`** — the new heart. Accounts: `keeper(signer,mut)`, `config(mut)`, `random` (ORAO PDA pinned to `config.round_seed`), `box_registry`, `curve(mut→passed by .account())`, `treasury`, `selected_box: PlayerBox(mut)`, **+ all `#[queue_computation_accounts("flip_box", keeper)]` accounts** (`sign_pda`, `mxe`, `mempool`, `execpool`, `computation`, `comp_def(flip_box)`, `cluster`, `fee_pool`, `clock`, `arcium_program`, `system_program`). Handler:
1. `require!(phase == VrfPending)`.
2. Read `RandomnessAccountData` from `random` (Path-A pattern, `settle_round.rs:46-56`); error → stay `VrfPending` (keeper retries / recovers).
3. `selected_index = u32::from_le_bytes(rand[..4]) % config.active_box_count` (uniform; `rand` is 64 bytes). Confirm `selected_box.index == selected_index` (keeper passed the right PDA) else error.
4. `emit!(BoxSelected{ round, index: selected_index, box: selected_box.key() })`.
5. Build `ArgBuilder`: public `k`, `fee_bps`; `Enc<Mxe,Curve>` from `curve` account; `Enc<Mxe,u128>` from `treasury`; `Enc<Shared,BoxState>` from `selected_box` (with its `encryption_key` + `nonce`). Use `.account(curve.key(), CURVE_CT_OFFSET, CURVE_CT_SIZE)` etc.
6. Snapshot guard: `selected_box.curve_version_at_queue = config.curve_version; selected_box.pending = true; config.selected_box = …; config.selected_index = …; config.phase = FlipPending`.
7. `queue_computation(flip_box, …, callback=FlipBoxCallback::callback_ix(offset, &mxe, &[curve(mut), treasury(mut), selected_box(mut)]))`.

**`FlipBoxCallback`** (`#[callback_accounts("flip_box")]`): standard 6 + `config(mut)` + `curve(mut)` + `treasury(mut)` + `selected_box(mut)`. Handler:
1. `let (new_curve, new_treasury, new_box) = output.verify_output(&cluster, &computation)?` (destructure `FlipBoxOutput` nesting — ⚠ shape from IDL).
2. **Stale-guard:** `require!(config.curve_version == selected_box.curve_version_at_queue, PathBError::StaleCallback)`. (A late callback whose curve was already mutated by a newer committed flip is rejected — prevents clobbering newer reserves.)
3. Write `curve.ct/nonce`, `treasury.ct/nonce`, `selected_box.ct/nonce` from the encrypted structs.
4. `config.curve_version += 1; selected_box.pending = false; config.phase = Idle; config.last_settled_ts = now; emit!(FlipSettled{ round, index })` (no amounts).

**`RecoverRound`** — extends Path-A's: if `VrfPending` past `RECOVER_AFTER_SECS` → cancel (Path-A logic). **New:** if `FlipPending` past a flip deadline → cancel the round, clear `config.phase = Idle`, and clear `selected_box.pending` (keeper unsticks a dropped/aborted computation). Does **not** mutate ciphertext (no committed result).

**`SweepTreasury`** — queue/callback pair: circuit `pay_treasury(treasury, amount_pub) -> (treasury, ok_pub)`; callback moves SOL `vault -> recipient` on `ok`. Authority-gated by `config.treasury_authority` (Path-A pattern).

### 2.4 How the existing circuit slots in

`encrypted-ixs/src/lib.rs` is dropped into the scaffold **verbatim**. `arcium build` compiles `flip_box` → `build/flip_box.arcis` + `build/flip_box.hash` + IDL. The IDL drives:
- `comp_def_offset("flip_box")` const,
- the generated `FlipBoxOutput` destructure shape,
- the `ArgBuilder` `encrypted_*` call sequence (input scalar count).

We **add two small circuits** to the same file for custody (`credit_box`, `debit_box`) and one optional `seed_curve`/`pay_treasury`. The proven `flip_box` math is frozen; the M0a exit gate (`arcium build` → run on devnet → decrypt → equals transparent curve) is the first M1 task.

### 2.5 `error.rs`

```rust
#[error_code]
pub enum PathBError {
    InvalidParams, RoundTooSoon, RoundPending, NoPendingRound,
    RandomnessNotResolved, RecoverTooSoon, Overflow,
    AbortedComputation,      // verify_output failed
    StaleCallback,           // curve_version mismatch
    BoxPending,              // box locked by in-flight computation
    IndexMismatch,           // keeper passed wrong box for selected_index
    InsufficientBalance,     // withdraw debit ok-flag false
}
```

### 2.6 SDK extensions — mirror Path-A's `flipvault-sdk`

New crate `app/crates/flipvault-pathb-sdk/` mirroring the existing `ids/disc/pda/ix/state` layout (single ABI source of truth, WASM-safe + native):

- **`ids.rs`** — `PROGRAM_ID` (new Path-B id), `ORAO_VRF_ID` (reuse), Arcium fixed accounts (`ARCIUM_PROGRAM_ID`, fee-pool, clock), seeds (`CONFIG`, `CURVE`, `TREASURY`, `BOX`, `REGISTRY`, `VAULT`), `NO_BOX`, `BPS_DENOM`, devnet `CLUSTER_OFFSET = 456`.
- **`pda.rs`** — `config_pda`, `curve_pda`, `treasury_pda`, `box_pda(owner)`, `registry_pda`, `vault_pda`, plus Arcium derivations (`mxe_pda(program_id)`, `cluster_pda(offset)`, `comp_def_pda(program_id, offset)`, `computation_pda(offset, computation_offset)`, `mempool/execpool_pda`, `sign_pda`) — mirroring the `getXAccAddress` TS helpers from the research.
- **`disc.rs`** — reuse verbatim (`global:<name>` sha256[..8]); add Arcium discriminators if needed.
- **`ix.rs`** — builders for every instruction in §2.1, each assembling discriminator + borsh args + AccountMetas in **exact on-chain order**. The crypto-bearing ones (`deposit`, `withdraw`, `select_and_queue_flip`) take pre-built ciphertext byte arrays + nonce + x25519 pubkey from the caller (the SDK does *not* do crypto; the frontend/keeper does, see §6).
- **`state.rs`** — borsh `PathBConfig`, `CurveState`, `TreasuryState`, `PlayerBox`, `BoxRegistry` decoders + `decode()` helper; **plus offset constants** `CURVE_CT_OFFSET/SIZE`, `BOX_CT_OFFSET/SIZE`, `TREASURY_CT_OFFSET/SIZE` consumed by the program's `ArgBuilder.account()` and asserted by a borsh-roundtrip test (mirrors Path-A's `config_borsh_roundtrip`/`vault_borsh_roundtrip`).
- **`crypto.rs`** (new, feature-gated) — the M0c client crypto: x25519 ECDH + RescueCipher encrypt/decrypt. Decision pending M0c (`x25519-dalek` native + Rescue reimpl, **or** wasm-bindgen interop to `@arcium-hq/client`). This is the *only* genuinely new SDK surface vs Path-A.

### 2.7 Keeper redesign

New keeper service (mirrors Path-A's tick loop but three-phase). **One computation per tick — never per-player-per-tick** (network ≈ 2.3 comp/s aggregate; per-player dies at 50–100 boxes).

```
loop every round_secs (soft 30s):
  phase = read config.phase
  match phase:
    Idle:
      if now >= last_settled_ts + round_secs:
        force = random32()
        send commit_round(force)            # ORAO request (public VRF)
    VrfPending:
      if orao randomness fulfilled:
        offset = random64()
        send select_and_queue_flip(offset)  # reads VRF, picks box, queues flip_box
        track(offset, round)                # await callback / timeout
      elif now >= commit_ts + RECOVER_AFTER_SECS:
        send recover_round()                # VRF stuck -> cancel, retry next tick
    FlipPending:
      if FlipSettled event for round seen:   # callback landed
        continue                            # next tick re-commits
      elif now >= queue_ts + FLIP_DEADLINE_SECS:
        send recover_round()                # computation dropped/aborted -> unstick box+phase
```
- **Timeout/retry/skip-round:** a round that can't VRF-fulfill or whose flip never finalizes is **skipped** via `recover_round` (no flip applied, box unlocked, `phase→Idle`), then retried next tick with a fresh seed/offset. No deadlock: `pending` and `phase` are always cleared by recover.
- **Callback handling:** keeper subscribes to `FlipSettled`/`BoxSelected` logs and to Arcium finalization (`awaitComputationFinalization`-equivalent) to advance phase deterministically rather than guessing.
- **Indexer feed:** keeper (or a sibling indexer, reusing Path-A's M3 indexer+Axum) records `BoxSelected{round,index,box}` and `FlipSettled{round}` for public round history (§6).

---

## 3. Concurrency & custody

### 3.1 Pending lock (per box)
`PlayerBox.pending: bool`. Set `true` at queue time (deposit/withdraw/flip), cleared in the matching callback (or by `recover_round`). Any new instruction touching a box `require!(!box.pending, BoxPending)`. Because only **one box** is flipped per tick and a player's deposit/withdraw also locks their box, no two computations ever race on the same `PlayerBox.ct`.

### 3.2 Curve-version stale-callback guard (shared curve)
The shared `CurveState.ct` is the **single serialization point** (every flip read-modify-writes it). `config.curve_version: u64` is bumped on every *committed* flip callback. At queue time we snapshot `box.curve_version_at_queue = config.curve_version`. The callback rejects (`StaleCallback`) if `config.curve_version != box.curve_version_at_queue`. With one-comp-per-tick this is normally a no-op, but it is the **hard safety net** against a late/duplicate callback clobbering newer reserves if cadence ever overlaps. (`u64` version chosen over the M0-doc's `u128` nonce idea — monotonic, no wraparound concern, cheaper compare; resolves that open question.)

### 3.3 One-computation-per-tick rationale
Arcium aggregate throughput ≈ 2.3–2.8 comp/s (permissioned alpha, no SLA). A naive per-player-per-tick design is O(N) computations/tick and saturates at 50–100 boxes. Path-B is **O(1) in player count**: VRF picks exactly one box, one `flip_box` computation runs. Latency budget: ~3–12s/flip (unmeasured → M0b) under a 30s **soft** cadence; settlement is async, so a flip spilling past 30s simply settles late without blocking the next commit (guarded by `phase`).

### 3.4 Internal-balance custody (v1)
- **One public custody `vault` PDA** (system-owned SOL account) holds *all* real lamports.
- **Deposit:** `owner → vault` (amount public on-chain), then `credit_box` MPC adds it to the encrypted `BoxState.sol`. **Withdraw:** `debit_box` MPC verifies/decrements encrypted balance (reveals only a public `ok` flag), then `vault → owner` (amount public).
- **Per flip: zero real SOL moves.** `flip_box` only rewrites ciphertext (box ↔ perp on the encrypted curve, fee → encrypted treasury). So no per-flip amount, balance, position, P&L, reserve, or fee size ever leaks on-chain.
- **Solvency invariant** (mirrors the spike's conservation test): `vault.lamports == Σ all box SOL + curve r_sol + treasury` — provable only inside the MXE, but the public vault balance bounds total custody. The 200k-flip conservation invariant already proven in `spikes/curve-precision` is the off-chain guarantee this rests on.

---

## 4. Public VRF selection (Anchor 0.32.x / Arcium 0.11.1)

**Selection MUST be public and verifiable — NOT `ArcisRNG`.** `ArcisRNG` is secret-shared inside MPC and only BLS-*tamper-evident*; it is **not publicly auditable as fair**. The whole product promise is "public doors, private contents": *which* box is picked must be openly verifiable against on-chain randomness; only the *math* is private.

**Flow (unchanged from Path-A's commit/settle split, re-pointed):**
1. `commit_round` CPIs `orao_solana_vrf::cpi::request_v2(force)` — **byte-identical to Path-A** (`commit_round.rs`). `force` bound into `config.round_seed`.
2. `select_and_queue_flip` reads `RandomnessAccountData::fulfilled_randomness()` (Path-A `settle_round.rs:46-56`), computes `selected_index = rand_u32 % active_box_count` as **plaintext**, emits `BoxSelected`, then queues `flip_box`. (Path-A used `rand_byte % 4`; we use `u32 % active_box_count` because box count is dynamic and >256.)

**ORAO vs Switchboard — ⚠ COMPATIBILITY GATE (resolved in first M2 hour):**
- Path-A pins `orao-solana-vrf` 0.6.x on Anchor 0.31.1. Arcium 0.11.1 ships `anchor-lang` ~0.32.x.
- **First M2 task:** add `orao-solana-vrf` to `programs/flipvault-pathb/Cargo.toml` and `arcium build`. If it compiles against 0.32.x → keep ORAO (reuse all Path-A VRF code). If the Anchor major mismatch breaks the CPI types → **migrate selection to Switchboard On-Demand** (which tracks newer Anchor), keeping the exact same commit/select state machine — only the CPI account list and the randomness deserialize change. The selection logic, stale-guard, and circuit are provider-agnostic.

---

## 5. Milestone mapping

| Milestone | Goal | Work (reusing M0 spike) | Exit gate |
|---|---|---|---|
| **M0 (done)** | De-risk | `spikes/curve-precision` (20M inputs, 0 mismatch), `encrypted-ixs/src/lib.rs` written | ✅ design-level proven |
| **M1 — single-box confidential flip** | One box flips correctly under MPC | `arcium init`; drop in circuit; §2.1 program with **just** `initialize`+`register_box`+`init_flip_box_comp_def`+a manual `queue_flip`+`flip_box_callback`; add `credit_box`/`debit_box` circuits; `flipvault-pathb-sdk` ids/pda/disc/ix/state; `tests/*.ts` does client-encrypt → queue → finalize → decrypt | **M0a gate on real MPC:** decrypted `flip_box` output equals transparent curve exactly; deposit→flip→withdraw round-trips one box |
| **M2 — public VRF + async settlement** | Wire ORAO/Switchboard + 3-phase round | `commit_round` (ORAO CPI, reuse Path-A); `select_and_queue_flip` (VRF→index→queue); `recover_round` (VRF + flip timeouts); resolve §4 ORAO-vs-Switchboard gate; keeper §2.7 | Keeper runs full Idle→VrfPending→FlipPending→Idle loop on devnet; `BoxSelected` index matches `rand % count`; stuck rounds auto-recover |
| **M3 — multi-box soak** | Many boxes, sustained ticks | `BoxRegistry` dense indexing + free-list; pending-lock + curve-version guard under load; indexer/Axum (reuse Path-A M3) ingests `BoxSelected`/`FlipSettled`; **M0b** latency p50/p95/p99 + per-comp SOL cost measured here | N boxes (target 50–100), hours of ticks, 0 stale-clobbers, 0 stuck boxes; measured p99 < tick; fee covers ~2,880 comp/day |
| **M4 — production candidate** | Hardened devnet, frontend live | Offchain comp-def (`circuit_hash!`) if bytecode large; M5/M6-style hardening (caps, audits-prep, recover paths); Dioxus frontend on live data (reuse M4 UI); mainnet-money gate criteria | End-to-end on devnet with real users; security review; owner sign-off on mainnet gate (Arcium decentralization/staking GA) |

M0c (client crypto) lands inside **M1** (TS test proves it) and is productized in the **frontend** step of M4.

---

## 6. Frontend connection plan (Dioxus / WASM)

Connects once the backend is on devnet. The frontend is the **only place that holds the player's x25519 key** and decrypts their own box.

### 6.1 What the frontend does
- **Deposit / withdraw:** build `deposit(amount)` / `withdraw(amount)` via `flipvault-pathb-sdk::ix`, sign with the player wallet, send. Amount is **public** (custody boundary, accepted v1). For deposit, no crypto needed (the MXE credits); for withdraw, the SDK passes the requested public `amount`.
- **Read own box (M0c crypto path):**
  1. Fetch the player's `PlayerBox` account (raw `ct` + `nonce` + `encryption_key`).
  2. Fetch the MXE x25519 pubkey (`getMXEPublicKey`-equivalent; cache per session).
  3. Derive `shared = x25519(player_priv, mxe_pub)`; `cipher = RescueCipher(shared)`.
  4. `decrypt(box.ct, box.nonce) -> BoxState { sol, perp, in_perp, cost_basis }`.
  5. Render balance / position (waiting vs flipped) / unrealized P&L locally.
- **Crypto runtime:** per M0c decision — either Rust-native `x25519-dalek` + Rescue reimpl in `crypto.rs`, or a thin wasm-bindgen shim to `@arcium-hq/client`'s `RescueCipher`. The player's ephemeral x25519 key is held in Dioxus session state (not localStorage), regenerated per session.
- **Public round view:** subscribe to / poll the indexer for `BoxSelected{round,index,box}`, `FlipSettled{round}`, `config.phase`, `last_settled_ts` → render which box was picked, the VRF proof link, and a countdown to the next commit (`last_settled_ts + round_secs`).

### 6.2 What the indexer / Axum API must expose (reuse Path-A M3 indexer)
- **Public:** round history (`round`, `selected_index`, `selected_box`, VRF seed/proof, `FlipSettled` timestamps), `config` (phase, k, fee_bps, round_secs, active_box_count, countdown), `BoxRegistry` (index→box, owner) — all on-chain plaintext.
- **Per-box (for owner to decrypt):** raw `PlayerBox.ct`/`nonce`/`encryption_key` bytes. The API **serves ciphertext only**; it never decrypts. Decryption happens client-side with the player's key.

### 6.3 CAN vs CANNOT see

| Frontend CAN see | Frontend CANNOT see (anyone) |
|---|---|
| Its **own** box: balance, waiting/flipped, P&L (decrypted locally) | **Other** players' box balances/positions/P&L |
| Which box was selected each round + VRF proof (public) | Bonding-curve reserves / current price (Enc<Mxe>) |
| Round phase, countdown, round history | Treasury / accrued fees (Enc<Mxe>) |
| Public deposit/withdraw amounts (incl. others', at custody boundary) | Per-flip amounts, fee size, anyone's position changes |
| Custody vault address + total vault balance | Per-box decryption of others (needs their x25519 key) |

> **Metadata caveat (threat model):** box *identity* (PDA/owner) and *selection+timing* are public. A motivated observer correlating selection + deposit timing + public amounts could attempt deanonymization. Documented as a known v1 limitation (M0 doc privacy table); amount-hiding is the future switch-on when Token-2022 Confidential Transfers / C-SPL return (Milestone 5).

---

## 7. Risks & open questions (consolidated, each with resolution path)

| # | Risk / question | Severity | Resolved by | When |
|---|---|---|---|---|
| 1 | `queue_computation` arity (5/6/7) + `callback_cu_limit` existence | Low (one trailing `0`) | Read macro-expanded signature / generated IDL after `arcium build` | M1 hour 1 |
| 2 | `init_comp_def` name + 2 vs 3 args | Low | Same — generated IDL / scaffold example | M1 hour 1 |
| 3 | `ArgBuilder.encrypted_*` granularity per 32-byte block | Low | IDL input scalar count vs circuit `to_arcis()` | M1 hour 1 |
| 4 | `FlipBoxOutput` nested destructure shape for 3-tuple return | Low | Generated IDL; mirror pattern (sealed_bid_auction precedent) | M1 hour 1 |
| 5 | ORAO 0.6.x vs `anchor-lang` 0.32.x compatibility | **Medium** | `arcium build` with ORAO dep; fall back to Switchboard On-Demand (same state machine) | M2 hour 1 |
| 6 | M0a empirical gate: does compiled `flip_box` equal transparent curve on real MPC? | **Medium** | Run on devnet, decrypt output, diff vs transparent (math already proven offline) | M1 |
| 7 | M0b: per-flip latency p50/p95/p99 on devnet; one flip < tick? | **Medium** | Hundreds of devnet runs under simulated load | M3 |
| 8 | M0b: per-computation SOL cost; does 10% fee cover ~2,880 comp/day? | **Medium** | Measure cost per `queue_computation` on devnet; compute daily budget | M3 |
| 9 | M0c: Rust/WASM crypto — native Rescue reimpl vs wasm-bindgen interop | **Medium** | Spike both in `crypto.rs`; benchmark encrypt/decrypt in WASM | M1 (proven in TS), M4 (productized) |
| 10 | Curve-version `u64` vs nonce `u128` for stale-guard wraparound | Low | Chosen `u64` monotonic version (decided here) | resolved |
| 11 | Callback payload < 1232 B for 3-tuple output | Low | Computed ~290 B — safe; re-confirm on real IDL | M1 |
| 12 | Custody circuits (`credit_box`/`debit_box`) not in original M0 scope | Low | Add to `encrypted-ixs/src/lib.rs` (trivial vs flip_box) | M1 |
| 13 | Solvency: `vault == Σ box SOL + r_sol + treasury` only verifiable in MXE | **Medium** | Rests on proven 200k-flip conservation invariant; add an MXE audit/attestation circuit if needed | M3/M4 |
| 14 | Arcium permissioned alpha, no liveness SLA; cluster can stall the game loop | **Medium** | `recover_round` skip/retry; keep transparent Path-A/Path-C as permanent liveness backstop | M2 (recover), M4 (gate) |
| 15 | Metadata deanonymization (selection + timing + public amounts) | Accepted v1 | Documented limitation; amount-hiding switch-on when Token-2022 CT / C-SPL return | Milestone 5 |
| 16 | Devnet RPC drops txs | Low | Use Helius/Triton/QuickNode for `arcium deploy` + keeper | M2 |

**Net:** there are **zero design blockers**. Every open item is either a one-line read-the-IDL fix (1–4, 11) or an empirical devnet measurement (6–9, 13) that the milestone plan already schedules. The math is frozen, the circuit is written, the VRF/SDK/keeper patterns are proven on Path-A and re-pointed here. The moment `Dockerfile.arcium` finishes, M1 starts with `arcium init` → drop in `encrypted-ixs/src/lib.rs` → `arcium build` → resolve items 1–4 from the IDL → write the program per §2.

---

Relevant files (absolute paths):
- Blueprint subject circuit (frozen): `C:\Users\Abcom\flipsol\path-b\encrypted-ixs\src\lib.rs`
- Curve proof artifact: `C:\Users\Abcom\flipsol\path-b\spikes\curve-precision\src\lib.rs`
- Locked decisions: `C:\Users\Abcom\flipsol\docs\FlipVault-pathb-M0.md`
- Path-A mirror sources: `C:\Users\Abcom\flipsol\flipvault\programs\flipvault\src\instructions\commit_round.rs`, `...\settle_round.rs`, `...\recover_round.rs`, `...\state\config.rs`, `...\lib.rs`
- SDK mirror sources: `C:\Users\Abcom\flipsol\app\crates\flipvault-sdk\src\{ids,disc,pda,ix,state,lib}.rs`
- New backend to create: `C:\Users\Abcom\flipsol\path-b\programs\flipvault-pathb\` and `C:\Users\Abcom\flipsol\app\crates\flipvault-pathb-sdk\`