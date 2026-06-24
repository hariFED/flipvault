I have all the context needed. The two adversarial verdicts both return "yes" with high confidence, so this is a clear go on the Rust-WASM thesis. Let me synthesize the layer designs and verdicts into one decisive build plan, reconciling the key disagreement (pure-Rust wallet vs JS shim, and which solana-* version lane).

# FlipVault — Canonical Build Plan (ORE-style Full-Stack Rust App)

**Target program:** FlipVault, Anchor 0.31.1, **deployed on devnet**, program ID `EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H`. The on-chain program is **done and frozen** — this plan builds the app *around* it and never modifies it.

---

## 1. Go / No-Go on the Rust-WASM-Frontend Thesis

**Verdict: GO. Build the frontend as a pure-Rust Dioxus → WASM SPA, with wallet connect + transaction sign/send in Rust.** Both adversarial verdicts return **feasible: "yes", confidence: high**:

- **`v-wallet`**: The JamiiDao `wallet-adapter` crate (**v1.4.2**, released 2026-02-18, stable 1.x line since 2025-03) does connect + `sign_transaction` + `sign_and_send_transaction` from Dioxus WASM on devnet *today*, is pure-Rust via Wallet Standard (auto-detects Phantom/Solflare/Backpack), and ships an Anchor-specific Dioxus template. The "beta" framing in the original brief is stale.
- **`v-rpc`**: The needed RPC calls (`getLatestBlockhash`, `getAccountInfo`/`getMultipleAccounts`, `sendTransaction`, `accountSubscribe`) work from Rust WASM — proven in production by ORE itself, and available off-the-shelf via maintained crates.

**The one hard decision the verdicts force — version lane.** The designs disagree: some propose `wasm_client_solana 0.10` (solana-sdk **v3**), others mirror ORE's `solana-client-wasm 2.1` fork (solana-sdk **2.1**). Mixing 2.x and 3.x solana-* crates produces duplicate-type hell (two `Pubkey` types). **Decision: commit to the solana-sdk v3 lane with `wasm_client_solana 0.10` + JamiiDao `wallet-adapter 1.4.2`.** Justification: (a) both are *published, maintained crates.io releases* — no git-pinned fork of a third-party monorepo (`solana-client-wasm 2.1` exists only inside `regolith-labs/solana-playground`, a supply-chain liability); (b) `wallet-adapter 1.4.2` already targets the modern SDK line; (c) we hand-build instructions from Borsh + the IDL discriminators, so we never link `anchor-client` into WASM and never depend on Anchor's 2.1 pin in the browser. The shared SDK pins **one** solana-* line workspace-wide.

**Wallet approach decision (designs split JS-shim vs pure-Rust).** Go **pure-Rust `wallet-adapter 1.4.2`** as primary — it removes the webpack/React island entirely and matches the "Rust everywhere" goal. **Fallback (only if it breaks against a target wallet): the ORE JS-shim path** — a tiny webpack-bundled `@solana/wallet-adapter` island bridged via Dioxus `document::eval` (`window.DwaTxSigner` + `dwa-pubkey` CustomEvent). This is the proven ore.supply path and is the documented escape hatch. **Milestone 0 (below) exists specifically to prove the primary path before any UI is built.**

There is **no "partial" or "no" verdict** — proceed with the pure-Rust plan; the JS shim is a known, scoped contingency, not a redesign.

---

## 2. Architecture & Data Flow

Four pieces: **(A) on-chain program** (done), **(B) Dioxus WASM frontend**, **(C) Rust keeper**, **(D) Rust indexer+API**. A shared **`flipvault-sdk`** crate is the single ABI source of truth for B, C, and D.

**Three data planes feed the frontend** (the core design decision, agreed across all designs):
1. **Live truth (RPC):** `config`, 4 `vault` PDAs, `reserve`/`treasury` balances, the user's `position` PDAs — one `get_multiple_accounts` call + `account_subscribe` websocket for instant flip repaint. Round countdown is **pure client math** off `config.last_settled_ts + round_secs` (zero network/tick).
2. **History/aggregation (Indexer API):** rounds, flips, deposits/withdraws, leaderboard, per-wallet PnL, fee totals, vault time-series — REST/SSE over `gloo-net`.
3. **Writes (wallet):** frontend builds the ix with `flipvault-sdk`, wallet signs+sends, optimistic UI, reconcile on confirm/websocket push.

