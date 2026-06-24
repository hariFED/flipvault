//! FlipVault indexer + API.
//!
//! Snapshot ingester: every few seconds it fetches Config + the 4 Vaults (one getMultipleAccounts),
//! decodes them via flipvault-sdk, records a vault snapshot, and — when Config.last_settled_ts
//! advances — records a settled round. An Axum API serves current state + history to the frontend.
//! (Deposit/withdraw/position/leaderboard ingestion via tx-arg decoding is the documented next
//! extension; big integers are stored as TEXT to avoid a decimal dependency.)
use anyhow::{anyhow, Result};
use axum::{extract::Path, extract::State, routing::get, Json, Router};
use base64::Engine;
use flipvault_sdk::{pda, state, Pubkey};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::time::Duration;
use tower_http::cors::CorsLayer;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS rounds (
  settled_ts     BIGINT PRIMARY KEY,
  round_seed     TEXT NOT NULL,
  selected_vault SMALLINT NOT NULL,
  recorded_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS vault_snapshots (
  id             BIGSERIAL PRIMARY KEY,
  vault_id       SMALLINT NOT NULL,
  slot0_asset    TEXT NOT NULL, slot0_amount TEXT NOT NULL, slot0_shares TEXT NOT NULL,
  slot1_asset    TEXT NOT NULL, slot1_amount TEXT NOT NULL, slot1_shares TEXT NOT NULL,
  vault_lamports TEXT NOT NULL,
  recorded_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS vault_snapshots_vault_idx ON vault_snapshots (vault_id, id DESC);
"#;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

struct Rpc {
    client: reqwest::Client,
    url: String,
}

impl Rpc {
    fn new(url: String) -> Self {
        Rpc { client: reqwest::Client::new(), url }
    }
    async fn multiple_accounts(&self, keys: &[Pubkey]) -> Result<Vec<Option<(Vec<u8>, u64)>>> {
        let ks: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        let body = json!({"jsonrpc":"2.0","id":1,"method":"getMultipleAccounts",
            "params":[ks, {"encoding":"base64"}]});
        let resp: Value = self.client.post(&self.url).json(&body).send().await?.json().await?;
        if let Some(e) = resp.get("error") {
            return Err(anyhow!("rpc error: {e}"));
        }
        let arr = resp["result"]["value"]
            .as_array()
            .ok_or_else(|| anyhow!("no value array"))?;
        let mut out = Vec::new();
        for a in arr {
            if a.is_null() {
                out.push(None);
            } else {
                let b64 = a["data"][0].as_str().ok_or_else(|| anyhow!("no data"))?;
                let lamports = a["lamports"].as_u64().unwrap_or(0);
                out.push(Some((base64::engine::general_purpose::STANDARD.decode(b64)?, lamports)));
            }
        }
        Ok(out)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn asset_str(a: &state::Asset) -> &'static str {
    match a {
        state::Asset::Sol => "SOL",
        state::Asset::Token => "TOKEN",
    }
}

async fn ingest_loop(pool: PgPool, rpc: Rpc) {
    let keys: Vec<Pubkey> = std::iter::once(pda::config_pda().0)
        .chain((0..4u8).map(|i| pda::vault_pda(i).0))
        .collect();
    let mut last_settled: i64 = -1;

    loop {
        if let Err(e) = ingest_once(&pool, &rpc, &keys, &mut last_settled).await {
            tracing::warn!("ingest error: {e:#}");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn ingest_once(
    pool: &PgPool,
    rpc: &Rpc,
    keys: &[Pubkey],
    last_settled: &mut i64,
) -> Result<()> {
    let accts = rpc.multiple_accounts(keys).await?;
    let cfg_raw = accts.get(0).and_then(|a| a.as_ref());
    let Some((cfg_data, _)) = cfg_raw else {
        return Err(anyhow!("config not found"));
    };
    let config: state::Config = state::decode(cfg_data)?;

    // Record a settled round when last_settled_ts advances and a vault was selected.
    if config.last_settled_ts > *last_settled
        && config.selected_vault != flipvault_sdk::NO_VAULT
    {
        sqlx::query(
            "INSERT INTO rounds (settled_ts, round_seed, selected_vault) VALUES ($1,$2,$3) ON CONFLICT (settled_ts) DO NOTHING",
        )
        .bind(config.last_settled_ts)
        .bind(hex(&config.round_seed))
        .bind(config.selected_vault as i16)
        .execute(pool)
        .await?;
        tracing::info!(vault = config.selected_vault, ts = config.last_settled_ts, "recorded round");
    }
    *last_settled = config.last_settled_ts;

    // Snapshot each vault.
    for i in 0..4usize {
        if let Some(Some((data, lamports))) = accts.get(1 + i) {
            if let Ok(v) = state::decode::<state::Vault>(data) {
                sqlx::query(
                    "INSERT INTO vault_snapshots
                     (vault_id, slot0_asset, slot0_amount, slot0_shares, slot1_asset, slot1_amount, slot1_shares, vault_lamports)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(v.vault_id as i16)
                .bind(asset_str(&v.tranches[0].asset))
                .bind(v.tranches[0].amount.to_string())
                .bind(v.tranches[0].total_shares.to_string())
                .bind(asset_str(&v.tranches[1].asset))
                .bind(v.tranches[1].amount.to_string())
                .bind(v.tranches[1].total_shares.to_string())
                .bind(lamports.to_string())
                .execute(pool)
                .await?;
            }
        }
    }
    Ok(())
}

// ---- API ----
async fn healthz() -> &'static str {
    "ok"
}

async fn rounds(State(st): State<AppState>) -> Json<Value> {
    let rows = sqlx::query("SELECT settled_ts, round_seed, selected_vault FROM rounds ORDER BY settled_ts DESC LIMIT 100")
        .fetch_all(&st.pool)
        .await
        .unwrap_or_default();
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "settled_ts": r.get::<i64, _>("settled_ts"),
                "round_seed": r.get::<String, _>("round_seed"),
                "selected_vault": r.get::<i16, _>("selected_vault"),
            })
        })
        .collect();
    Json(json!({ "rounds": out }))
}

async fn vault_history(State(st): State<AppState>, Path(id): Path<i16>) -> Json<Value> {
    let rows = sqlx::query(
        "SELECT slot0_asset, slot0_amount, slot0_shares, slot1_asset, slot1_amount, slot1_shares, vault_lamports, extract(epoch from recorded_at)::bigint AS ts
         FROM vault_snapshots WHERE vault_id = $1 ORDER BY id DESC LIMIT 200",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "ts": r.get::<i64, _>("ts"),
                "slot0": { "asset": r.get::<String,_>("slot0_asset"), "amount": r.get::<String,_>("slot0_amount"), "shares": r.get::<String,_>("slot0_shares") },
                "slot1": { "asset": r.get::<String,_>("slot1_asset"), "amount": r.get::<String,_>("slot1_amount"), "shares": r.get::<String,_>("slot1_shares") },
                "lamports": r.get::<String,_>("vault_lamports"),
            })
        })
        .collect();
    Json(json!({ "vault_id": id, "history": out }))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://flip:flip@localhost:5432/flipvault".into());
    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "https://api.devnet.solana.com".into());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());

    let pool = PgPoolOptions::new().max_connections(5).connect(&db).await?;
    sqlx::raw_sql(SCHEMA).execute(&pool).await?;
    tracing::info!("db ready");

    tokio::spawn(ingest_loop(pool.clone(), Rpc::new(rpc_url)));

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/rounds", get(rounds))
        .route("/vaults/{id}/history", get(vault_history))
        .layer(CorsLayer::permissive())
        .with_state(AppState { pool });

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("API listening on {bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
