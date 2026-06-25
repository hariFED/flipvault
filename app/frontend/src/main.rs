//! FlipVault — live Dioxus/WASM dashboard over the deployed devnet program.
//! - Live state: getMultipleAccounts (config + 4 vaults + reserve/treasury + your positions),
//!   decoded in-browser via flipvault-sdk, polled every 5s.
//! - Client-side round countdown (no per-tick network).
//! - Per-vault deposit + withdraw, phase-aware (locked while a round is settling).
//! - Wallet: wallet-adapter connect; deposits/withdrawals are signed by the wallet and
//!   broadcast to devnet through our own RPC (network-independent of the wallet).
use base64::Engine;
use dioxus::prelude::*;
use flipvault_sdk::{ix, pda, state, Instruction, Pubkey};
use solana_transaction::Transaction;
use std::str::FromStr;
use wallet_adapter::wasm_bindgen_futures::JsFuture;
use wallet_adapter::web_sys::{wasm_bindgen::JsCast, window, Headers, Request, RequestInit, Response};
use wallet_adapter::{Cluster, WalletAdapter};

const RPC: &str = "https://api.devnet.solana.com";
const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

static WALLET: GlobalSignal<WalletAdapter> = Signal::global(|| WalletAdapter::init().expect("wallet init"));
static USER: GlobalSignal<Option<[u8; 32]>> = Signal::global(|| None);
static STATUS: GlobalSignal<String> = Signal::global(String::new);
static CHAIN: GlobalSignal<ChainState> = Signal::global(ChainState::default);

// ---------- views (Clone+PartialEq so they live in signals) ----------
#[derive(Clone, PartialEq, Default)]
struct TrancheView {
    is_sol: bool,
    amount: u64,
    shares: u64,
    user_shares: u64,
}
#[derive(Clone, PartialEq, Default)]
struct VaultView {
    tranches: [TrancheView; 2],
}
impl VaultView {
    fn sol_slot(&self) -> usize {
        if self.tranches[0].is_sol { 0 } else { 1 }
    }
}
#[derive(Clone, PartialEq, Default)]
struct ChainState {
    loaded: bool,
    pending: bool,
    round_secs: i64,
    last_settled_ts: i64,
    fee_bps: u16,
    selected_vault: u8,
    r_sol: u64,        // reserve spendable-ish (we show raw reserve lamports)
    treasury: u64,
    vaults: [VaultView; 4],
}

fn sol(lamports: u64) -> String {
    format!("{:.4}", lamports as f64 / LAMPORTS_PER_SOL)
}
fn short(pk: &[u8; 32]) -> String {
    let s = Pubkey::new_from_array(*pk).to_string();
    format!("{}…{}", &s[..4], &s[s.len() - 4..])
}

// ---------- RPC (raw web-sys fetch; no reqwest in wasm) ----------
async fn rpc_post(body: String) -> Result<String, String> {
    let opts = RequestInit::new();
    opts.set_method("POST");
    let headers = Headers::new().map_err(|e| format!("{e:?}"))?;
    headers.append("content-type", "application/json").map_err(|e| format!("{e:?}"))?;
    opts.set_headers(&headers);
    opts.set_body(&body.into());
    let req = Request::new_with_str_and_init(RPC, &opts).map_err(|e| format!("{e:?}"))?;
    let win = window().ok_or("no window")?;
    let resp_val = JsFuture::from(win.fetch_with_request(&req)).await.map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_val.dyn_into().map_err(|e| format!("{e:?}"))?;
    let text = JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?).await.map_err(|e| format!("{e:?}"))?;
    text.as_string().ok_or_else(|| "response not a string".into())
}

async fn get_accounts(keys: &[Pubkey]) -> Result<Vec<Option<(Vec<u8>, u64)>>, String> {
    let ks: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
    let body = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"getMultipleAccounts",
        "params":[ks, {"encoding":"base64","commitment":"confirmed"}]
    })
    .to_string();
    let text = rpc_post(body).await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let arr = v["result"]["value"].as_array().ok_or("no value array")?;
    let mut out = Vec::new();
    for a in arr {
        if a.is_null() {
            out.push(None);
        } else {
            let b64 = a["data"][0].as_str().unwrap_or("");
            let lamports = a["lamports"].as_u64().unwrap_or(0);
            let data = base64::engine::general_purpose::STANDARD.decode(b64).unwrap_or_default();
            out.push(Some((data, lamports)));
        }
    }
    Ok(out)
}

