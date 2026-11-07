//! Data access for the UI. Same calls, two backends:
//! inside Tauri (desktop/mobile) → `invoke` into the local store; in a browser → HTTP to `/api`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use symbiot_core::{
    Edge, EdgeId, EdgeKind, LedgerEntry, Node, NodeId, NodeKind, Policy, Proposal, PubKey,
    VerifyReport,
};
use wasm_bindgen::{JsCast, JsValue};

pub type ApiResult<T> = Result<T, String>;

pub async fn list_nodes(kind: Option<NodeKind>) -> ApiResult<Vec<Node>> {
    #[derive(Serialize)]
    struct Args {
        kind: Option<NodeKind>,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("list_nodes", &Args { kind }).await;
    }
    let url = match kind {
        Some(k) => format!("/api/nodes?kind={}", k.as_str()),
        None => "/api/nodes".into(),
    };
    http_json(gloo_net::http::Request::get(&url).build()).await
}

pub async fn create_node(
    kind: NodeKind,
    title: String,
    body: String,
    policy: Policy,
) -> ApiResult<Node> {
    // Flat fields, not `#[serde(flatten)]`: serde_wasm_bindgen would turn that into a JS Map.
    #[derive(Serialize)]
    struct Args {
        kind: NodeKind,
        title: String,
        body: String,
        deletable: bool,
        exportable: bool,
    }
    let args = Args {
        kind,
        title,
        body,
        deletable: policy.deletable,
        exportable: policy.exportable,
    };
    if tauri_invoke_fn().is_some() {
        return invoke("create_node", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/nodes").json(&args)).await
}

pub async fn update_node(id: NodeId, title: String, body: String) -> ApiResult<Node> {
    #[derive(Serialize)]
    struct Args {
        id: NodeId,
        title: String,
        body: String,
    }
    let args = Args { id, title, body };
    if tauri_invoke_fn().is_some() {
        return invoke("update_node", &args).await;
    }
    let url = format!("/api/nodes/{id}");
    http_json(gloo_net::http::Request::put(&url).json(&args)).await
}

/// E2: deletes a node. Returns how many entries lost their content (0 if not deletable).
pub async fn delete_node(id: NodeId) -> ApiResult<usize> {
    #[derive(Serialize)]
    struct Args {
        id: NodeId,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("delete_node", &Args { id }).await;
    }
    #[derive(Deserialize)]
    struct Deleted {
        pruned: usize,
    }
    let url = format!("/api/nodes/{id}");
    let d: Deleted = http_json(gloo_net::http::Request::delete(&url).build()).await?;
    Ok(d.pruned)
}

pub async fn node_policy(id: NodeId) -> ApiResult<Policy> {
    #[derive(Serialize)]
    struct Args {
        id: NodeId,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("node_policy", &Args { id }).await;
    }
    let url = format!("/api/nodes/{id}/policy");
    http_json(gloo_net::http::Request::get(&url).build()).await
}

/// E3: every change is a new entry in the chain.
pub async fn set_policy(id: NodeId, policy: Policy) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        id: NodeId,
        deletable: bool,
        exportable: bool,
    }
    if tauri_invoke_fn().is_some() {
        let args = Args {
            id,
            deletable: policy.deletable,
            exportable: policy.exportable,
        };
        return invoke("set_policy", &args).await;
    }
    let url = format!("/api/nodes/{id}/policy");
    http_empty(gloo_net::http::Request::put(&url).json(&policy)).await
}

pub async fn unlink(id: EdgeId) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        id: EdgeId,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("unlink_edge", &Args { id }).await;
    }
    let url = format!("/api/edges/{id}");
    http_empty(gloo_net::http::Request::delete(&url).build()).await
}

pub async fn node_history(id: NodeId) -> ApiResult<Vec<LedgerEntry>> {
    #[derive(Serialize)]
    struct Args {
        id: NodeId,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("node_history", &Args { id }).await;
    }
    let url = format!("/api/nodes/{id}/history");
    http_json(gloo_net::http::Request::get(&url).build()).await
}

