//! HTTP routes.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use zip311::{Network, PaymentDisclosure, VerifyContext, NOT_REVEALED};

use crate::chain::{Chain, ChainError};
use crate::challenges::Store;
use crate::demo::Demo;

pub struct AppState {
    pub chain: Chain,
    pub vk: orchard::circuit::VerifyingKey,
    pub tickets: Store,
    pub demo: Option<Demo>,
    pub app_origin: String,
    pub gallery: Value,
    pub limiter: Limiter,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/verify", post(verify))
        .route("/v1/tx/{network}/{txid}", get(transaction))
        .route("/v1/block/{network}/{height}", get(block))
        .route("/v1/challenges", post(create_challenge))
        .route("/v1/challenges/{id}", get(get_challenge))
        .route("/v1/challenges/{id}/answer", post(answer_challenge))
        .route("/v1/demo", get(demo_info))
        .route("/v1/demo/prove", post(demo_prove))
        .route("/v1/demo/forge", post(demo_forge))
        .route("/v1/gallery", get(gallery))
        .with_state(state)
}

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl ToString) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.to_string())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn parse_network(s: &str) -> Result<Network, ApiError> {
    match s {
        "main" | "mainnet" => Ok(Network::Main),
        "test" | "testnet" => Ok(Network::Test),
        _ => Err(bad("network must be main or test")),
    }
}

/// Per-client fixed-window counters for the endpoints that cost real work.
pub struct Limiter {
    windows: Mutex<HashMap<(IpAddr, &'static str), (Instant, u32)>>,
}

impl Limiter {
    pub fn new() -> Self {
        Limiter { windows: Mutex::new(HashMap::new()) }
    }

    fn check(&self, ip: IpAddr, bucket: &'static str, per_minute: u32) -> Result<(), ApiError> {
        let mut w = self.windows.lock().unwrap();
        if w.len() > 50_000 {
            w.retain(|_, (start, _)| start.elapsed() < Duration::from_secs(60));
        }
        let entry = w.entry((ip, bucket)).or_insert((Instant::now(), 0));
        if entry.0.elapsed() >= Duration::from_secs(60) {
            *entry = (Instant::now(), 0);
        }
        entry.1 += 1;
        if entry.1 > per_minute {
            return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "slow down: too many requests this minute".into()));
        }
        Ok(())
    }
}

/// The caller's address: the proxy's `x-forwarded-for` if present, else the peer.
fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(peer.ip())
}

async fn health(State(s): State<Arc<AppState>>) -> Json<Value> {
    let (main, test) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(5), s.chain.tip(Network::Main)),
        tokio::time::timeout(Duration::from_secs(5), s.chain.tip(Network::Test)),
    );
    let tip = |r: Result<Result<u32, ChainError>, _>| match r {
        Ok(Ok(h)) => json!(h),
        _ => Value::Null,
    };
    Json(json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "tips": { "main": tip(main), "test": tip(test) },
        "demo": s.demo.is_some(),
    }))
}

#[derive(Deserialize)]
struct VerifyRequest {
    disclosure: String,
    challenge: Option<String>,
}

impl AppState {
    /// Verifies a decoded disclosure against the chain and returns the verdict
    /// as JSON, with the fixed list of what a disclosure never reveals.
    pub async fn verdict(self: &Arc<Self>, d: PaymentDisclosure, expected: Option<Vec<u8>>) -> Result<Value, ApiError> {
        let found = match self.chain.transaction(d.network, d.txid).await {
            Ok(t) => Some(t),
            Err(ChainError::NotFound) => None,
            Err(e) => return Err(ApiError(StatusCode::BAD_GATEWAY, e.to_string())),
        };
        let Some(found) = found else {
            return Ok(json!({
                "valid": false,
                "network": d.network,
                "txid": d.txid_display(),
                "height": d.height,
                "challenge_text": std::str::from_utf8(&d.msg).ok(),
                "challenge": zip311::Challenge::parse(&d.msg),
                "checks": [{ "id": "mined", "label": "Mined", "status": "fail", "detail": "the chain has no transaction with this txid" }],
                "revealed": null,
                "not_revealed": NOT_REVEALED,
            }));
        };
        let tx = found.parse(d.network, d.height).map_err(|e| bad(e.to_string()))?;
        let state = self.clone();
        let verdict = tokio::task::spawn_blocking(move || {
            let cx = VerifyContext { network: d.network, expected_msg: expected.as_deref(), vk: &state.vk, now: now() };
            zip311::verify(&d, &tx, found.height, &cx)
        })
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let mut v = serde_json::to_value(verdict).expect("verdict serializes");
        v["not_revealed"] = json!(NOT_REVEALED);
        Ok(v)
    }
}

fn decode(text: &str) -> Result<PaymentDisclosure, ApiError> {
    let text = text.trim();
    let text = text.rsplit_once('#').map(|(_, t)| t).unwrap_or(text);
    PaymentDisclosure::decode(text).map_err(|e| bad(e))
}

async fn verify(State(s): State<Arc<AppState>>, Json(req): Json<VerifyRequest>) -> Result<Json<Value>, ApiError> {
    let d = decode(&req.disclosure)?;
    Ok(Json(s.verdict(d, req.challenge.map(String::into_bytes)).await?))
}

