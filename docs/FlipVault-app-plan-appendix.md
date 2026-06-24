# FlipVault App Plan — Raw Layer Designs & Verdicts (appendix)

_From the planning workflow. Synthesized plan: FlipVault-app-plan.md._

## Adversarial WASM verdicts

### v-wallet — feasible: **yes** (confidence: high)

**Claim:** The Rust 'wallet-adapter' crate can connect a Solana wallet AND sign+send a real transaction from a Dioxus WASM browser app on devnet today, with acceptable maturity for production.

**Evidence:**
- wallet-adapter crate (JamiiDao/SolanaWalletAdapter) is at v1.4.2, released 2026-02-18, ~10,475 total downloads (crates.io/api/v1/crates/wallet-adapter). It has been on a STABLE 1.x line since 1.1.0 (2025-03-04); the 1.0.x-beta era ended over a year ago, so the 'beta' framing in the claim is outdated.
- docs.rs (docs.rs/wallet-adapter/latest) confirms the WalletAdapter struct exposes connect()/connect_by_name(), disconnect(), sign_message(), sign_transaction(), sign_and_send_transaction(), and sign_in() (SIWS). The Cluster enum explicitly includes DevNet (https://api.devnet.solana.com). Signatures: sign_transaction(&tx_bytes, Some(cluster)) -> WalletResult<Vec<Vec<u8>>>; sign_and_send_transaction(&tx_bytes, cluster, SendOptions) -> WalletResult<Vec<u8>>.
- The project book (jamiidao.github.io/SolanaWalletAdapter) documents a working DEVNET sign-and-send example: build a system_instruction::transfer, bincode-serialize, then adapter.sign_and_send_transaction(&tx_bytes, Cluster::DevNet, send_options) returning a signature with an explorer.solana.com/tx/{sig}?cluster=devnet link.
- It is PURE RUST via the Wallet Standard browser events (wallet-standard:register-wallet / app-ready), so it detects any Wallet-Standard wallet (Phantom, Solflare, Backpack) — no JS adapter wrapper required.
- First-class Dioxus support: the repo ships `dioxus-adapter` AND `dioxus-adapter-anchor` cargo-generate templates (the latter is built around an Anchor IDL — directly relevant to FlipVault). Dioxus (>=0.6.0) is the recommended framework. Templates demonstrate connect, balance, airdrop (non-mainnet), Sign Message, Sign Transaction, SIWS, and 'Sign and send tx'.
- Dependency on solana-sdk ^2.1.2 (current SDK era), not the abandoned 1.18 line.
- GitHub open issues (9) are all feature/architecture requests (mobile MWA, Leptos template, move-off-Solana-SDK, cargo-audit in CI) — NONE report sign/send failures, devnet breakage, getrandom/wasm build errors, or Solana 2.x incompatibility.
- Corroborating model: ORE itself (the app the user is modeling) uses regolith-labs/dioxus-wallet-adapter, which is a JS-WRAPPED adapter (wraps @solana/wallet-adapter, ~18% JS, npm build step), pinned to an outdated Dioxus git rev (ffa36a6) + Solana 1.18, git-only/unpublished, 43 stars, ~11 commits, no releases. This proves Dioxus+Solana wallet sign/send works in production today, and gives a battle-tested fallback.

**Caveats:**
- Maturity is 'acceptable for a small/devnet-first production app' but NOT 'mature ecosystem'. The crate carries a 'Passively Maintained' badge, is effectively single-maintainer, ~32 GitHub stars / ~10k downloads. Bus-factor and breakage risk are real if Wallet Standard or solana-sdk shift; budget for vendoring/forking before mainnet.
- WASM build friction: requires RUSTFLAGS='--cfg getrandom_backend="wasm_js"' (or equivalent .cargo/config.toml) because getrandom 0.3 needs an explicit wasm_js backend on wasm32-unknown-unknown. This is documented and the templates pre-configure it, but a from-scratch (non-template) setup will fail to build until you add it.
- sign_and_send_transaction delegates RPC send to the wallet; for tighter control (preflight, your own RPC, confirmation) you may prefer sign_transaction + your own wasm RPC client (wasm_client_solana / solana-client-wasm). Confirm the connected wallet actually advertises the solana:signAndSendTransaction feature, since not all do.
- Template build tooling is opinionated (Trunk + Tailwind v4); integrating into an existing Dioxus 0.6/0.7 + Dioxus CLI project may need manual wiring rather than `cargo generate`.
- 'Real transaction on devnet' depends on a Wallet-Standard browser extension being installed (Phantom/Solflare/Backpack); there is no headless/keypair path in-browser, which is correct but worth stating.
- The claim's word 'beta' is stale — do not pin to a 1.0.x-beta; use latest 1.4.x.

**Fallback:** If the pure-Rust wallet-adapter crate proves too thin or breaks against current Solana/Dioxus, mirror ORE exactly: use regolith-labs/dioxus-wallet-adapter (or a thin wasm-bindgen shim) to wrap the battle-tested JS @solana/wallet-adapter for connect+sign, and build/sign transactions in Rust (anchor-style ix construction) handing bytes across the JS boundary for signing. This is the proven ore.supply path. A second fallback is to keep wallet connect/sign in a small TypeScript island (the JS @solana/wallet-adapter + @coral-xyz/anchor you already use in flipvault/scripts) and keep the rest of the UI in Dioxus/Rust — slightly less 'Rust everywhere' but maximally reliable for mainnet.

**Recommended:** Use the JamiiDao `wallet-adapter` crate at the latest 1.4.x (NOT a 1.0.x-beta) for connect + sign/send in the Dioxus WASM frontend. Bootstrap from the `dioxus-adapter-anchor` template since FlipVault is an Anchor program with an existing IDL. Add `.cargo/config.toml` with rustflag `--cfg getrandom_backend=\"wasm_js\"` for the wasm32-unknown-unknown target. For deposit/withdraw, construct the instruction in Rust against the flipvault IDL/program ID (EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H), bincode/serialize the transaction, and call sign_and_send_transaction(&tx_bytes, Cluster::DevNet, SendOptions) — or sign_transaction + a wasm RPC client if you want your own preflight/confirmation. Verify the connected wallet advertises solana:signAndSendTransaction at runtime and fall back to sign_transaction otherwise. Before mainnet, vendor/fork the crate (or switch to the ORE-style JS-wrapped adapter) to remove single-maintainer risk.

---

### v-rpc — feasible: **yes** (confidence: high)

**Claim:** We can perform the needed Solana RPC calls (getLatestBlockhash, getAccountInfo for config+vaults, sendTransaction, optionally accountSubscribe) from a Rust WASM app reliably enough for the FlipVault dApp.

**Evidence:**
- PRODUCTION PROOF (strongest): ore-app, the exact app the user explicitly models, is a Dioxus 0.6.1 -> WASM browser app whose Cargo.toml uses solana-client-wasm = "2.1" (web feature) patched to regolith-labs/solana-playground fork, alongside solana-sdk 2.1 / solana-extra-wasm 2.1. ORE.supply runs this in browsers at scale. Source: github.com/regolith-labs/ore-app/blob/master/Cargo.toml
- wasm_client_solana v0.10.0 (published 2025-11-08, actively maintained by ifiokjr, ~24k downloads) is purpose-built: 'A WebAssembly (WASM) compatible client for interacting with the Solana RPC and PubSub APIs ... sending transactions, fetching account data, subscribing to account changes ... from a WASM compatible environment like the web.' Confirmed to expose GetLatestBlockhashRequest, GetAccountInfoRequest, SendTransactionRequest, and accountSubscribe via Subscription/WebSocketNotification (pubsub). Source: docs.rs/wasm_client_solana, crates.io/crates/wasm_client_solana
- Multiple independent WASM RPC client crates exist and are maintained: wasm_client_solana (Solana SDK v3), solana-client-wasm 2.1 (regolith fork used by ORE), agsol-wasm-client (async reqwest-based). The standard solana-client blocking RpcClient does NOT compile to wasm32-unknown-unknown, which is exactly why these fetch/web-sys-based replacements exist and are used.
- Wallet connect + sign is solved in Rust/WASM via wallet-adapter (crates.io/crates/wallet-adapter, v1.x, JamiiDao/SolanaWalletAdapter) and wasi-sol, which bridge to the browser wallet (window provider / Wallet Standard) through wasm-bindgen/web-sys. regolith-labs/dioxus-wallet-adapter is the ORE-lineage adapter (uses solana-client-wasm). Signing is delegated to the JS wallet extension, not performed by Rust crypto in WASM.
- Build requirement is well-documented and stable: set RUSTFLAGS='--cfg getrandom_backend="wasm_js"' (or .cargo/config.toml) for wasm32-unknown-unknown so getrandom works in-browser. This is a known one-line config, not a blocker.
- Anchor 0.31.1 targets Solana 2.1 (anchor-lang re-exports solana-program 2.1). The on-chain program and a 2.1-based WASM client are version-aligned; instruction/account (de)serialization in the frontend is Borsh-over-IDL, so it does not require linking the on-chain crate.

**Caveats:**
- Version skew is the main sharp edge: wasm_client_solana 0.10.0 is on the Solana SDK v3 crates (solana-pubkey ^3, solana-transaction ^3), while Anchor 0.31.1 / the deployed program are on Solana 2.1. Mixing 2.x and 3.x solana-* crates in one Cargo tree causes duplicate-type conflicts (two Pubkey types, etc.). Pick ONE lane: either solana-client-wasm 2.1 (regolith fork, matches Anchor 0.31.1 cleanly, exactly the ORE stack) OR go all-in on the v3 crate family with wasm_client_solana and re-derive instruction/account encoding from Borsh/IDL rather than the anchor-client crate.
- @coral-xyz/anchor's TS client convenience (auto account resolution, .methods builder, event parsing) does NOT exist in Rust/WASM. You must hand-build instructions: compute PDAs (config/reserve/treasury/vault/position seeds), Borsh-serialize args with the 8-byte sighash discriminator, assemble AccountMetas in the documented order, and deserialize Config/Vault/Position from getAccountInfo bytes yourself. This is mechanical but is real work the claim glosses over.
- accountSubscribe over browser WebSockets is the least reliable piece: standard Solana WS pubsub is known to disconnect under load, only fires at end-of-slot, and many public RPCs throttle/drop WS. For an 'instant ORE-like' feel, do not depend solely on WS in the browser. ORE itself leans on configurable premium RPC (Helius/Triton/QuickNode).
- solana-client-wasm 2.1 lives only in a fork (regolith-labs/solana-playground) and is not published as 2.1 on crates.io (crates.io tops out at 1.18.0, Mar 2024). You take a git/path dependency on a fork that ORE maintains for itself, not a versioned upstream release — a supply-chain/maintenance consideration.
- Public/free RPC endpoints (incl. devnet) rate-limit getAccountInfo polling and sendTransaction; 'reliably enough' for a snappy multi-vault live UI realistically needs a paid RPC with reasonable limits, plus client-side retry/backoff and confirmation polling.

**Fallback:** If the all-Rust RPC path causes friction (version-conflict hell or flaky WS), keep RPC reads/writes in a thin JS/TS shim and call it from WASM: use wasm-bindgen to invoke the mature @solana/web3.js (or the existing @coral-xyz/anchor TS client you already have working in flipvault/scripts) and the JS wallet-adapter for connect/sign, while Dioxus owns rendering/state. This is a smaller, lower-risk surface and still 'Rust-everywhere' for UI/keeper/indexer. The most robust live-data fallback is to NOT rely on browser accountSubscribe at all: have the Rust indexer+API (piece 3) push current vault/round state over its own WebSocket/SSE to the frontend, and use direct getAccountInfo polling (250-500ms) only for the user's own optimistic actions.

**Recommended:** Mirror the ORE stack exactly, since it is the proven precedent: Dioxus -> WASM frontend using solana-client-wasm 2.1 (regolith-labs/solana-playground fork) + solana-sdk/solana-extra-wasm 2.1, which is version-aligned with Anchor 0.31.1. Use a Rust/WASM wallet adapter (regolith dioxus-wallet-adapter, or crates.io wallet-adapter / wasi-sol) for connect + delegate signing to the browser wallet. Add RUSTFLAGS='--cfg getrandom_backend=\"wasm_js\"'. Hand-build FlipVault instructions in Rust: PDA derivation per the documented seeds, 8-byte Anchor discriminator + Borsh args, AccountMetas in order; deserialize Config/Vault/Position from getAccountInfo. For the 'instant' live view, drive it from the Rust indexer+API via SSE/WebSocket push plus short-interval getAccountInfo polling, and treat browser accountSubscribe as a best-effort optimization, not the source of truth. Use a paid RPC (Helius/Triton/QuickNode) with retry/backoff; reserve sendTransaction confirmation via signature polling. If at any point the Rust RPC client fights the Anchor 2.1 types, fall back to a JS web3.js/anchor shim called from WASM rather than fighting crate-version conflicts.

---

## Layer designs


### Dioxus (Rust to WASM) Frontend for FlipVault dApp (frontend)

Design for the FlipVault web UI as a Dioxus 0.7 (released 2026-01-29; latest 0.7.9, 2026-05-08) single-page WASM app, modeled directly on ORE's stack (regolith-labs/ore-app is Dioxus + dx CLI + Tailwind). Wallet connect/sign uses JamiiDao `wallet-adapter` v1.4.2 (Feb 2026), a maintained pure-Rust Wallet-Standard adapter that compiles to WASM and ships official Dioxus templates (connect/disconnect, signTransaction, signMessage, SIWS, accountChanged events). Live chain reads use `wasm_client_solana` 0.10 (browser `js` feature) for getMultipleAccounts + a WebSocket `account_subscribe` provider. State is signals + `use_resource`; the round countdown is a pure client-side `use_interval` (dioxus-sdk) ticking off `last_settled_ts + round_secs` so it feels instant with zero network. Two data planes: RPC for live truth (config + 4 vaults + user reserve/treasury balances, instant flip via accountSubscribe websocket) and the indexer REST/GraphQL API for history/leaderboard/positions-list/fee totals (anything requiring aggregation or getProgramAccounts). Build/host: `dx bundle --platform web` emits a static `public/` dir (index.html + wasm + JS shim + hashed assets) served from any CDN/static host; Tailwind compiled to a CSS asset via the `asset!` macro. Component tree: App -> WalletProvider/ChainProvider context -> Router with Home (VaultGrid + RoundTimer + Deposit/Withdraw modal), Positions, History/Leaderboard pages, plus a global Header with ConnectWalletButton.

**Recommended stack:**
- dioxus 0.7.9 — UI framework (Rust -> WASM), web renderer, signals reactivity, RSX: Current stable line (0.7 released 2026-01-29, 0.7.9 on 2026-05-08). Same framework ORE (regolith-labs/ore-app) uses, so the ORE-like feel is a direct port of a proven stack. Provides signals, use_resource, asset! macro, and the dx CLI bundler.
- dioxus-cli (dx) 0.7.x (match dioxus) — dev server (dx serve), production bundler (dx bundle --platform web): Official build tool; ORE's README uses `dx serve`/`dx bundle`. Handles wasm-opt, asset hashing, Tailwind invocation, and emits a static dir. Preferred over Trunk because it natively understands the asset! macro, server-fns, and is the path ORE took.
- dioxus-router 0.7.x (match dioxus) — client-side routing via #[derive(Routable)] enum + Router::<Route>{}: First-party router; gives type-safe routes, nested #[layout] for the shared Header/footer, Link, and use_navigator. Needed for Home/Positions/History pages without full reloads.
- wallet-adapter (JamiiDao SolanaWalletAdapter) 1.4.2 — Browser Solana wallet connect/disconnect, signTransaction, signAndSendTransaction, signMessage, Sign-In-With-Solana, accountChanged/disconnect events: Maintained (latest 2026-02-19), pure-Rust, compiles to WASM, implements Wallet-Standard so it auto-discovers Phantom/Solflare/Backpack. Ships official Dioxus, Yew, Leptos, Sycamore templates. More mature/maintained than regolith-labs/dioxus-wallet-adapter (which pins a git Dioxus rev and local solana path deps).
- wasm_client_solana 0.10.0 — Browser RPC client (get_account, get_multiple_accounts, get_balance, send_transaction) + WebSocketProvider for account_subscribe: Maintained WASM-first Solana RPC + pubsub client targeting solana-sdk v3, with a `js` feature for the browser. Gives both the poll path (getMultipleAccounts on config+4 vaults) and the instant path (accountSubscribe websocket) for flip updates.
- dioxus-sdk (timing) or dioxus-time latest 0.7-compatible — use_interval hook for the 1s countdown tick and the RPC poll cadence: use_interval(Duration, FnMut) repeatedly fires a closure; ideal for the per-second round countdown (pure client math off last_settled_ts) and a coarse fallback poll. Avoids hand-rolling gloo-timers loops.
- solana-sdk / solana-program (wasm-compatible subset) v3.x (to match wasm_client_solana 0.10) — Pubkey, Instruction, Transaction, PDA derivation (Pubkey::find_program_address): Needed to derive config/reserve/treasury/vault/position PDAs and build deposit/withdraw instructions client-side before handing to the wallet for signing.
- borsh 1.x (Anchor 0.31-compatible) — Decode raw account bytes (skip 8-byte Anchor discriminator, then BorshDeserialize) into Config/Vault/Position structs: Anchor accounts are 8-byte discriminator + borsh. Decoding in Rust avoids shipping a JS Anchor client; mirror the on-chain struct layout and try_from_slice(&data[8..]).
- Tailwind CSS 3.x (CLI, compiled at build) — Styling, compiled to a static CSS asset referenced via asset! macro: Exactly what ORE does (build Tailwind CSS, then dx build). CDN is fine for devnet prototyping, but compiling input.css -> asset gives smaller bundles and purge for mainnet.
- gloo-net / gloo-timers latest — reqwest-free fetch to the indexer REST/GraphQL API; timer futures if not using dioxus-sdk: gloo-net::http::Request is the lean WASM fetch wrapper for history/leaderboard calls; gloo-timers::future::TimeoutFuture backs delays inside use_resource loops.

**Architecture:** SINGLE-PAGE WASM APP, client-rendered (no SSR needed for devnet; the indexer API is a separate Rust service). dx builds the web platform target.\n\nDATA PLANES (the core design decision):\n1) LIVE FROM RPC (truth, low-latency): Config PDA, the 4 Vault PDAs, reserve/treasury lamport balances, and the connected user's Position PDAs. These are small fixed accounts read with one `get_multiple_accounts([config, vault0..3, reserve, treasury])` call, decoded with borsh. For ORE-like instant flip reaction, also open an `account_subscribe` WebSocket on config + the 4 vaults; a settle_round flip mutates the selected vault's tranches and config.selected_vault, so the push notification updates the grid within ~1 RPC round-trip instead of waiting for the next poll. Polling via use_interval (e.g. every 5s) is the fallback when websocket drops.\n2) FROM INDEXER API (aggregation/history): round/flip history, leaderboard (ranked PnL/deposits), the full list of a wallet's positions across all (vault,slot) without scanning getProgramAccounts in the browser, cumulative fees to treasury, per-vault TVL time-series/sparklines. These are REST (e.g. GET /rounds, /leaderboard, /positions/:owner) or GraphQL, fetched with gloo-net inside use_resource keyed on the wallet pubkey / pagination signal.\n\nRULE OF THUMB: anything that must be exactly current and is a fixed known account -> RPC. Anything historical, aggregated, ranked, or requiring a scan -> indexer. The user's CURRENT position shares are read live from RPC (the Position PDA) for the deposit/withdraw forms; the user's position LIST and history come from the indexer.\n\nROUND COUNTDOWN: computed purely client-side. next_round_ts = config.last_settled_ts + config.round_secs. A use_interval(1s) decrements a derived signal `seconds_left = max(0, next_round_ts - now())`. When it hits 0 the UI shows \"Flipping...\" and relies on the websocket/poll to deliver the new config (new last_settled_ts) and the flipped vault, at which point the timer resets. Zero network per tick -> feels instant. Server time skew is corrected by reading the cluster clock once (getBlockTime / Clock) at load and storing an offset signal.\n\nGLOBAL STATE (shared via Dioxus context, provided at App root):\n- WalletCtx: WalletAdapter instance, connected: Signal<Option<Pubkey>>, wallets list, connect/disconnect/sign actions.\n- ChainCtx: SolanaRpcClient, ChainState signal { config: Option<Config>, vaults: [Option<Vault>;4], reserve_lamports, treasury_lamports }, refreshed by the websocket subscriber task + poll fallback.\n- Derived signals (use_memo): r_sol (reserve spendable), spot_price = r_sol/r_tok, per-vault share price, seconds_left.\n\nTRANSACTION FLOW (deposit/withdraw): user submits form -> derive PDAs in Rust -> build Anchor instruction (discriminator + borsh args: vault_id u8, slot u8, amount/shares u64) -> assemble Transaction with recent blockhash from RPC -> hand to wallet adapter signAndSendTransaction (wallet popup) -> on signature, optimistic UI (mark pending) -> confirm via RPC, then force a ChainCtx refresh. The 10% withdrawal fee is shown in the form preview (computed client-side from fee_bps) so the user sees net SOL before signing.\n\nROUTING (dioxus-router Routable enum): #[layout(AppShell)] wraps all pages with Header (logo, RoundTimer pill, ConnectWalletButton); routes: / -> Home (VaultGrid + ActionPanel), /positions -> Positions, /history -> History, /leaderboard -> Leaderboard. AppShell holds the WalletProvider/ChainProvider context so every page shares live state.\n\nBUILD + HOSTING: `dx bundle --platform web --release` produces a static directory (index.html, the .wasm, the JS glue/bootstrap, and hashed Tailwind CSS + image assets). This deploys to any static host / CDN (Cloudflare Pages, Vercel static, S3+CloudFront, GitHub Pages). RPC endpoint + indexer base URL come from a small runtime config (env baked at build via a Dioxus const, or a fetched /config.json) so devnet->mainnet is a one-line switch. wasm-bindgen + wasm-opt are run by dx automatically.

**Code sketches:**
```
// ---- Routing + app shell (dioxus-router 0.7) ----
use dioxus::prelude::*;
use dioxus_router::prelude::*;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[layout(AppShell)]
        #[route("/")]
        Home {},
        #[route("/positions")]
        Positions {},
        #[route("/history")]
        History {},
        #[route("/leaderboard")]
        Leaderboard {},
}

const TAILWIND: Asset = asset!("/assets/tailwind.css");

fn main() { dioxus::launch(App); }

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND }
        Router::<Route> {}
    }
}

#[component]
fn AppShell() -> Element {
    // Provide global contexts once, above all pages.
    use_context_provider(|| WalletCtx::new());
    use_context_provider(|| ChainCtx::new());
    spawn(subscribe_chain());      // websocket account_subscribe task
    rsx! {
        Header {}
        main { class: "mx-auto max-w-5xl px-4", Outlet::<Route> {} }
    }
}
```

