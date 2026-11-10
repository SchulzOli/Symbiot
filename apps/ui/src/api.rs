//! Data access for the UI. Same calls, two backends:
//! inside Tauri (desktop/mobile) → `invoke` into the local store; in a browser → HTTP to `/api`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use symbiot_core::source::Grant;
use symbiot_core::{
    Edge, EdgeId, EdgeKind, Evidence, Hash, LedgerEntry, Node, NodeId, NodeKind, Policy, Proposal,
    PubKey, Rule, RuleId, VerifyReport,
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
    #[serde(default)]
    pub files_sent: usize,
    #[serde(default)]
    pub files_received: usize,
}

/// Where syncing with the home server stands — only in the app; the home server itself is where devices sync.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct SyncStatus {
    pub url: Option<String>,
    pub last_ok_ms: Option<u64>,
    pub last_error: Option<String>,
    pub last: Option<SyncReport>,
}

/// `None` in the browser: the web UI runs on the home server, which is the hub.
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

/// Saves the address of the home server in the app and syncs right away. Empty switches syncing off.
pub async fn set_sync_url(url: String) -> ApiResult<SyncStatus> {
    #[derive(Serialize)]
    struct Args {
        url: String,
    }
    invoke("set_sync_url", &Args { url }).await
}

/// Whether this UI runs inside the app (local data) or in the browser on the home server.
pub fn in_app() -> bool {
    tauri_invoke_fn().is_some()
}

/// Mirrors `symbiot_ai::RuleProposal` (E4): a rule the AI suggests, not saved yet.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RuleProposal {
    pub label: String,
    pub keywords: Vec<String>,
    pub matches: Vec<NodeId>,
    pub model: String,
}

/// Mirrors `symbiot_store::Sensitivity`: why a node is sensitive or not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Sensitivity {
    pub sensitive: bool,
    pub by_hand: Option<bool>,
    pub rules: Vec<String>,
}

/// #30: one piece of raw data (E14) as the store returns it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct StoredEvidence {
    pub key: Hash,
    pub entry: Hash,
    pub author: PubKey,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Origin {
    pub origin: String,
    pub count: usize,
}

/// Which raw data to load; empty fields do not filter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct EvidenceFilter {
    /// #31: any identifier of a person; the store widens it to all of theirs.
    pub person: Option<String>,
    pub q: Option<String>,
    pub origin: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub async fn list_evidence(filter: EvidenceFilter) -> ApiResult<Vec<StoredEvidence>> {
    #[derive(Serialize)]
    struct Args {
        filter: EvidenceFilter,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("list_evidence", &Args { filter }).await;
    }
    let mut params = Vec::new();
    let mut add = |key: &str, value: String| {
        params.push(format!(
            "{key}={}",
            String::from(js_sys::encode_uri_component(&value))
        ))
    };
    if let Some(q) = filter.q {
        add("q", q);
    }
    if let Some(o) = filter.origin {
        add("origin", o);
    }
    if let Some(p) = filter.person {
        add("person", p);
    }
    if let Some(l) = filter.limit {
        add("limit", l.to_string());
    }
    if let Some(o) = filter.offset {
        add("offset", o.to_string());
    }
    let url = format!("/api/evidence?{}", params.join("&"));
    http_json(gloo_net::http::Request::get(&url).build()).await
}

/// E16: what became of an accepted suggestion.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Outcome {
    pub executed: Vec<Executed>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Executed {
    pub index: usize,
    pub node: Option<NodeId>,
}

