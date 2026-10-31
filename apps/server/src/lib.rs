//! Symbiot server — runs on the home server `heimserver`, reachable only via Tailscale.
//! A replica and helper for the devices, never the owner of the data.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use symbiot_ai::Advisor;
use symbiot_core::{Edge, EdgeId, EdgeKind, LedgerEntry, Node, NodeId, NodeKind, Policy, Proposal};
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

struct AppState {
    store: Mutex<Store>,
    ai: Option<Arc<dyn Advisor>>,
}

type Shared = Arc<AppState>;

/// Routes: `/health`, `/api/*`, and — if `ui_dir` is set — the Leptos UI for everything else.
/// Without `ai` everything works except „Nächsten Schritt vorschlagen“.
pub fn app(store: Store, ui_dir: Option<PathBuf>, ai: Option<Arc<dyn Advisor>>) -> Router {
    let api = Router::new()
        .route("/nodes", get(list_nodes).post(create_node))
        .route(
            "/nodes/{id}",
            get(get_node).put(update_node).delete(delete_node),
        )
        .route("/nodes/{id}/history", get(history))
        .route("/nodes/{id}/policy", get(get_policy).put(set_policy))
        .route("/nodes/{id}/sensitive", put(set_sensitive))
        .route("/nodes/{id}/propose", post(propose))
        .route("/sensitive", get(list_sensitive))
        .route("/proposals/accept", post(accept_proposal))
        .route("/proposals/reject", post(reject_proposal))
        .route("/ai", get(ai_info))
        .route("/edges", get(list_edges).post(create_edge))
        .route("/edges/{id}", delete(unlink))
        .route("/ledger", get(ledger))
        .route("/ledger/verify", get(verify));

    let state = AppState {
        store: Mutex::new(store),
        ai,
    };
    let router = Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .with_state(Arc::new(state));

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
    /// E3: `deletable` and `exportable`, both true when left out.
    #[serde(flatten)]
    pub policy: Policy,
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
    let node = lock(&s).create_node_with(n.kind, &n.title, &n.body, n.policy)?;
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

#[derive(Serialize)]
pub struct Deleted {
    /// Entries whose content was removed (0 if the node is not deletable).
    pub pruned: usize,
}

/// E2: deletion marker; a deletable node also loses its content on this device.
async fn delete_node(
    State(s): State<Shared>,
    Path(id): Path<String>,
) -> Result<Json<Deleted>, ApiError> {
    let id = parse_id(&id)?;
    let pruned = lock(&s).delete_node(id)?;
    tracing::info!(%id, pruned, "node deleted");
    Ok(Json(Deleted { pruned }))
}

async fn get_policy(
    State(s): State<Shared>,
    Path(id): Path<String>,
) -> Result<Json<Policy>, ApiError> {
    Ok(Json(lock(&s).policy(parse_id(&id)?)?))
}

async fn set_policy(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Json(p): Json<Policy>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    lock(&s).set_policy(id, p)?;
    tracing::info!(%id, deletable = p.deletable, exportable = p.exportable, "policy set");
    Ok(StatusCode::NO_CONTENT)
}

async fn history(
    State(s): State<Shared>,
    Path(id): Path<String>,
) -> Result<Json<Vec<LedgerEntry>>, ApiError> {
    Ok(Json(lock(&s).history(parse_id(&id)?)?))
}

#[derive(Deserialize)]
pub struct NewEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
}

async fn list_edges(State(s): State<Shared>) -> Result<Json<Vec<Edge>>, ApiError> {
    Ok(Json(lock(&s).edges()?))
}

async fn unlink(State(s): State<Shared>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let id = EdgeId::from_hex(&id).ok_or(ApiError::bad("ungültige Verbindungs-ID"))?;
    lock(&s).unlink(id)?;
    tracing::info!(%id, "edge removed");
    Ok(StatusCode::NO_CONTENT)
}