async fn get_blockhash() -> Result<solana_hash::Hash, String> {
    let body = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"getLatestBlockhash","params":[{"commitment":"confirmed"}]
    })
    .to_string();
    let text = rpc_post(body).await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let bh = v["result"]["value"]["blockhash"].as_str().ok_or("no blockhash")?;
    solana_hash::Hash::from_str(bh).map_err(|e| e.to_string())
}

async fn send_tx(signed: &[u8]) -> Result<String, String> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(signed);
    let body = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"sendTransaction",
        "params":[b64, {"encoding":"base64","preflightCommitment":"confirmed"}]
    })
    .to_string();
    let text = rpc_post(body).await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if let Some(err) = v.get("error") {
        return Err(format!("rpc send: {err}"));
    }
    v["result"].as_str().map(String::from).ok_or_else(|| "no signature".into())
}

// ---------- live state ----------
async fn fetch_chain() -> Result<ChainState, String> {
    let mut keys = vec![pda::config_pda().0, pda::reserve_pda().0, pda::treasury_pda().0];
    for i in 0..4u8 {
        keys.push(pda::vault_pda(i).0);
    }
    let accts = get_accounts(&keys).await?;

    let cfg_data = accts.get(0).and_then(|a| a.clone()).ok_or("config missing")?.0;
    let config: state::Config = state::decode(&cfg_data).map_err(|e| e.to_string())?;
    let reserve = accts.get(1).and_then(|a| a.as_ref()).map(|a| a.1).unwrap_or(0);
    let treasury = accts.get(2).and_then(|a| a.as_ref()).map(|a| a.1).unwrap_or(0);

    let mut st = ChainState {
        loaded: true,
        pending: matches!(config.phase, state::RoundPhase::Pending),
        round_secs: config.round_secs,
        last_settled_ts: config.last_settled_ts,
        fee_bps: config.fee_bps,
        selected_vault: config.selected_vault,
        r_sol: reserve,
        treasury,
        vaults: Default::default(),
    };
    for i in 0..4usize {
        if let Some(Some((data, _))) = accts.get(3 + i) {
            if let Ok(v) = state::decode::<state::Vault>(data) {
                for s in 0..2 {
                    st.vaults[i].tranches[s] = TrancheView {
                        is_sol: matches!(v.tranches[s].asset, state::Asset::Sol),
                        amount: v.tranches[s].amount,
                        shares: v.tranches[s].total_shares,
                        user_shares: 0,
                    };
                }
            }
        }
    }

    // Second pass: the connected user's positions (both slots × 4 vaults).
    if let Some(user) = *USER.read() {
        let owner = Pubkey::new_from_array(user);
        let mut pkeys = Vec::new();
        for i in 0..4u8 {
            for s in 0..2u8 {
                pkeys.push(pda::position_pda(&owner, i, s).0);
            }
        }
        if let Ok(pos) = get_accounts(&pkeys).await {
            for i in 0..4usize {
                for s in 0..2usize {
                    if let Some(Some((data, _))) = pos.get(i * 2 + s) {
                        if let Ok(p) = state::decode::<state::Position>(data) {
                            st.vaults[i].tranches[s].user_shares = p.shares;
                        }
                    }
                }
            }
        }
    }
    Ok(st)
}

// ---------- actions ----------
async fn sign_and_broadcast(ix: Instruction, user: [u8; 32]) -> Result<String, String> {
    let user_pk = Pubkey::new_from_array(user);
    let mut tx = Transaction::new_with_payer(&[ix], Some(&user_pk));
    tx.message.recent_blockhash = get_blockhash().await?;
    let bytes = bincode::serialize(&tx).map_err(|e| format!("serialize: {e}"))?;
    let signed = WALLET
        .read()
        .sign_transaction(&bytes, Some(Cluster::DevNet))
        .await
        .map_err(|e| format!("sign: {e:?}"))?;
    let first = signed.into_iter().next().ok_or("no signed tx")?;
    send_tx(&first).await
}