/// E16: the human's request in words; the AI answers with a suggestion of actions.
pub async fn assist(
    instruction: String,
    remote: bool,
) -> ApiResult<symbiot_core::action::Suggestion> {
    #[derive(Serialize)]
    struct Args {
        instruction: String,
        remote: bool,
    }
    let args = Args {
        instruction,
        remote,
    };
    if tauri_invoke_fn().is_some() {
        return invoke("assist", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/assist").json(&args)).await
}

/// E16: the human decides; `chosen` are the indexes of the actions to carry out.
pub async fn decide_suggestion(
    suggestion: symbiot_core::action::Suggestion,
    accept: bool,
    chosen: Vec<usize>,
) -> ApiResult<Outcome> {
    #[derive(Serialize)]
    struct Args {
        suggestion: symbiot_core::action::Suggestion,
        accept: bool,
        chosen: Vec<usize>,
    }
    let args = Args {
        suggestion,
        accept,
        chosen,
    };
    if tauri_invoke_fn().is_some() {
        return invoke("decide_suggestion", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/suggestions/decide").json(&args)).await
}

/// #31: one identifier of a person.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Identifier {
    pub id: String,
    pub names: Vec<String>,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub identifiers: Vec<Identifier>,
    pub count: usize,
    pub first_ms: u64,
    pub last_ms: u64,
    pub origins: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Suggestion {
    pub a: Person,
    pub b: Person,
    pub reason: String,
}

pub async fn list_persons() -> ApiResult<Vec<Person>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("list_persons", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/persons").build()).await
}

pub async fn person_suggestions() -> ApiResult<Vec<Suggestion>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("person_suggestions", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/persons/suggestions").build()).await
}

/// The human decides: these identifiers are one person.
pub async fn merge_identities(ids: Vec<String>, name: String) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        ids: Vec<String>,
        name: String,
    }
    let args = Args { ids, name };
    if tauri_invoke_fn().is_some() {
        return invoke("merge_identities", &args).await;
    }
    http_empty(gloo_net::http::Request::post("/api/persons/merge").json(&args)).await
}

pub async fn split_identity(id: String) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        id: String,
    }
    let args = Args { id };
    if tauri_invoke_fn().is_some() {
        return invoke("split_identity", &args).await;
    }
    http_empty(gloo_net::http::Request::post("/api/persons/split").json(&args)).await
}

pub async fn keep_apart(a: String, b: String) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        a: String,
        b: String,
    }
    let args = Args { a, b };
    if tauri_invoke_fn().is_some() {
        return invoke("keep_apart", &args).await;
    }
    http_empty(gloo_net::http::Request::post("/api/persons/apart").json(&args)).await
}

/// #44: a source as the person manages it (see `symbiot_store::SourceInfo`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SourceInfo {
    pub source: String,
    pub grant: Grant,
    pub decided_ms: Option<u64>,
    pub revoked_ms: Option<u64>,
    pub items: usize,
    pub used: usize,
    pub first_ms: Option<u64>,
    pub last_ms: Option<u64>,
    pub loaded_ms: Option<u64>,
    pub adapters: Vec<String>,
    pub scopes: Vec<Scope>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Scope {
    pub name: String,
    pub items: usize,
    pub excluded: bool,
}

pub async fn list_sources() -> ApiResult<Vec<SourceInfo>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("list_sources", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/sources").build()).await
}

pub async fn grant_source(source: String, grant: Grant) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        source: String,
        grant: Grant,
    }
    let args = Args { source, grant };
    if tauri_invoke_fn().is_some() {
        return invoke("grant_source", &args).await;
    }
    http_empty(gloo_net::http::Request::post("/api/sources/grant").json(&args)).await
}

/// What an import did (see `symbiot_store::Recorded`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Recorded {
    pub added: usize,
    pub known: usize,
    pub entries: usize,
}

