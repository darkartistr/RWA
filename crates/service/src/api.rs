use crate::{db::Db, kafka::KafkaProducer, metrics::Metrics, solana::SolanaTokenClient};
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, time::Instant};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub kafka: KafkaProducer,
    pub sol: SolanaTokenClient,
    pub metrics: Metrics,
}

pub fn router(state: AppState, _metrics: Metrics) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics_handler))
        .route("/v1/mints", post(create_mint))
        .route("/v1/mint_to", post(mint_to))
        .route("/v1/transfer", post(transfer))
        .route("/v1/burn", post(burn))
        .layer(CorsLayer::new().allow_origin(Any).allow_headers(Any).allow_methods(Any))
        .layer(middleware::from_fn(track_http_metrics))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, state.metrics.gather())
}

async fn track_http_metrics(req: Request<axum::body::Body>, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let start = Instant::now();
    let res = next.run(req).await;
    let status = res.status().as_u16().to_string();
    tracing::debug!(%method, %path, %status, elapsed_ms = start.elapsed().as_millis(), "http request");
    res
}

#[derive(Deserialize)]
pub struct CreateMintRequest {
    pub decimals: u8,
}

#[derive(Serialize)]
pub struct CreateMintResponse {
    pub mint: String,
    pub signature: String,
}

pub async fn create_mint(State(state): State<AppState>, Json(req): Json<CreateMintRequest>) -> Result<Json<CreateMintResponse>, ApiError> {
    let started = Instant::now();
    let (mint, sig) = state
        .sol
        .create_mint(req.decimals)
        .map_err(ApiError::solana)?;

    sqlx::query("insert into token_mints (mint, decimals) values ($1, $2) on conflict do nothing")
        .bind(mint.to_string())
        .bind(i32::from(req.decimals))
        .execute(state.db.pool())
        .await
        .map_err(ApiError::db)?;

    record_op(
        &state,
        "CREATE_MINT",
        mint.to_string(),
        "0",
        None,
        None,
        Some(sig.to_string()),
        "SUCCESS",
        None,
    )
    .await;

    state.metrics.token_ops.with_label_values(&["create_mint", "success"]).inc();
    state.metrics.solana_rpc_seconds.with_label_values(&["create_mint"]).observe(started.elapsed().as_secs_f64());

    Ok(Json(CreateMintResponse {
        mint: mint.to_string(),
        signature: sig.to_string(),
    }))
}

#[derive(Deserialize)]
pub struct MintToRequest {
    pub mint: String,
    pub recipient_owner: String,
    pub amount: u64,
}

#[derive(Serialize)]
pub struct MintToResponse {
    pub signature: String,
    pub recipient_ata: String,
}

pub async fn mint_to(State(state): State<AppState>, Json(req): Json<MintToRequest>) -> Result<Json<MintToResponse>, ApiError> {
    let started = Instant::now();
    let mint = Pubkey::from_str(&req.mint).map_err(ApiError::bad_request)?;
    let recipient = Pubkey::from_str(&req.recipient_owner).map_err(ApiError::bad_request)?;

    let (ata, sig) = state.sol.mint_to(mint, recipient, req.amount).map_err(ApiError::solana)?;

    record_op(
        &state,
        "MINT_TO",
        req.mint.clone(),
        req.amount.to_string(),
        None,
        Some(req.recipient_owner.clone()),
        Some(sig.to_string()),
        "SUCCESS",
        None,
    )
    .await;

    let event = serde_json::json!({
        "type": "MINT_TO",
        "mint": req.mint,
        "recipient_owner": req.recipient_owner,
        "recipient_ata": ata.to_string(),
        "amount": req.amount.to_string(),
        "signature": sig.to_string(),
        "ts": chrono::Utc::now().to_rfc3339(),
    });
    let _ = state.kafka.publish_json(&sig.to_string(), &event).await;

    state.metrics.token_ops.with_label_values(&["mint_to", "success"]).inc();
    state.metrics.solana_rpc_seconds.with_label_values(&["mint_to"]).observe(started.elapsed().as_secs_f64());

    Ok(Json(MintToResponse {
        signature: sig.to_string(),
        recipient_ata: ata.to_string(),
    }))
}

#[derive(Deserialize)]
pub struct TransferRequest {
    pub mint: String,
    pub sender_owner: String,
    pub recipient_owner: String,
    pub amount: u64,
}

#[derive(Serialize)]
pub struct TransferResponse {
    pub signature: String,
    pub sender_ata: String,
    pub recipient_ata: String,
}

