//! FlipVault M0 spike: connect a Solana wallet (wallet-adapter, Wallet Standard) and send a
//! real `deposit` to the live devnet program, with the instruction built by flipvault-sdk.
//! Proves the whole Rust/WASM wallet+tx path end-to-end.
use base64::Engine;
use dioxus::prelude::*;
use flipvault_sdk::Pubkey;
use solana_transaction::Transaction;
use std::str::FromStr;
use wallet_adapter::wasm_bindgen_futures::JsFuture;
use wallet_adapter::web_sys::{wasm_bindgen::JsCast, window, Headers, Request, RequestInit, Response};
use wallet_adapter::{Cluster, WalletAdapter};

const RPC: &str = "https://api.devnet.solana.com";

static WALLET: GlobalSignal<WalletAdapter> = Signal::global(|| WalletAdapter::init().expect("wallet init"));
static USER: GlobalSignal<Option<[u8; 32]>> = Signal::global(|| None);
static STATUS: GlobalSignal<String> = Signal::global(String::new);

fn main() {
    dioxus::launch(App);
}

fn short(pk: &[u8; 32]) -> String {
    let s = Pubkey::new_from_array(*pk).to_string();
    format!("{}…{}", &s[..4], &s[s.len() - 4..])
}

async fn rpc_post(body: String) -> Result<String, String> {
    let opts = RequestInit::new();
    opts.set_method("POST");
    let headers = Headers::new().map_err(|e| format!("{e:?}"))?;
    headers
        .append("content-type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    opts.set_headers(&headers);
    opts.set_body(&body.into());
    let req = Request::new_with_str_and_init(RPC, &opts).map_err(|e| format!("{e:?}"))?;
    let win = window().ok_or("no window")?;
    let resp_val = JsFuture::from(win.fetch_with_request(&req))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_val.dyn_into().map_err(|e| format!("{e:?}"))?;
    let text = JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    text.as_string().ok_or_else(|| "response not a string".into())
}

async fn get_blockhash() -> Result<solana_hash::Hash, String> {
    let body = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"getLatestBlockhash","params":[{"commitment":"confirmed"}]
    })
    .to_string();
    let text = rpc_post(body).await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let bh = v["result"]["value"]["blockhash"]
        .as_str()
        .ok_or("no blockhash in response")?;
    solana_hash::Hash::from_str(bh).map_err(|e| e.to_string())
}

/// Send fully-signed transaction bytes to devnet via our own RPC (decoupled from the
/// wallet's selected network). Surfaces the real RPC error.
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
    v["result"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "no signature in response".into())
}

async fn do_deposit(user: [u8; 32], vault_id: u8, slot: u8, amount: u64) -> Result<String, String> {
    let user_pk = Pubkey::new_from_array(user);
    let ix = flipvault_sdk::ix::deposit(&user_pk, vault_id, slot, amount);
    let mut tx = Transaction::new_with_payer(&[ix], Some(&user_pk));
    tx.message.recent_blockhash = get_blockhash().await?;
    let bytes = bincode::serialize(&tx).map_err(|e| format!("serialize: {e}"))?;
    // Wallet only SIGNS; we broadcast to devnet ourselves.
    let signed = WALLET
        .read()
        .sign_transaction(&bytes, Some(Cluster::DevNet))
        .await
        .map_err(|e| format!("sign: {e:?}"))?;
    let first = signed.into_iter().next().ok_or("wallet returned no signed tx")?;
    send_tx(&first).await
}

#[component]
fn App() -> Element {
    rsx! {
        div { style: "font-family: system-ui; max-width: 480px; margin: 48px auto; color:#e4e4e7;",
            h1 { "FlipVault — M0 spike" }
            p { style: "color:#a1a1aa;", "Connect a Solana wallet (Devnet) and send a real deposit." }
            ConnectButton {}
            DepositPanel {}
            p { style: "margin-top:16px; font-family:monospace; word-break:break-all;", "{STATUS}" }
        }
    }
}

#[component]
fn ConnectButton() -> Element {
    let label = match USER() {
        Some(pk) => format!("{} | connected", short(&pk)),
        None => "Connect Wallet".to_string(),
    };
    rsx! {
        button {
            onclick: move |_| {
                spawn(async move {
                    // Bind to a let so the WALLET write guard is released before we read it below
                    // (otherwise the match scrutinee temporary holds the borrow through the arms).
                    let connected = WALLET.write().connect_by_name("Phantom").await;
                    match connected {
                        Ok(_) => {
                            let adapter = WALLET.read();
                            let info = adapter.connection_info().await;
                            if let Ok(acct) = info.connected_account() {
                                *USER.write() = Some(acct.public_key());
                                *STATUS.write() = "wallet connected".into();
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
fn DepositPanel() -> Element {
    let mut amount = use_signal(|| "100000000".to_string());
    rsx! {
        div { style: "margin-top:16px;",
            input {
                value: "{amount}",
                oninput: move |e| amount.set(e.value()),
                style: "padding:6px; margin-right:8px;",
            }
            button {
                disabled: USER().is_none(),
                onclick: move |_| {
                    let amt = amount().parse::<u64>().unwrap_or(0);
                    spawn(async move {
                        if let Some(user) = USER() {
                            *STATUS.write() = "building + signing…".into();
                            match do_deposit(user, 0, 0, amt).await {
                                Ok(sig) => *STATUS.write() = format!("deposited! sig: {sig}"),
                                Err(e) => *STATUS.write() = format!("error: {e}"),
                            }
                        }
                    });
                },
                "Deposit into vault 0 (SOL tranche)"
            }
        }
    }
}