```
// ---- On-chain account decoding: 8-byte Anchor discriminator + borsh ----
use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::pubkey::Pubkey;

#[derive(BorshDeserialize, Clone, PartialEq)]
#[repr(u8)] enum Asset { Sol, Token }
#[derive(BorshDeserialize, Clone, PartialEq)]
struct Tranche { asset: Asset, amount: u64, total_shares: u64 }
#[derive(BorshDeserialize, Clone, PartialEq)]
struct Vault { vault_id: u8, tranches: [Tranche; 2], bump: u8 }
#[derive(BorshDeserialize, Clone, PartialEq)]
struct Config {
    treasury_authority: Pubkey, r_tok: u128, k: u128, round_secs: i64,
    last_settled_ts: i64, fee_bps: u16, min_reserve: u64,
    phase: u8 /*0=Idle,1=Pending*/, round_seed: [u8;32], commit_slot: u64,
    commit_ts: i64, selected_vault: u8, bump: u8, reserve_bump: u8, treasury_bump: u8,
}

fn decode<T: BorshDeserialize>(data: &[u8]) -> Option<T> {
    if data.len() < 8 { return None; }
    T::try_from_slice(&data[8..]).ok()   // skip discriminator
}

fn vault_pda(program: &Pubkey, id: u8) -> Pubkey {
    Pubkey::find_program_address(&[b"vault", &[id]], program).0
}
fn position_pda(program: &Pubkey, owner: &Pubkey, vid: u8, slot: u8) -> Pubkey {
    Pubkey::find_program_address(
        &[b"position", owner.as_ref(), &[vid], &[slot]], program).0
}
```

```
// ---- Live chain state: one getMultipleAccounts + decode (wasm_client_solana 0.10) ----
use wasm_client_solana::{SolanaRpcClient, DEVNET};

#[derive(Clone, Default, PartialEq)]
struct ChainState {
    config: Option<Config>,
    vaults: [Option<Vault>; 4],
    reserve_lamports: u64,
    treasury_lamports: u64,
}

async fn fetch_chain(rpc: &SolanaRpcClient, pid: &Pubkey) -> ChainState {
    let cfg_pda  = Pubkey::find_program_address(&[b"config"],  pid).0;
    let res_pda  = Pubkey::find_program_address(&[b"reserve"], pid).0;
    let trez_pda = Pubkey::find_program_address(&[b"treasury"],pid).0;
    let keys: Vec<Pubkey> = std::iter::once(cfg_pda)
        .chain((0..4u8).map(|i| vault_pda(pid, i)))
        .chain([res_pda, trez_pda]).collect();
    let accts = rpc.get_multiple_accounts(&keys).await.unwrap_or_default();
    let mut st = ChainState::default();
    if let Some(Some(a)) = accts.get(0) { st.config = decode::<Config>(&a.data); }
    for i in 0..4 { if let Some(Some(a)) = accts.get(1 + i) { st.vaults[i] = decode::<Vault>(&a.data); } }
    if let Some(Some(a)) = accts.get(5) { st.reserve_lamports  = a.lamports; }
    if let Some(Some(a)) = accts.get(6) { st.treasury_lamports = a.lamports; }
    st
}
```

```
// ---- use_resource poll fallback + use_interval-driven refresh trigger ----
use dioxus_sdk::utils::timing::use_interval;
use std::time::Duration;

#[component]
fn ChainProvider(children: Element) -> Element {
    let rpc = use_signal(|| SolanaRpcClient::new(DEVNET));
    let pid = use_signal(|| PROGRAM_ID); // EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H
    let mut tick = use_signal(|| 0u64);

    // Coarse poll fallback (websocket is the fast path); bump `tick` every 5s.
    use_interval(Duration::from_secs(5), move || { tick += 1; });

    // Re-runs whenever `tick` (poll) OR a websocket push (also bumps tick) changes.
    let chain = use_resource(move || async move {
        let _ = tick();                 // subscribe to the trigger
        fetch_chain(&rpc(), &pid()).await
    });
    use_context_provider(|| chain);
    rsx! { {children} }
}
```

```
// ---- Instant flip via accountSubscribe websocket (the ORE-like fast path) ----
async fn subscribe_chain() {
    let provider = WebSocketProvider::new(DEVNET_WS); // wss endpoint
    let mut tick = use_context::<Signal<u64>>();      // shared refresh trigger
    let pid = PROGRAM_ID;
    let cfg = Pubkey::find_program_address(&[b"config"], &pid).0;
    // subscribe to config (selected_vault/last_settled_ts change on settle)
    let mut sub = provider.account_subscribe(&cfg).await.unwrap();
    while let Some(_notification) = sub.next().await {
        // A flip just landed -> trigger a fresh getMultipleAccounts decode.
        tick.with_mut(|t| *t += 1);
    }
}
```

```
// ---- Pure client-side round countdown (zero network per tick) ----
#[component]
fn RoundTimer() -> Element {
    let chain = use_context::<Resource<ChainState>>();
    let mut now = use_signal(|| js_now_secs());
    use_interval(Duration::from_secs(1), move || now.set(js_now_secs()));

    let secs_left = use_memo(move || {
        let st = chain.read();
        let cfg = st.as_ref().and_then(|s| s.config.as_ref())?;
        let next = cfg.last_settled_ts + cfg.round_secs;
        Some((next - now() as i64).max(0))
    });
    rsx! {
        match secs_left() {
            Some(0) => rsx!{ span { class:"animate-pulse text-amber-400", "Flipping..." } },
            Some(s) => rsx!{ span { class:"font-mono", "{s}s" } },
            None    => rsx!{ span { "--" } },
        }
    }
}
```

```
// ---- Vault grid: 4 cards, per-tranche asset/amount/shares, highlight selected ----
#[component]
fn VaultGrid() -> Element {
    let chain = use_context::<Resource<ChainState>>();
    let st = chain.read();
    let sel = st.as_ref().and_then(|s| s.config.as_ref()).map(|c| c.selected_vault);
    rsx! {
        div { class: "grid grid-cols-2 gap-4 md:grid-cols-4",
            for i in 0u8..4 {
                {
                    let v = st.as_ref().and_then(|s| s.vaults[i as usize].clone());
                    rsx! {
                        div { key: "{i}",
                            class: if sel == Some(i) { "rounded-xl border-2 border-amber-400 p-4" }
                                   else { "rounded-xl border border-zinc-700 p-4" },
                            h3 { class:"text-sm text-zinc-400", "Vault {i}" }
                            if let Some(v) = v {
                                for (slot, t) in v.tranches.iter().enumerate() {
                                    TrancheRow { slot: slot as u8, asset: t.asset.clone(),
                                                 amount: t.amount, shares: t.total_shares }
                                }
                            } else { p { "loading..." } }
                        }
                    }
                }
            }
        }
    }
}
```

```
// ---- Withdraw form: live Position read + fee preview, then wallet sign ----
#[component]
fn WithdrawForm(vault_id: u8, slot: u8) -> Element {
    let wallet = use_context::<WalletCtx>();
    let chain  = use_context::<Resource<ChainState>>();
    let mut shares = use_signal(|| String::new());

    // 10% fee preview from config.fee_bps (e.g. 1000 bps = 10%).
    let net_preview = use_memo(move || {
        let bps = chain.read().as_ref().and_then(|s| s.config.as_ref()).map(|c| c.fee_bps).unwrap_or(0);
        // ...derive lamports out from shares vs tranche, apply (10000-bps)/10000...
        format!("-{:.1}% fee", bps as f64 / 100.0)
    });

    let mut submit = use_action(move |_| async move {
        let owner = wallet.connected().expect("connect first");
        let ix = build_withdraw_ix(&PROGRAM_ID, &owner, vault_id, slot,
                                   shares().parse::<u64>()?); // discriminator + borsh args
        let bh = wallet.rpc().get_latest_blockhash().await?;
        let tx = Transaction::new_with_payer(&[ix], Some(&owner)); // + blockhash
        let sig = wallet.adapter().sign_and_send_transaction(tx).await?; // wallet popup
        wallet.rpc().confirm_transaction(&sig).await?;
        Ok::<_, anyhow::Error>(sig)
    });

    rsx! {
        input { value: "{shares}", oninput: move |e| shares.set(e.value()) }
        span { class:"text-xs text-zinc-500", "{net_preview}" }
        button { disabled: wallet.connected().is_none(),
            onclick: move |_| submit.call(()),
            "Withdraw"
        }
    }
}
```

```
// ---- Connect-wallet button (JamiiDao wallet-adapter 1.4.2, Wallet-Standard) ----
#[component]
fn ConnectWalletButton() -> Element {
    let mut wallet = use_context::<WalletCtx>();
    rsx! {
        match wallet.connected() {
            Some(pk) => rsx!{
                button { class:"btn", onclick: move |_| async move { wallet.disconnect().await; },
                    "{short(&pk)} | Disconnect" }
            },
            None => rsx!{
                // adapter.connect_by_name("Phantom") under the hood; list discovered wallets
                button { class:"btn-primary",
                    onclick: move |_| async move { wallet.connect("Phantom").await; },
                    "Connect Wallet" }
            },
        }
    }
}
```

```
// ---- Indexer-backed history/leaderboard via gloo-net + use_resource ----
use gloo_net::http::Request;
#[derive(serde::Deserialize, Clone, PartialEq)]
struct RoundRow { round: u64, selected_vault: u8, ts: i64, sig: String }

#[component]
fn History() -> Element {
    let page = use_signal(|| 0u32);
    let rows = use_resource(move || async move {
        let url = format!("{API}/rounds?page={}&limit=50", page());
        Request::get(&url).send().await?.json::<Vec<RoundRow>>().await
    });
    rsx! { /* table over rows.read() with Pending/Done/Err states */ }
}
```

**Risks:**
- [high] JamiiDao wallet-adapter (or any pure-Rust adapter) targets a specific solana-sdk major (v2/v3); wasm_client_solana 0.10 targets solana-sdk v3. A version mismatch between the wallet adapter's Transaction/Pubkey types and the RPC client's types breaks signing (type incompatibility across crate versions). → Pin a single solana-sdk v3.x across the whole workspace and verify both crates resolve to it in Cargo.lock before building UI. If the adapter only supports v2, either upgrade the adapter, downgrade wasm_client_solana to a v2-compatible release, or sign with the adapter's own transaction type and avoid mixing. Prototype the connect+sign+send path FIRST, before building any UI.
- [high] getrandom and wasm-bindgen configuration: Solana/borsh crates pull getrandom which needs the `js` backend on wasm32-unknown-unknown, and several solana crates historically fail to compile to WASM without the right RUSTFLAGS/feature flags. → Set getrandom features = ["js"] (or the v0.3 wasm_js cfg + RUSTFLAGS='--cfg getrandom_backend="wasm_js"'), enable the `js` feature on wasm_client_solana, and confirm `dx build --platform web` compiles cleanly as the very first milestone. The JamiiDao templates already encode the working flag set; copy their Cargo config.
- [medium] accountSubscribe websocket reliability: public devnet/mainnet RPC websockets drop connections, rate-limit, or silently stall, which would freeze the 'instant flip' UX. → Treat websocket as an accelerator, not the source of truth: always keep the 5s use_interval getMultipleAccounts poll running as fallback, auto-reconnect the subscription with backoff on close, and use a paid RPC (Helius/Triton) with generous WS limits for mainnet. The countdown reaching 0 also force-triggers a poll regardless of WS state.
- [medium] Clock skew between the browser and the cluster makes the round countdown wrong (last_settled_ts is on-chain Unix time; browser Date.now may differ by seconds). → On load, read the cluster time once (getBlockTime of the latest slot, or the Clock sysvar) and store an offset signal; compute now() as browser_time + offset. Re-sync the offset periodically. Cap displayed seconds_left at config.round_secs so a bad offset can't show an absurd value.
- [medium] Anchor account layout drift: the borsh struct mirrored in Rust must EXACTLY match the on-chain field order/types (u128 r_tok/k, i64 timestamps, [u8;32] round_seed, enum phase as u8). A wrong offset silently decodes garbage. → Generate or hand-verify the struct against target/idl/flipvault.json (which lists exact field types and order), and add a unit test that decodes a known-good account dump fetched from devnet. Validate the 8-byte discriminator matches the IDL discriminator before deserializing.
- [medium] Dioxus 0.7 ecosystem churn: dioxus-sdk/dioxus-time use_interval, dioxus-router, and asset! APIs shifted between 0.6 and 0.7, and some helper crates lag the core release. → Pin dioxus, dioxus-router, dioxus-cli to matching 0.7.x; if dioxus-sdk timing lags, fall back to a use_future loop with gloo_timers::future::TimeoutFuture (documented working pattern). Follow the official 0.7 docs (dioxuslabs.com/learn/0.7) rather than 0.6 examples.
- [low] Building Anchor instructions by hand (discriminator + borsh args) is error-prone without the TS Anchor client; a wrong discriminator or arg encoding yields an InstructionDidNotDeserialize error. → Copy the 8-byte instruction discriminators directly from target/idl/flipvault.json (the deposit/withdraw entries) and borsh-encode args in declared order (vault_id u8, slot u8, amount/shares u64). Test each instruction against devnet using the same account set the existing TS scripts use.
- [low] Bundle size / cold-start: a Rust+Solana WASM bundle can be multi-MB, hurting the 'instant' first paint that ORE targets. → Let dx run wasm-opt in --release, enable opt-level='z' + lto + codegen-units=1 in the wasm profile, gzip/brotli at the CDN, and show a lightweight loading shell. Lazy-load the History/Leaderboard data (indexer) after first paint so the vault grid renders immediately.

**Open questions:**
- Which exact solana-sdk major does JamiiDao wallet-adapter 1.4.2 compile against, and does it line up with wasm_client_solana 0.10's solana-sdk v3? This must be verified by a spike build before committing to both crates together.
- Will the keeper/indexer expose a WebSocket/SSE push for new rounds, or is the websocket purely RPC accountSubscribe? If the indexer streams round events, the History/Leaderboard could update live too (closer to ORE) instead of polling.
- What is the exact unit/meaning of fee_bps (is 1000 = 10%?) and how is withdraw share->lamports math computed on-chain, so the form's net-out preview matches the program exactly (avoid showing a number that differs from what the user receives)?
- Does the deployed program enforce a max number of position slots per (owner, vault), and how does the UI discover which (vault,slot) a user already holds without scanning getProgramAccounts in the browser (likely needs an indexer /positions/:owner endpoint)?
- Should the frontend support signTransaction + manual send (so the app controls priority fees/retries) or signAndSendTransaction (wallet handles it)? Priority fees matter for landing deposits/withdraws near round boundaries on mainnet.
- SSR/SEO: is a static client-only SPA acceptable, or does the product want server-rendered landing pages (Dioxus fullstack/axum) for sharing/marketing? Affects whether dx is configured web-only or fullstack.
- Mainnet RPC provider choice (Helius/Triton/QuickNode) and whether the WS endpoint and an API key need to be injected at build vs fetched at runtime from /config.json.


### Wallet + Transaction Layer (Dioxus/WASM) — connect, build, sign, and send a FlipVault deposit (wallet)

Use the JamiiDao `wallet-adapter` crate v1.4.2 (pure-Rust Wallet Standard adapter, published Feb 2026) as the wallet layer, and the modern *modular* Solana crates (solana-transaction 4.0, solana-instruction 3.1, solana-pubkey 4.1, solana-hash 4.2, solana-system-interface 3.0) — NOT the monolithic solana-sdk — to build transactions in WASM. This is exactly the stack of JamiiDao's official `dioxus-adapter-anchor` template (Dioxus 0.7.1), which is the closest real-world analog to what ORE does (ORE uses regolith-labs/dioxus-wallet-adapter, but that one is git-pinned to a Dioxus fork and depends on local-path solana WASM forks — not reusable off-monorepo, so I recommend wallet-adapter instead). The canonical flow, verbatim from that template: build an Instruction with AccountMeta + Anchor-encoded data → Transaction::new_with_payer(payer = wallet pubkey) → fetch a recent blockhash over raw web-sys fetch RPC → set message.recent_blockhash → bincode::serialize(&tx) → WALLET_ADAPTER.sign_and_send_transaction(&bytes, Cluster::DevNet, SendOptions::default()) which returns the Signature. The adapter detects Phantom/Solflare/Backpack via Wallet Standard browser events. wallet-adapter is mature enough that NO wasm-bindgen-to-window-wallet fallback is needed; I document the fallback only as a contingency. Critical build gotcha: WASM requires `.cargo/config.toml` with `rustflags = ['--cfg', 'getrandom_backend="wasm_js"']` or the build fails at link time (getrandom 0.3 has no default wasm backend).

**Recommended stack:**
- wallet-adapter 1.4.2 — Wallet Standard adapter (connect, pubkey, sign+send) — pure Rust, compiles to wasm32-unknown-unknown: Latest non-yanked release (Feb 18 2026). Implements the Solana Wallet Standard register-wallet browser events, so Phantom/Solflare/Backpack are auto-detected. Operates on raw transaction BYTES (no solana-sdk dependency), so it stays light in WASM. Exposes init(), connect_by_name(), connection_info().connected_account().public_key(), sign_and_send_transaction(&[u8], Cluster, SendOptions)->Signature, sign_transaction(&[u8], Option<Cluster>)->Vec<Vec<u8>>, sign_message(), events(). Used by JamiiDao's official Dioxus template.
- dioxus 0.7.1 — Rust->WASM UI framework (features = ["web","router"]): Same framework ORE.supply uses. 0.7.x is the current stable line with GlobalSignal/Signal state, hot-reload, and a CLI (dx serve / dx build). The wallet-adapter template targets exactly 0.7.1.
- solana-transaction 4.0.0 — Transaction + Message types (legacy v0 message): Modular successor to solana-sdk's transaction module; compiles cleanly to WASM. Provides Transaction::new_with_payer and message.recent_blockhash. Enable feature "serde" so bincode can serialize it into the bytes the adapter expects. (For Address Lookup Tables later, switch to a VersionedTransaction builder; for FlipVault's small account lists a legacy message is sufficient and simplest.)
- solana-instruction 3.1.0 — Instruction + AccountMeta: Build the FlipVault deposit instruction with explicit AccountMeta ordering. Use feature "borsh" (it's already a borsh-aware build) — though instruction DATA here is hand-encoded Anchor discriminator + borsh args.
- solana-pubkey 4.1.0 — Pubkey type + find_program_address (PDA derivation): Derive config/vault/position PDAs client-side with the same seeds as scripts/lib.ts. Feature "borsh" + curve25519 support compiles to WASM. find_program_address runs in-browser so no extra RPC round-trips for PDAs.
- solana-hash 4.2.0 — Hash type for recent_blockhash: Hash::from_str parses the base58 blockhash returned by getLatestBlockhash JSON-RPC.
- solana-system-interface 3.0.0 — System program instruction helpers (feature "bincode"): Not strictly needed for deposit (FlipVault encodes its own ix), but needed for the System Program pubkey constant and any SOL-transfer helpers; the template uses it for transfer().
- anchor-lang-idl 0.1.2 — IDL parsing/typing (feature "convert"): Optional: lets you load flipvault.json at build/run time to validate account names/discriminators instead of hardcoding. The template ships it for the anchor variant.
- bincode 1.3.3 — Serialize Transaction -> Vec<u8>: The adapter's sign_and_send_transaction takes &[u8]; bincode::serialize(&tx) is exactly how the template encodes it. Pin 1.x (not 2.x) to match solana-transaction's serde wire format.
- jzon 0.12.5 — Build JSON-RPC request bodies in WASM: Lightweight JSON used by the template for getLatestBlockhash/sendTransaction bodies; pairs with raw web-sys fetch. (serde_json 1.0.x is used for typed response deserialization.)
- getrandom 0.3.x — RNG backend (transitive via ed25519-dalek/blake3): MUST be configured with the wasm_js backend via RUSTFLAGS/.cargo config or the wasm build fails to link. This is the single most common setup failure for this stack.
- gloo-timers 0.3.0 — Async timers in WASM (round countdown, confirmation polling): Used for the ORE-like live countdown and for polling getSignatureStatuses after send without blocking the UI thread.

**Architecture:** THREE SUBLAYERS, all in Rust/WASM.

1) WALLET SESSION (wallet-adapter). At app mount, create the adapter once and stash it in a Dioxus GlobalSignal: `static WALLET_ADAPTER: GlobalSignal<WalletAdapter> = Signal::global(|| WalletAdapter::init().unwrap());`. init() subscribes to the Wallet Standard `wallet-standard:register-wallet` / `app-ready` browser CustomEvents, so any installed Phantom/Solflare/Backpack registers itself into the adapter's WalletStorage. A Connect button calls `WALLET_ADAPTER.write().connect_by_name(\"Phantom\").await` (or iterate `wallets()` to render a wallet picker). After connect, read the pubkey via `connection_info().await.connected_account()?.public_key()` -> `[u8;32]`, and store it in an ACCOUNT_STATE signal so every view (vaults, positions) can derive Position PDAs and gate the deposit/withdraw buttons. Spawn a task on `WALLET_ADAPTER.read().events()` to react to account-change / disconnect events and update ACCOUNT_STATE reactively (this is what keeps UX 'instant' — no polling for wallet state).

2) TX BUILDER (modular solana crates, pure Rust). PDAs are derived in-browser with solana-pubkey::find_program_address using the SAME seeds as scripts/lib.ts: config=[b\"config\"], vault=[b\"vault\",[id]], position=[b\"position\", owner, [vault_id], [slot]], program id EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H. The deposit instruction data is the 8-byte Anchor discriminator for `deposit` ([242,35,198,137,82,225,242,182], taken straight from flipvault.json) followed by borsh(vault_id:u8, slot:u8, amount:u64). AccountMeta order MUST mirror the IDL/deposit.ts exactly: user(signer,writable), config(ro), vault(writable), position(writable), system_program(ro). Then `Transaction::new_with_payer(&[ix], Some(&user_pubkey))`, fetch blockhash, set `tx.message.recent_blockhash`, `bincode::serialize(&tx)`.

3) RPC + SUBMIT. A tiny FetchReq wrapper around web-sys `window().fetch_with_request` issues JSON-RPC: getLatestBlockhash (before build), and after the adapter signs+sends we poll getSignatureStatuses for confirmation. The DEFAULT and recommended submit path is `sign_and_send_transaction` — the wallet itself broadcasts via its own RPC, which is the smoothest UX (one wallet approval, wallet handles blockhash freshness/retries). The ALTERNATE path (`sign_transaction` -> Vec<Vec<u8>> signed bytes -> our own sendTransaction RPC) is kept for when we want to attach our own priority fees / send through a private RPC / Jito; both are wired but deposit uses sign_and_send.

OPTIMISTIC UX: on send, immediately flip the local position/vault signals optimistically and show a 'pending' chip; reconcile against the indexer's websocket/poll once confirmed. This is the ORE-like 'feels instant' trick — the heavy state (4 vaults, leaderboard) comes from the Indexer+API layer, while this layer only owns the wallet session and the single in-flight tx.

WALLET-ADAPTER MATURITY VERDICT: mature enough — no JS interop fallback needed for the happy path. Fallback (documented in code_sketches) is a thin wasm-bindgen extern block to `window.solana.signAndSendTransaction` for an exotic wallet that ships a non-standard provider; not expected for Phantom/Solflare/Backpack which all implement Wallet Standard.