pub async fn transfer(State(state): State<AppState>, Json(req): Json<TransferRequest>) -> Result<Json<TransferResponse>, ApiError> {
    let started = Instant::now();
    let mint = Pubkey::from_str(&req.mint).map_err(ApiError::bad_request)?;
    let sender = Pubkey::from_str(&req.sender_owner).map_err(ApiError::bad_request)?;
    let recipient = Pubkey::from_str(&req.recipient_owner).map_err(ApiError::bad_request)?;

    let ((sender_ata, recipient_ata), sig) = state
        .sol
        .transfer(mint, sender, recipient, req.amount)
        .map_err(ApiError::solana)?;

    record_op(
        &state,
        "TRANSFER",
        req.mint.clone(),
        req.amount.to_string(),
        Some(req.sender_owner.clone()),
        Some(req.recipient_owner.clone()),
        Some(sig.to_string()),
        "SUCCESS",
        None,
    )
    .await;

    let event = serde_json::json!({
        "type": "TRANSFER",
        "mint": req.mint,
        "sender_owner": req.sender_owner,
        "recipient_owner": req.recipient_owner,
        "sender_ata": sender_ata.to_string(),
        "recipient_ata": recipient_ata.to_string(),
        "amount": req.amount.to_string(),
        "signature": sig.to_string(),
        "ts": chrono::Utc::now().to_rfc3339(),
    });
    let _ = state.kafka.publish_json(&sig.to_string(), &event).await;

    state.metrics.token_ops.with_label_values(&["transfer", "success"]).inc();
    state.metrics.solana_rpc_seconds.with_label_values(&["transfer"]).observe(started.elapsed().as_secs_f64());

    Ok(Json(TransferResponse {
        signature: sig.to_string(),
        sender_ata: sender_ata.to_string(),
        recipient_ata: recipient_ata.to_string(),
    }))
}

#[derive(Deserialize)]
pub struct BurnRequest {
    pub mint: String,
    pub owner: String,
    pub amount: u64,
}

#[derive(Serialize)]
pub struct BurnResponse {
    pub signature: String,
    pub owner_ata: String,
}

pub async fn burn(State(state): State<AppState>, Json(req): Json<BurnRequest>) -> Result<Json<BurnResponse>, ApiError> {
    let started = Instant::now();
    let mint = Pubkey::from_str(&req.mint).map_err(ApiError::bad_request)?;
    let owner = Pubkey::from_str(&req.owner).map_err(ApiError::bad_request)?;

    let (ata, sig) = state.sol.burn(mint, owner, req.amount).map_err(ApiError::solana)?;

    record_op(
        &state,
        "BURN",
        req.mint.clone(),
        req.amount.to_string(),
        Some(req.owner.clone()),
        None,
        Some(sig.to_string()),
        "SUCCESS",
        None,
    )
    .await;

    let event = serde_json::json!({
        "type": "BURN",
        "mint": req.mint,
        "owner": req.owner,
        "owner_ata": ata.to_string(),
        "amount": req.amount.to_string(),
        "signature": sig.to_string(),
        "ts": chrono::Utc::now().to_rfc3339(),
    });
    let _ = state.kafka.publish_json(&sig.to_string(), &event).await;

    state.metrics.token_ops.with_label_values(&["burn", "success"]).inc();
    state.metrics.solana_rpc_seconds.with_label_values(&["burn"]).observe(started.elapsed().as_secs_f64());

    Ok(Json(BurnResponse {
        signature: sig.to_string(),
        owner_ata: ata.to_string(),
    }))
}

async fn record_op(
    state: &AppState,
    op_type: &str,
    mint: String,
    amount: String,
    sender: Option<String>,
    recipient: Option<String>,
    signature: Option<String>,
    status: &str,
    error: Option<String>,
) {
    let _ = sqlx::query(
        "insert into token_operations (op_type, mint, amount, sender, recipient, signature, status, error)
         values ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(op_type)
    .bind(mint)
    .bind(amount)
    .bind(sender)
    .bind(recipient)
    .bind(signature)
    .bind(status)
    .bind(error)
    .execute(state.db.pool())
    .await;
}

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("solana error: {0}")]
    Solana(String),
    #[error("db error: {0}")]
    Db(String),
    #[error("internal error")]
    Internal,
}

impl ApiError {
    fn bad_request(e: impl ToString) -> Self {
        Self::BadRequest(e.to_string())
    }
    fn solana(e: impl ToString) -> Self {
        Self::Solana(e.to_string())
    }
    fn db(e: impl ToString) -> Self {
        Self::Db(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ApiError::Solana(m) => (StatusCode::BAD_GATEWAY, m.clone()),
            ApiError::Db(m) => (StatusCode::SERVICE_UNAVAILABLE, m.clone()),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal".to_string()),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