/// #44: an export chosen in the app. The file goes as raw bytes, never through JSON.
pub async fn import_file(
    source: &str,
    name: &str,
    chat: &str,
    utc_offset: i32,
    bytes: js_sys::Uint8Array,
) -> ApiResult<Recorded> {
    let enc = |v: &str| String::from(js_sys::encode_uri_component(v));
    if let Some(f) = tauri_invoke_fn() {
        let headers = js_sys::Object::new();
        for (k, v) in [
            ("x-source", enc(source)),
            ("x-name", enc(name)),
            ("x-chat", enc(chat)),
            ("x-utc-offset", utc_offset.to_string()),
        ] {
            js_sys::Reflect::set(&headers, &k.into(), &v.into()).map_err(js_error)?;
        }
        let opts = js_sys::Object::new();
        js_sys::Reflect::set(&opts, &"headers".into(), &headers).map_err(js_error)?;
        let promise: js_sys::Promise = f
            .call3(&JsValue::NULL, &"import_file".into(), &bytes, &opts)
            .map_err(js_error)?
            .dyn_into()
            .map_err(js_error)?;
        let value = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(js_error)?;
        return serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string());
    }
    let url = format!(
        "/api/import?source={}&name={}&chat={}&utc_offset={utc_offset}",
        enc(source),
        enc(name),
        enc(chat)
    );
    http_json(
        gloo_net::http::Request::post(&url)
            .header("content-type", "application/octet-stream")
            .body(JsValue::from(bytes)),
    )
    .await
}

/// Returns how many items lost their raw data.
pub async fn revoke_source(source: String) -> ApiResult<usize> {
    #[derive(Serialize)]
    struct Args {
        source: String,
    }
    let args = Args { source };
    if tauri_invoke_fn().is_some() {
        return invoke("revoke_source", &args).await;
    }
    #[derive(Deserialize)]
    struct Revoked {
        removed: usize,
    }
    let r: Revoked =
        http_json(gloo_net::http::Request::post("/api/sources/revoke").json(&args)).await?;
    Ok(r.removed)
}

/// #28: types the UI shows as a picture. The same list as the server's inline types.
pub fn is_picture(mime: &str) -> bool {
    matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

/// Where the browser loads a file from. `None` in the app: there files come by `invoke`.
pub fn file_url(hash: &Hash) -> Option<String> {
    tauri_invoke_fn()
        .is_none()
        .then(|| format!("/api/files/{hash}"))
}

/// In the app: a picture as `data:` URL, `None` if it is not here (yet) or too large.
pub async fn file_data_url(hash: Hash) -> ApiResult<Option<String>> {
    #[derive(Serialize)]
    struct Args {
        hash: Hash,
    }
    invoke("file_data_url", &Args { hash }).await
}

pub async fn evidence_origins() -> ApiResult<Vec<Origin>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("evidence_origins", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/evidence/origins").build()).await
}

pub async fn list_rules() -> ApiResult<Vec<Rule>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("list_rules", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/rules").build()).await
}

/// E4: the human accepts a rule — their own (`model: None`) or one the AI suggested.
pub async fn add_rule(
    label: String,
    keywords: Vec<String>,
    model: Option<String>,
) -> ApiResult<Rule> {
    #[derive(Serialize)]
    struct Args {
        label: String,
        keywords: Vec<String>,
        model: Option<String>,
    }
    let args = Args {
        label,
        keywords,
        model,
    };
    if tauri_invoke_fn().is_some() {
        return invoke("add_rule", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/rules").json(&args)).await
}

pub async fn remove_rule(id: RuleId) -> ApiResult<()> {
    #[derive(Serialize)]
    struct Args {
        id: RuleId,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("remove_rule", &Args { id }).await;
    }
    let url = format!("/api/rules/{id}");
    http_empty(gloo_net::http::Request::delete(&url).build()).await
}

/// E4: the local model suggests rules. Writes nothing.
pub async fn suggest_rules() -> ApiResult<Vec<RuleProposal>> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("suggest_rules", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::post("/api/rules/suggest").build()).await
}

pub async fn node_sensitivity(id: NodeId) -> ApiResult<Sensitivity> {
    #[derive(Serialize)]
    struct Args {
        id: NodeId,
    }
    if tauri_invoke_fn().is_some() {
        return invoke("node_sensitivity", &Args { id }).await;
    }
    let url = format!("/api/nodes/{id}/sensitivity");
    http_json(gloo_net::http::Request::get(&url).build()).await
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