**Code sketches:**
```
// .cargo/config.toml  — MANDATORY or the wasm build fails to link (getrandom 0.3 has no default wasm backend)
[target.wasm32-unknown-unknown]
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']

// frontend/Cargo.toml (excerpt, versions verified on crates.io Feb 2026)
[dependencies]
dioxus = { version = "0.7.1", features = ["web", "router"] }
wallet-adapter = "1.4.2"
solana-transaction = { version = "4.0.0", features = ["serde"] }
solana-instruction = { version = "3.1.0", features = ["borsh"] }
solana-pubkey = { version = "4.1.0", features = ["borsh"] }
solana-hash = "4.2.0"
solana-system-interface = { version = "3.0.0", features = ["bincode"] }
bincode = "1.3.3"
jzon = "0.12.5"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
gloo-timers = { version = "0.3.0", features = ["futures"] }
borsh = "1"

[features]
default = ["web"]
web = ["dioxus/web"]
```

```
// ---- Global wallet session (Dioxus 0.7) ----
use dioxus::prelude::*;
use wallet_adapter::{WalletAdapter, Cluster, SendOptions};

pub static WALLET_ADAPTER: GlobalSignal<WalletAdapter> =
    Signal::global(|| WalletAdapter::init().expect("adapter init"));
// connected wallet pubkey as raw bytes, None until connected
pub static USER_PK: GlobalSignal<Option<[u8; 32]>> = Signal::global(|| None);

#[component]
fn ConnectButton() -> Element {
    let connect = move |_| async move {
        // connect_by_name takes &mut self -> WalletAccount
        match WALLET_ADAPTER.write().connect_by_name("Phantom").await {
            Ok(_) => {
                let info = WALLET_ADAPTER.read().connection_info().await;
                if let Ok(acct) = info.connected_account() {
                    *USER_PK.write() = Some(*acct.public_key());
                }
            }
            Err(e) => tracing::error!("connect failed: {e}"),
        }
    };
    let label = match *USER_PK.read() {
        Some(pk) => short_addr(&pk),
        None => "Connect Wallet".into(),
    };
    rsx!(button { onclick: connect, "{label}" })
}
// To render a multi-wallet picker instead of hardcoding Phantom:
//   for w in WALLET_ADAPTER.read().wallets() { /* button -> connect(w) */ }
```

```
// ---- FlipVault PDAs + deposit instruction (mirrors scripts/lib.ts + deposit.ts) ----
use solana_pubkey::Pubkey;
use solana_instruction::{Instruction, AccountMeta};
use borsh::BorshSerialize;

const PROGRAM_ID: Pubkey = solana_pubkey::pubkey!("EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H");
const SYS_PROGRAM: Pubkey = solana_pubkey::pubkey!("11111111111111111111111111111111");
// 8-byte Anchor discriminator for `deposit`, copied verbatim from flipvault.json
const DEPOSIT_DISC: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];

fn config_pda() -> Pubkey { Pubkey::find_program_address(&[b"config"], &PROGRAM_ID).0 }
fn vault_pda(id: u8) -> Pubkey { Pubkey::find_program_address(&[b"vault", &[id]], &PROGRAM_ID).0 }
fn position_pda(owner: &Pubkey, vault_id: u8, slot: u8) -> Pubkey {
    Pubkey::find_program_address(
        &[b"position", owner.as_ref(), &[vault_id], &[slot]], &PROGRAM_ID).0
}

#[derive(BorshSerialize)]
struct DepositArgs { vault_id: u8, slot: u8, amount: u64 }

fn build_deposit_ix(user: Pubkey, vault_id: u8, slot: u8, amount: u64) -> Instruction {
    let mut data = DEPOSIT_DISC.to_vec();
    DepositArgs { vault_id, slot, amount }.serialize(&mut data).unwrap();
    Instruction {
        program_id: PROGRAM_ID,
        // ORDER MUST MATCH THE IDL: user, config, vault, position, system_program
        accounts: vec![
            AccountMeta::new(user, true),                       // signer + writable
            AccountMeta::new_readonly(config_pda(), false),
            AccountMeta::new(vault_pda(vault_id), false),
            AccountMeta::new(position_pda(&user, vault_id, slot), false),
            AccountMeta::new_readonly(SYS_PROGRAM, false),
        ],
        data,
    }
}
```

```
// ---- The full client tx flow: build -> blockhash -> serialize -> sign+send ----
use solana_transaction::Transaction;
use wallet_adapter::{Cluster, SendOptions, WalletResult};

pub async fn deposit(vault_id: u8, slot: u8, lamports: u64) -> WalletResult<String> {
    let user_bytes = USER_PK.read().ok_or_else(|| wallet_adapter::WalletError::Op("not connected".into()))?;
    let user = Pubkey::new_from_array(user_bytes);

    let ix = build_deposit_ix(user, vault_id, slot, lamports);
    // payer = the connected wallet; wallet adds its signature
    let mut tx = Transaction::new_with_payer(&[ix], Some(&user));
    tx.message.recent_blockhash = get_blockhash().await?;   // raw fetch JSON-RPC, see next sketch
    let tx_bytes = bincode::serialize(&tx)
        .map_err(|e| wallet_adapter::WalletError::Op(e.to_string()))?;

    // DEFAULT path: wallet signs AND broadcasts (smoothest UX, one approval)
    let sig = WALLET_ADAPTER
        .read()
        .sign_and_send_transaction(&tx_bytes, Cluster::DevNet, SendOptions::default())
        .await?;
    Ok(sig.to_string())
}

// ALTERNATE path (custom RPC / priority fee / Jito): sign locally then send ourselves
// let signed: Vec<Vec<u8>> = WALLET_ADAPTER.read()
//     .sign_transaction(&tx_bytes, Some(Cluster::DevNet)).await?;
// rpc_send_transaction(&signed[0]).await?;  // base64 -> sendTransaction
```

```
// ---- Blockhash + sendTransaction over raw web-sys fetch (no reqwest in WASM) ----
use solana_hash::Hash;
use std::str::FromStr;
use wallet_adapter::WalletResult;

const RPC: &str = "https://api.devnet.solana.com";

pub async fn get_blockhash() -> WalletResult<Hash> {
    let body = jzon::object!{ id:1, jsonrpc:"2.0", method:"getLatestBlockhash", params:[] }.to_string();
    let resp = FetchReq::new("POST")?
        .add_header("content-type", "application/json")?
        .set_body(&body)
        .send(RPC)
        .await?;
    #[derive(serde::Deserialize)] struct V { blockhash: String }
    #[derive(serde::Deserialize)] struct Ctx { value: V }
    #[derive(serde::Deserialize)] struct R { result: Ctx }
    let r: R = serde_json::from_str(&resp).map_err(|e| wallet_adapter::WalletError::Op(e.to_string()))?;
    Hash::from_str(&r.result.value.blockhash).map_err(|e| wallet_adapter::WalletError::Op(e.to_string()))
}
// FetchReq wraps web_sys::{Request, RequestInit, Headers} + window().fetch_with_request,
// then JsFuture(resp.text()).await -> String. (This is the template's pattern; reused verbatim.)
```

```
// ---- CONTINGENCY ONLY: wasm-bindgen fallback to a non-Wallet-Standard injected provider ----
// Not needed for Phantom/Solflare/Backpack (all implement Wallet Standard, handled by wallet-adapter).
// Kept for an exotic wallet that only exposes window.solana.signAndSendTransaction.
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "solana"], js_name = signAndSendTransaction)]
    fn js_sign_and_send(tx_b58_or_bytes: JsValue) -> js_sys::Promise;
}

pub async fn fallback_send(tx_bytes: &[u8]) -> Result<String, JsValue> {
    let arr = js_sys::Uint8Array::from(tx_bytes);
    let res = JsFuture::from(js_sign_and_send(arr.into())).await?; // { signature }
    Ok(js_sys::Reflect::get(&res, &"signature".into())?.as_string().unwrap_or_default())
}
```

**Risks:**
- [high] getrandom wasm backend not configured -> the entire WASM build fails to link with an opaque 'the wasm*-unknown-unknown targets are not supported by default' error. Bites every new dev and CI. → Commit `.cargo/config.toml` with `rustflags = ['--cfg', 'getrandom_backend="wasm_js"']` at the WORKSPACE root (must be workspace root, not crate). Document in README and bake into CI. Verified requirement per getrandom 0.3 docs + JamiiDao book.
- [medium] wallet-adapter is low-adoption (only ~155 downloads on 1.4.2, single maintainer JamiiDao) and 1.3.x/1.4.0/1.4.1 were yanked. API churn or an abandoned crate could strand the app. → Pin exact version (=1.4.2). Keep the tx-builder layer (modular solana crates) fully decoupled from the adapter — it only consumes the adapter's sign_and_send_transaction(&[u8],...) boundary, so swapping to regolith-labs/dioxus-wallet-adapter or a wasm-bindgen shim is a localized change. Vendor/fork the crate if needed (it's pure Rust, ~small).
- [medium] sign_and_send_transaction makes the WALLET broadcast via its own RPC; you lose control over priority fees, the RPC endpoint, and may hit the wallet's stale-blockhash/retry behavior — hurting the 'instant' feel under devnet congestion. → Offer both paths: use sign_and_send for the simple case, and sign_transaction + your own sendTransaction (with computeBudget priority-fee ixs prepended) through a dedicated/private RPC for reliability. Poll getSignatureStatuses with gloo-timers and show optimistic UI immediately.
- [high] Account-meta ordering or discriminator drift between the hardcoded Rust ix and the deployed program causes 'AccountNotEnoughKeys'/'InstructionDidNotDeserialize' failures that are hard to debug client-side. → Source the discriminator and account order directly from target/idl/flipvault.json (deposit disc = [242,35,198,137,82,225,242,182]; order user,config,vault,position,system_program). Add a wasm unit test that rebuilds the deposit ix and asserts bytes against a known-good tx produced by the existing scripts/deposit.ts. Optionally load the IDL at runtime via anchor-lang-idl to validate.
- [medium] Legacy bincode Transaction wire format vs versioned-transaction expectations: some wallets only accept v0 VersionedTransaction; bincode 2.x or solana-transaction serde mismatch can corrupt bytes. → Pin bincode 1.3.3 and solana-transaction 4.0 with feature "serde" exactly as the template does (this combination is known-good against Phantom/Solflare). If a target wallet rejects legacy messages, switch to building a v0 VersionedTransaction (same crate) — FlipVault's <6 accounts fit either format.
- [medium] Deposit/withdraw are BLOCKED on-chain while Config.phase == Pending (round settling). A user signs, the wallet broadcasts, and it fails with RoundPending (6003) ~every 30s window — confusing UX. → This is a UX concern that lands in the wallet layer: read Config.phase (from the indexer or a getAccountInfo call) and the round countdown; disable the deposit button and show 'locked, settling…' during Pending. Map error code 6003 to a friendly toast. Decode 6000-6015 from the IDL errors list.
- [low] Heavy WASM bundle / slow first paint undercuts the ORE-like 'instant' goal because the modular solana crates + dalek still add weight. → Build with opt-level='z'/LTO (workspace release profile already does), wasm-opt via dx bundle, and lazy-load non-critical views. Keep curve25519/blake3 features minimal. The modular crates are far lighter than full solana-sdk, which is the main win.

**Open questions:**
- Confirm whether Phantom/Solflare on devnet accept a legacy bincode Transaction via signAndSendTransaction, or whether a v0 VersionedTransaction is required — validate against a real wallet before committing to Transaction::new_with_payer vs a v0 message builder.
- Decide submit path policy: default to wallet's sign_and_send (simplest) vs always sign_transaction + our own RPC with priority fees. Depends on how aggressive the 'instant' SLA is and whether we run a dedicated RPC (Helius/Triton) for mainnet.
- wallet-adapter's exact Cluster enum variants and SendOptions fields for 1.4.2 (DevNet vs Devnet casing, preflight/skipPreflight/maxRetries) — verify against docs.rs source before wiring SendOptions; the doc page lists the type but not field names.
- Should the frontend reuse the on-chain Anchor account structs (Config/Vault/Position) for deserialization to render the 4 vaults live, or consume everything from the Indexer+API layer? Recommend the latter for the bulk read path and reserve direct getAccountInfo for the user's own Position freshness after a deposit.
- Whether to load flipvault.json at runtime (anchor-lang-idl convert) for discriminators/error decoding vs codegen a typed client at build time — runtime IDL is more flexible, build-time is faster and type-safe.
- Wallet-picker UX: hardcode connect_by_name("Phantom") for the MVP vs render a Wallet Standard picker from adapter.wallets(). Picker is the correct ORE-like UX but needs the wallet icons/metadata the adapter exposes.


### SHARED `flipvault-sdk` Rust crate (WASM-safe + server-side glue: program/ORAO IDs, PDA derivations, instruction builders, account decoders) (sdk)

Design a single `no_std`-friendly, dependency-light crate `flipvault-sdk` that is the one source of truth for the FlipVault program ABI, consumed identically by the Dioxus/WASM frontend, the Rust keeper, and the Rust indexer. The hard constraint is WASM: it must compile to `wasm32-unknown-unknown` with NO transaction/RPC/signing machinery in it. The crate therefore depends ONLY on the lean, wasm-safe Solana data crates — `solana-pubkey` (no_std, wasm-OK, `borsh` feature), `solana-instruction` (no_std, wasm-OK, `borsh` feature, supplies `Instruction`/`AccountMeta`), `borsh` 1.x, and `sha2` for the Anchor discriminator — and explicitly does NOT depend on `solana-client`, `anchor-client`, or `solana-sdk` (all pull `tokio`/`reqwest`/sockets and DO NOT compile to wasm32). It deliberately avoids `solana-program` too: although `solana-program` 3.x/4.x does list `wasm32-unknown-unknown` as a target, it drags a large graph (sysvars, program-error, hashers) the SDK never needs, so we take its constituent micro-crates directly. We do NOT use `anchor-lang` even for `Discriminator`/`InstructionData`: in current Anchor those traits live behind heavy code paths and `anchor-lang` historically fights wasm (proc-macro/zeroize/getrandom transitively); hand-rolling `sha256("global:<ix>")[..8]` and borsh is ~30 lines and removes all risk. The crate exports: constant `Pubkey`s (program id, ORAO VRF id, system program); a `pda` module deriving config/reserve/treasury/vault/position + ORAO network_state/randomness; an `ix` module of pure builders returning `solana_instruction::Instruction` (args borsh-serialized after the 8-byte discriminator, AccountMetas assembled in IDL order including the ORAO accounts on commit/settle); and an `accounts` module decoding Config/Vault/Position (and ORAO NetworkState for the keeper) by stripping the 8-byte discriminator then borsh-deserializing, with the `RoundPhase`/`Asset` enums and `u128` fields modeled exactly. Callers (frontend wallet-adapter, keeper RpcClient, indexer) own RPC/signing; the SDK only computes bytes. getrandom is NOT a real dependency of this crate, but because the wasm consumer app pulls it transitively the design pins `.cargo/config.toml` RUSTFLAGS `--cfg getrandom_backend="wasm_js"` at the app level.

**Recommended stack:**
- solana-pubkey 2.x / 3.x (track whatever solana-program your keeper uses re-exports; pin =2.x for devnet parity, both wasm-OK) — `Pubkey` type, `find_program_address`/`create_program_address`, `pubkey!` macro for const ids: no_std, compiles cleanly to wasm32-unknown-unknown, has a `borsh` feature for (de)serializing the Pubkey fields inside Config/Position. This is the modern lean replacement for `solana_program::pubkey`; confirmed wasm32 target support and used standalone.
- solana-instruction matching solana-pubkey line (2.x/3.x) — `Instruction` + `AccountMeta` structs returned by every builder: no_std and wasm-safe; supplies exactly the instruction data model the builders need without dragging the validator runtime. Decouples the SDK from `solana-sdk`/`solana-client` (neither compiles to wasm).
- borsh 1.5.x — derive + (de)serialize instruction args and account structs: Anchor's wire format IS borsh; borsh 1.x is no_std-capable and compiles to wasm32. Use `borsh-derive` for `BorshSerialize`/`BorshDeserialize`. Matches the on-chain serialization byte-for-byte.
- sha2 0.10.x — compute the Anchor 8-byte discriminator = sha256("global:<ix_name>")[..8] and account discriminators sha256("account:<Name>")[..8]: Pure-Rust, no_std, wasm32-clean. Lets us hand-roll discriminators instead of pulling `anchor-lang`. Can be made `const`-evaluated or computed once via `OnceLock`/lazy on native and plain fn on wasm.
- thiserror (native) / hand-rolled enum (wasm) 1.x — `DecodeError` for account decoders: Decoders are fallible (bad discriminator, short buffer, bad enum tag). thiserror is wasm-safe (no_std off by default — gate it behind a `std` feature; expose a plain enum + `core::fmt` impl when `no_std`).
- (consumer-side, NOT a dep of this crate) solana-client-wasm + solana-extra-wasm OR wasm_client_solana latest — RPC from the Dioxus frontend: `solana-client` does NOT compile to wasm; ORE itself uses the `solana-client-wasm`/`solana-extra-wasm` forks. The SDK stays RPC-agnostic so the frontend can pick the wasm RPC client and the keeper/indexer can use the real `solana-client` — both consume the SAME builders/decoders.
- (consumer-side) wasm-bindgen + a JS wallet adapter bridge (or `wallet-adapter`/`wasi-sol` Rust crates) wasm-bindgen 0.2.9x — wallet connect + sign in the browser: Signing/sending lives outside the SDK. The SDK hands back unsigned `Instruction`s; the frontend wraps them into a Transaction and routes to the injected wallet. Keeps the SDK pure and wasm-thin.

**Architecture:** CRATE LAYOUT (`flipvault-sdk`):
```
flipvault-sdk/
  Cargo.toml
  src/
    lib.rs        // re-exports, prelude, crate-level docs, no_std attr (optional)
    ids.rs        // const Pubkeys: PROGRAM_ID, ORAO_VRF_ID, SYSTEM_PROGRAM_ID
    discriminator.rs // sha256("global:"|"account:" + name)[..8]
    pda.rs        // all PDA derivations (program + ORAO)
    state.rs      // Config, Vault, Tranche, Position, enums, NetworkState (decoders live here)
    ix.rs         // instruction builders -> solana_instruction::Instruction
    error.rs      // DecodeError
```

FEATURE FLAGS:
- default = ["std"]. `std` gates thiserror + any `OnceLock` caching of discriminators. Under `no_std` the crate still works (frontend can build either way; default std is fine for wasm32 since wasm32-unknown-unknown has `std`). The real wasm constraint is the DEPENDENCY GRAPH, not std — so we keep `std` on but forbid heavy crates.
- NO `getrandom` feature here. Document that the WASM APP must set `RUSTFLAGS=--cfg getrandom_backend="wasm_js"` (getrandom 0.2) in its `.cargo/config.toml` because transitive deps (wasm RPC client, ed25519) need it.

WASM COMPATIBILITY DECISION TABLE (the load-bearing analysis):
- solana-client / anchor-client / solana-sdk -> NOT wasm-safe (tokio, reqwest, sockets, rocksdb-ish graph). EXCLUDED. ORE uses `solana-client-wasm` fork instead — proof that mainline `solana-client` can't target wasm32.
- solana-program 3.x/4.x -> DOES declare wasm32-unknown-unknown target and `borsh` default feature, BUT heavy. Allowed-but-avoided; we use its leaf crates directly.
- solana-pubkey -> wasm-safe, no_std, `borsh` feature. INCLUDED.
- solana-instruction -> wasm-safe, no_std, `borsh` feature, gives Instruction/AccountMeta. INCLUDED.
- borsh 1.x -> wasm-safe, no_std. INCLUDED.
- sha2 0.10 -> wasm-safe, no_std. INCLUDED.
- anchor-lang (for Discriminator/InstructionData only) -> AVOIDED. It is a fat framework crate (proc-macros, zeroize, bytemuck, getrandom transitively) that historically requires wasm workarounds; the two things we'd want from it (8-byte discriminator + borsh arg encoding) are trivially hand-rolled. Hand-rolling also pins us to the program's exact byte layout and removes a version-coupling headache.

DATA FLOW: SDK is pure/synchronous and side-effect-free. It computes (a) addresses, (b) `Instruction` byte blobs, (c) decoded structs from `&[u8]` account data. It NEVER fetches, signs, or sends. The three consumers:
  1. Dioxus frontend: derives PDAs for display, builds deposit/withdraw Instructions, hands them to the wallet adapter for signing; decodes account bytes pulled via the wasm RPC client / websocket subscription for the live vault view.
  2. Keeper: uses real `solana-client::RpcClient`, builds commit_round/settle_round/recover_round Instructions from the SDK (the SDK supplies the full ORAO AccountMeta set), reads `NetworkState` via the SDK decoder to get `orao_treasury`.
  3. Indexer: decodes Config/Vault/Position from historical account snapshots / tx logs using the SAME `state.rs` decoders — guaranteeing the API and the frontend agree on layout.

DISCRIMINATOR & ENUM CORRECTNESS: builders use sha256("global:"+snake_name)[..8] (verified against IDL: deposit=[242,35,198,137,82,225,242,182], commit_round=[229,102,157,34,152,217,15,70], settle_round=[40,101,18,1,31,129,52,77], withdraw=[183,18,70,156,148,109,161,34], recover_round=[87,173,77,57,40,45,2,160]) — we also ship a const-table mirror of the IDL discriminators and a debug_assert that the hashed value matches, catching name typos at test time. Account decoders check sha256("account:"+CamelName)[..8] before deserializing. Enums RoundPhase{Idle=0,Pending=1} and Asset{Sol=0,Token=1} are #[derive(BorshDeserialize)] field-order matching the IDL; borsh encodes them as a single u8 tag, which is exactly the on-chain layout.

ORAO ACCOUNT ASSEMBLY (the subtle part): commit_round AccountMetas in IDL order = keeper(signer,writable), config(writable, derived), random(writable; derived from ORAO program with seeds [b"orao-vrf-randomness-request", force]), orao_treasury(writable; NOT a PDA — caller passes it, sourced from NetworkState.config.treasury which the keeper decodes via the SDK), network_state(writable; PDA under ORAO program), vrf(ORAO program id, not signer/not writable), system_program. settle_round = config(writable), reserve(writable PDA), vault0..vault3(writable PDAs), random(readonly; re-derived from Config.round_seed). The SDK exposes both a high-level `commit_round(keeper, force, orao_treasury)` and the raw account list so the keeper can fill orao_treasury after reading NetworkState.

**Code sketches:**
```
// Cargo.toml (key bits)
[package]
name = "flipvault-sdk"
edition = "2021"

[features]
default = ["std"]
std = ["thiserror", "dep:once_cell"]

[dependencies]
solana-pubkey   = { version = "2", default-features = false, features = ["borsh", "curve25519"] }
solana-instruction = { version = "2", default-features = false, features = ["borsh", "std"] }
borsh = { version = "1.5", features = ["derive"] }
sha2  = { version = "0.10", default-features = false }
thiserror = { version = "1", optional = true }
once_cell = { version = "1", optional = true }
// NOTE: deliberately NO solana-client / anchor-client / solana-sdk / solana-program / anchor-lang.
```