```
                         SOLANA DEVNET/MAINNET
        program EkfN5...rV4H  +  ORAO VRF (VRFzZoJ...Dr7y)
        PDAs: config, reserve, treasury, vault0..3, position[owner,vid,slot]
   ┌──────────┬───────────────────┬───────────────────┬──────────────────┐
   │ (A1) RPC │ (A2) RPC reads     │ ingest (gRPC/poll) │ commit/settle/    │
   │  send tx │  + accountSubscribe│  + getTransaction  │ recover (signed)  │
   ▼          ▼                    ▼                    ▼
┌─────────────────────────┐   ┌──────────────────┐   ┌──────────────────┐
│ (B) FRONTEND  Dioxus/WASM│   │ (D) INDEXER+API   │   │ (C) KEEPER        │
│  static on CDN          │   │  Carbon→Postgres   │   │  native tokio     │
│  • wallet-adapter sign  │   │  +Axum REST/SSE    │   │  1Hz tick:        │
│  • wasm_client_solana   │◄──┤  history/leaderbd/ │   │  Idle→commit_round│
│    live read+subscribe  │(2)│  positions/fees    │   │  Pending→settle   │
│  • gloo-net → API (2)   │   │  /state/current    │   │  stuck→recover    │
└───────────┬─────────────┘   └────────┬───────────┘   └────────┬─────────┘
            │ build ix from             │ decode via               │ build ix from
            └──────────► flipvault-sdk ◄─┴──────────────────────────┘
                       (PDAs · ix builders · borsh account decoders · ids)
```

Keeper and indexer talk **only** to the RPC provider and share `flipvault-sdk` types; they run fully independently (keeper works with indexer down and vice-versa). The frontend countdown reads on-chain `Config`, **not** the keeper — no keeper coupling.

---

## 3. Monorepo / Cargo Workspace Layout

**Two cargo workspaces, one git repo.** Workspace A (the deployed Anchor program) is left untouched — its SBF/fat-LTO profile and `Cargo.lock` must never be polluted by the wasm32 + web-sys + getrandom-js graph. Workspace B is the app.

```
flipsol/                              # repo root (one git repo)
  flipvault/                          # WORKSPACE A — EXISTING, DO NOT TOUCH
    Cargo.toml / Cargo.lock           #   SBF lockfile, isolated
    Anchor.toml
    programs/flipvault/
    target/idl/flipvault.json         # * canonical IDL consumed by app crates
    target/types/flipvault.ts
    scripts/*.ts                      #   existing TS client = golden test oracle

  app/                                # WORKSPACE B — NEW (separate Cargo.lock)
    Cargo.toml                        #   [workspace] resolver=2
    rust-toolchain.toml               #   host + wasm32-unknown-unknown target
    .cargo/config.toml                #   getrandom_backend="wasm_js" (see §4/§6)
    crates/flipvault-sdk/             # * SHARED ABI crate (wasm + native)
      src/{ids,discriminator,pda,ix,state,error}.rs
      tests/discriminators.rs
    frontend/                         #   Dioxus 0.7 CSR → WASM
      Dioxus.toml ; assets/tailwind.css
      src/{app,components,rpc,api,wallet}.rs
    keeper/                           #   native tokio bin
      Dockerfile ; src/{main,round,recover,land,config}.rs
    indexer/                          #   native bin: Carbon ingest + Axum API
      Dockerfile ; migrations/ ; src/{main,ingest,decoders,db,api}.rs
  docker-compose.yml                  #   local: postgres + keeper + indexer
  .github/workflows/ci.yml            #   matrix: wasm32 build + native build/test + sqlx check
  docs/                               #   this plan + runbook
```

**Why two workspaces:** two `Cargo.lock` files = zero cross-contamination of profiles/deps across incompatible targets (SBF vs wasm32+native), while keeping atomic commits and one IDL source. App crates consume the program **only** via the static IDL JSON and the hand-written mirrors in `flipvault-sdk` — they never compile the program crate.

---

## 4. `flipvault-sdk` (Shared Crate)

The linchpin: one definition of every PDA seed, account layout, discriminator, and instruction — consumed identically by frontend (wasm), keeper, and indexer (native). **Pure, synchronous, side-effect-free: computes addresses, instruction bytes, and decoded structs from `&[u8]`. It never fetches, signs, or sends.**