async fn transaction(State(s): State<Arc<AppState>>, Path((network, txid)): Path<(String, String)>) -> Result<Json<Value>, ApiError> {
    let network = parse_network(&network)?;
    let mut bytes: [u8; 32] = hex::decode(&txid).ok().and_then(|b| b.try_into().ok()).ok_or_else(|| bad("txid must be 64 hex characters"))?;
    // Accept the display order explorers use.
    bytes.reverse();
    match s.chain.transaction(network, bytes).await {
        Ok(t) => Ok(Json(json!({ "txid": txid, "height": t.height, "raw": hex::encode(&t.raw) }))),
        Err(ChainError::NotFound) => Err(ApiError(StatusCode::NOT_FOUND, "the chain has no transaction with this txid".into())),
        Err(e) => Err(ApiError(StatusCode::BAD_GATEWAY, e.to_string())),
    }
}

async fn block(State(s): State<Arc<AppState>>, Path((network, height)): Path<(String, u32)>) -> Result<Json<Value>, ApiError> {
    let network = parse_network(&network)?;
    let txids = s.chain.block_txids(network, height).await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({ "height": height, "txids": txids })))
}

#[derive(Deserialize)]
struct NewChallenge {
    verifier: String,
    context: String,
    ttl_seconds: Option<u64>,
    network: Option<String>,
}

fn ticket_json(s: &AppState, t: &crate::challenges::Ticket) -> Value {
    json!({
        "id": t.id,
        "network": t.network,
        "msg": t.msg,
        "verifier": t.verifier,
        "context": t.context,
        "created": t.created,
        "expires": t.expires,
        "answer_url": format!("{}/answer/{}", s.app_origin, t.id),
        "status": if t.answered.is_some() { "answered" } else { "pending" },
        "answered": t.answered,
        "disclosure": t.disclosure,
        "verdict": t.verdict,
    })
}

async fn create_challenge(
    State(s): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<NewChallenge>,
) -> Result<Json<Value>, ApiError> {
    s.limiter.check(client_ip(&headers, peer), "challenge", 20)?;
    let network = parse_network(req.network.as_deref().unwrap_or("main"))?;
    let ttl = req.ttl_seconds.unwrap_or(24 * 3600).clamp(60, 7 * 24 * 3600);
    if req.verifier.trim().is_empty() || req.context.trim().is_empty() {
        return Err(bad("say who is asking and what the proof is for"));
    }
    let t = s.tickets.create(network, &req.verifier, &req.context, ttl, now()).map_err(bad)?;
    Ok(Json(ticket_json(&s, &t)))
}

async fn get_challenge(State(s): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let t = s.tickets.get(&id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such challenge".into()))?;
    Ok(Json(ticket_json(&s, &t)))
}

#[derive(Deserialize)]
struct Answer {
    disclosure: String,
}

async fn answer_challenge(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<Answer>,
) -> Result<Json<Value>, ApiError> {
    s.limiter.check(client_ip(&headers, peer), "answer", 30)?;
    let t = s.tickets.get(&id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such challenge".into()))?;
    let d = decode(&req.disclosure)?;
    if d.network != t.network {
        return Err(bad(format!("this challenge is for {}, the disclosure is for {}", t.network, d.network)));
    }
    let verdict = s.verdict(d, Some(t.msg.clone().into_bytes())).await?;
    let t = s.tickets.answer(&id, req.disclosure.trim().to_string(), verdict, now()).expect("ticket exists");
    Ok(Json(ticket_json(&s, &t)))
}

async fn demo_info(State(s): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let demo = s.demo.as_ref().ok_or_else(|| ApiError(StatusCode::SERVICE_UNAVAILABLE, "the live demo is not configured".into()))?;
    Ok(Json(demo.info()))
}

#[derive(Deserialize)]
struct DemoProve {
    challenge: String,
    payment: Option<usize>,
}

async fn demo_prove(
    State(s): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<DemoProve>,
) -> Result<Json<Value>, ApiError> {
    s.limiter.check(client_ip(&headers, peer), "demo", 12)?;
    if req.challenge.len() > 512 {
        return Err(bad("keep the challenge under 512 bytes"));
    }
    let demo = s.demo.as_ref().ok_or_else(|| ApiError(StatusCode::SERVICE_UNAVAILABLE, "the live demo is not configured".into()))?;
    let out = demo.prove(&s.chain, req.challenge.into_bytes(), req.payment.unwrap_or(0)).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct DemoForge {
    challenge: String,
    payment: Option<usize>,
}

/// The recipient of a demo payment tries to disclose it as if they had sent
/// it. The prover refuses before anything is produced; the reason is returned.
async fn demo_forge(
    State(s): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<DemoForge>,
) -> Result<Json<Value>, ApiError> {
    s.limiter.check(client_ip(&headers, peer), "demo", 12)?;
    let demo = s.demo.as_ref().ok_or_else(|| ApiError(StatusCode::SERVICE_UNAVAILABLE, "the live demo is not configured".into()))?;
    let out = demo.forge_as_recipient(&s.chain, req.challenge.into_bytes(), req.payment.unwrap_or(0)).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(out))
}

async fn gallery(State(s): State<Arc<AppState>>) -> Json<Value> {
    Json(s.gallery.clone())
}