pub async fn list_edges() -> ApiResult<Vec<Edge>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("list_edges", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/edges").build()).await
}

pub async fn link(from: NodeId, to: NodeId, kind: EdgeKind) -> ApiResult<Edge> {
    #[derive(Serialize)]
    struct Args {
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
    }
    let args = Args { from, to, kind };
    if tauri_invoke_fn().is_some() {
        return invoke("link_nodes", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/edges").json(&args)).await
}

pub async fn verify() -> ApiResult<VerifyReport> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("verify_ledger", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/ledger/verify").build()).await
}

/// M4: every chain known here, this device's first.
pub async fn verify_all() -> ApiResult<Vec<VerifyReport>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("verify_all", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/ledger/verify-all").build()).await
}

/// Mirrors `symbiot_store::DeviceInfo` (the store is native-only).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct DeviceInfo {
    pub key: PubKey,
    pub name: String,
    pub revoked_after: Option<u64>,
    pub entries: u64,
    pub last_ms: Option<u64>,
    pub this_device: bool,
}

/// E5: this device, the person it belongs to (if set up) and the person's devices.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PersonInfo {
    pub device: PubKey,
    pub person: Option<PubKey>,
    pub devices: Vec<DeviceInfo>,
}

pub async fn person() -> ApiResult<PersonInfo> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("person", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/person").build()).await
}

#[derive(Serialize, Deserialize)]
struct Words {
    words: String,
}

/// 24 new recovery words. Nothing is stored anywhere.
pub async fn new_words() -> ApiResult<String> {
    #[derive(Serialize)]
    struct NoArgs {}
    let w: Words = if tauri_invoke_fn().is_some() {
        invoke("new_words", &NoArgs {}).await?
    } else {
        http_json(gloo_net::http::Request::post("/api/person/words").build()).await?
    };
    Ok(w.words)
}

pub async fn certify(words: String, name: String) -> ApiResult<PersonInfo> {
    #[derive(Serialize)]
    struct Args {
        words: String,
        name: String,
    }
    let args = Args { words, name };
    if tauri_invoke_fn().is_some() {
        return invoke("certify", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/person/certify").json(&args)).await
}

pub async fn revoke(words: String, device: PubKey) -> ApiResult<PersonInfo> {
    #[derive(Serialize)]
    struct Args {
        words: String,
        device: PubKey,
    }
    let args = Args { words, device };
    if tauri_invoke_fn().is_some() {
        return invoke("revoke", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/person/revoke").json(&args)).await
}

/// Mirrors `symbiot_sync::Report`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct SyncReport {
    pub sent: usize,
    pub received: usize,
}

/// Where syncing with sz stands — only in the app; sz itself is where devices sync.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct SyncStatus {
    pub url: Option<String>,
    pub last_ok_ms: Option<u64>,
    pub last_error: Option<String>,
    pub last: Option<SyncReport>,
}

/// `None` in the browser: the web UI runs on sz, which is the hub.
pub async fn sync_status() -> ApiResult<Option<SyncStatus>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_none() {
        return Ok(None);
    }
    invoke("sync_status", &NoArgs {}).await.map(Some)
}

pub async fn sync_now() -> ApiResult<SyncStatus> {
    #[derive(Serialize)]
    struct NoArgs {}
    invoke("sync_now", &NoArgs {}).await
}

/// Saves the address of sz in the app and syncs right away. Empty switches syncing off.
pub async fn set_sync_url(url: String) -> ApiResult<SyncStatus> {
    #[derive(Serialize)]
    struct Args {
        url: String,
    }
    invoke("set_sync_url", &Args { url }).await
}

/// Whether this UI runs inside the app (local data) or in the browser on sz.
pub fn in_app() -> bool {
    tauri_invoke_fn().is_some()
}

/// Mirrors `symbiot_ai::Info` (that crate is native-only).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct AiInfo {
    pub available: bool,
    pub local_model: Option<String>,
    pub remote_model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Accepted {
    pub node: Node,
    pub edge: Edge,
}

pub async fn ai_info() -> ApiResult<AiInfo> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("ai_info", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/ai").build()).await
}

/// F4: asks the AI for the next step. Writes nothing.
pub async fn propose(goal: NodeId, remote: bool) -> ApiResult<Proposal> {
    #[derive(Serialize)]
    struct Args {
        goal: NodeId,
        remote: bool,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("propose_next", &Args { goal, remote }).await;
    }
    #[derive(Serialize)]
    struct Body {
        remote: bool,
    }
    let url = format!("/api/nodes/{goal}/propose");
    http_json(gloo_net::http::Request::post(&url).json(&Body { remote })).await
}

pub async fn accept_proposal(proposal: Proposal) -> ApiResult<Accepted> {
    #[derive(Serialize)]
    struct Args {
        proposal: Proposal,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("accept_proposal", &Args { proposal }).await;
    }
    http_json(gloo_net::http::Request::post("/api/proposals/accept").json(&proposal)).await
}

pub async fn reject_proposal(proposal: Proposal) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        proposal: Proposal,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("reject_proposal", &Args { proposal }).await;
    }
    http_empty(gloo_net::http::Request::post("/api/proposals/reject").json(&proposal)).await
}