// ---------- components ----------
fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // Poll chain state every 5s.
    use_future(|| async move {
        loop {
            match fetch_chain().await {
                Ok(st) => *CHAIN.write() = st,
                Err(e) => *STATUS.write() = format!("load error: {e}"),
            }
            gloo_timers::future::TimeoutFuture::new(5000).await;
        }
    });

    rsx! {
        style { {CSS} }
        div { class: "wrap",
            header { class: "top",
                h1 { "FlipVault" }
                ConnectButton {}
            }
            p { class: "sub", "One curve, four vaults. Each round, VRF flips one vault SOL↔TOKEN." }
            StatsBar {}
            VaultGrid {}
            p { class: "status", "{STATUS}" }
        }
    }
}

#[component]
fn ConnectButton() -> Element {
    let label = match *USER.read() {
        Some(pk) => format!("{} ◦ connected", short(&pk)),
        None => "Connect Wallet".to_string(),
    };
    rsx! {
        button { class: "btn primary",
            onclick: move |_| {
                spawn(async move {
                    let connected = WALLET.write().connect_by_name("Phantom").await;
                    match connected {
                        Ok(_) => {
                            let adapter = WALLET.read();
                            let info = adapter.connection_info().await;
                            if let Ok(acct) = info.connected_account() {
                                *USER.write() = Some(acct.public_key());
                                *STATUS.write() = "connected".into();
                            }
                        }
                        Err(e) => *STATUS.write() = format!("connect error: {e}"),
                    }
                });
            },
            "{label}"
        }
    }
}

#[component]
fn StatsBar() -> Element {
    let st = CHAIN.read();
    let now = (js_sys::Date::now() / 1000.0) as i64;
    let secs_left = if st.loaded { (st.last_settled_ts + st.round_secs - now).max(0) } else { 0 };
    let timer = if !st.loaded {
        "—".to_string()
    } else if st.pending {
        "flipping…".to_string()
    } else {
        format!("{secs_left}s")
    };
    rsx! {
        div { class: "stats",
            div { class: "stat", span { class: "k", "Reserve" } span { class: "v", "{sol(st.r_sol)} SOL" } }
            div { class: "stat", span { class: "k", "Treasury" } span { class: "v", "{sol(st.treasury)} SOL" } }
            div { class: "stat", span { class: "k", "Fee" } span { class: "v", "{st.fee_bps as f64 / 100.0}%" } }
            div { class: "stat", span { class: "k", "Next flip" } span { class: "v timer", "{timer}" } }
        }
    }
}

#[component]
fn VaultGrid() -> Element {
    let st = CHAIN.read();
    rsx! {
        div { class: "grid",
            for i in 0u8..4 {
                VaultCard { id: i }
            }
        }
        if !st.loaded {
            p { class: "sub", "loading on-chain state…" }
        }
    }
}

#[component]
fn VaultCard(id: u8) -> Element {
    let st = CHAIN.read();
    let v = st.vaults[id as usize].clone();
    let selected = st.selected_vault == id;
    let pending = st.pending;
    let sol_slot = v.sol_slot();
    let mut amount = use_signal(|| "100000000".to_string());
    let mut shares = use_signal(|| String::new());

    let connected = USER.read().is_some();
    let my_shares = v.tranches[sol_slot].user_shares;

    rsx! {
        div { class: if selected { "card sel" } else { "card" },
            div { class: "card-h",
                span { "Vault {id}" }
                if selected { span { class: "badge", "last flip" } }
            }
            for s in 0usize..2 {
                {
                    let t = &v.tranches[s];
                    rsx! {
                        div { class: if t.is_sol { "tr sol" } else { "tr tok" },
                            span { class: "asset", { if t.is_sol { "SOL" } else { "TOKEN" } } }
                            span { class: "amt",
                                { if t.is_sol { format!("{} SOL", sol(t.amount)) } else { format!("{} tok", t.amount) } }
                            }
                            span { class: "sh", "shares {t.shares}" }
                        }
                    }
                }
            }
            if connected && my_shares > 0 {
                p { class: "mine", "your shares: {my_shares}" }
            }
            // Actions operate on the current SOL tranche.
            div { class: "actions",
                input {
                    r#type: "number", value: "{amount}",
                    oninput: move |e| amount.set(e.value()),
                    placeholder: "lamports",
                }
                button { class: "btn",
                    disabled: !connected || pending,
                    onclick: move |_| {
                        let amt = amount().parse::<u64>().unwrap_or(0);
                        spawn(async move {
                            if let Some(user) = *USER.read() {
                                *STATUS.write() = format!("depositing into vault {id}…");
                                let ixn = ix::deposit(&Pubkey::new_from_array(user), id, sol_slot as u8, amt);
                                match sign_and_broadcast(ixn, user).await {
                                    Ok(sig) => *STATUS.write() = format!("deposited! {sig}"),
                                    Err(e) => *STATUS.write() = format!("error: {e}"),
                                }
                            }
                        });
                    },
                    "Deposit"
                }
            }
            div { class: "actions",
                input {
                    r#type: "number", value: "{shares}",
                    oninput: move |e| shares.set(e.value()),
                    placeholder: "shares to withdraw",
                }
                button { class: "btn ghost",
                    disabled: !connected || pending,
                    onclick: move |_| {
                        let sh = shares().parse::<u64>().unwrap_or(0);
                        spawn(async move {
                            if let Some(user) = *USER.read() {
                                *STATUS.write() = format!("withdrawing from vault {id}…");
                                let ixn = ix::withdraw(&Pubkey::new_from_array(user), id, sol_slot as u8, sh);
                                match sign_and_broadcast(ixn, user).await {
                                    Ok(sig) => *STATUS.write() = format!("withdrew! {sig}"),
                                    Err(e) => *STATUS.write() = format!("error: {e}"),
                                }
                            }
                        });
                    },
                    "Withdraw"
                }
            }
            if pending {
                p { class: "locked", "locked — round settling" }
            }
        }
    }
}