async fn create_edge(
    State(s): State<Shared>,
    Json(e): Json<NewEdge>,
) -> Result<(StatusCode, Json<Edge>), ApiError> {
    let edge = lock(&s).link(e.from, e.to, e.kind)?;
    tracing::info!(id = %edge.id, kind = edge.kind.as_str(), "edge created");
    Ok((StatusCode::CREATED, Json(edge)))
}

#[derive(Deserialize)]
pub struct Sensitivity {
    pub sensitive: bool,
}

async fn set_sensitive(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Json(b): Json<Sensitivity>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    lock(&s).set_sensitive(id, b.sensitive)?;
    tracing::info!(%id, sensitive = b.sensitive, "sensitivity set");
    Ok(StatusCode::NO_CONTENT)
}

async fn list_sensitive(State(s): State<Shared>) -> Result<Json<Vec<NodeId>>, ApiError> {
    Ok(Json(lock(&s).sensitive_nodes()?))
}

async fn ai_info(State(s): State<Shared>) -> Json<symbiot_ai::Info> {
    Json(s.ai.as_ref().map(|a| a.info()).unwrap_or_default())
}

#[derive(Deserialize)]
pub struct ProposeRequest {
    /// Use the model outside the home network. Sensitive nodes are left out.
    #[serde(default)]
    pub remote: bool,
}

/// F4: asks the AI for the next step of a goal. Writes nothing — the answer is only a
/// proposal until the human accepts or rejects it.
async fn propose(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Json(r): Json<ProposeRequest>,
) -> Result<Json<Proposal>, ApiError> {
    let goal = parse_id(&id)?;
    let ai = s.ai.clone().ok_or(symbiot_ai::Error::NotConfigured)?;
    let ctx = {
        let store = lock(&s);
        let (nodes, edges, home_only) =
            (store.nodes(None)?, store.edges()?, store.home_only_nodes()?);
        symbiot_ai::context(goal, &nodes, &edges, &home_only, r.remote)?
    };
    let withheld = ctx.withheld;
    let proposal = tokio::task::spawn_blocking(move || ai.propose(&ctx))
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "interner Fehler".into()))??;
    // Only metadata in the log: what the AI said stays in the app.
    tracing::info!(%goal, model = %proposal.model, sources = proposal.sources.len(), withheld, "proposal made");
    Ok(Json(proposal))
}

#[derive(Serialize)]
pub struct Accepted {
    pub node: Node,
    pub edge: Edge,
}

async fn accept_proposal(
    State(s): State<Shared>,
    Json(p): Json<Proposal>,
) -> Result<(StatusCode, Json<Accepted>), ApiError> {
    let (node, edge) = lock(&s).accept_proposal(p)?;
    tracing::info!(id = %node.id, "proposal accepted");
    Ok((StatusCode::CREATED, Json(Accepted { node, edge })))
}

async fn reject_proposal(
    State(s): State<Shared>,
    Json(p): Json<Proposal>,
) -> Result<StatusCode, ApiError> {
    lock(&s).reject_proposal(p)?;
    tracing::info!("proposal rejected");
    Ok(StatusCode::NO_CONTENT)
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
    s.store.lock().unwrap_or_else(|p| p.into_inner())
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
            UnknownNode(_) | UnknownEdge(_) => ApiError(StatusCode::NOT_FOUND, e.to_string()),
            Sqlite(_) | Encoding(_) => {
                tracing::error!(error = %e, "store failure");
                ApiError(StatusCode::INTERNAL_SERVER_ERROR, "interner Fehler".into())
            }
        }
    }
}

impl From<symbiot_ai::Error> for ApiError {
    fn from(e: symbiot_ai::Error) -> Self {
        use symbiot_ai::Error::*;
        let status = match e {
            NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            UnknownGoal(_) => StatusCode::NOT_FOUND,
            NoRemote | SensitiveGoal => StatusCode::BAD_REQUEST,
            NoSources => StatusCode::UNPROCESSABLE_ENTITY,
            Http(_) | BadReply(_) => {
                tracing::warn!(error = %e, "ai failure");
                StatusCode::BAD_GATEWAY
            }
        };
        ApiError(status, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}