pub async fn list_sensitive() -> ApiResult<Vec<NodeId>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("list_sensitive", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/sensitive").build()).await
}

pub async fn set_sensitive(id: NodeId, sensitive: bool) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        id: NodeId,
        sensitive: bool,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("set_sensitive", &Args { id, sensitive }).await;
    }
    #[derive(Serialize)]
    struct Body {
        sensitive: bool,
    }
    let url = format!("/api/nodes/{id}/sensitive");
    http_empty(gloo_net::http::Request::put(&url).json(&Body { sensitive })).await
}

/// Where the data lives — shown in the footer so it is always clear.
pub fn backend_label() -> &'static str {
    if tauri_invoke_fn().is_some() {
        "lokal auf diesem Gerät"
    } else {
        "Server"
    }
}

fn tauri_invoke_fn() -> Option<js_sys::Function> {
    let internals = js_sys::Reflect::get(&js_sys::global(), &"__TAURI_INTERNALS__".into()).ok()?;
    if internals.is_undefined() {
        return None;
    }
    js_sys::Reflect::get(&internals, &"invoke".into())
        .ok()?
        .dyn_into()
        .ok()
}

async fn invoke<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> ApiResult<T> {
    let f = tauri_invoke_fn().ok_or("Tauri nicht verfügbar")?;
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    let promise: js_sys::Promise = f
        .call2(&JsValue::NULL, &cmd.into(), &args)
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let value = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}

fn js_error(v: JsValue) -> String {
    v.as_string().unwrap_or_else(|| format!("{v:?}"))
}

/// For calls that answer „204 No Content“.
async fn http_empty(req: Result<gloo_net::http::Request, gloo_net::Error>) -> ApiResult<()> {
    let res = req
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| format!("Server nicht erreichbar: {e}"))?;
    if res.ok() {
        return Ok(());
    }
    Err(error_of(res).await)
}

async fn error_of(res: gloo_net::http::Response) -> String {
    #[derive(serde::Deserialize)]
    struct ErrBody {
        error: String,
    }
    match res.json::<ErrBody>().await {
        Ok(b) => b.error,
        Err(_) => format!("Fehler {}", res.status()),
    }
}

async fn http_json<T: DeserializeOwned>(
    req: Result<gloo_net::http::Request, gloo_net::Error>,
) -> ApiResult<T> {
    let res = req
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| format!("Server nicht erreichbar: {e}"))?;
    if res.ok() {
        return res.json().await.map_err(|e| e.to_string());
    }
    Err(error_of(res).await)
}
