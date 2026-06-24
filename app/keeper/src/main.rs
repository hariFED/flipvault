//! FlipVault keeper: drives the round lifecycle on a schedule.
//!
//! Loop: read Config; if Idle and the round interval elapsed -> commit_round (request ORAO VRF);
//! if Pending -> try settle_round once VRF is fulfilled (checked via simulateTransaction); if a
//! round is stuck past RECOVER_AFTER_SECS -> recover_round. Permissionless settle, so any keeper
//! can run this.
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use flipvault_sdk::{ix, pda, state, Pubkey};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RECOVER_AFTER_SECS: i64 = 300;

struct Cfg {
    rpc_url: String,
    keypair_path: String,
    orao_treasury: Pubkey,
    poll: Duration,
}

impl Cfg {
    fn from_env() -> Self {
        let get = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        Cfg {
            rpc_url: get("RPC_URL", "https://api.devnet.solana.com"),
            keypair_path: get("KEEPER_KEYPAIR", "/root/.config/solana/devnet.json"),
            orao_treasury: Pubkey::from_str(&get(
                "ORAO_TREASURY",
                "9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR",
            ))
            .expect("valid ORAO_TREASURY"),
            poll: Duration::from_secs(get("POLL_SECS", "3").parse().unwrap_or(3)),
        }
    }
}

struct Rpc {
    client: reqwest::Client,
    url: String,
}

impl Rpc {
    fn new(url: String) -> Self {
        Rpc { client: reqwest::Client::new(), url }
    }

    async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let body = serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let resp: serde_json::Value = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if let Some(err) = resp.get("error") {
            return Err(anyhow!("rpc {method} error: {err}"));
        }
        Ok(resp["result"].clone())
    }

    async fn account_data(&self, key: &Pubkey) -> Result<Option<Vec<u8>>> {
        let r = self
            .call(
                "getAccountInfo",
                serde_json::json!([key.to_string(), {"encoding":"base64","commitment":"confirmed"}]),
            )
            .await?;
        let val = &r["value"];
        if val.is_null() {
            return Ok(None);
        }
        let b64 = val["data"][0]
            .as_str()
            .ok_or_else(|| anyhow!("no account data"))?;
        Ok(Some(base64::engine::general_purpose::STANDARD.decode(b64)?))
    }

    async fn latest_blockhash(&self) -> Result<Hash> {
        let r = self
            .call("getLatestBlockhash", serde_json::json!([{"commitment":"confirmed"}]))
            .await?;
        let s = r["value"]["blockhash"]
            .as_str()
            .ok_or_else(|| anyhow!("no blockhash"))?;
        Ok(Hash::from_str(s)?)
    }

    /// True if the tx would succeed right now (used to detect ORAO fulfillment before sending).
    async fn simulate_ok(&self, tx_b64: &str) -> Result<bool> {
        let r = self
            .call(
                "simulateTransaction",
                serde_json::json!([tx_b64, {"encoding":"base64","sigVerify":false,"replaceRecentBlockhash":true,"commitment":"confirmed"}]),
            )
            .await?;
        Ok(r["value"]["err"].is_null())
    }

    async fn send(&self, tx_b64: &str) -> Result<String> {
        let r = self
            .call(
                "sendTransaction",
                serde_json::json!([tx_b64, {"encoding":"base64","skipPreflight":false,"preflightCommitment":"confirmed"}]),
            )
            .await?;
        r.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("no signature"))
    }
}

fn load_keypair(path: &str) -> Result<Keypair> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read keypair {path}"))?;
    let bytes: Vec<u8> = serde_json::from_str(&raw).context("keypair JSON")?;
    Keypair::try_from(bytes.as_slice()).map_err(|e| anyhow!("bad keypair: {e}"))
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

async fn build_tx_b64(
    rpc: &Rpc,
    payer: &Keypair,
    ixs: &[solana_instruction::Instruction],
) -> Result<String> {
    let bh = rpc.latest_blockhash().await?;
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), &[payer], bh);
    let bytes = bincode::serialize(&tx)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = Cfg::from_env();
    let keeper = load_keypair(&cfg.keypair_path)?;
    let keeper_pk = keeper.pubkey();
    let rpc = Rpc::new(cfg.rpc_url.clone());
    tracing::info!(keeper = %keeper_pk, rpc = %cfg.rpc_url, "keeper started");

    let config_pda = pda::config_pda().0;

    loop {
        if let Err(e) = tick(&rpc, &cfg, &keeper, &keeper_pk, &config_pda).await {
            tracing::warn!("tick error: {e:#}");
        }
        tokio::time::sleep(cfg.poll).await;
    }
}

async fn tick(
    rpc: &Rpc,
    cfg: &Cfg,
    keeper: &Keypair,
    keeper_pk: &Pubkey,
    config_pda: &Pubkey,
) -> Result<()> {
    let data = rpc
        .account_data(config_pda)
        .await?
        .ok_or_else(|| anyhow!("config account not found — is the program initialized?"))?;
    let config: state::Config = state::decode(&data)?;
    let t = now();

    match config.phase {
        state::RoundPhase::Idle => {
            let next = config.last_settled_ts + config.round_secs;
            if t >= next {
                let force: [u8; 32] = rand::random();
                let ix = ix::commit_round(keeper_pk, force, &cfg.orao_treasury);
                let b64 = build_tx_b64(rpc, keeper, &[ix]).await?;
                let sig = rpc.send(&b64).await?;
                tracing::info!(seed = %hex(&force), %sig, "committed round");
            } else {
                tracing::debug!("idle, {}s until next round", next - t);
            }
        }
        state::RoundPhase::Pending => {
            // Recover a round stuck waiting on VRF.
            if t >= config.commit_ts + RECOVER_AFTER_SECS {
                let b64 = build_tx_b64(rpc, keeper, &[ix::recover_round()]).await?;
                let sig = rpc.send(&b64).await?;
                tracing::warn!(%sig, "recovered stuck round");
                return Ok(());
            }
            // Settle once ORAO has fulfilled (probe with simulate to avoid spending on failures).
            let ix = ix::settle_round(&config.round_seed);
            let b64 = build_tx_b64(rpc, keeper, &[ix]).await?;
            if rpc.simulate_ok(&b64).await? {
                let sig = rpc.send(&b64).await?;
                tracing::info!(%sig, "settled round (flip)");
            } else {
                tracing::debug!("pending: VRF not yet fulfilled");
            }
        }
    }
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