```
// ids.rs
use solana_pubkey::{pubkey, Pubkey};
pub const PROGRAM_ID:  Pubkey = pubkey!("EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H");
pub const ORAO_VRF_ID: Pubkey = pubkey!("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");
```

```
// discriminator.rs  — hand-rolled, no anchor-lang
use sha2::{Digest, Sha256};
pub fn ix_discriminator(name: &str) -> [u8; 8] { preimage("global", name) }
pub fn account_discriminator(name: &str) -> [u8; 8] { preimage("account", name) }
fn preimage(ns: &str, name: &str) -> [u8; 8] {
    let mut h = Sha256::new();
    h.update(ns.as_bytes()); h.update(b":"); h.update(name.as_bytes());
    let d = h.finalize();
    let mut out = [0u8; 8]; out.copy_from_slice(&d[..8]); out
}
// Mirror of the IDL discriminators, asserted in tests against ix_discriminator():
pub const DISC_DEPOSIT:      [u8;8] = [242,35,198,137,82,225,242,182];
pub const DISC_WITHDRAW:     [u8;8] = [183,18,70,156,148,109,161,34];
pub const DISC_COMMIT_ROUND: [u8;8] = [229,102,157,34,152,217,15,70];
pub const DISC_SETTLE_ROUND: [u8;8] = [40,101,18,1,31,129,52,77];
pub const DISC_RECOVER_ROUND:[u8;8] = [87,173,77,57,40,45,2,160];
```

```
// pda.rs
use solana_pubkey::Pubkey;
use crate::ids::{PROGRAM_ID, ORAO_VRF_ID};
pub fn config()   -> (Pubkey, u8) { Pubkey::find_program_address(&[b"config"],   &PROGRAM_ID) }
pub fn reserve()  -> (Pubkey, u8) { Pubkey::find_program_address(&[b"reserve"],  &PROGRAM_ID) }
pub fn treasury() -> (Pubkey, u8) { Pubkey::find_program_address(&[b"treasury"], &PROGRAM_ID) }
pub fn vault(id: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault", &[id]], &PROGRAM_ID)
}
pub fn position(owner: &Pubkey, vault_id: u8, slot: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"position", owner.as_ref(), &[vault_id], &[slot]], &PROGRAM_ID)
}
// ORAO PDAs (seeds under the ORAO program id)
pub fn orao_network_state() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"orao-vrf-network-configuration"], &ORAO_VRF_ID)
}
pub fn orao_randomness(seed: &[u8; 32]) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"orao-vrf-randomness-request", seed], &ORAO_VRF_ID)
}
```

```
// ix.rs  — builders return solana_instruction::Instruction
use borsh::BorshSerialize;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use crate::{ids::*, pda, discriminator::ix_discriminator};

fn data<A: BorshSerialize>(name: &str, args: &A) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + 64);
    buf.extend_from_slice(&ix_discriminator(name));
    args.serialize(&mut buf).expect("borsh ix args");
    buf
}

#[derive(BorshSerialize)]
struct DepositArgs { vault_id: u8, slot: u8, amount: u64 }

pub fn deposit(user: &Pubkey, vault_id: u8, slot: u8, amount: u64) -> Instruction {
    let (config, _)   = pda::config();
    let (vault, _)    = pda::vault(vault_id);
    let (position, _) = pda::position(user, vault_id, slot);
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*user, true),            // user (signer, mut)
            AccountMeta::new_readonly(config, false), // config
            AccountMeta::new(vault, false),           // vault (mut)
            AccountMeta::new(position, false),        // position (init_if_needed, mut)
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: data("deposit", &DepositArgs { vault_id, slot, amount }),
    }
}
```

```
// ix.rs (cont) — withdraw + the ORAO-heavy commit/settle
#[derive(BorshSerialize)] struct WithdrawArgs { vault_id: u8, slot: u8, shares: u64 }
pub fn withdraw(user: &Pubkey, vault_id: u8, slot: u8, shares: u64) -> Instruction {
    let (config,_)=pda::config(); let (vault,_)=pda::vault(vault_id);
    let (position,_)=pda::position(user,vault_id,slot); let (treasury,_)=pda::treasury();
    Instruction { program_id: PROGRAM_ID, data: data("withdraw", &WithdrawArgs{vault_id,slot,shares}),
        accounts: vec![
            AccountMeta::new(*user, true), AccountMeta::new_readonly(config,false),
            AccountMeta::new(vault,false), AccountMeta::new(position,false),
            AccountMeta::new(treasury,false),
        ]}
}

#[derive(BorshSerialize)] struct CommitArgs { force: [u8;32] }
/// `orao_treasury` is read from NetworkState.config.treasury (decode it first via state::NetworkState).
pub fn commit_round(keeper: &Pubkey, force: [u8;32], orao_treasury: &Pubkey) -> Instruction {
    let (config,_)=pda::config();
    let (random,_)=pda::orao_randomness(&force);
    let (network_state,_)=pda::orao_network_state();
    Instruction { program_id: PROGRAM_ID, data: data("commit_round", &CommitArgs{force}),
        accounts: vec![
            AccountMeta::new(*keeper, true),               // keeper signer mut
            AccountMeta::new(config, false),               // config mut
            AccountMeta::new(random, false),               // orao randomness PDA mut
            AccountMeta::new(*orao_treasury, false),       // orao treasury mut (caller-supplied)
            AccountMeta::new(network_state, false),        // orao network_state mut
            AccountMeta::new_readonly(ORAO_VRF_ID, false), // vrf program
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ]}
}

/// settle re-derives the randomness PDA from the seed stored in Config.round_seed.
pub fn settle_round(round_seed: &[u8;32]) -> Instruction {
    let (config,_)=pda::config(); let (reserve,_)=pda::reserve();
    let (random,_)=pda::orao_randomness(round_seed);
    let mut accounts = vec![AccountMeta::new(config,false), AccountMeta::new(reserve,false)];
    for id in 0u8..4 { accounts.push(AccountMeta::new(pda::vault(id).0, false)); }
    accounts.push(AccountMeta::new_readonly(random, false)); // random (readonly)
    Instruction { program_id: PROGRAM_ID, data: data("settle_round", &()), accounts }
}

pub fn recover_round() -> Instruction {
    Instruction { program_id: PROGRAM_ID, data: data("recover_round", &()),
        accounts: vec![AccountMeta::new(pda::config().0, false)] }
}
```

```
// state.rs — decoders: strip 8-byte discriminator, borsh-deserialize
use borsh::{BorshDeserialize, BorshSerialize};
use solana_pubkey::Pubkey;
use crate::{discriminator::account_discriminator, error::DecodeError};

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoundPhase { Idle, Pending }   // borsh u8 tag 0/1
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Asset { Sol, Token }            // borsh u8 tag 0/1

#[derive(BorshDeserialize, Clone, Debug)]
pub struct Config {
    pub treasury_authority: Pubkey,
    pub r_tok: u128, pub k: u128,        // u128 fields decode natively under borsh
    pub round_secs: i64, pub last_settled_ts: i64,
    pub fee_bps: u16, pub min_reserve: u64,
    pub phase: RoundPhase, pub round_seed: [u8;32],
    pub commit_slot: u64, pub commit_ts: i64,
    pub selected_vault: u8,
    pub bump: u8, pub reserve_bump: u8, pub treasury_bump: u8,
}
#[derive(BorshDeserialize, Clone, Copy, Debug)]
pub struct Tranche { pub asset: Asset, pub amount: u64, pub total_shares: u64 }
#[derive(BorshDeserialize, Clone, Debug)]
pub struct Vault { pub vault_id: u8, pub tranches: [Tranche;2], pub bump: u8 }
#[derive(BorshDeserialize, Clone, Debug)]
pub struct Position { pub owner: Pubkey, pub vault_id: u8, pub slot: u8, pub shares: u64, pub bump: u8 }

fn decode<T: BorshDeserialize>(name: &str, data: &[u8]) -> Result<T, DecodeError> {
    if data.len() < 8 { return Err(DecodeError::TooShort); }
    if data[..8] != account_discriminator(name) { return Err(DecodeError::BadDiscriminator); }
    T::try_from_slice(&data[8..]).map_err(|_| DecodeError::Borsh)
}
pub fn config(d: &[u8])   -> Result<Config, DecodeError>   { decode("Config", d) }
pub fn vault(d: &[u8])    -> Result<Vault, DecodeError>    { decode("Vault", d) }
pub fn position(d: &[u8]) -> Result<Position, DecodeError> { decode("Position", d) }
```

```
// state.rs (cont) — ORAO NetworkState so the keeper can find orao_treasury.
// ORAO uses an anchor-style 8-byte discriminator too (IDL: [212,237,148,56,97,245,51,169]).
#[derive(BorshDeserialize, Clone, Debug)]
pub struct OraoTokenFeeConfig { pub mint: Pubkey, pub treasury: Pubkey, pub fee: u64 }
#[derive(BorshDeserialize, Clone, Debug)]
pub struct NetworkConfiguration {
    pub authority: Pubkey, pub treasury: Pubkey, pub request_fee: u64,
    pub fulfillment_authorities: Vec<Pubkey>, pub token_fee_config: Option<OraoTokenFeeConfig>,
}
#[derive(BorshDeserialize, Clone, Debug)]
pub struct NetworkState { pub config: NetworkConfiguration, pub num_received: u64 }
pub fn network_state(d: &[u8]) -> Result<NetworkState, DecodeError> {
    // ORAO's discriminator namespace differs across versions; verify against the IDL bytes directly.
    const DISC: [u8;8] = [212,237,148,56,97,245,51,169];
    if d.len() < 8 || d[..8] != DISC { return Err(DecodeError::BadDiscriminator); }
    NetworkState::try_from_slice(&d[8..]).map_err(|_| DecodeError::Borsh)
}
// keeper usage: let treasury = sdk::state::network_state(&acct.data)?.config.treasury;
```

```
// tests/discriminators.rs — guards against ix-name typos using the IDL as oracle
#[test] fn disc_matches_idl() {
    use flipvault_sdk::discriminator::*;
    assert_eq!(ix_discriminator("deposit"),      DISC_DEPOSIT);
    assert_eq!(ix_discriminator("withdraw"),     DISC_WITHDRAW);
    assert_eq!(ix_discriminator("commit_round"), DISC_COMMIT_ROUND);
    assert_eq!(ix_discriminator("settle_round"), DISC_SETTLE_ROUND);
    assert_eq!(ix_discriminator("recover_round"),DISC_RECOVER_ROUND);
}
```

```
// app-side .cargo/config.toml (in the Dioxus frontend crate, NOT in the SDK)
// Required because transitive wasm deps (RPC client, ed25519) use getrandom 0.2.
[target.wasm32-unknown-unknown]
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']
[build]
# trunk/dx will set the target; getrandom 'js'/'wasm_js' feature must also be enabled
//  via the consumer's getrandom = { version="0.2", features=["js"] } if pinned.
```

**Risks:**
- [high] `solana-pubkey`/`solana-instruction` major-version churn (2.x vs 3.x) can mismatch whatever `solana-client` version the keeper uses, causing two incompatible `Pubkey`/`Instruction` types in the keeper's graph (E0308 across crate boundaries). → Pin the SDK's solana-pubkey/solana-instruction to the SAME minor line that the keeper's `solana-client`/`solana-sdk` re-exports, via a workspace-level `[patch]`/unified version. Add a keeper build in CI that compiles SDK+client together. For the frontend, the wasm RPC fork must also agree on the Pubkey type or convert at the boundary.
- [high] Hand-rolled discriminator/borsh layout silently drifts from the deployed program if the program is ever upgraded or if a field type was mis-modeled (e.g. u128 vs u64), producing corrupt decodes or rejected instructions. → Ship the IDL discriminator mirror + the `disc_matches_idl` test (already sketched). Add an integration test that fetches a live devnet Config/Vault/Position account and asserts a successful decode with sane field values. Generate the struct field order directly from `flipvault.json` (a tiny build.rs codegen) so the SDK regenerates if the IDL changes.
- [medium] getrandom wasm build failure ('the wasm32-unknown-unknown target is not supported by default') from transitive deps in the FRONTEND, even though the SDK itself doesn't use getrandom. → Document and commit the `.cargo/config.toml` RUSTFLAGS `--cfg getrandom_backend="wasm_js"` (getrandom 0.2) in the frontend crate, plus `getrandom = { version="0.2", features=["js"] }`. If a dep pulls getrandom 0.3 the cfg name differs — pin getrandom to one major line across the wasm app.
- [high] Accidentally pulling `solana-program`, `solana-sdk`, `anchor-lang`, or `solana-client` into the SDK (directly or via a convenience helper) breaks the wasm build of the whole frontend. → Add a CI job `cargo build -p flipvault-sdk --target wasm32-unknown-unknown` that MUST pass. Add `cargo-deny`/a `#![forbid]`-style dep check or a test that asserts these crates are absent from `cargo tree`. Keep all RPC/signing strictly in consumer crates.
- [medium] ORAO `NetworkState`/randomness account layout or discriminator differs between the on-chain `orao-solana-vrf 0.6.1` dep and the JS SDK 0.8.0 expectations, so the keeper decodes the wrong `orao_treasury` or builds a bad `random` PDA. → Decode against the exact IDL bytes embedded above ([212,237,148,56,97,245,51,169]); cross-check the decoded `config.treasury` against the value the working TS `round.ts` script uses on devnet before trusting it in commit_round. Treat ORAO structs as version-pinned and covered by a live decode test.
- [medium] AccountMeta writable/signer flags or ORDER diverging from the program's `#[derive(Accounts)]` expectation (esp. commit_round's 7 accounts and settle_round's 8) -> 'AccountNotEnoughKeys'/'ConstraintSeeds' at runtime. → Order and writability are transcribed directly from the IDL accounts arrays (verified above). Add a devnet integration test that actually lands a commit_round+settle_round round through the SDK builders and asserts success, mirroring the existing working TS scripts.

