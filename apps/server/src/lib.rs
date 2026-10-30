//! Symbiot server — runs on the home server `heimserver`, reachable only via Tailscale.
//! A replica and helper for the devices, never the owner of the data.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use symbiot_core::{LedgerEntry, Node, NodeId, NodeKind};
use symbiot_store::{Store, VerifyReport};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

#[derive(Serialize)]
pub struct Health {
    pub ok: bool,
    pub app: &'static str,
    pub version: &'static str,
    pub core: &'static str,
    pub protocol: u16,
}

type Shared = Arc<Mutex<Store>>;

/// Routes: `/health`, `/api/*`, and — if `ui_dir` is set — the Leptos UI for everything else.
pub fn app(store: Store, ui_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/nodes", get(list_nodes).post(create_node))
        .route("/nodes/{id}", get(get_node).put(update_node))
        .route("/ledger", get(ledger))
        .route("/ledger/verify", get(verify));

    let router = Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .with_state(Arc::new(Mutex::new(store)));

    let router = match ui_dir {
        Some(dir) => {
            let index = dir.join("index.html");
            router.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
        }
        None => router,
    };
    router.layer(TraceLayer::new_for_http())
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        app: "symbiot",
        version: env!("CARGO_PKG_VERSION"),
        core: symbiot_core::VERSION,
        protocol: symbiot_core::PROTOCOL_VERSION,
    })
}

#[derive(Deserialize)]
struct KindFilter {
    kind: Option<String>,
}

#[derive(Deserialize)]
pub struct NewNode {
    pub kind: NodeKind,
    pub title: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Deserialize)]
pub struct EditNode {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

async fn list_nodes(
    State(s): State<Shared>,
    Query(f): Query<KindFilter>,
) -> Result<Json<Vec<Node>>, ApiError> {
    let kind = match f.kind.as_deref() {
        None | Some("") => None,
        Some(k) => Some(NodeKind::parse(k).ok_or(ApiError::bad("unbekannte Art"))?),
    };
    Ok(Json(lock(&s).nodes(kind)?))
}

async fn create_node(
    State(s): State<Shared>,
    Json(n): Json<NewNode>,
) -> Result<(StatusCode, Json<Node>), ApiError> {
    let node = lock(&s).create_node(n.kind, &n.title, &n.body)?;
    tracing::info!(id = %node.id, kind = node.kind.as_str(), "node created");
    Ok((StatusCode::CREATED, Json(node)))
}

async fn get_node(State(s): State<Shared>, Path(id): Path<String>) -> Result<Json<Node>, ApiError> {
    let id = parse_id(&id)?;
    lock(&s).node(id)?.map(Json).ok_or(ApiError(
        StatusCode::NOT_FOUND,
        "Knoten nicht gefunden".into(),
    ))
}

async fn update_node(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Json(e): Json<EditNode>,
) -> Result<Json<Node>, ApiError> {
    let node = lock(&s).update_node(parse_id(&id)?, &e.title, &e.body)?;
    tracing::info!(id = %node.id, version = node.version, "node updated");
    Ok(Json(node))
}

async fn ledger(State(s): State<Shared>) -> Result<Json<Vec<LedgerEntry>>, ApiError> {
    Ok(Json(lock(&s).entries()?))
}

async fn verify(State(s): State<Shared>) -> Result<Json<VerifyReport>, ApiError> {
    let report = lock(&s).verify()?;
    if !report.ok {
        // ERROR level so Dozzle raises an alert: a broken chain must never go unnoticed.
        tracing::error!(error = ?report.error, "ledger verification failed");
    }
    Ok(Json(report))
}

fn lock(s: &Shared) -> MutexGuard<'_, Store> {
    // Writes are transactions, so a panic while holding the lock leaves SQLite consistent.
    s.lock().unwrap_or_else(|p| p.into_inner())
}

fn parse_id(id: &str) -> Result<NodeId, ApiError> {
    NodeId::from_hex(id).ok_or(ApiError::bad("ungültige Knoten-ID"))
}

pub struct ApiError(StatusCode, String);

impl ApiError {
    fn bad(msg: &str) -> Self {
        ApiError(StatusCode::BAD_REQUEST, msg.into())
    }
}

impl From<symbiot_store::Error> for ApiError {
    fn from(e: symbiot_store::Error) -> Self {
        use symbiot_store::Error::*;
        match e {
            Invalid(_) => ApiError(StatusCode::BAD_REQUEST, e.to_string()),
            UnknownNode(_) => ApiError(StatusCode::NOT_FOUND, e.to_string()),
            Sqlite(_) | Encoding(_) => {
                tracing::error!(error = %e, "store failure");
                ApiError(StatusCode::INTERNAL_SERVER_ERROR, "interner Fehler".into())
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}