**WASM-safe crate choices (the load-bearing decision):** depend **only** on lean, wasm-clean micro-crates; **forbid** `solana-client`, `anchor-client`, `solana-sdk`, `solana-program`, and `anchor-lang` (all either don't compile to wasm32 or drag heavy graphs / getrandom-transitive churn). Hand-roll the Anchor discriminator (`sha256("global:<ix>")[..8]`) — ~30 lines, removes all version-coupling risk.

| Crate | Version | Role |
|---|---|---|
| `solana-pubkey` | pin one line, **v3** (`features=["borsh","curve25519"]`) | `Pubkey`, `find_program_address`, `pubkey!` |
| `solana-instruction` | matching v3 (`features=["borsh"]`) | `Instruction` + `AccountMeta` |
| `borsh` | `1.5` (`derive`) | args + account (de)serialization — Anchor's wire format |
| `sha2` | `0.10` (no_std) | discriminator hashing |
| `thiserror` | `1` (gated behind `std`) | `DecodeError` |

> Reconciliation note: one design suggested pinning `solana-pubkey 2.x` "for devnet parity." **Override: pin v3** to stay consistent with `wasm_client_solana 0.10` and `wallet-adapter 1.4.2` (the frontend's chosen lane). The keeper/indexer then use a `solana-client` whose re-exported `solana-pubkey` matches v3; CI builds SDK+client together to catch dual-type errors.

**Exports:**
- **`ids`** — `PROGRAM_ID = EkfN5...rV4H`, `ORAO_VRF_ID = VRFzZoJ...Dr7y`, `SYSTEM_PROGRAM_ID`.
- **`pda`** — `config`/`reserve`/`treasury`/`vault(id)`/`position(owner,vid,slot)` + ORAO `network_state` (`[b"orao-vrf-network-configuration"]`) / `randomness(seed)` (`[b"orao-vrf-randomness-request", seed]`).
- **`ix`** — builders returning `Instruction` for all 7 ixs: `deposit`, `withdraw`, `commit_round`, `settle_round`, `recover_round` (+ `initialize`, `sweep_treasury`). Args borsh-serialized after the 8-byte discriminator; AccountMetas in **exact IDL order**. Verified discriminators (asserted in tests against `sha256` and the IDL): `deposit=[242,35,198,137,82,225,242,182]`, `withdraw=[183,18,70,156,148,109,161,34]`, `commit_round=[229,102,157,34,152,217,15,70]`, `settle_round=[40,101,18,1,31,129,52,77]`, `recover_round=[87,173,77,57,40,45,2,160]`.
- **`state`** — `Config`, `Vault`{`[Tranche;2]`}, `Tranche`, `Position`, enums `RoundPhase{Idle,Pending}` / `Asset{Sol,Token}` (borsh u8 tags), plus ORAO `NetworkState` (so the keeper reads `config.treasury`). Decoders strip the 8-byte discriminator, verify it (`sha256("account:<Name>")[..8]`), then `try_from_slice`. **Model `u128` (`r_tok`,`k`), `i64` timestamps, `[u8;32] round_seed` exactly.**

**Correctness guards (CI):** (1) `disc_matches_idl` test asserts every hashed discriminator equals the IDL constant; (2) a live-devnet decode test fetches real `Config`/`Vault`/`Position` and asserts sane fields; (3) a `cargo build -p flipvault-sdk --target wasm32-unknown-unknown` job + a `cargo tree` assertion that forbidden crates are absent.

---

## 5. Frontend (Dioxus)

**Stack:** `dioxus 0.7.9` (`web`,`router`) — the framework ORE uses; `dioxus-cli (dx) 0.7.x` for `dx serve`/`dx bundle --platform web --release`; `dioxus-router 0.7.x`; `wasm_client_solana 0.10` (RPC + `WebSocketProvider`); `wallet-adapter 1.4.2`; `dioxus-sdk` timing (`use_interval`) with a `gloo_timers::future::TimeoutFuture` loop fallback; `gloo-net` for the indexer API; `web-time` for wasm-safe time; `borsh 1.x` + `flipvault-sdk` for decoding; **Tailwind 3.x** compiled to a CSS `asset!`.

**Architecture — single-page CSR WASM app.** No SSR (the indexer is a separate Rust service). `dx bundle` emits a static `public/` dir for any CDN.

**Component tree:**
```
App → document::Link{tailwind} → Router<Route>
  #[layout(AppShell)]  (provides WalletCtx + ChainCtx; spawns ws subscriber)
    Header{ logo · RoundTimer pill · ConnectWalletButton }
    "/"           Home      { VaultGrid (4 cards) + ActionPanel (Deposit/Withdraw) }
    "/positions"  Positions { user position list (from API) }
    "/history"    History   { paginated rounds/flips (from API, SSE live) }
    "/leaderboard"Leaderboard{ ranked PnL/volume (from API) }
```

**Reactive live-state strategy (ORE secret sauce):**
- **`ChainCtx`** signal `{config, vaults:[Option<Vault>;4], reserve_lamports, treasury_lamports}`, refreshed by **(a)** one `get_multiple_accounts([config, vault0..3, reserve, treasury])` decoded via `flipvault-sdk`, **(b)** an `account_subscribe` websocket on `config`+4 vaults that bumps a `tick` signal → instant re-fetch on settle (a flip mutates `config.selected_vault` + the selected vault's tranches), **(c)** a 5s `use_interval` poll **fallback** when WS drops, with auto-reconnect+backoff.
- **Countdown:** `use_interval(1s)` derives `seconds_left = max(0, (last_settled_ts + round_secs) - now())` with **zero network**; at 0 it shows "Flipping…" and relies on WS/poll to deliver the new `config`. Clock skew corrected once at load via cluster time → stored offset signal.
- **Optimistic writes:** on deposit/withdraw submit, mutate the local `Position`/`Vault` signal to expected post-state + pending badge; reconcile on confirm; rollback on error/timeout.
- **Cache-first paint:** hydrate last-known vault/config from `gloo-storage` so the board renders before first RPC returns; lazy-load History/Leaderboard after first paint.

**Build/host:** `dx bundle --platform web --release` (runs wasm-bindgen + wasm-opt). Release profile `opt-level="z"`, `lto=true`, `codegen-units=1`, `panic="abort"`, `strip=true`; brotli/gzip at CDN. **Host: Cloudflare Pages** (SPA fallback — omit a top-level 404.html so all routes serve `index.html`). Runtime config (`PROGRAM_ID`, `CLUSTER`, `RPC_URL`, `RPC_WS`, `INDEXER_API_URL`) baked at build via Dioxus consts or fetched `/config.json` — devnet→mainnet is a one-line switch.

---

## 6. Wallet + Transaction Layer

**Recommended (primary):** JamiiDao **`wallet-adapter 1.4.2`**, pure-Rust, Wallet-Standard (auto-detects Phantom/Solflare/Backpack). Bootstrap from its **`dioxus-adapter-anchor`** template (FlipVault is Anchor). Store the adapter in a Dioxus `GlobalSignal`; render a wallet picker from `adapter.wallets()` (not hardcoded Phantom) for the ORE-like UX.

**Mandatory build config** (workspace-root `app/.cargo/config.toml`, or the build will fail to link):
```toml
[target.wasm32-unknown-unknown]
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']
```

**Transaction flow (deposit/withdraw):**
1. User submits form → read connected pubkey from adapter (`connection_info().connected_account().public_key()`).
2. Build the instruction with **`flipvault-sdk::ix::deposit/withdraw`** (discriminator + borsh args; AccountMetas in IDL order).
3. `Transaction::new_with_payer(&[ix], Some(&user))`; fetch fresh blockhash **immediately before signing** (`wasm_client_solana`); set `message.recent_blockhash`.
4. **Submit path policy:** default to `sign_and_send_transaction(&bytes, Cluster::DevNet, SendOptions)` (one approval, simplest). **For near-round-boundary reliability / mainnet, prefer `sign_transaction` + our own `wasm_client_solana` `send_transaction` with prepended ComputeBudget priority-fee ixs** so we control RPC/retry. Runtime-check the wallet advertises `solana:signAndSendTransaction`; fall back to `sign_transaction` otherwise.
5. Optimistic UI; confirm by polling `get_signature_statuses` (~20×500ms) → force `ChainCtx` refresh.

**Critical UX guard:** deposit/withdraw are **blocked on-chain while `Config.phase == Pending`** (settling, ~every 30s) and fail with `RoundPending (6003)`. The wallet layer reads `Config.phase` + the countdown and **disables the action button during Pending** ("locked, settling…"), mapping 6000–6015 error codes to friendly toasts.

**Concrete fallback (only if the primary breaks against a target wallet):** the ORE JS-shim — a one-file webpack `@solana/wallet-adapter` island mounted into a hidden div, bridged via Dioxus `document::eval`: Rust `bincode`-serializes → base64 → `window.DwaTxSigner({b64})` signs → returns base64 → Rust deserializes + submits via `wasm_client_solana`; connected pubkey flows back via a `dwa-pubkey` CustomEvent. The tx-builder layer (`flipvault-sdk`) is unchanged — only the sign boundary swaps. This is the proven ore.supply path and is fully decoupled, so the swap is localized.

---

## 7. Keeper Service

A single native `tokio` binary: a **best-effort scheduler over an authoritative on-chain state machine** — it never trusts its own clock for correctness; it re-reads `Config` and lets the program reject races.

**Stack:** `tokio 1.x`; **raw `solana-rpc-client` (nonblocking) + hand-rolled ix builders from `flipvault-sdk`** (recommended over `anchor-client` to avoid Anchor's solana-* pins and keep tight control of priority fees/blockhash/retries); `orao-solana-vrf` **0.6.1** (lib only — must match the on-chain dep) for randomness PDA derivation + `RandomnessAccountData::fulfilled_randomness()`; `borsh 1.x`; `tracing` + `tracing-subscriber` (JSON); `prometheus` + `axum 0.7` for `/metrics`+`/healthz`; `figment`/`envy` config; `backon` for retry/backoff.

**Main loop (1 Hz `tokio::time::interval`, `MissedTickBehavior::Delay`):** each tick fetch `Config` at `confirmed`, read the validator clock (`get_block_time(get_slot())`), and branch on the fresh snapshot:
- `phase==Idle` and `now >= last_settled_ts + round_secs` → **commit_round**: generate non-zero 32-byte `force`; derive `randomness(force)` + `network_state`; decode `NetworkState` to read `orao_treasury = config.treasury` (live); build ix (accounts: keeper(signer,mut), config(mut), random(mut), orao_treasury(mut), network_state(mut), vrf, system_program) with prepended ComputeBudget ixs.
- `phase==Pending` and randomness **fulfilled** → **settle_round**: re-derive `randomness` from on-chain `Config.round_seed` (not local); accounts config(mut), reserve(mut), vault0..3(mut), random(ro).
- `phase==Pending` and `now >= commit_ts + 300` and not fulfilled → **recover_round** (config(mut) only).
- else → WAIT (a dedicated ~400–800ms VRF poller, bounded by the 300s recover window, handles fulfillment faster than 1 Hz).

**Concurrency guard:** an in-process `tokio::sync::Mutex` / `AtomicBool` ensures only one commit/settle/recover is outstanding. **Idempotency is delegated to the program:** `RoundPending`/`RoundTooSoon`/`NoPendingRound`/`RandomnessNotResolved`/`RecoverTooSoon` are caught, logged at debug as **benign races**, and advance the loop — never retried blindly or treated as errors. Distinct buckets: transient RPC → backoff; blockhash expired → rebuild + bump fee; insufficient funds → **alert** + pause commits (keep settling); unknown → alert + keep looping.

**Tx landing (shared):** fetch confirmed blockhash + `lastValidBlockHeight`; sign; send; **rebroadcast the same signed tx every ~2s** until confirmed or blockhash expires, then rebuild with fresh blockhash + bumped priority fee (from `getRecentPrioritizationFees`, floor+cap). `skip_preflight=false` on devnet, flag to flip true on mainnet.

**Crash safety:** stateless — all durable state is on-chain; restart re-reads `Config` and resumes mid-round (covers the commit-crash-before-settle case automatically).

**Secrets:** keeper keypair from env (`KEEPER_KEYPAIR`, Fly/k8s secret/KMS), never in the image. It is **not** `treasury_authority` and cannot sweep funds — leaked-key blast radius is limited to grief (rate-limited by the on-chain time guard).

**Hosting:** distroless Docker, **exactly one replica** (the `Config.phase` global lock makes multiple instances correct-but-wasteful, racing to pay OREO fees), restart-always. `/healthz` returns 200 only if the last `Config` read < N s ago **and** keeper balance > min-fee floor. Alert on "no settle in 3× round_secs" and "balance < N rounds of fees."

---

## 8. Indexer + API

**Decisive finding (from reading the IDL): the program emits NO events** — no `events` array, no custom logs. Every fact (selected_vault, flip SOL moved, shares minted, withdrawal fee) must be reconstructed from **(1) decoded instruction args + (2) account-state pre/post deltas** (from Geyser/Yellowstone in-tx pre/post data, or `getMultipleAccounts` at slot vs slot-1 on plain RPC). Log subscription is only a cheap "a flipvault tx happened" trigger.

**Stack:** **Carbon** (`carbon-core 0.12` + `carbon-rpc-program-subscribe-datasource` for devnet, `carbon-yellowstone-grpc-datasource` for mainnet, `carbon-rpc-transaction-crawler-datasource` for backfill) as the pipeline skeleton (one `Processor` across realtime/backfill/snapshot; same code devnet→mainnet by swapping a datasource line). Carbon is an *accelerator, not a hard dependency* — if its versions conflict, fall back to a hand-rolled `logsSubscribe`/`getTransaction` loop with no schema/API change. Plus `solana-client`/`solana-transaction-status` (v3 lane, matching the SDK), `flipvault-sdk` `state.rs` decoders (so API and frontend agree byte-for-byte), `axum 0.8` + `tower-http`, `sqlx 0.8` (compile-checked, `PgListener`), **Postgres 16**, optional `async-graphql 7.0.15` on the same router.

**Two processes, one Postgres, shared `flipvault-sdk`:** indexer (3 tasks: realtime tail, tx decoder computing deltas, backfill crawler — all converge on idempotent upserts keyed by `(tx_signature, instruction_index)`, then `pg_notify`) and Axum API (PgPool reads + a `PgListener`→`broadcast`→SSE fan-out).

**Postgres schema (concise):**
- `rounds(round_seed PK, commit_sig, commit_slot/ts, settle_sig, settle_slot/ts, selected_vault, outcome{pending|settled|recovered}, status)`
- `flips(settle_sig, instruction_index, round_seed FK, vault_id, sol_in_lamports, tok_out NUMERIC(40,0), reserve_delta, status, PK(settle_sig,instruction_index))`
- `deposits(sig, ix_index, owner, vault_id, tranche_slot, amount_lamports, shares_minted, status, PK(sig,ix_index))`
- `withdrawals(... shares_burned, gross_lamports, fee_lamports, net_lamports, status ...)`
- `vault_snapshots(vault_id, tranche_slot, slot, write_version, asset, amount, total_shares, vault_lamports, status, PK(vault_id,tranche_slot,slot,write_version))`
- `positions(owner, vault_id, tranche_slot, shares, last_slot, status, PK(owner,vault_id,tranche_slot))`
- `fees(sig, ix_index, kind{accrual|sweep}, amount_lamports, recipient, status)`
- `cursor(newest_sig, backfill_oldest_sig)`

Use `NUMERIC(40,0)` for u128/virtual-token quantities; `BIGINT` for lamports/shares (u64 fits < 2^63). **Reorg handling:** every row carries `status ∈ {confirmed,finalized,orphaned}`; write at `confirmed` for speed, a finality task re-checks `getSignatureStatuses` and flips to `finalized`/`orphaned`; **all API reads filter `status<>'orphaned'`**; reorg cleanup is a targeted flag by signature, never a rebuild. Only write a `flips` row when `Config.selected_vault != NO_VAULT` (255); `recover_round` → `outcome='recovered'`; skip txs with `err != null`.

**Endpoints (REST; GraphQL optional/additive):**
`/state/current` (ORE-instant first paint: latest snapshots + open round + countdown, with a live `getMultipleAccounts` fallback if the indexer lags) · `/rounds` (paginated) · `/rounds/{seed}` · `/flips?vault_id=` · `/vaults/{id}/history` · `/vaults/{id}/series?bucket=` · `/users/{owner}/positions` · `/users/{owner}/pnl` · `/users/{owner}/history` · `/leaderboard?metric=` · `/fees/totals` · `/sse/stream` (live push via PG NOTIFY→broadcast).

---

## 9. Deployment Topology

| Piece | Runs on | Notes |
|---|---|---|
| **Frontend** | Cloudflare Pages (static WASM, SPA fallback, global CDN) | `dx bundle --platform web --release`; build-time env bakes `PROGRAM_ID`/`CLUSTER`/`RPC_URL`/`RPC_WS`/`INDEXER_API_URL` |
| **Keeper** | Fly.io machine (distroless Docker), **1 replica**, always-on, restart-always | secret `KEEPER_KEYPAIR`; `/metrics`+`/healthz` on :9100 |
| **Indexer+API** | Fly.io machine (Docker), 1 replica | exposes :8080 API; holds `DATABASE_URL`+`RPC_URL`/`GRPC_URL` |
| **Postgres** | Neon/Supabase managed | `DATABASE_URL` as secret |
| **RPC** | **Helius** (free tier devnet → Developer $49/mo mainnet for enhanced WS; LaserStream gRPC for the indexer on higher tiers) | public `api.devnet.solana.com` only as fallback — it rate-limits and drops WS |

**Secrets:** typed `Config` per binary (`figment`/`envy`) from env. Non-secrets in per-env `.env`; secrets (`KEEPER_KEYPAIR`, `DATABASE_URL`, `HELIUS_API_KEY`) via Fly secrets / SOPS / CF Pages env. `PROGRAM_ID` is a constant in `flipvault-sdk`; **cluster is the only thing that changes devnet→mainnet.**

**CI/CD (GitHub Actions):** job-1 wasm (`rustup target add wasm32-unknown-unknown`, `dx build --platform web`, wasm-opt size gate, forbidden-crate `cargo tree` check); job-2 native (`cargo build -p keeper -p indexer`, `cargo test`, `cargo sqlx prepare --check` against ephemeral Postgres); job-3 (rare) `anchor build` to refresh the IDL. On `main`: build+push keeper/indexer images, `flyctl deploy` each; CF Pages auto-builds the frontend.

**devnet→mainnet promotion:** code is cluster-agnostic. (1) point RPC/GRPC at mainnet Helius, (2) fund + set the mainnet keeper keypair, (3) fresh mainnet Postgres, (4) run `initialize` on mainnet, (5) flip program upgrade authority to `--final` when satisfied.

---

## 10. Sequenced Build Milestones

> **MILESTONE 0 is a hard gate. Nothing else starts until it passes.** It is the smallest possible spike that proves the wallet+tx-in-WASM thesis against the *live devnet program*. If M0 fails on the pure-Rust path, switch to the JS-shim wallet fallback (§6) **before** building anything else — the rest of the plan is unaffected.

**M0 — Prove wallet + tx in WASM against live devnet (THE GATE).**
First actions: (1) `cargo new app` workspace + `flipvault-sdk` skeleton with `ids`/`pda`/`ix::deposit` only; commit `app/.cargo/config.toml` with `getrandom_backend="wasm_js"`. (2) `cargo build -p flipvault-sdk --target wasm32-unknown-unknown` — must compile clean. (3) Add `disc_matches_idl` test (asserts `deposit=[242,35,198,...]`). (4) `cargo generate` the `dioxus-adapter-anchor` template; wire `wallet-adapter 1.4.2` connect → read pubkey; `wasm_client_solana 0.10` `get_latest_blockhash`; build a real `deposit` ix from `flipvault-sdk`, sign + send via the wallet, **confirm a real tx on devnet explorer**. **Exit criteria:** Phantom connects, a deposit lands on `EkfN5...rV4H`, the Position PDA shows the shares. If the wallet/RPC version lanes fight (the `v-rpc` sharp edge) → resolve by pinning all solana-* to one v3 line; if still broken → adopt JS-shim fallback and re-run M0.

**M1 — `flipvault-sdk` complete + verified.**
Finish all 7 ix builders, all account decoders (`Config`/`Vault`/`Position`/ORAO `NetworkState`), error enum. Add the live-devnet decode test (fetch real `Config`/`Vault`, assert sane fields) and the forbidden-crate CI check. This crate is now frozen-ish; frontend/keeper/indexer build on it.

**M2 — Keeper (drives the rounds the UI will visualize).**
Native tokio loop + `land_tx` + ORAO poll + error taxonomy. First action: port `scripts/round.ts` → Rust `run_round` using `flipvault-sdk` ix builders; run against devnet and confirm `selected_vault` changes each round. Add `/healthz`+`/metrics`. **Reason to do early:** the frontend countdown/flip UX is meaningless without rounds actually settling on schedule.

**M3 — Indexer + API.**
Migrations + Carbon (or hand-rolled) backfill crawler + tx-delta decoder + Axum. First actions: `carbon-cli parse --idl ../../flipvault/target/idl/flipvault.json`; implement `handle_settle`/`handle_deposit`/`handle_withdraw` computing deltas; stand up `/state/current` + `/leaderboard` + `/users/{owner}/positions` + `/sse/stream`. Verify rows against the existing TS scripts as oracle.

**M4 — Frontend full UI on live data.**
Build `AppShell` + `ChainCtx` (`get_multiple_accounts` + `account_subscribe` + 5s poll fallback) + `RoundTimer` (client-side) + `VaultGrid` + Deposit/Withdraw `ActionPanel` (with `phase==Pending` button-disable + fee preview) + Positions/History/Leaderboard pages on the indexer API. Optimistic writes + cache-first paint.

**M5 — Harden + deploy devnet.**
Wasm size pass (`opt-level=z`/lto/wasm-opt, brotli), WS reconnect/backoff, clock-skew offset, error toasts (6000–6015). Dockerize keeper+indexer → Fly.io; frontend → Cloudflare Pages; Neon Postgres; Helius devnet RPC. End-to-end smoke: connect → deposit → watch a flip repaint → withdraw → see it in history.

**M6 — Mainnet readiness.**
Vendor/fork `wallet-adapter` to remove single-maintainer risk (or confirm JS-shim path); switch indexer to Yellowstone gRPC; paid Helius mainnet; priority-fee tuning + `skip_preflight=true`; keeper hot-wallet top-up watcher; run `initialize` on mainnet; flip upgrade authority `--final`.

---

## 11. Risk Register

| # | Risk | Severity | Fallback / Mitigation |
|---|---|---|---|
| 1 | **solana-* version skew** (`wasm_client_solana` v3 vs Anchor 2.1 / wallet adapter) → duplicate `Pubkey` types | High | Pin **one v3 lane** across the workspace; CI builds SDK+client together. If unresolvable, drop to JS-shim + `web3.js` for the RPC/sign boundary. |
| 2 | **getrandom wasm build fails to link** | High | Commit `getrandom_backend="wasm_js"` rustflag at `app/.cargo/config.toml` root from day one (M0); it's in the adapter template. |
| 3 | **Pure-Rust wallet adapter is single-maintainer / could strand mainnet** | Med | Keep sign boundary decoupled (only `sign_and_send_transaction(&[u8])`); **vendor/fork before mainnet** (M6) or swap to the ORE JS-shim — localized change. |
| 4 | **Program emits NO events** → naive log indexing yields zero economics | Critical | Indexer is built on account-state **deltas** + instruction args (Geyser pre/post, or `getMultipleAccounts` at slot vs slot-1). Already baked into the design. |
| 5 | **Account-layout/discriminator drift** (hand-rolled borsh mirrors) | High | `disc_matches_idl` test + live-devnet decode test in CI; generate from `flipvault.json` if drift ever appears. Program is frozen, so risk is low. |
| 6 | **Keeper SPOF / under-funded / hot key leak** | High | 1 replica + restart-always + heartbeat; auto-`recover_round`; balance alert + pause-commits floor; key can't sweep funds (only grief). |
| 7 | **ORAO stalls / round stuck Pending** | Med | Bounded VRF poll → auto `recover_round` after 300s; frontend shows "settling…" on `phase==Pending`; keep `recover.ts` as manual fallback. |
| 8 | **Blockhash expiry** (user lingers in wallet popup on a 30s game) | High | Fetch blockhash **immediately before sign**, not at page load; surface a distinct Timeout state + one-click retry; versioned tx + fresh blockhash per attempt. |
| 9 | **Public devnet RPC rate-limits / drops WS** → breaks instant feel | Med | Helius from M5; treat `account_subscribe` as accelerator with 5s poll fallback; indexer SSE as a second live source. |
| 10 | **WASM bundle bloat** (multi-MB → slow first paint) | Low | `opt-level=z`+lto+strip + wasm-opt -Oz + brotli; lazy-load History/Leaderboard; keep curve25519/blake3 features minimal. |
| 11 | **Carbon (0.12, young) version conflicts** | Med | Carbon is an accelerator: decoder+store are framework-agnostic; fall back to hand-rolled `logsSubscribe`/`getTransaction` loop with no schema change. |
| 12 | **Devnet reorgs corrupt aggregates** | Med | `status ∈ {confirmed,finalized,orphaned}` two-phase; reads filter orphaned; finalized = leaderboard source of truth. |

---

## 12. Open Decisions for the Owner

1. **Submit path:** default to wallet `sign_and_send` (simplest) or always `sign_transaction` + our own RPC with priority fees (better near round boundaries / mainnet)? **Recommend:** `sign_and_send` for devnet MVP, switch to self-submit at M6.
2. **RPC provider commitment:** budget Helius Developer ($49/mo) for mainnet + LaserStream gRPC for the indexer now, or defer? Public devnet is fine *only* through M4.
3. **Leaderboard PnL semantics:** realized-only (net of the 10% withdraw fee) vs mark-to-market against the live curve (`k`, `r_tok`) for open positions? Mark-to-market is more ORE-like but heavier. **Recommend:** realized for MVP, MTM later.
4. **Confirm three on-chain constants** (frozen-program facts the app must mirror exactly): the `NO_VAULT` sentinel for `selected_vault` (assumed **255**); `fee_bps` unit (assumed 1000 = 10%); and whether `settle_round` can legitimately **skip** a flip (min_reserve floor) — if so the indexer needs a `skipped` outcome. *(Resolve by reading `flipvault.json` + program constants — gates M3/M4 correctness.)*
5. **GraphQL now or REST-only?** GraphQL is low marginal cost on the same Axum router and suits composed leaderboard/chart queries. **Recommend:** REST first, add GraphQL only if the frontend needs it.
6. **Mobile scope for devnet:** "works in Phantom/Solflare in-app browser + deep-link CTA" sufficient now, or first-class Mobile Wallet Adapter soon? **Recommend:** in-app-browser + deep-link CTA for devnet; MWA is a later, separate target.
7. **Indexer combined vs split binary:** start combined (shared pool/types); split only if the Axum read path contends with the Carbon write path. **Recommend:** combined.