const CSS: &str = r#"
* { box-sizing: border-box; }
body { margin:0; background:#0b0b0f; color:#e4e4e7; font-family: system-ui, sans-serif; }
.wrap { max-width: 920px; margin: 32px auto; padding: 0 16px; }
.top { display:flex; justify-content:space-between; align-items:center; }
h1 { font-size: 28px; margin: 0; letter-spacing: -0.5px; }
.sub { color:#a1a1aa; font-size: 14px; }
.status { margin-top: 16px; font-family: monospace; font-size: 12px; color:#fbbf24; word-break: break-all; min-height: 18px; }
.stats { display:flex; gap:12px; margin:16px 0; flex-wrap:wrap; }
.stat { background:#16161d; border:1px solid #27272a; border-radius:12px; padding:10px 14px; min-width:120px; }
.stat .k { display:block; color:#71717a; font-size:11px; text-transform:uppercase; }
.stat .v { font-size:18px; font-weight:600; }
.timer { color:#34d399; }
.grid { display:grid; grid-template-columns: repeat(2, 1fr); gap:14px; }
@media (max-width:640px){ .grid { grid-template-columns:1fr; } }
.card { background:#16161d; border:1px solid #27272a; border-radius:14px; padding:14px; }
.card.sel { border-color:#f59e0b; box-shadow:0 0 0 1px #f59e0b55; }
.card-h { display:flex; justify-content:space-between; align-items:center; font-weight:600; margin-bottom:10px; }
.badge { font-size:10px; color:#f59e0b; border:1px solid #f59e0b; border-radius:999px; padding:1px 8px; }
.tr { display:flex; justify-content:space-between; align-items:center; padding:8px 10px; border-radius:8px; margin-bottom:6px; font-size:13px; }
.tr.sol { background:#10261b; } .tr.tok { background:#1a1626; }
.tr .asset { font-weight:700; } .tr.sol .asset { color:#34d399; } .tr.tok .asset { color:#a78bfa; }
.sh { color:#71717a; font-size:11px; }
.mine { color:#34d399; font-size:12px; margin:6px 0; }
.actions { display:flex; gap:8px; margin-top:8px; }
.actions input { flex:1; background:#0b0b0f; border:1px solid #3f3f46; color:#e4e4e7; border-radius:8px; padding:6px 8px; font-size:12px; }
.btn { background:#27272a; color:#e4e4e7; border:1px solid #3f3f46; border-radius:8px; padding:6px 12px; cursor:pointer; font-size:13px; }
.btn:hover:not(:disabled) { background:#3f3f46; }
.btn:disabled { opacity:0.4; cursor:not-allowed; }
.btn.primary { background:#f59e0b; color:#0b0b0f; border:none; font-weight:600; }
.btn.ghost { background:transparent; }
.locked { color:#f87171; font-size:11px; margin:8px 0 0; }
"#;