**Open questions:**
- Which solana-pubkey/solana-instruction major line to pin: the keeper's `solana-client` will dictate it (2.1.x for devnet parity per ORE's 2.1 stack, vs 3.x newest). Need to confirm the keeper's chosen solana-client version first so the SDK matches and avoids dual-Pubkey-type errors.
- Frontend RPC choice: `solana-client-wasm`+`solana-extra-wasm` (ORE's forks, possibly stale) vs `wasm_client_solana` (newer, maintained). Affects whether the frontend's Pubkey type matches the SDK's and whether a conversion shim at the SDK boundary is needed.
- Wallet adapter path: pure-Rust (`wallet-adapter`/`wasi-sol` crates, both early/WIP) vs a thin wasm-bindgen bridge to the JS `@solana/wallet-adapter`. Determines how the SDK's unsigned `Instruction`s get wrapped into a `Transaction` and signed — out of SDK scope but it constrains what type the SDK should hand back (Instruction vs a serialized message).
- Whether to generate `state.rs`/`ix.rs` from `flipvault.json` via a build.rs (drift-proof, more machinery) or keep them hand-written and guarded by tests (simpler, sketched here). For a frozen on-chain program, hand-written + IDL-oracle tests is likely enough.
- Does the deployed program write `selected_vault = NO_VAULT` as 255 or another sentinel? The decoder reads it as a plain u8; the frontend needs the exact sentinel to render 'no vault selected' — confirm from the program constants.
- ORAO randomness *result* decoding (`RandomnessAccountData.getFulfilledRandomness()` in JS): does the keeper need to decode the ORAO randomness account in Rust to know fulfillment, or can it rely on settle_round failing with `RandomnessNotResolved` (6006) and just retry? If decoding is needed, the SDK should also expose an ORAO randomness decoder.


### Rust Keeper Service (native tokio round driver for FlipVault) (keeper)

A single-binary native tokio service that drives FlipVault rounds: a 1 Hz tick loop reads Config (phase, last_settled_ts, round_secs, commit_ts, round_seed) and runs a per-state machine — Idle+due -> commit_round (fresh 32-byte force, supply ORAO network_state + its treasury read from NetworkState.config.treasury), Pending -> poll ORAO randomness PDA until fulfilled then settle_round, Pending+stuck>300s -> recover_round. Because the on-chain program already encodes every guard (RoundTooSoon, RoundPending, NoPendingRound, RandomnessNotResolved, RecoverTooSoon) and both settle and commit are permissionless, the keeper is a "best-effort scheduler over an authoritative state machine": it never trusts its own clock for correctness, only re-reads Config and lets the program reject races. The deployed program's on-chain dep is orao-solana-vrf 0.6.1, but the keeper is OFF-CHAIN so it is free to use a current client stack: anchor-client 0.31.1 (matches the program/IDL, async feature) with Agave/solana v2.x crates, or — recommended for least dependency friction and tightest control over priority fees, blockhash, retries and ORAO account derivation — raw solana-rpc-client (nonblocking) + a hand-rolled flipvault instruction builder reusing the published discriminators, plus orao-solana-vrf as a lib only for its address-derivation + RandomnessAccountData::fulfilled_randomness() decoder. Keypair from an env-injected secret (or KMS/Vault), never on disk in prod. Observability via tracing -> JSON logs + a prometheus /metrics endpoint (rounds_settled_total, commit/settle latency, vrf_wait_seconds, keeper_sol_balance, stuck_round/recover counters) with Alertmanager rules on "no settle in 3x round_secs" and "balance below N rounds of fees". Ships as a distroless container, one replica (rounds are globally serialized by Config.phase so multi-instance is safe but pointless), run on Fly.io / a small VM / k8s Deployment with restart-always; crash recovery is automatic because all durable state is on-chain.

**Recommended stack:**
- tokio 1.x (latest 1.4x) — Async runtime: tick loop, timers (tokio::time::interval), ORAO polling, graceful shutdown via tokio::signal: De-facto Solana async runtime; the whole nonblocking RPC stack is built on it. Single-threaded flavor is enough (one round at a time), but multi-thread default is fine and simplest.
- solana-rpc-client + solana-rpc-client-api 2.x (Agave; pin to the validator line, e.g. 2.1/2.2) — nonblocking::rpc_client::RpcClient for get_account_data(Config/NetworkState/randomness), get_latest_blockhash, send/confirm tx, getRecentPrioritizationFees: First-class async client in 2025. Using the split Agave crates (solana-rpc-client, solana-keypair, solana-signer, solana-transaction, solana-instruction, solana-pubkey, solana-commitment-config) keeps the dep tree small and avoids the monolithic solana-sdk where possible.
- anchor-client 0.31.1 (async feature) — OPTIONAL — If you prefer Program::request().accounts().args().signer() ergonomics + typed account fetch, use the version that matches the deployed IDL/program (0.31.1): Matching anchor-client to the program's Anchor version (0.31.1) guarantees discriminator/borsh layout parity. The tradeoff: it pulls a large dep tree and pins solana crate versions. Recommended only if you want zero hand-rolled (de)serialization. Otherwise prefer raw RPC + a tiny instruction builder.
- orao-solana-vrf 0.6.1 (lib usage only, must match the on-chain layout the program was built against) — Off-chain helpers: randomness_account_address(seed), network_state_account_address(), RandomnessAccountData / fulfilled_randomness() decode, and CONFIG_ACCOUNT_SEED/RANDOMNESS_ACCOUNT_SEED constants: Reusing the SAME crate version the program compiled with guarantees the randomness account decode matches what settle_round expects. The keeper does NOT call ORAO directly — commit_round CPIs into ORAO — so the keeper only needs ORAO for PDA derivation + reading fulfillment status. Pin to 0.6.1 to stay byte-compatible with the deployed program.
- borsh 1.x (or 0.10 to match Anchor 0.31's borsh) — Decode Config (manual struct mirroring the on-chain layout after the 8-byte discriminator) when going the raw-RPC route: Anchor accounts are 8-byte discriminator + borsh. A hand-written #[derive(BorshDeserialize)] mirror of Config is ~30 lines and removes the anchor-client dependency for reads. Match the borsh version Anchor 0.31.1 uses to avoid enum/option encoding drift.
- tracing + tracing-subscriber 0.1 / 0.3 — Structured, leveled logs; JSON formatter in prod, pretty in dev; span per round carrying round_seed/commit_ts: Standard Rust observability. Spans give you per-round correlation (commit sig -> vrf wait -> settle sig) for free.
- prometheus (tikv/rust client) + axum prometheus 0.13, axum 0.7 — Tiny HTTP server exposing /metrics and /healthz; counters/gauges/histograms for round lifecycle: Pull-based metrics are the norm for long-running services; axum is a minimal tokio-native server. /healthz lets the orchestrator restart a wedged process.
- config: figment or envy + serde figment 0.10 — Load RPC URL(s), commitment, keeper secret source, poll/backoff intervals, priority-fee policy, recover threshold from env + optional TOML: 12-factor config; env-only in containers, file for local dev. RECOVER_AFTER_SECS is on-chain (300s) so the keeper threshold is a client-side mirror, not a source of truth.
- backon (or tokio-retry) backon 1.x — Exponential backoff with jitter around RPC sends and confirmations: Composable async retry combinators; cleaner than hand-rolled loops and gives jitter to avoid thundering-herd on a shared RPC.
- reqwest (optional, for alerting webhook) 0.12 — Fire a Slack/Discord/PagerDuty webhook on critical conditions if not using Alertmanager: Lightweight escape hatch when you don't run a full Prometheus+Alertmanager stack on devnet.

**Architecture:** TOPOLOGY: one async binary, one logical round-driver task + one metrics/health HTTP task, sharing an Arc<Keeper> (RpcClient, signer, parsed config, prometheus registry). No DB, no queue — the on-chain Config IS the durable state machine, so the keeper is stateless and crash-safe (restart re-reads Config and resumes mid-round).

MAIN LOOP (1 Hz tick, tokio::time::interval, MissedTickBehavior::Delay): every tick fetch Config (single get_account_data at commitment=confirmed) and branch on a freshly-read snapshot — never on cached/local timers — so all correctness lives on-chain:
  1. phase==Idle and now >= last_settled_ts + round_secs  -> COMMIT.
  2. phase==Pending and randomness fulfilled               -> SETTLE.
  3. phase==Pending and now >= commit_ts + RECOVER_AFTER_SECS (300) and NOT fulfilled -> RECOVER.
  4. phase==Pending, not fulfilled, not past recover window -> WAIT (do nothing this tick; the dedicated poll path below handles it faster than 1 Hz).
  5. phase==Idle and not yet due -> sleep until due (compute deadline, await tokio::time::sleep, but re-read on wake — clocks drift, validator unix_timestamp is the authority).
Concurrency guard: an in-process tokio::sync::Mutex<()> (or an AtomicBool "action in flight") ensures only ONE commit/settle/recover RPC is outstanding at a time, so a slow confirmation can't trigger a duplicate on the next tick. Idempotency beyond that is delegated to the program: a double commit hits RoundPending; a premature settle hits RandomnessNotResolved/NoPendingRound; a premature recover hits RecoverTooSoon — all are caught, logged at info/debug, and treated as benign no-ops, not errors.

COMMIT path: generate force = 32 random bytes via rand (reject all-zero, which the program also rejects via InvalidParams); derive random = randomnessAccountAddress(force) and network_state = networkStateAccountAddress() under ORAO program id; read NetworkState account, decode it, and pull orao_treasury = network_state.config.treasury (must be the live value, ORAO validates it on-chain). Build the commit_round instruction (discriminator 229,102,157,34,152,217,15,70 + 32-byte force arg; accounts in IDL order: keeper(signer,mut), config(mut), random(mut), orao_treasury(mut), network_state(mut), vrf=VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y, system_program). Prepend ComputeBudget set_compute_unit_limit + set_compute_unit_price (priority fee) instructions. Send with the landing pattern below. On success, store the force seed in an in-memory "current round" cell so the settle/poll path knows the seed even before re-reading Config (Config.round_seed is also authoritative — re-read it on settle to be safe, since another keeper could have committed a different round if you ever ran >1 instance).

POLL-FOR-VRF path: after a successful commit (or whenever a Pending round is observed), spin a tight bounded poll: every ~400-800ms get_account_data(random PDA); if account exists, decode RandomnessAccountData (orao 0.6.1) and check fulfilled_randomness().is_some(). ORAO on devnet is typically sub-second to a few seconds. Cap the poll at RECOVER_AFTER_SECS; if exceeded, fall through to RECOVER on the main loop. Use config.round_seed read back from chain to derive the PDA for settle so the keeper settles exactly the round the program committed.

SETTLE path: build settle_round (no args; accounts: config(mut), reserve(mut), vault0..3(mut) derived as PDA [b"vault",[i]], random PDA derived from config.round_seed). Send with the landing pattern. settle_round is permissionless and has NO signer requirement beyond the fee payer, so the keeper just pays fees. After confirmation, re-read Config to log selected_vault and emit metrics (round latency commit_ts->settle confirm, vrf_wait).

RECOVER path: build recover_round (accounts: config(mut) only). The program enforces now >= commit_ts + 300; if called early it returns RecoverTooSoon (caught, no-op). On success the round is cancelled (no flip), phase->Idle, last_settled_ts=now, so the next tick can commit a fresh round.

TRANSACTION LANDING (shared by commit/settle/recover): fetch a confirmed blockhash + lastValidBlockHeight; sign; send with skip_preflight=false initially (catch program errors early on devnet) but expose a config flag to flip skip_preflight=true for low-latency mainnet; rebroadcast the SAME signed tx every ~2s until either confirmed (poll get_signature_statuses at confirmed) or the blockhash expires (block height > lastValidBlockHeight), then rebuild with a fresh blockhash and a bumped priority fee. Priority fee derived from getRecentPrioritizationFees over the writable accounts (config/reserve/vaults), with a floor + cap. This is the Helius/Solana-docs canonical land-and-retry loop. Wrap the whole send in backon exponential backoff for transient RPC/network errors (distinct from program errors, which are terminal-but-benign and must NOT be retried blindly).

ERROR TAXONOMY (decides retry vs no-op vs alert): (a) Transient RPC/network/timeout -> backoff+retry. (b) Blockhash expired -> rebuild+retry with higher fee. (c) Program guard errors (RoundPending/RoundTooSoon/NoPendingRound/RandomnessNotResolved/RecoverTooSoon) -> structured-log at debug, treat as benign race, advance loop. (d) Insufficient keeper SOL / ORAO fee unpayable -> ERROR + alert (keeper can't fund commits), pause commits but keep trying settles (settle is cheap). (e) Unknown program error -> ERROR + alert, keep looping (don't crash; the program is the source of truth and a human can recover_round manually via the existing TS script).

SCHEDULING PRECISION: round_secs ~30s and the on-chain due-check is now >= last_settled_ts + round_secs using the validator's unix_timestamp, which can lag wall-clock by a few seconds. So the keeper deliberately attempts commit slightly LATE rather than early (avoids RoundTooSoon churn): when computing the sleep deadline, add a small skew margin (e.g. +1s) and always re-read Config on wake. The 1 Hz tick guarantees a missed deadline costs at most ~1s of latency; ORE-like perceived speed is preserved because the frontend reads the indexer/RPC, not the keeper, and the round cadence is bounded by round_secs + commit-confirm + vrf-wait + settle-confirm (~31-35s typical on devnet).

SECRETS: keeper signer loaded at startup from (priority order) (1) KMS/Vault-fetched secret, (2) env var KEEPER_SECRET_KEY as base58 or JSON byte array parsed via Keypair::try_from, (3) local file path for dev only. Never bake the keypair into the image; mount via container secret / Fly secrets / k8s Secret. The keeper key needs only enough SOL to pay tx fees + ORAO request fees + randomness-account rent per commit; it is NOT the treasury_authority and cannot sweep funds, so blast radius of a leaked keeper key is limited to grief (spamming commits, which the time-guard rate-limits anyway).

DEPLOYMENT: multi-stage Dockerfile (rust:1.92 builder -> gcr.io/distroless/cc-debian12 runtime), single static-ish binary, USER nonroot, EXPOSE 9100 for /metrics + /healthz. One replica (Deployment replicas=1, or a Fly machine with min=max=1) — the Config.phase global lock makes multiple instances correct-but-redundant and they'd waste ORAO fees racing to commit, so run exactly one and rely on restart-always for availability. Healthcheck hits /healthz which returns 200 only if the last successful Config read was within N seconds AND (keeper balance > min-fee-floor). Add it to the existing docker-compose as a `keeper` service alongside indexer/api, pointing at the same devnet RPC; for mainnet later, point at a paid RPC (Helius/Triton) and enable skip_preflight + priority fees.

**Code sketches:**
```
// ---- Config mirror (raw-RPC route): borsh layout after 8-byte anchor discriminator ----
#[derive(borsh::BorshDeserialize)]
struct Config {
    treasury_authority: [u8; 32], // Pubkey
    r_tok: u128,
    k: u128,
    round_secs: i64,
    last_settled_ts: i64,
    fee_bps: u16,
    min_reserve: u64,
    phase: u8,            // 0 = Idle, 1 = Pending  (enum repr)
    round_seed: [u8; 32],
    commit_slot: u64,
    commit_ts: i64,
    selected_vault: u8,
    bump: u8,
    reserve_bump: u8,
    treasury_bump: u8,
}

async fn fetch_config(rpc: &RpcClient, config_pda: &Pubkey) -> anyhow::Result<Config> {
    let data = rpc.get_account_data(config_pda).await?;
    // skip 8-byte discriminator
    Ok(Config::try_from_slice(&data[8..])?)
}
```

```
// ---- Main round-driver loop ----
async fn run(k: Arc<Keeper>) -> anyhow::Result<()> {
    let mut tick = tokio::time::interval(Duration::from_millis(1000));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let _guard = match k.action_lock.try_lock() { Ok(g) => g, Err(_) => continue };
        let cfg = match fetch_config(&k.rpc, &k.config_pda).await {
            Ok(c) => c,
            Err(e) => { k.metrics.config_read_errors.inc(); warn!(?e, "config read failed"); continue }
        };
        let now = k.rpc.get_block_time(k.rpc.get_slot().await?).await?; // validator clock
        match cfg.phase {
            PHASE_IDLE => {
                if now >= cfg.last_settled_ts + cfg.round_secs {
                    if let Err(e) = commit_round(&k).await { handle_action_err(&k, "commit", e) }
                }
            }
            PHASE_PENDING => {
                if vrf_fulfilled(&k, &cfg.round_seed).await? {
                    if let Err(e) = settle_round(&k, &cfg).await { handle_action_err(&k, "settle", e) }
                } else if now >= cfg.commit_ts + RECOVER_AFTER_SECS {
                    if let Err(e) = recover_round(&k).await { handle_action_err(&k, "recover", e) }
                } // else: still waiting on VRF, fast poller handles it
            }
            _ => warn!(phase = cfg.phase, "unknown phase"),
        }
    }
}
```

```
// ---- commit_round: fresh seed + ORAO accounts (orao_treasury read live) ----
async fn commit_round(k: &Keeper) -> anyhow::Result<Signature> {
    let force: [u8; 32] = loop { let s: [u8;32] = rand::random(); if s != [0u8;32] { break s } };
    let random       = randomness_account_address(&force);          // orao helper
    let network_state = network_state_account_address();            // orao helper
    let ns = fetch_network_state(&k.rpc, &network_state).await?;    // decode NetworkState
    let orao_treasury = ns.config.treasury;                         // live treasury

    let mut data = COMMIT_ROUND_DISCM.to_vec();   // [229,102,157,34,152,217,15,70]
    data.extend_from_slice(&force);
    let ix = Instruction {
        program_id: k.program_id,
        accounts: vec![
            AccountMeta::new(k.signer.pubkey(), true),     // keeper (signer, mut)
            AccountMeta::new(k.config_pda, false),         // config (mut)
            AccountMeta::new(random, false),               // random (mut)
            AccountMeta::new(orao_treasury, false),        // orao_treasury (mut)
            AccountMeta::new(network_state, false),        // network_state (mut)
            AccountMeta::new_readonly(ORAO_VRF_ID, false), // vrf program
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    };
    let sig = land_tx(k, &[cu_limit_ix(), cu_price_ix(k.prio_fee), ix]).await?;
    info!(seed = %hex::encode(force), %sig, "committed round");
    k.metrics.commits_total.inc();
    Ok(sig)
}
```

```
// ---- VRF fulfillment check (orao 0.6.1 decoder, byte-compatible with the program) ----
async fn vrf_fulfilled(k: &Keeper, seed: &[u8;32]) -> anyhow::Result<bool> {
    let random = randomness_account_address(seed);
    match k.rpc.get_account(&random).await {
        Ok(acc) => {
            let r = RandomnessAccountData::try_deserialize(&mut &acc.data[..])?;
            Ok(r.fulfilled_randomness().is_some())
        }
        Err(_) => Ok(false), // account not visible yet
    }
}
```

```
// ---- settle_round: permissionless, random PDA pinned to on-chain round_seed ----
async fn settle_round(k: &Keeper, cfg: &Config) -> anyhow::Result<Signature> {
    let random = randomness_account_address(&cfg.round_seed); // use chain's seed, not local
    let mut accounts = vec![
        AccountMeta::new(k.config_pda, false),
        AccountMeta::new(k.reserve_pda, false),
    ];
    for i in 0u8..4 { accounts.push(AccountMeta::new(vault_pda(&k.program_id, i), false)); }
    accounts.push(AccountMeta::new_readonly(random, false));
    let ix = Instruction { program_id: k.program_id, accounts, data: SETTLE_ROUND_DISCM.to_vec() };
    let sig = land_tx(k, &[cu_limit_ix(), cu_price_ix(k.prio_fee), ix]).await?;
    k.metrics.settles_total.inc();
    Ok(sig)
}
```

```
// ---- error classification: program guards are benign no-ops, not failures ----
fn handle_action_err(k: &Keeper, action: &str, e: anyhow::Error) {
    let s = e.to_string();
    // Anchor custom error names surface in logs / error codes
    if ["RoundPending","RoundTooSoon","NoPendingRound","RandomnessNotResolved","RecoverTooSoon"]
        .iter().any(|g| s.contains(g)) {
        debug!(action, err = %s, "benign on-chain race, skipping");
        k.metrics.benign_races.inc();
    } else if s.contains("insufficient") || s.contains("0x1") /* InsufficientFunds */ {
        error!(action, err = %s, "keeper funding problem");
        k.metrics.funding_alerts.inc();
        k.alert(format!("FlipVault keeper {action} funding error: {s}"));
    } else {
        error!(action, err = %s, "unexpected action error");
        k.metrics.action_errors.inc();
    }
}
```

```
// ---- land-and-retry: rebroadcast same tx until confirmed or blockhash expires ----
async fn land_tx(k: &Keeper, ixs: &[Instruction]) -> anyhow::Result<Signature> {
    loop {
        let (bh, last_valid) = k.rpc.get_latest_blockhash_with_commitment(CommitmentConfig::confirmed()).await?.into();
        let tx = Transaction::new_signed_with_payer(ixs, Some(&k.signer.pubkey()), &[&k.signer], bh);
        let sig = k.rpc.send_transaction_with_config(&tx, RpcSendTransactionConfig {
            skip_preflight: k.skip_preflight, max_retries: Some(0), ..Default::default()
        }).await?;
        loop {
            if let Some(Ok(())) = k.rpc.get_signature_status(&sig).await? { return Ok(sig) }
            if k.rpc.get_block_height().await? > last_valid { break } // expired -> rebuild w/ higher fee
            tokio::time::sleep(Duration::from_millis(2000)).await;
            let _ = k.rpc.send_transaction(&tx).await; // rebroadcast same signed tx
        }
    }
}
```

**Risks:**
- [high] orao-solana-vrf client/account layout mismatch: the program was built against on-chain dep 0.6.1; if the keeper decodes NetworkState/RandomnessAccountData with a different ORAO crate version, treasury or fulfilled_randomness reads can silently break and the keeper commits to a bad treasury or never detects fulfillment. → Pin the keeper's orao-solana-vrf to the same 0.6.1 the program uses; add an integration test on devnet that does a full commit->poll->settle and asserts selected_vault changed. Treat ORAO version bumps as a coordinated program+keeper change.
- [high] Keeper key drained / under-funded: every commit pays tx fee + ORAO request fee + randomness-account rent. If balance falls below one round's cost, commits silently stop and rounds stall until refunded. → keeper_sol_balance gauge + Alertmanager rule firing when balance < (N rounds * per-commit cost); auto-pause commits (but keep settles, which are cheap) below floor; document a funding runbook; on mainnet, a watcher that top-ups from a hot wallet.
- [low] Validator unix_timestamp lag vs wall clock causes RoundTooSoon churn (commit attempted before on-chain now reaches last_settled_ts+round_secs), wasting RPC and emitting noisy errors. → Always gate on the on-chain clock (get_block_time of current slot), add a +1s skew margin to the local sleep deadline, classify RoundTooSoon as a benign debug-level race, and rely on the 1 Hz tick to retry.
- [medium] ORAO never fulfills (devnet outage / fee config change): round sits Pending, vaults locked (no deposit/withdraw) for up to 300s before recover is even allowed. → Fast poller bounded to RECOVER_AFTER_SECS, then automatic recover_round; metric vrf_wait_seconds with an alert at >60s; expose stuck-round count; keep the existing TS recover.ts as a manual fallback. Communicate to frontend that Pending = locked so UX shows a 'flipping...' state.
- [medium] Double-commit / wasted ORAO fees if more than one keeper instance runs, since both race to commit and one loses to RoundPending after already paying nothing — but request_v2 rent/fee could be paid by the loser if timing overlaps. → Run exactly ONE replica (replicas=1 / single Fly machine); rely on restart-always for HA. In-process action_lock prevents intra-process duplicates. The on-chain time-guard + phase lock make a stray second instance correctness-safe, just wasteful — so don't horizontally scale.
- [medium] Transaction landing failure under congestion (mainnet): settle/commit dropped, blockhash expires, round latency spikes and frontend feels laggy. → Priority fees from getRecentPrioritizationFees with floor+cap, rebroadcast-until-expiry loop, fee bump on each rebuild, paid RPC (Helius/Triton) on mainnet, optional skip_preflight=true for latency; histogram on commit/settle confirm latency with alerting.
- [medium] Silent crash / hang (e.g. RPC websocket wedge, deadlocked poll) leaves rounds undriven with no error. → /healthz returns 200 only if last successful Config read < N seconds ago; container restart-always + orchestrator liveness probe on /healthz; watchdog metric last_settle_timestamp with alert 'no settle in 3x round_secs'.
- [low] Anchor-client vs Agave crate version conflicts blow up the build (anchor 0.31.1 pins older solana crates than current Agave 2.x). → Prefer the raw solana-rpc-client + hand-rolled instruction builder route (no anchor-client) to decouple from Anchor's pins; if using anchor-client, pin the whole solana-* tree to the versions anchor 0.31.1 resolves and don't mix in newer Agave crates. Reuse the program's published discriminators rather than the IDL codegen.

**Open questions:**
- RPC provider for devnet now and mainnet later: public api.devnet.solana.com is rate-limited and will throttle the 1 Hz poll + send loop. Confirm whether to budget a Helius/Triton/QuickNode endpoint (recommended) and whether to use its staked-connection / sendTransaction enhancements for landing.
- Exact per-commit cost on mainnet (ORAO request fee + randomness rent + priority fee) to size the keeper hot-wallet float and the low-balance alert threshold — needs a live measurement against ORAO's current fee config.
- Whether to add an out-of-band 'force settle nudge' for the case where ORAO fulfilled but the keeper missed it (e.g. crashed between commit and settle): on restart the loop will see Pending+fulfilled and settle, so likely no extra mechanism is needed — confirm restart-resume covers it (it should).
- Priority-fee policy parameters (floor, cap, percentile of getRecentPrioritizationFees, bump factor on rebroadcast) — devnet basically needs none, mainnet needs tuning under real congestion.
- Alerting transport: full Prometheus+Alertmanager vs a simple webhook (Slack/Discord) for a small devnet deployment — pick based on what the indexer/API layer already runs so metrics/alerting is shared.
- Should the keeper expose the current round seed / next-due timestamp over its HTTP endpoint so the indexer/frontend can show an exact countdown source-of-truth, or should the frontend derive countdown purely from on-chain Config.last_settled_ts + round_secs (cleaner, no keeper coupling)? Recommend the latter.


### Rust Indexer + axum API for FlipVault (indexer)

Ingestion + Postgres + axum API layer for the deployed FlipVault program (EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H). DECISIVE FINDING from reading the IDL: there is NO `events` array — the program emits zero Anchor events and writes no custom logs. So every fact the frontend wants (selected_vault, the SOL amount a flip moved, the fee on a withdrawal, a position's share delta) is NOT in any log line; it must be reconstructed from (1) decoded instruction args (vault_id, slot, amount, shares, the 32-byte VRF force) plus (2) account-state DELTAS, i.e. pre/post borsh-decoded Config/Vault/Position pulled from the transaction's loaded account writes (Geyser) or re-fetched at the tx slot (RPC). Design: a single Rust binary running two datasources into one processing core — a real-time tail (devnet starts on RPC logsSubscribe/getProgramAccounts; mainnet upgrades to Yellowstone gRPC with zero processor changes) and an RPC backfill crawler (getSignaturesForAddress before/until + getTransaction). Decoded rows land in Postgres via idempotent upserts keyed by (tx_signature, instruction_index); an axum 0.8 service serves REST for current-state fallback, round/flip history, vault history, user positions+PnL, leaderboard, fee totals, and chart time-series, with an SSE channel fed by Postgres LISTEN/NOTIFY so the Dioxus frontend feels ORE-instant without polling. Recommend Carbon (carbon-core 0.12) as the pipeline skeleton because it gives RPC tail, gRPC tail, AND a transaction-backfill crawler behind one Processor trait and ships an Anchor-IDL decoder generator, while letting us hand-write the account-delta logic Carbon can't infer. Devnet reorg tolerance via a confirmed→finalized two-phase commit on every row.

**Recommended stack:**
- carbon-core (+ carbon-rpc-program-subscribe-datasource, carbon-rpc-transaction-crawler-datasource, carbon-yellowstone-grpc-datasource) 0.12.0 — Indexing pipeline skeleton: one Processor trait fed by three interchangeable datasources (RPC logs/program tail for devnet now, RPC tx-crawler for backfill, Yellowstone gRPC for mainnet later). carbon-cli can generate an Anchor-IDL instruction decoder from flipvault.json.: Sevenlabs Carbon is the 2025 production-standard Rust indexing framework: same processor model across real-time, backfill, and snapshot, swappable datasources, and a Postgres example. Lets us move devnet->mainnet (RPC->gRPC) by swapping a datasource line, not rewriting decode/store logic. Latest crates.io is 0.12.0.
- solana-client / solana-transaction-status / solana-sdk 2.1.x — Direct RPC fallback + types: get_signatures_for_address_with_config (before/until pagination), get_transaction_with_config(maxSupportedTransactionVersion=0), get_multiple_accounts at a slot for state snapshots, and EncodedConfirmedTransactionWithStatusMeta decoding.: Even with Carbon we need raw RPC for the current-state fallback endpoint and for re-fetching account state at a slot (since no events carry flip amounts). 2.x is the current agave client family and matches transaction-status 2.x.
- anchor-lang (no_entrypoint) / borsh 0.31.1 / 1.5 — Borsh-decode Config, Vault(+Tranche/Asset enums), Position using the exact discriminators in the IDL; decode instruction args after the 8-byte sighash. Pull the generated types from flipvault/target/types or a thin shared `flipvault-types` crate.: Must match the deployed program's encoding byte-for-byte. The IDL gives every account discriminator and the Tranche{asset:enum,amount:u64,total_shares:u64} layout we deserialize.
- axum + tower-http 0.8.9 / 0.6 — REST + SSE HTTP server. tower-http for CORS (Dioxus/WASM origin), compression, trace, and timeout layers.: Current axum is 0.8.x; macro-free routing, extractor model, native SSE (sse module no longer requires the tokio feature except keep-alive as of 0.8.5). Standard 2025 choice.
- sqlx (postgres, runtime-tokio-rustls, macros, migrate, bigdecimal) 0.8.6 — Compile-time-checked async Postgres queries, PgPool, embedded migrations, and a dedicated PgListener task for LISTEN/NOTIFY -> SSE.: Pure-Rust async, compile-time query verification catches schema drift, built-in pooling and migrations. 0.8.x is current.
- async-graphql + async-graphql-axum 7.0.15 — OPTIONAL second surface mounted on the same axum router at /graphql for the leaderboard/history/chart queries the frontend composes flexibly; GraphQL subscriptions map cleanly onto the same NOTIFY stream.: 7.0.15 is the current release and is documented compatible with axum 0.8. Lets the frontend ask for exactly the fields a vault card or PnL view needs in one round trip. REST stays the primary, GraphQL is additive.
- tokio 1.40+ — Async runtime for the indexer tasks (datasource stream, NOTIFY listener, backfill crawler) and the axum server.: Required by axum, sqlx, solana-client, and carbon.
- PostgreSQL 16 — Store of record for rounds, flips, vault_snapshots, deposits, withdrawals, positions, position_events, fees; serves all API reads; LISTEN/NOTIFY drives live push.: Carbon, sqlx, and the team's TS scripts already imply Postgres; JSONB for raw-tx archival, generated columns and BRIN indexes on slot for cheap time-series.

**Architecture:** TWO PROCESSES sharing one Postgres + one shared `flipvault-types` crate (re-exports the Anchor account/arg structs + discriminators so indexer and API agree on decoding).

PROCESS A — indexer (one binary, three async tasks under tokio):
  TASK 1 realtime tail. Devnet now: Carbon RPC datasource = logsSubscribe(mentions=[programId]) for tx signatures + programSubscribe/account tail for Config+4 Vaults+Positions. Because the program emits NO events, the log subscription is only a cheap "a flipvault tx touched the chain" trigger; the real signal is the account writes. For each program-owned account update Carbon hands us (pubkey, slot, data, write_version), we borsh-decode by discriminator and persist a *_snapshot row. Mainnet later: swap this one datasource for carbon-yellowstone-grpc-datasource (typed account+tx stream, sub-50ms) — the Processor below is unchanged.
  TASK 2 transaction decoder. For every confirmed flipvault signature, fetch the full tx (Carbon tx datasource, or get_transaction_with_config maxSupportedTransactionVersion=0). Walk instructions whose program_id == flipvault; match the 8-byte discriminator against the IDL's instruction discriminators (deposit 242,35,..; withdraw 183,18,..; commit_round 229,102,..; settle_round 40,101,..; recover_round 87,173,..). Borsh-decode the args after the sighash. Crucially, derive the *effects* that have no event by diffing account state in the SAME tx: meta.pre/post account data for the touched Vault/Config/Position (Geyser gives this directly; on plain RPC we fetch the accounts at tx.slot and tx.slot-1 via getMultipleAccounts and decode both). From those diffs we compute: deposit shares minted = post.tranche.total_shares - pre; withdrawal fee lamports = the treasury PDA's post-pre lamport delta; flip's moved SOL = the selected vault's SOL-tranche amount pre vs post + reserve lamport delta; selected_vault = post Config.selected_vault. lamports come from meta.pre/postBalances by account index.
  TASK 3 backfill crawler. carbon-rpc-transaction-crawler-datasource (or hand-rolled loop) paginates getSignaturesForAddress(programId, before=oldest_seen, limit=1000) backward to genesis of the program, feeding the SAME decoder as Task 2. A small `cursor` table stores the newest finalized signature processed so restarts resume forward, and the backfill watermark so we never re-walk history. Runs once on cold start, then idles.
  All three converge on a single store() that does idempotent upserts (see below) and, after commit, issues `pg_notify('flipvault_events', json)` so the API can push.

PROCESS B — axum API:
  PgPool for reads; a dedicated background task holds a sqlx PgListener on 'flipvault_events' and fans each NOTIFY into a tokio::sync::broadcast channel; the /sse/stream handler turns a broadcast Receiver into an axum Sse stream (keep-alive 15s). A /state/current handler is the ORE-instant fallback: it serves the latest vault_snapshots + the open round + countdown straight from Postgres (single indexed query), and if the indexer is briefly behind, optionally does a live getMultipleAccounts(Config,4 Vaults) decode so the UI is never stale on first paint. Optional /graphql via async-graphql-axum on the same Router.

CONFIRMED vs FINALIZED / REORG (devnet). Every datasource update carries a commitment. We write rows at `confirmed` immediately (status='confirmed') so the UI is fast, then a lightweight finality task re-reads each confirmed signature's status (getSignatureStatuses) once it should be finalized and flips status='finalized'. If a previously-confirmed signature is dropped/forked away on devnet, its rows (keyed by signature) are marked status='orphaned' and excluded from API reads (all read queries filter status != 'orphaned'). Because every table is keyed by (tx_signature, instruction_index) and slot is stored, a reorg is a targeted delete/flag by signature, not a rebuild. Account-snapshot rows additionally carry (slot, write_version) so a later same-slot write supersedes via upsert.

**Code sketches:**
```
-- migrations/0001_init.sql  (idempotent, signature-keyed)
CREATE TYPE rec_status AS ENUM ('confirmed','finalized','orphaned');

-- one row per commit_round + its settle (round lifecycle)
CREATE TABLE rounds (
  round_seed      BYTEA PRIMARY KEY,            -- the 32-byte VRF force; unique per round
  commit_sig      TEXT NOT NULL,
  commit_slot     BIGINT NOT NULL,
  commit_ts       TIMESTAMPTZ,
  settle_sig      TEXT,                         -- null until settled
  settle_slot     BIGINT,
  settle_ts       TIMESTAMPTZ,
  selected_vault  SMALLINT,                     -- from post-settle Config.selected_vault (NO_VAULT=255 => null)
  outcome         TEXT NOT NULL DEFAULT 'pending', -- pending|settled|recovered
  status          rec_status NOT NULL DEFAULT 'confirmed'
);
CREATE INDEX rounds_commit_slot ON rounds(commit_slot);

-- the actual flip economics, derived from Vault/Reserve deltas in the settle tx
CREATE TABLE flips (
  settle_sig      TEXT, instruction_index INT,
  round_seed      BYTEA REFERENCES rounds(round_seed),
  slot            BIGINT NOT NULL,
  vault_id        SMALLINT NOT NULL,
  sol_in_lamports  BIGINT NOT NULL,             -- SOL tranche spent into curve
  tok_out          NUMERIC(40,0) NOT NULL,      -- virtual tokens received (u128-safe)
  reserve_delta   BIGINT NOT NULL,              -- reserve lamport change
  status          rec_status NOT NULL DEFAULT 'confirmed',
  PRIMARY KEY (settle_sig, instruction_index)
);

CREATE TABLE deposits (
  sig TEXT, instruction_index INT,
  slot BIGINT NOT NULL, block_ts TIMESTAMPTZ,
  owner TEXT NOT NULL, vault_id SMALLINT NOT NULL, tranche_slot SMALLINT NOT NULL,
  amount_lamports BIGINT NOT NULL,              -- arg `amount`
  shares_minted   BIGINT NOT NULL,              -- post.total_shares - pre.total_shares (no event!)
  status rec_status NOT NULL DEFAULT 'confirmed',
  PRIMARY KEY (sig, instruction_index)
);
CREATE INDEX deposits_owner ON deposits(owner);

CREATE TABLE withdrawals (
  sig TEXT, instruction_index INT,
  slot BIGINT NOT NULL, block_ts TIMESTAMPTZ,
  owner TEXT NOT NULL, vault_id SMALLINT NOT NULL, tranche_slot SMALLINT NOT NULL,
  shares_burned BIGINT NOT NULL,                -- arg `shares`
  gross_lamports BIGINT NOT NULL,               -- pre-fee payout (reserve/vault delta)
  fee_lamports   BIGINT NOT NULL,               -- treasury PDA post-pre lamport delta
  net_lamports   BIGINT NOT NULL,
  status rec_status NOT NULL DEFAULT 'confirmed',
  PRIMARY KEY (sig, instruction_index)
);
CREATE INDEX withdrawals_owner ON withdrawals(owner);

-- per-(vault,tranche) point-in-time snapshot from account writes; powers charts + current state
CREATE TABLE vault_snapshots (
  vault_id SMALLINT, tranche_slot SMALLINT,
  slot BIGINT, write_version BIGINT,
  asset SMALLINT NOT NULL,                       -- 0=Sol 1=Token
  amount BIGINT NOT NULL, total_shares BIGINT NOT NULL,
  vault_lamports BIGINT NOT NULL,
  captured_ts TIMESTAMPTZ DEFAULT now(),
  status rec_status NOT NULL DEFAULT 'confirmed',
  PRIMARY KEY (vault_id, tranche_slot, slot, write_version)
);
CREATE INDEX vault_snap_latest ON vault_snapshots(vault_id, tranche_slot, slot DESC);

-- latest-known share balance per Position PDA (upsert on every touch)
CREATE TABLE positions (
  owner TEXT, vault_id SMALLINT, tranche_slot SMALLINT,
  shares BIGINT NOT NULL, last_slot BIGINT NOT NULL,
  status rec_status NOT NULL DEFAULT 'confirmed',
  PRIMARY KEY (owner, vault_id, tranche_slot)
);

CREATE TABLE fees (                              -- treasury accrual + sweeps
  sig TEXT, instruction_index INT,
  slot BIGINT, block_ts TIMESTAMPTZ,
  kind TEXT NOT NULL,                            -- 'accrual'|'sweep'
  amount_lamports BIGINT NOT NULL,
  recipient TEXT,                                -- for sweeps
  status rec_status NOT NULL DEFAULT 'confirmed',
  PRIMARY KEY (sig, instruction_index)
);

CREATE TABLE cursor (id INT PRIMARY KEY DEFAULT 1, newest_sig TEXT, backfill_oldest_sig TEXT, updated_at TIMESTAMPTZ DEFAULT now());
```

```
// indexer: decode one flipvault instruction (no events => match discriminator + diff state)
const DEPOSIT: [u8;8]=[242,35,198,137,82,225,242,182];
const WITHDRAW:[u8;8]=[183,18,70,156,148,109,161,34];
const SETTLE:  [u8;8]=[40,101,18,1,31,129,52,77];
const CONFIG_DISC: [u8;8]=[155,12,170,224,30,250,204,130];
const VAULT_DISC:  [u8;8]=[211,8,232,43,2,152,117,119];

async fn handle_ix(ix:&DecodedIx, meta:&TxMeta, db:&PgPool) -> anyhow::Result<()> {
    let (disc, body) = ix.data.split_at(8);
    match disc.try_into()? {
        DEPOSIT => {
            let a: DepositArgs = DepositArgs::deserialize(&mut &body[..])?; // vault_id,slot,amount
            // shares are NOT in the tx; derive from the Vault account delta in this same tx
            let pre  = decode_vault(meta.pre_account_data(&ix.accounts.vault))?;
            let post = decode_vault(meta.post_account_data(&ix.accounts.vault))?;
            let minted = post.tranches[a.slot as usize].total_shares
                       - pre.tranches[a.slot as usize].total_shares;
            sqlx::query!("INSERT INTO deposits (sig,instruction_index,slot,block_ts,owner,vault_id,tranche_slot,amount_lamports,shares_minted)\
                          VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)\
                          ON CONFLICT (sig,instruction_index) DO UPDATE SET shares_minted=EXCLUDED.shares_minted, status='confirmed'",
                meta.sig, ix.index as i32, meta.slot as i64, meta.block_ts,
                ix.accounts.user.to_string(), a.vault_id as i16, a.slot as i16,
                a.amount as i64, minted as i64).execute(db).await?;
        }
        SETTLE => {
            let cfg_post = decode_config(meta.post_account_data(&ix.accounts.config))?;
            let vid = cfg_post.selected_vault;
            if vid != NO_VAULT {
                let pre=decode_vault(meta.pre_account_data(&vault_key(vid)))?;
                let post=decode_vault(meta.post_account_data(&vault_key(vid)))?;
                let (sol_in, tok_out) = flip_delta(&pre,&post); // SOL tranche spent / virtual tok recv
                upsert_flip(db, &meta, vid, sol_in, tok_out).await?;
            }
            upsert_round_settled(db, &cfg_post.round_seed, &meta, vid).await?;
        }
        WITHDRAW => { /* shares from arg; fee = treasury post-pre lamports; gross from vault/reserve delta */ }
        _ => {}
    }
    sqlx::query!("SELECT pg_notify('flipvault_events',$1)", meta.notify_json()).execute(db).await?;
    Ok(())
}
```

```
// axum router: REST + SSE (+ optional GraphQL), all reads filter out orphaned rows
let app = Router::new()
    .route("/state/current",        get(current_state))   // ORE-instant fallback: latest snapshots + open round + countdown
    .route("/rounds",               get(list_rounds))      // ?limit&before  paginated history
    .route("/rounds/{seed}",        get(round_detail))     // round + its flip
    .route("/flips",                get(list_flips))       // ?vault_id filter
    .route("/vaults/{id}/history",  get(vault_history))    // tranche timeline for one vault
    .route("/vaults/{id}/series",   get(vault_series))     // downsampled time-series for charts (?bucket=1m)
    .route("/users/{owner}/positions", get(user_positions))// open positions
    .route("/users/{owner}/pnl",       get(user_pnl))      // deposits-in vs withdrawals-net + current value
    .route("/users/{owner}/history",   get(user_history))  // their deposits+withdrawals
    .route("/leaderboard",          get(leaderboard))      // ?metric=realized_pnl|volume|net_deposit
    .route("/fees/totals",          get(fee_totals))       // accrued + swept + current treasury
    .route("/sse/stream",           get(sse_stream))       // live push fed by PG NOTIFY -> broadcast
    .route("/graphql",              post(graphql_handler)) // optional async-graphql surface
    .layer(CorsLayer::permissive())
    .with_state(AppState{ db, bus });

async fn sse_stream(State(s):State<AppState>) -> Sse<impl Stream<Item=Result<Event,Infallible>>> {
    let rx = s.bus.subscribe(); // tokio::sync::broadcast fed by a PgListener task on 'flipvault_events'
    let stream = BroadcastStream::new(rx).filter_map(|m| async move {
        m.ok().map(|json| Ok(Event::default().event("flipvault").data(json))) });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
```

```
// backfill crawler: getSignaturesForAddress before/until pagination, same decoder as live tail
async fn backfill(rpc:&RpcClient, db:&PgPool, program:Pubkey) -> anyhow::Result<()> {
    let mut before = load_backfill_oldest(db).await?; // resume watermark
    loop {
        let cfg = GetConfirmedSignaturesForAddress2Config{ before, until:None, limit:Some(1000), commitment:Some(CommitmentConfig::finalized()) };
        let sigs = rpc.get_signatures_for_address_with_config(&program, cfg).await?;
        if sigs.is_empty() { break; }
        for s in &sigs {
            if s.err.is_some() { continue; } // skip failed txs
            let tx = rpc.get_transaction_with_config(&s.signature.parse()?,
                RpcTransactionConfig{ encoding:Some(Json), max_supported_transaction_version:Some(0),
                                      commitment:Some(CommitmentConfig::finalized()) }).await?;
            for ix in flipvault_ixs(&tx, &program) { handle_ix(&ix, &tx_meta(&tx), db).await?; }
        }
        before = Some(sigs.last().unwrap().signature.parse()?);
        save_backfill_oldest(db, before).await?;
    }
    Ok(())
}
```

```
-- user PnL query (no events => reconstructed from deposits/withdrawals + current snapshot value)
-- realized: net SOL out via withdrawals minus SOL in via deposits; unrealized: shares * current tranche value
WITH dep AS (SELECT owner, SUM(amount_lamports) sol_in FROM deposits WHERE owner=$1 AND status<>'orphaned' GROUP BY owner),
     wd  AS (SELECT owner, SUM(net_lamports) sol_out, SUM(fee_lamports) fees_paid FROM withdrawals WHERE owner=$1 AND status<>'orphaned' GROUP BY owner),
     cur AS (
       SELECT p.owner, SUM( (p.shares::numeric / NULLIF(vs.total_shares,0)) * vs.amount ) est_value
       FROM positions p
       JOIN LATERAL (SELECT amount,total_shares FROM vault_snapshots vs
                     WHERE vs.vault_id=p.vault_id AND vs.tranche_slot=p.tranche_slot AND vs.status<>'orphaned'
                     ORDER BY slot DESC LIMIT 1) vs ON true
       WHERE p.owner=$1 AND p.shares>0 AND p.status<>'orphaned' GROUP BY p.owner)
SELECT COALESCE(wd.sol_out,0) - COALESCE(dep.sol_in,0)                       AS realized_pnl_lamports,
       COALESCE(cur.est_value,0)                                            AS unrealized_value_lamports,
       COALESCE(cur.est_value,0) + COALESCE(wd.sol_out,0) - COALESCE(dep.sol_in,0) AS total_pnl_lamports,
       COALESCE(wd.fees_paid,0)                                            AS fees_paid_lamports
FROM dep FULL JOIN wd USING(owner) FULL JOIN cur USING(owner);
```

**Risks:**
- [critical] The program emits NO events (IDL has no `events` array). Flip SOL amount, minted/burned shares, withdrawal fee, and selected_vault exist ONLY in account state, so naive log-based indexing yields zero economics. → Indexer is built around account-state DELTAS, not logs: decode pre/post Vault/Config/Position (Geyser provides pre/post directly; plain RPC re-fetches the touched accounts at slot and slot-1 via getMultipleAccounts) and compute effects. Log subscription is used only as a cheap 'a flipvault tx happened' trigger. This is baked into handle_ix above.
- [high] Plain-RPC re-fetch of account state at slot-1 is racy/expensive and historical state at an old slot may be unavailable on a default devnet RPC (limited history). → Prefer a Geyser/Yellowstone source (Triton/Helius/QuickNode devnet) that delivers pre+post account data inside the tx update so no re-fetch is needed. For backfill where only finalized tx meta is available, derive shares/fees from meta.pre/postBalances (lamports are always in meta) and from arithmetic invariants (e.g. fee = gross*fee_bps/10000 using Config.fee_bps), reserving full account-diff for the live path.
- [medium] Devnet reorgs / dropped confirmed transactions corrupt derived aggregates (PnL, leaderboard, fee totals). → Two-phase status per row (confirmed->finalized) plus an 'orphaned' state; every API read filters status<>'orphaned'. A finality task re-checks confirmed sigs via getSignatureStatuses and flips/orphans by signature. Because all tables are keyed by (sig,instruction_index) and snapshots by (slot,write_version), reorg cleanup is a targeted flag, never a full rebuild. Treat finalized as source of truth for leaderboard.
- [medium] selected_vault==NO_VAULT (255) and recover_round (cancelled rounds) create rounds with no flip; counting them as flips skews stats. → rounds.outcome ∈ {pending,settled,recovered}; only write a flips row when post Config.selected_vault != NO_VAULT. recover_round (disc 87,173,..) sets outcome='recovered'. The IDL's NoPendingRound/RandomnessNotResolved errors mean failed settles must be skipped (signature.err is not null) so we never index a non-effecting tx.
- [medium] u128 fields (r_tok, k) and virtual token amounts overflow i64/BIGINT, and Position shares are non-transferable so PnL must be reconstructed not read. → Store curve/token quantities as NUMERIC(40,0) (tok_out, r_tok, k); keep lamports/shares (u64) as BIGINT (fits since < 2^63 for realistic SOL). PnL query reconstructs from deposits(sol_in) vs withdrawals(net) plus a current-snapshot valuation of remaining shares — see the PnL SQL sketch.
- [medium] Carbon (carbon-core 0.12) is young (V1 late 2025) and may not pin cleanly against agave solana-* 2.x, risking dependency-version churn. → Carbon is the accelerator, not a hard dependency: the decoder (discriminator match + borsh) and store layer are framework-agnostic and run identically on a hand-rolled logsSubscribe/getTransaction loop. If Carbon's versions conflict, fall back to solana-client 2.x + the backfill loop sketch with no schema or API change. Pin exact versions in Cargo.lock and CI.
- [low] Frontend 'ORE-instant' feel breaks if /state/current is served only from a lagging indexer right after a settle. → /state/current reads Postgres first (single indexed query on vault_snapshots latest + open round) and, when the latest snapshot slot is older than the chain head by > N slots, falls back to a live getMultipleAccounts(Config,vault0..3) decode in the handler so first paint is never stale. SSE/NOTIFY then keeps it live without polling.

**Open questions:**
- Will the deployment use a Geyser/Yellowstone-capable RPC (Triton/Helius/QuickNode) on devnet, or only public devnet RPC? This decides whether live flip economics come from in-tx pre/post account data (clean) or from getMultipleAccounts re-fetch + lamport-delta inference (the fallback).
- Confirm the exact NO_VAULT sentinel value for Config.selected_vault (assumed 255). The schema maps it to NULL selected_vault; needs the program constant to be certain.
- Does settle_round ever flip more than one tranche or skip a flip when min_reserve floor would break (the IDL mentions a flip can be 'skipped')? If a settle can legitimately produce no flip while selecting a vault, the flips-vs-rounds derivation needs a 'skipped' outcome too.
- REST-only, or also stand up the async-graphql surface now? GraphQL is low marginal cost on the same router and suits the frontend's composed leaderboard/chart queries, but adds a schema to maintain — confirm priority.
- Retention/granularity for vault_snapshots: at ~30s rounds plus every deposit/withdraw this grows steadily. Decide on a rollup (continuous aggregate / TimescaleDB hypertable, or periodic downsample into vault_series) for the chart endpoints before mainnet volume.
- Does the frontend want SSE (simplest, one-way, great for live vault/round updates) or full WebSocket/GraphQL-subscriptions? SSE is recommended; confirm it satisfies the Dioxus client.


### Deployment + Integration + Data Flow (monorepo layout, data-flow wiring, deploy topology, devnet→mainnet promotion) (deploy)

FlipVault becomes a two-cargo-workspace monorepo. Workspace A is the already-deployed Anchor program, left untouched (its SBF profile/lockfile must not be polluted by the app's wasm32 stack). Workspace B is the "app" workspace with four members: flipvault-sdk (a no-entrypoint, dual-target crate that is the SINGLE source of truth for PDA seeds, account borsh structs, and instruction builders, compiling to both wasm32-unknown-unknown and native), the Dioxus 0.7 frontend (CSR WASM), the keeper bin, and the indexer+api bin. Three data planes feed the frontend: (1) live config/vault state pulled straight from an RPC provider over HTTPS + websocket account subscriptions (sub-second, ORE-like), (2) history/leaderboard/positions from the indexer's Axum REST API backed by Postgres, (3) user tx signing through the regolith-labs dioxus-wallet-adapter (the same adapter ORE ships) which builds the unsigned tx with flipvault-sdk and hands it to the browser wallet. The keeper is a native tokio service that ports round.ts: commit_round -> poll ORAO -> settle_round each round, recover_round on stuck rounds. The indexer uses Carbon (Anchor-IDL-generated decoders) over Yellowstone gRPC on mainnet, with an RPC-polling fallback datasource for devnet, writing typed rows to Postgres which the same binary serves via Axum. Deploy topology: frontend as static WASM on Cloudflare Pages (SPA fallback), keeper + indexer/api as two Docker services on Fly.io (or a VPS), managed Postgres (Neon/Supabase), and a Helius RPC endpoint (free tier devnet, Developer tier mainnet for enhanced websockets + LaserStream). Secrets via env/Fly secrets/SOPS, CI via GitHub Actions with a wasm32 + native matrix, devnet->mainnet flips by config (cluster URL + program-id env), never code. Grounded facts: Dioxus stable is 0.7.9 (0.8.0-alpha exists, stay on 0.7.x); ORE itself is built on Dioxus + regolith-labs/dioxus-wallet-adapter; wasm_client_solana is the browser RPC client because solana-client does not compile to wasm32; Carbon generates Anchor-IDL decoders and supports both Yellowstone gRPC and RPC-poll datasources; Helius free tier covers devnet, Developer ($49/mo) adds enhanced websockets and mainnet.

**Recommended stack:**
- Cargo workspaces (two: program + app) resolver=2, Rust 1.92 host, ~1.86 platform-tools for SBF — Monorepo structure. Keep the existing Anchor workspace (flipvault/) standalone; add a sibling app/ workspace so the wasm32 dependency graph and lockfile never touch the SBF program build.: The deployed program is done and built with Solana platform-tools + a fat-LTO release profile (Cargo.toml pins lto=fat, codegen-units=1, overflow-checks). The Dioxus/wasm stack needs different profiles and pulls getrandom/js + web-sys that must NEVER enter the on-chain crate. Two workspaces = two Cargo.lock files = zero cross-contamination, still one git repo.
- flipvault-sdk (shared crate) edition 2021, anchor-lang 0.31.1 (no-entrypoint), borsh 1.5 — Single source of truth for the integration surface: PDA derivation (config/reserve/treasury/vault/position + ORAO network_state/randomness), account structs (Config/Vault/Tranche/Position with 8-byte discriminator), and instruction builders for all 7 ixs. Compiles to both wasm32-unknown-unknown (frontend) and native (keeper/indexer).: Eliminates three-way drift between TS scripts, frontend, keeper and indexer. anchor-lang with default-features off + no-entrypoint gives Discriminator/AccountDeserialize derives and Pubkey/borsh without the SBF entrypoint. getrandom pinned with the js feature behind cfg(wasm32) so the frontend links.
- Dioxus 0.7.9 (stable; 0.8.0-alpha exists but stay on 0.7.x) — Frontend framework, web/CSR target compiled to WASM via dx bundle --web --release.: Exactly what ORE is built on (regolith-labs/ore-app). 0.7 brought sub-second hot reload + Axum-based fullstack story, but we use pure CSR (static files) for the ORE-like instant feel and trivial CDN hosting. WASM streams-compiles so first paint is fast; wasm-opt trims MBs to ~200-300kb.
- dioxus-wallet-adapter (regolith-labs) git pin to a known-good commit (track ore-app's pin) — Wallet connect + sign/sendTransaction in the browser via the Wallet Standard (Phantom/Solflare/Backpack), surfaced as Dioxus hooks/signals.: It is the adapter ORE itself ships, battle-tested against the exact UX target. Implements wallet-standard browser custom-event discovery. Alternatives: JamiiDao wallet-adapter (1.0.x-beta on crates.io) for a versioned crate; wasi-sol (less maintained).
- wasm_client_solana latest (Nov 2025 release line) — Browser-side async Solana RPC client: getAccountInfo for config/vaults/positions, accountSubscribe websockets for live updates, simulate/serialize for tx preview.: solana-client's native RpcClient does not compile to wasm32 (tokio/socket deps). wasm_client_solana / solana-client-wasm are built on web-sys + reqwest-wasm and mirror the nonblocking RpcClient API, so the frontend reads chain state directly with no backend hop — critical for the instant feel.
- anchor-client (nonblocking) + tokio anchor-client 0.31.1, tokio 1.x — Keeper transaction engine. Builds/sends commit_round, settle_round, recover_round against the RPC provider.: anchor-client's nonblocking RpcClient is the native counterpart to the frontend's wasm client and shares flipvault-sdk's instruction builders, making the keeper a faithful Rust port of round.ts. Pairs with backoff/tokio-retry for send-and-confirm with priority fees.
- orao-solana-vrf (Rust SDK) match the on-chain 0.6.1 line (client SDK) — Keeper-side ORAO randomness: derive randomness/network_state PDAs, read NetworkState.config.treasury, poll getFulfilledRandomness between commit and settle.: round.ts uses the JS SDK (@orao-network/solana-vrf 0.8.0). The Rust keeper needs the equivalent derivations; the orao Rust crate provides RandomnessAccountData parsing so the keeper detects fulfillment without reimplementing borsh layouts.
- Carbon (sevenlabs-hq) latest carbon-core + carbon-yellowstone-grpc-datasource + carbon-rpc-program-subscribe-datasource — Indexer ingest framework. carbon CLI generates a typed decoder from flipvault.json (the existing IDL); processors write rounds/flips/deposits/withdraws/positions/fees to Postgres.: Carbon generates an Anchor-IDL decoder for our exact program, has a Yellowstone gRPC datasource for mainnet (sub-100ms) AND an RPC program-subscribe/poll datasource for devnet (where free Yellowstone is rarely available). One framework spans both clusters; Vixen is the alternative but Carbon's IDL-decoder generation maps more directly onto our Anchor shapes.
- Axum + sqlx + Postgres axum 0.7/0.8, sqlx 0.8 (compile-checked queries), Postgres 16 — Indexer's API half: REST (optionally GraphQL via async-graphql) serving history, leaderboard, per-user positions, fee totals, round timeline to the frontend.: Rust-native, async, de-facto stack. sqlx compile-time query checking catches schema drift in CI. Indexer and API live in ONE binary (shared DB pool + decoded types) to cut ops surface; split later if write/read load diverges.
- Helius RPC Free tier (devnet now), Developer $49/mo (mainnet later) — The RPC + websocket provider for frontend live reads, keeper sends, and indexer ingest fallback.: Solana-native, 99.99% uptime, enhanced websockets on paid tiers, devnet on Developer; LaserStream gRPC (for the indexer) on Business+. Public devnet (api.devnet.solana.com) works to start but rate-limits/drops websockets — fine for the keeper, weak for live frontend. Triton is the lower-latency alternative for dedicated infra.
- Cloudflare Pages n/a (static hosting + SPA fallback) — Hosts the Dioxus CSR WASM bundle (HTML/JS/wasm/assets) on a global CDN.: Free, global edge, native SPA fallback (no top-level 404.html -> all routes serve index.html for client-side routing), trivial dx bundle -> upload. Vercel/Netlify/Nginx are drop-in equivalents; Pages has the best free WASM-on-edge story.
- Fly.io (Docker) + Neon Postgres n/a — Runs the keeper and indexer/api as two long-running Docker services close to the DB; Neon as managed serverless Postgres.: Both backend services are stateful long-runners (keeper holds a hot keypair and ticks every ~30s; indexer holds a gRPC/ws stream). Fly gives cheap always-on machines + private networking to Neon. Railway or a $5 VPS are equivalent; the design is host-agnostic via Docker.

**Architecture:** MONOREPO (one git repo, TWO cargo workspaces):

flipsol/                              # repo root
  flipvault/                          # WORKSPACE A — EXISTING, DO NOT REDESIGN
    Cargo.toml                        #   [workspace] members=["programs/*"] (untouched)
    Cargo.lock                        #   SBF lockfile — isolated from the app
    Anchor.toml                       #   program id EkfN5...rV4H on localnet+devnet
    programs/flipvault/               #   the deployed on-chain program
    target/idl/flipvault.json         #   * canonical IDL — consumed by app crates
    target/types/flipvault.ts         #   TS types (legacy scripts)
    scripts/*.ts                      #   existing TS client (kept as oracle/reference)

  app/                                # WORKSPACE B — NEW (the Rust app)
    Cargo.toml                        #   [workspace] members=[sdk,frontend,keeper,indexer]
    Cargo.lock                        #   app lockfile (wasm+native), separate from A
    rust-toolchain.toml               #   host toolchain + wasm32-unknown-unknown target
    crates/flipvault-sdk/             #   * DUAL-TARGET shared crate (no on-chain entry)
      Cargo.toml                      #     anchor-lang 0.31.1 default-features=false
      src/{pdas,accounts,ix,consts}.rs
    frontend/                         #   Dioxus 0.7 CSR (compiles to wasm32)
      Cargo.toml                      #     dioxus, dioxus-wallet-adapter, wasm_client_solana
      Dioxus.toml                     #     web/CSR config, asset dir, base_path
      src/{app,components,rpc,api,wallet}.rs
    keeper/                           #   native bin (tokio + anchor-client + orao)
      Cargo.toml ; Dockerfile ; src/{main,round,recover,config}.rs
    indexer/                          #   native bin (Carbon ingest + Axum API + sqlx)
      Cargo.toml ; Dockerfile ; migrations/ ; src/{main,ingest,decoders,db,api}.rs
  docker-compose.yml                  #   local: postgres + keeper + indexer (dev)
  .github/workflows/ci.yml            #   matrix: wasm32 build + native build/test + sqlx check
  docs/                               #   existing analysis + runbook

WHY TWO WORKSPACES: the on-chain crate builds with Solana platform-tools and a fat-LTO SBF profile; the app builds for wasm32-unknown-unknown + native with web-sys/getrandom-js. Sharing one workspace forces one Cargo.lock and one profile set across incompatible targets and risks pulling browser/random deps into the audited on-chain crate. Sibling workspaces in ONE repo preserve atomic commits, shared CI, and a single IDL source while fully isolating dependency graphs. The app crates consume the program ONLY through (a) the static IDL JSON and (b) hand-written types in flipvault-sdk that mirror the program's account layout — they never compile the program crate.

flipvault-sdk (the linchpin): exposes program_id(), all PDA finders, the borsh account structs with their 8-byte discriminators, enum Phase, Tranche{asset,amount,total_shares}, Position, and instruction builders that return an Instruction (data = discriminator ++ borsh(args), accounts in program order). default-features off on anchor-lang; getrandom pinned with feature=js under cfg(target_arch=wasm32). Both the frontend (wasm) and keeper/indexer (native) build the SAME instruction bytes from this crate, so there is exactly one definition of what each tx looks like.

DATA FLOW (three planes into the frontend):

  Solana devnet/mainnet (program EkfN5...rV4H + ORAO VRF)
    holds Config/Reserve/Treasury/Vault0-3/Position PDAs + round events (commit/settle/flip)
       ^ (3) sends tx      ^ (1a) RPC reads + ws subs     ^ ingest        ^ commit/settle/recover
       |                   |                               |               |
   FRONTEND (WASM, CDN)                                  INDEXER          KEEPER
     - wallet-adapter -> user signs tx (built from         Carbon over      tokio loop /~30s
       flipvault-sdk)                                      Yellowstone gRPC  commit_round ->
     - wasm_client_solana -> live vault & config           (mainnet) /      poll ORAO VRF ->
       state, round countdown  <----(1a)----              RPC poll (devnet)  settle_round;
     - REST/GraphQL fetch -> history, leaderboard,         IDL-gen decoders  recover_round on
       positions, fees  <----(2)----                       -> Postgres(Neon) stuck VRF
                                                            -> Axum API ->(2)

  (1a) LIVE: frontend reads config (phase, round_secs, last_settled_ts, selected_vault, k, r_tok)
       and all 4 vaults + the user's positions directly via getAccountInfo, and subscribes via
       accountSubscribe so a flip/settle repaints instantly. flipvault-sdk parses the borsh.
       Round countdown is pure client math from last_settled_ts + round_secs (no backend hop).
  (2)  HISTORY: leaderboard, per-round flip log, deposit/withdraw history, cumulative fees, and a
       user's realized P&L come from the indexer's Axum API (Postgres). Frontend never scans chain
       history itself (getProgramAccounts/tx history is too slow for the ORE feel).
  (3)  WRITE: deposit/withdraw — frontend builds the ix with flipvault-sdk, wallet-adapter signs &
       sends, then optimistically updates from the simulated result and reconciles on the ws push.
  KEEPER and INDEXER both talk only to the RPC provider; they share flipvault-sdk types but run
  independently (keeper can run with the indexer down, and vice-versa).

DEPLOY TOPOLOGY:
  - Frontend : dx bundle --web --release -> static dir -> Cloudflare Pages (SPA fallback, CDN).
               Build-time env bakes RPC_URL, PROGRAM_ID, INDEXER_API_URL, CLUSTER into the WASM.
  - Keeper   : Docker (rust -> distroless), one Fly.io machine, always-on, holds keeper keypair
               (Fly secret), env: RPC_URL, KEEPER_KEYPAIR, PROGRAM_ID, ROUND_SECS, RECOVER_AFTER_SECS.
  - Indexer  : Docker, one Fly.io machine, holds DATABASE_URL + RPC_URL/GRPC_URL; exposes :8080 API.
  - Postgres : Neon/Supabase managed; DATABASE_URL injected as a secret.
  - RPC      : Helius (devnet free -> mainnet Developer). Public devnet only as a fallback.

CONFIG/SECRETS: one typed Config per binary (figment/envy) read from env. Non-secrets
(PROGRAM_ID, CLUSTER, ROUND_SECS, INDEXER_API_URL) in per-env .env files; secrets
(KEEPER_KEYPAIR, DATABASE_URL, HELIUS_API_KEY) via Fly secrets / SOPS / CF Pages env. Program ID is
the same string everywhere (EkfN5...rV4H) and is a constant in flipvault-sdk; cluster is the ONLY
thing that changes devnet->mainnet.

CI/CD (GitHub Actions): job-1 wasm — rustup target add wasm32-unknown-unknown, dx build --web,
wasm-opt size gate; job-2 native — cargo build -p keeper -p indexer, cargo test,
cargo sqlx prepare --check against an ephemeral Postgres service; job-3 program (optional, rare) —
anchor build in flipvault/ to keep the IDL fresh. On main: build+push keeper/indexer Docker images,
flyctl deploy each; CF Pages auto-builds the frontend from the repo.

DEVNET->MAINNET PROMOTION: code is cluster-agnostic. Promotion = (1) point RPC_URL/GRPC_URL at a
mainnet Helius endpoint, (2) fund + set the mainnet keeper keypair, (3) run a fresh Postgres
(mainnet data separate), (4) re-run initialize against mainnet, (5) flip the program upgrade
authority to --final once satisfied (already in the devnet runbook). The program binary is identical;
only env and the one-time on-chain initialize differ.

**Code sketches:**
```
// app/Cargo.toml — the NEW app workspace (sibling of flipvault/, separate Cargo.lock)
[workspace]
resolver = "2"
members = ["crates/flipvault-sdk", "frontend", "keeper", "indexer"]

[workspace.dependencies]
flipvault-sdk = { path = "crates/flipvault-sdk" }
anchor-lang   = { version = "0.31.1", default-features = false }
borsh         = "1.5"
tokio         = { version = "1", features = ["full"] }
getrandom     = { version = "0.2" }  # wasm build adds feature="js"; native must NOT get it

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
```

```
// app/crates/flipvault-sdk/Cargo.toml — dual-target, no on-chain entrypoint
[package]
name = "flipvault-sdk"
edition = "2021"

[dependencies]
anchor-lang = { workspace = true }   # Pubkey, Discriminator, borsh derives, no entrypoint
borsh = { workspace = true }

# Only the browser build pulls the js RNG backend; native is unaffected.
[target.'cfg(target_arch = "wasm32")'.dependencies]
getrandom = { workspace = true, features = ["js"] }
```

```
// app/crates/flipvault-sdk/src/pdas.rs — ONE definition of every seed, shared by all 3 apps.
use anchor_lang::prelude::Pubkey;

// Same string on devnet and mainnet — only the cluster RPC changes.
pub const PROGRAM_ID: Pubkey =
    anchor_lang::solana_program::pubkey!("EkfN5vSrnCNt5Y9xVD5RibU23kkquWXk4EFFhwSNrV4H");
pub const ORAO_VRF_ID: Pubkey =
    anchor_lang::solana_program::pubkey!("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");

pub fn config()   -> Pubkey { Pubkey::find_program_address(&[b"config"],   &PROGRAM_ID).0 }
pub fn reserve()  -> Pubkey { Pubkey::find_program_address(&[b"reserve"],  &PROGRAM_ID).0 }
pub fn treasury() -> Pubkey { Pubkey::find_program_address(&[b"treasury"], &PROGRAM_ID).0 }
pub fn vault(id: u8) -> Pubkey { Pubkey::find_program_address(&[b"vault", &[id]], &PROGRAM_ID).0 }
pub fn position(owner: &Pubkey, vault_id: u8, slot: u8) -> Pubkey {
    Pubkey::find_program_address(&[b"position", owner.as_ref(), &[vault_id], &[slot]], &PROGRAM_ID).0
}
pub fn orao_network_state() -> Pubkey {
    Pubkey::find_program_address(&[b"orao-vrf-network-configuration"], &ORAO_VRF_ID).0
}
pub fn orao_randomness(seed: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(&[b"orao-vrf-randomness-request", seed], &ORAO_VRF_ID).0
}
```

```
// app/crates/flipvault-sdk/src/accounts.rs — borsh structs mirroring on-chain state.
// Used by the frontend (parse getAccountInfo) AND the indexer (decode account writes).
use anchor_lang::prelude::*;

#[derive(Clone, Copy, AnchorDeserialize, AnchorSerialize)]
pub enum Phase { Idle, Pending }
#[derive(Clone, Copy, AnchorDeserialize, AnchorSerialize)]
pub enum Asset { Sol, Token }

#[account]  // gives the 8-byte discriminator + AccountDeserialize
pub struct Config {
    pub treasury_authority: Pubkey, pub r_tok: u128, pub k: u128,
    pub round_secs: i64, pub last_settled_ts: i64, pub fee_bps: u16,
    pub min_reserve: u64, pub phase: Phase, pub round_seed: [u8; 32],
    pub commit_slot: u64, pub commit_ts: i64, pub selected_vault: u8,
    pub bump: u8, pub reserve_bump: u8, pub treasury_bump: u8,
}
#[derive(Clone, Copy, AnchorDeserialize, AnchorSerialize)]
pub struct Tranche { pub asset: Asset, pub amount: u64, pub total_shares: u64 }
#[account] pub struct Vault { pub vault_id: u8, pub tranches: [Tranche; 2], pub bump: u8 }
#[account] pub struct Position {
    pub owner: Pubkey, pub vault_id: u8, pub slot: u8, pub shares: u64, pub bump: u8,
}
```

```
// app/frontend/src/rpc.rs — live state in the browser via wasm_client_solana (plane 1a).
use flipvault_sdk::{pdas, accounts::{Config, Vault}};
use wasm_client_solana::SolanaRpcClient;

pub async fn load_live(rpc: &SolanaRpcClient) -> anyhow::Result<(Config, [Vault; 4])> {
    let cfg_raw = rpc.get_account_data(&pdas::config()).await?;
    let config: Config = Config::try_deserialize(&mut cfg_raw.as_slice())?;
    let mut vaults = Vec::with_capacity(4);
    for id in 0u8..4 {
        let raw = rpc.get_account_data(&pdas::vault(id)).await?;
        vaults.push(Vault::try_deserialize(&mut raw.as_slice())?);
    }
    Ok((config, vaults.try_into().unwrap()))
}
// Round countdown is pure client math — no backend round-trip, instant repaint:
//   remaining = (config.last_settled_ts + config.round_secs) - now_unix();
// accountSubscribe(config) pushes settle/flip -> re-render the moment chain confirms.
```

```
// app/keeper/src/round.rs — Rust port of scripts/round.ts (keeper -> RPC).
pub async fn run_round(ctx: &Keeper) -> anyhow::Result<()> {
    let force: [u8; 32] = rand::random();                  // 32-byte VRF seed
    let randomness    = flipvault_sdk::pdas::orao_randomness(&force);
    let orao_treasury = ctx.read_orao_treasury().await?;    // NetworkState.config.treasury

    // 1) commit_round — requests ORAO VRF
    ctx.send(flipvault_sdk::ix::commit_round(
        force, ctx.keeper.pubkey(), flipvault_sdk::pdas::config(),
        randomness, orao_treasury, flipvault_sdk::pdas::orao_network_state())).await?;

    // 2) poll ORAO until fulfilled (sub-second on devnet; bail to recover after timeout)
    ctx.await_vrf(&randomness, RECOVER_AFTER_SECS).await?;

    // 3) settle_round — flips vault = rand % 4
    ctx.send(flipvault_sdk::ix::settle_round(
        flipvault_sdk::pdas::config(), flipvault_sdk::pdas::reserve(),
        [0,1,2,3].map(flipvault_sdk::pdas::vault), randomness)).await?;
    Ok(())
}
// main loop: tick at (last_settled_ts + round_secs) -> run_round();
// on Err past RECOVER_AFTER_SECS -> recover_round() to unstick the phase.
```

```
// app/indexer/src/decoders.rs + db.rs — Carbon decoder (IDL-generated) -> Postgres.
// `carbon-cli parse --idl ../../flipvault/target/idl/flipvault.json` generates the decoder.
async fn handle_settle(ev: SettleRound, slot: u64, sig: &str, db: &PgPool) -> Result<()> {
    sqlx::query!(
        "INSERT INTO rounds (slot, signature, selected_vault, settled_ts)
         VALUES ($1,$2,$3,$4) ON CONFLICT (signature) DO NOTHING",
        slot as i64, sig, ev.selected_vault as i16, ev.settled_ts
    ).execute(db).await?;
    Ok(())
}
// Datasource is cluster-switched: Yellowstone gRPC on mainnet, RPC program-subscribe on devnet.
// Same Carbon pipeline + same flipvault-sdk account types feed both.

// app/indexer/src/api.rs — Axum read side the frontend calls (plane 2).
let app = Router::new()
    .route("/rounds",        get(rounds))        // round/flip timeline
    .route("/leaderboard",   get(leaderboard))   // realized P&L by owner
    .route("/positions/:pk", get(positions_for)) // a user's deposit/withdraw history
    .route("/fees",          get(fee_totals))
    .with_state(pool);
```

```
# Fly.io deploy (keeper + indexer); frontend goes to Cloudflare Pages separately.
# --- keeper ---
flyctl secrets set RPC_URL="https://devnet.helius-rpc.com/?api-key=..." \
  KEEPER_KEYPAIR="$(cat keeper.json)" -a flipvault-keeper
flyctl deploy app/keeper -a flipvault-keeper

# --- indexer + api ---
flyctl secrets set DATABASE_URL="postgres://...neon..." \
  RPC_URL="https://devnet.helius-rpc.com/?api-key=..." -a flipvault-indexer
flyctl deploy app/indexer -a flipvault-indexer

# --- frontend (static WASM) ---
cd app/frontend && dx bundle --web --release
# bake build-time env: PROGRAM_ID, CLUSTER=devnet, RPC_URL, INDEXER_API_URL
npx wrangler pages deploy ./dist --project-name flipvault   # SPA fallback (no 404.html)
```

**Risks:**
- [high] solana/anchor crates frequently fail to compile to wasm32-unknown-unknown (getrandom backend, tokio sockets, zstd/proc-macro). The frontend + flipvault-sdk wasm build can break on a transitive dep at any version bump. → Use wasm_client_solana / solana-client-wasm (purpose-built for the browser) instead of solana-client; keep flipvault-sdk minimal (anchor-lang default-features=false, no full solana-sdk); pin getrandom feature=js only under cfg(wasm32); add a CI job that runs dx build --web on every PR so a breaking transitive dep is caught immediately; pin the dioxus-wallet-adapter git commit to the exact one ore-app uses.
- [medium] Two cargo workspaces in one repo can confuse tooling (rust-analyzer, cargo at repo root) and let flipvault-sdk's hand-written account structs silently drift from the on-chain layout. → Document the two-root layout in CLAUDE.md/README and set rust-analyzer linkedProjects to both Cargo.tomls; add a CI assertion test that deserializes a fixture account fetched from devnet (the real Config/Vault PDAs) with flipvault-sdk and compares against the IDL — fails the build on layout drift. The program is done/immutable-bound so drift risk is low, but the test makes it zero.
- [high] Keeper is a single point of failure and holds a hot keypair: if it dies, rounds stall (phase stuck Pending); if the key leaks, an attacker can spam commit/settle/recover. → Keeper key can only run rounds — it cannot touch reserve/curve/vault funds (only treasury_authority can sweep). Run with restart-on-crash (Fly auto-restart), a heartbeat metric, and recover_round logic that auto-unsticks after RECOVER_AFTER_SECS. Keypair in Fly secrets, never in the image or git. Optionally a warm standby keeper that acts only if the primary's heartbeat is stale.
- [medium] Free/public devnet RPC (api.devnet.solana.com) rate-limits and drops websockets, breaking the instant frontend feel and indexer ingest; Yellowstone gRPC is generally not free on devnet. → Use Helius (free tier covers devnet reads + enhanced websockets on Developer) for frontend and keeper; for the indexer on devnet use Carbon's RPC program-subscribe/poll datasource instead of gRPC, switching to Yellowstone gRPC only on mainnet (Helius LaserStream / Triton). Treat the public endpoint strictly as fallback; cache live reads briefly and debounce accountSubscribe repaints.
- [medium] ORAO VRF fulfillment can stall (devnet flakiness), leaving a round Pending and the UI countdown frozen, which looks broken to users. → Keeper polls with a timeout and calls recover_round past RECOVER_AFTER_SECS (already in the program + scripts/recover.ts). Frontend reads config.phase and, when Pending beyond the expected window, shows a 'settling / awaiting randomness' state instead of a stuck timer. Indexer records commit->settle latency so stalls are observable in monitoring.
- [low] Dioxus 0.7 CSR + static hosting hydration mismatch: a plain static index.html can look for window.initial_dioxus_hydration_data (SSR artifact) and error, and deep links 404 without SPA fallback. → Build pure CSR (web platform, no fullstack/SSR) with dx bundle --web --release; rely on Cloudflare Pages SPA fallback (omit a top-level 404.html so all routes serve index.html); verify the bundle in CI with a headless smoke test; set Dioxus.toml base_path correctly for the deploy domain.
- [medium] Indexer reorg / duplicate-event handling: replays or forks can double-insert rounds/deposits and corrupt leaderboard P&L. → Make all writes idempotent keyed on (signature) or (slot, signature, ix-index) with ON CONFLICT DO NOTHING; track a processed-slot watermark and finalize aggregates only at confirmed/finalized commitment; expose a re-sync endpoint that re-derives aggregates from the raw event tables.

**Open questions:**
- Does the on-chain program emit Anchor #[event]s for deposit/withdraw/commit/settle/flip, or must the indexer reconstruct history purely from account-state diffs + instruction decoding? This determines whether Carbon uses event decoders or account+instruction decoders and how cleanly per-user P&L is attributed. The IDL at flipvault/target/idl/flipvault.json answers this (read its events section).
- Confirm flipvault-sdk can build the exact instruction discriminators without compiling the program crate — hardcode the 8-byte sighash constants (sha256('global:deposit')[..8]) in the sdk, or generate them from the IDL at build time via a build.rs (most drift-proof)?
- Is anchor-lang 0.31.1 itself wasm32-clean with default-features off, or does it transitively pull solana-program pieces that break the browser build? May need to drop anchor-lang in flipvault-sdk in favor of solana-program/borsh directly + manual discriminators if the wasm build fights it.
- Leaderboard semantics: is P&L realized (only on withdraw, net of the 10% fee) or mark-to-market against the live curve (k, r_tok) for open positions? Mark-to-market prices shares via the constant-product curve each round — more compute but more ORE-like.
- Single combined indexer+api binary vs split: at what ingest/query load do we separate them? Start combined; revisit if the Axum read path contends with the Carbon write path on the shared Postgres pool.
- dioxus-wallet-adapter mobile / in-wallet-browser support (Phantom mobile, Solana Mobile) — is Wallet Standard browser-event discovery enough, or do we need a deep-link / MWA path for the ORE-like mobile experience?


### ORE.supply Rust/WASM Solana dApp reference patterns (Dioxus frontend layer) (ore-ref)

ORE.supply is the canonical real example of what FlipVault wants: a Dioxus Rust-to-WASM Solana dApp that feels instant. Traced from public repos regolith-labs/ore-app, regolith-labs/dioxus-wallet-adapter, regolith-labs/solana-playground. Key reality: ORE does NOT sign in pure Rust. The Dioxus WASM app delegates wallet connect + signing to a tiny webpack-bundled JS shim wrapping the standard @solana/wallet-adapter ecosystem; Rust talks to it over Dioxus document::eval. Rust serializes the tx with bincode then base64, calls window.DwaTxSigner({b64}), JS signs via the wallet signTransaction, returns base64, Rust deserializes and submits. The connected pubkey flows back to Rust via a dwa-pubkey CustomEvent. For RPC, ORE avoids stock solana-client (does not compile to wasm32-unknown-unknown) and uses its own forks solana-client-wasm + solana-extra-wasm pinned via [patch], now on solana-sdk 2.1. The fast feel comes from client-side tx building (no server round-trip before signing), optimistic UI + fine-grained signal re-render, polling/subscribing to a small fixed set of PDAs, and pushing all history to the indexer so the frontend only reads cheap current account state. For a 2026 greenfield build there is a real choice between ORE's battle-tested JS bridge and the now-mature pure-Rust path (wallet-adapter crate + wasm_client_solana). Biggest hazards are WASM toolchain gotchas: getrandom wasm_js backend, solana-crate/wasm incompatibility, blockhash expiry, 2-4MB+ WASM bundles. FlipVault maps cleanly: 4 vault PDAs + config + user positions is a tiny fixed account set ideal for the polling/subscribe pattern, and deposit/withdraw are single-instruction txs ideal for the eval-sign-submit loop.

**Recommended stack:**
- dioxus (web feature) 0.7.9 (latest stable 2026-05-08; 0.8.0-alpha exists) — Rust->WASM UI framework: rsx! components, signals, router, use_resource/use_coroutine async, document::eval JS bridge, asset! pipeline, dx serve/bundle CLI.: Literally the framework ORE is built on. ORE shipped 0.6.1; a new build should start on 0.7.x for sub-second hot reload, unified asset! API, Axum-0.8 server fns, better wasm-opt integration. Exact ORE-like stack the user is modeling.
- wasm_client_solana 0.10.0 (solana-sdk v3 line; features js/ssr/zstd) — WASM-native Solana RPC + WebSocket PubSub client: get_account, get_multiple_accounts, get_program_accounts, send_transaction, get_latest_blockhash, get_signature_statuses, accountSubscribe in-browser.: Maintained crates.io alternative to ORE's private solana-playground forks. ORE vendored solana-client-wasm/solana-extra-wasm because stock solana-client never compiled to wasm32-unknown-unknown. wasm_client_solana solves the same problem without forking AND adds WebSocket account subscriptions, exactly what FlipVault needs for live vault state. To mirror ORE 1:1 instead, [patch] in regolith solana-playground solana-client-wasm 2.1.
- JS wallet shim (webpack) + @solana/wallet-adapter-react/-react-ui/-wallets/-base + @solana/web3.js wallet-adapter-react ^0.15, -react-ui ^0.9, -wallets ^0.19, web3.js ^1.95 (or @solana/kit 2.x) — Browser wallet detection + connect modal (Phantom, Solflare, Backpack) + signTransaction. Compiled once to static assets/main.js, called from Rust via window.DwaTxSigner and a dwa-pubkey CustomEvent.: ORE's actual production approach (regolith dioxus-wallet-adapter). Gives the polished, universally-compatible wallet modal for free and dodges every Rust wallet-standard edge case. Cost: a tiny JS build step and crypto/stream browserify polyfills in webpack.
- wallet-adapter crate (jamiidao/SolanaWalletAdapter) 1.0.x beta (crates.io) — Pure-Rust wallet-standard impl: listens for wallet-standard:register-wallet and app-ready events; supports connect/disconnect/events, solana:signIn (SIWS), signMessage, signTransaction, signAndSendTransaction. No JS bundle.: The Rust-everywhere alternative to the JS shim. Mature enough in 2026 to skip webpack entirely. Trade-off: you reimplement the connect-modal UX in Dioxus and own wallet-standard quirks. Recommend prototype with the JS shim, keep this as the migration target once UX is stable.
- getrandom 0.2/0.3 with the wasm_js backend cfg — RNG used transitively by solana-sdk/curve25519 signing/keypair code in the browser.: Non-optional gotcha: without RUSTFLAGS --cfg getrandom_backend="wasm_js" (or the equivalent in .cargo/config.toml) the wasm build fails to link. Every Solana-on-WASM stack including ORE hits this.
- dioxus-sdk (timing) + gloo (storage/net/utils) + web-time dioxus-sdk 0.6+, gloo 0.11, web-time 1.x — use_interval for round countdown + poll loop; gloo-storage for caching last-known vault state and wallet auto-reconnect; gloo-net for REST calls to the indexer API; web-time as the wasm-safe Instant/SystemTime.: ORE depends on exactly these (gloo 0.11, web-time 1.0, dioxus-sdk timing). std::time::Instant panics on wasm32; web-time is the standard fix. dioxus-sdk use_interval is the idiomatic countdown/poll primitive.
- steel + Anchor/IDL borsh decode (borsh, serde-wasm-bindgen, sha2) steel 4.x, borsh 1.x, serde-wasm-bindgen 0.6, sha2 0.10 — Decode FlipVault Config/Vault/Position accounts (8-byte anchor discriminator + borsh) client-side; compute Anchor instruction discriminators; pass typed structs into rsx!.: FlipVault is Anchor not Steel but the decode pattern is identical: fetch account bytes via the wasm RPC client, strip the 8-byte discriminator, borsh-deserialize into a #[derive(BorshDeserialize)] mirror of the on-chain struct. ORE does this for every account it renders. sha2 computes the global:<ix> discriminator; serde-wasm-bindgen bridges anything crossing the JS boundary.

**Architecture:** HOW ORE IS ACTUALLY BUILT, and how to replicate it for FlipVault.

1) APP SHELL. A single Dioxus web SPA (dx serve dev / dx bundle --release prod). All UI is rsx! components driven by Signals. Global app state (connected wallet, selected RPC, cached vault/config/position accounts) lives in a use_context_provider(Signal<...>) at the root, the same pattern as ORE's WalletAdapter signal (Disconnected | Connected{pubkey}).

2) WALLET LAYER (ORE exact pattern, JS bridge). index.html mounts a hidden <div id="dioxus-wallet-adapter">; a webpack-bundled assets/main.js renders the React WalletProvider/WalletModalProvider/WalletMultiButton into it (the only React in the app). On connect a React Dispatcher watches useWallet().publicKey and dispatches window CustomEvent("dwa-pubkey", {detail:{pubkey: bytes-as-json}}). Rust does window.addEventListener("dwa-pubkey", ...) via eval and writes the pubkey into the WalletAdapter signal. main.js sets window.DwaTxSigner = async ({b64}) => { rebuild Transaction.from(Buffer.from(b64,"base64")); signed = await signTransaction(tx); return signed.serialize() as base64 }. Rust invoke_signature(): bincode::serialize(&tx) -> base64 -> document::eval to call window.DwaTxSigner; eval.send(b64); signed_b64 = eval.recv().await; bincode::deserialize -> submit via wasm RPC client -> confirm by polling get_signature_statuses ~20x at 500ms until Confirmed/Finalized. An InvokeSignatureStatus enum {Start, Waiting, Done(sig), DoneWithError, Timeout} drives button state.

3) RPC AND STATE LAYER. FlipVault renderable state is a tiny fixed set of PDAs: config(1), reserve(1), treasury(1), vault0..vault3(4), plus the connected user's positions (deterministically derived, vault_id 0-3 x slot). That is ONE get_multiple_accounts call for the whole board. Offer live updates two ways. (a) ORE-style poll: use_interval every 2-3s + on every confirmed tx, refetch get_multiple_accounts and write decoded structs into signals. (b) faster: wasm_client_solana WebSocket accountSubscribe on config + the 4 vault PDAs so flips render the instant settle_round lands. Round countdown is computed client-side from Config.last_settled_ts + round_secs and ticked by use_interval(1s) with no RPC. History/leaderboard/positions-over-time come from the Rust indexer REST/GraphQL via gloo-net, NOT from RPC (RPC only serves current account state). This split is what keeps it fast.

4) INSTANT-FEEL TECHNIQUES (the ORE secret sauce). Build+sign client-side so click-to-wallet-popup is immediate. Optimistic UI: on deposit/withdraw submit, immediately mutate the local Position/Vault signal to expected post-state and show a pending badge; reconcile on confirmation; rollback on DoneWithError/Timeout. Cache-first paint: hydrate vault/config from gloo-storage on load so the board renders before the first RPC returns. Fine-grained signals: only the changed vault card/countdown re-renders, not the tree. Pre-fetch a recent blockhash and refresh on the same interval as state so a tx is ready to send the moment the user clicks.

5) BUILD AND SHIP. Cargo release profile tuned for size (opt-level=z, lto=true, codegen-units=1, panic=abort, strip=true), then wasm-opt -Oz, served as static files (works on any CDN). The JS shim is built once with npm run build into /assets. .cargo/config.toml carries the getrandom_backend cfg + rustflags.

KEY DIVERGENCE FROM ORE FOR FLIPVAULT. ORE talks to a Steel program with crates.io ore-api types; FlipVault is Anchor with an IDL. You will NOT get a Rust client crate for free, so hand-write thin #[derive(Borsh)] mirrors of Config/Vault/Tranche/Position and instruction-arg structs, and build instructions manually (program id, derived PDAs, anchor 8-byte instruction discriminator = first 8 bytes of sha256(\"global:deposit\") etc.). Straightforward, and the one piece ORE's repo will not hand you.

**Code sketches:**
```
// .cargo/config.toml -- non-negotiable WASM gotcha fixes (every Solana-on-WASM app needs this)
[build]
target = "wasm32-unknown-unknown"
rustflags = ['--cfg', 'getrandom_backend="wasm_js"']

// Cargo.toml size profile mirroring ORE / Dioxus optimizing guide
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
incremental = false
// then: dx bundle --release ; wasm-opt target/.../app_bg.wasm -o app_bg.wasm -Oz
```

```
// ORE wallet-sign loop in Rust (Dioxus 0.7 document::eval), reusable for FlipVault deposit/withdraw
async fn invoke_signature(tx: Transaction) -> Result<Signature, SigErr> {
    let b64 = STANDARD.encode(bincode::serialize(&tx)?);
    let mut eval = document::eval(r#"
        const b64 = await dioxus.recv();
        const signed = await window.DwaTxSigner({ b64 });
        dioxus.send(signed);
    "#);
    eval.send(b64)?;
    let signed_b64: String = serde_json::from_value(eval.recv().await?)?;
    let signed: Transaction = bincode::deserialize(&STANDARD.decode(signed_b64)?)?;
    let sig = RPC.send_transaction(&signed).await?;   // wasm_client_solana
    confirm(&sig).await                               // poll get_signature_statuses ~20x500ms
}
```

```
// webpack JS shim (assets/main.js) -- the ONLY JS, wraps the standard wallet-adapter ecosystem
import { WalletProvider, useWallet } from '@solana/wallet-adapter-react';
import { WalletModalProvider, WalletMultiButton } from '@solana/wallet-adapter-react-ui';
import { Transaction } from '@solana/web3.js';
function Inner() {
  const { publicKey, signTransaction } = useWallet();
  window.DwaTxSigner = async ({ b64 }) => {
    const tx = Transaction.from(Buffer.from(b64, 'base64'));
    const signed = await signTransaction(tx);
    return Buffer.from(signed.serialize()).toString('base64');
  };
  React.useEffect(() => {
    const detail = { pubkey: publicKey ? Array.from(publicKey.toBytes()) : null };
    window.dispatchEvent(new CustomEvent('dwa-pubkey', { detail }));
  }, [publicKey]);
  return <WalletMultiButton />;
}
// mount <WalletProvider autoConnect><WalletModalProvider><Inner/> into #dioxus-wallet-adapter
```

```
// FlipVault live board: ONE get_multiple_accounts for the whole game, decoded into signals, ticked by use_interval
fn use_flip_board() -> Signal<Board> {
    let mut board = use_signal(Board::default);
    use_resource(move || async move {
        let pdas = [CONFIG, VAULT0, VAULT1, VAULT2, VAULT3];
        let accts = RPC.get_multiple_accounts(&pdas).await?;
        board.set(Board::decode(accts)?);   // strip 8-byte anchor discriminator + borsh
        Ok::<_, anyhow::Error>(())
    });
    use_interval(Duration::from_secs(2), move || { /* re-trigger resource */ });
    board
}
// countdown is pure client math: remaining = (config.last_settled_ts + config.round_secs) - web_time_now_secs()
```

```
// Hand-written Anchor instruction discriminator (the piece ORE's Steel repo will not give you)
fn anchor_disc(ix_name: &str) -> [u8; 8] {
    let mut h = Sha256::new();
    h.update(format!("global:{ix_name}").as_bytes()); // e.g. "global:deposit"
    let d = h.finalize();
    let mut out = [0u8; 8]; out.copy_from_slice(&d[..8]); out
}
// data = anchor_disc("deposit") ++ borsh(DepositArgs{ vault_id, slot, amount })
// position PDA = find_program_address(&[b"position", owner.as_ref(), &[vault_id], &[slot]], &PROGRAM_ID)
```

**Risks:**
- [critical] Stock solana-client/solana-sdk do NOT compile to wasm32-unknown-unknown (tokio, mio, socket2, native getrandom). This is why ORE forked solana-client-wasm/solana-extra-wasm. Naively adding solana-client to a Dioxus web crate fails to build. → Use wasm_client_solana 0.10 (crates.io, solana-sdk v3) as the RPC client, or mirror ORE exactly by [patch]-ing regolith solana-playground solana-client-wasm/solana-extra-wasm 2.1. Keep all RPC/tx-build code behind a web feature; gate desktop-only solana-client behind a desktop feature, exactly as ore-app Cargo.toml does.
- [high] getrandom wasm_js backend not configured -> cryptic linker/build failure as soon as any signing/keypair/curve25519 code is pulled in. → Set RUSTFLAGS --cfg getrandom_backend="wasm_js" in .cargo/config.toml from day one and add getrandom with the js/wasm_js feature. Document in README; it is the single most common first-build failure for Solana-on-WASM.
- [high] Recent-blockhash expiry: blockhashes are valid ~150 slots (~60-90s). Building a tx at page load then signing after the user lingers in the wallet popup yields 'blockhash not found'/expired and a failed deposit -- terrible UX on a 30s-round game. → Fetch get_latest_blockhash immediately before invoke_signature (not at page load), refresh on the same interval as board state, prefer durable confirmation polling. Surface InvokeSignatureStatus::Timeout distinctly and auto-offer retry. Consider versioned txs + a fresh blockhash per attempt.
- [medium] WASM bundle bloat: Dioxus + full Solana stack easily produces a 2-4MB+ .wasm; ORE-class apps must fight this or first paint is slow (the opposite of instant). → Release profile opt-level=z + lto + codegen-units=1 + panic=abort + strip, then wasm-opt -Oz (Dioxus guide shows 2.36MB -> ~234KB on a small app; a Solana stack lands higher but the deltas hold). Serve brotli/gzip, lazy-load heavy routes (history/leaderboard) via asset!/code-split, keep the React wallet shim out of the WASM (separate JS).
- [medium] Dioxus 0.6 -> 0.7 breaking changes bite if you copy ORE 0.6.1 code verbatim: asset! API unified (ImageAssetOptions::new -> AssetOptions::image()), prelude items removed (use_drop, Runtime, queue_effect, provide_root_context now need explicit import), server-fn default codec URL-encoded -> JSON, Axum 0.8. The regolith dioxus-wallet-adapter is pinned to an old dioxus git rev and the old eval module path. → Target 0.7.x fresh; use document::eval (dioxus_document::Eval) with the send/recv/await API shown above, not the 0.6 eval. Treat ORE code as a pattern reference, not a drop-in. Follow the official 0.7 migration guide. Optionally use dioxus-use-js for typed, compile-checked JS bindings instead of raw eval strings.
- [medium] Mobile: desktop browser-extension wallets do not exist on phones. A plain WASM dApp in mobile Safari/Chrome has no injected wallet; users must open it inside the Phantom/Solflare in-app dApp browser (which injects the provider) or you need Mobile Wallet Adapter deep-linking, and MWA is Android/native-oriented and awkward from a pure browser WASM app. → Ship the @solana/wallet-adapter JS shim which already handles wallet-standard + mobile in-app-browser injection and deep-link wallets; document 'open in Phantom/Solflare in-app browser' for mobile. Detect no-wallet and show a deep-link CTA (e.g. phantom.app/ul/browse/<url>). Treat full native mobile (MWA) as a later, separate Dioxus-mobile target, out of scope for the web layer.
- [medium] JS-bridge fragility: invoke_signature depends on window.DwaTxSigner and the dwa-pubkey event existing before Rust calls them; a race where Rust eval runs before the React shim mounts causes silent nothing-happens on connect/sign. Dioxus has known eval edge cases (eval suddenly stopping, return-value issues on web). → Gate the sign button on a connected pubkey (proves the shim mounted and dispatched), guard window.DwaTxSigner existence in the eval JS, add explicit timeouts to InvokeSignatureStatus::Timeout. Long term, migrate to the pure-Rust wallet-adapter crate (no JS, no event races) once UX is locked.
- [medium] RPC rate limits / no public WebSocket: public devnet/mainnet RPC throttle getMultipleAccounts polling and many do not expose stable WS; a per-2s poll across many users hammers limits and accountSubscribe may be unavailable, breaking the instant-flip path. → Use a paid RPC (Helius/Triton/QuickNode) with WS for production; have the frontend prefer the indexer API for history and only hit RPC for the small current-state set. Make poll interval and WS-vs-poll configurable and back off on 429. The keeper/indexer already hold authoritative round data so the frontend can fall back to indexer push/SSE if RPC degrades.

**Open questions:**
- Sign-only vs signAndSend: ORE's DwaTxSigner does signTransaction then the Rust client submits via RPC. wallet-standard also supports solana:signAndSendTransaction (wallet submits). For FlipVault, do we want the wallet to submit (simpler, uses wallet RPC) or the app to submit (lets us control RPC, retry, confirmation UX)? Recommend app-submits to own retry/optimistic reconcile.
- Live updates: is a 2-3s get_multiple_accounts poll (ORE default, RPC-portable) acceptable for a 30s round, or is accountSubscribe WebSocket (instant flip reveal, needs paid RPC WS) worth the dependency? Affects whether wasm_client_solana PubSub is on the critical path.
- JS-bridge vs pure-Rust wallet: start with ORE's proven webpack @solana/wallet-adapter shim (fastest to a working modal) or go pure-Rust wallet-adapter now (no JS build, Rust-everywhere goal)? Recommend shim first then migrate, but confirm tolerance for one small JS build step.
- Dioxus 0.7 vs pin to ORE 0.6.1: 0.7 is current and better but every public ORE snippet is 0.6-era. Confirm we accept porting effort (eval API, asset! API, prelude imports) rather than matching ORE's exact pinned versions.
- How are FlipVault account decoders generated: hand-written borsh mirrors of Config/Vault/Tranche/Position, or auto-generated from the IDL (anchor-IDL -> Rust codegen)? No off-the-shelf Anchor-IDL -> Rust-WASM client crate matches ORE's Steel-based flow, so this is bespoke work the reference repos will not cover.
- Mobile priority for devnet phase: is 'works in Phantom/Solflare in-app browser + deep-link CTA' sufficient now, or is first-class mobile (Dioxus-mobile + MWA) in scope soon? Determines whether the JS shim's mobile handling is enough.

