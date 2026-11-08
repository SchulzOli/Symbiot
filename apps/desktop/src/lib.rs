//! Symbiot desktop/mobile shell: the shared Leptos UI plus a local store.
//! Data lives on this device (app data dir); the UI reaches it via `invoke`.

use std::sync::{Arc, Mutex};

use symbiot_ai::{Advisor, Info};
use symbiot_core::{
    Edge, EdgeId, EdgeKind, Hash, LedgerEntry, Node, NodeId, NodeKind, Policy, Proposal, PubKey,
    Rule, RuleId, VerifyReport,
};
use symbiot_store::{
    DeviceInfo, EvidenceQuery, Origin, Person, Sensitivity, Store, StoredEvidence, Suggestion,
};
use tauri::{Manager, State};

struct AppStore(Arc<Mutex<Store>>);

impl AppStore {
    fn with<T>(&self, f: impl FnOnce(&mut Store) -> symbiot_store::Result<T>) -> Result<T, String> {
        let mut store = self.0.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut store).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn list_nodes(store: State<AppStore>, kind: Option<NodeKind>) -> Result<Vec<Node>, String> {
    store.with(|s| s.nodes(kind))
}

#[tauri::command]
fn create_node(
    store: State<AppStore>,
    kind: NodeKind,
    title: String,
    body: String,
    deletable: Option<bool>,
    exportable: Option<bool>,
) -> Result<Node, String> {
    let policy = Policy {
        deletable: deletable.unwrap_or(true),
        exportable: exportable.unwrap_or(true),
    };
    store.with(|s| s.create_node_with(kind, &title, &body, policy))
}

/// E2: returns how many entries lost their content.
#[tauri::command]
fn delete_node(store: State<AppStore>, id: NodeId) -> Result<usize, String> {
    store.with(|s| s.delete_node(id))
}

#[tauri::command]
fn node_policy(store: State<AppStore>, id: NodeId) -> Result<Policy, String> {
    store.with(|s| s.policy(id))
}

#[tauri::command]
fn set_policy(
    store: State<AppStore>,
    id: NodeId,
    deletable: bool,
    exportable: bool,
) -> Result<(), String> {
    store.with(|s| {
        s.set_policy(
            id,
            Policy {
                deletable,
                exportable,
            },
        )
    })
}

#[tauri::command]
fn unlink_edge(store: State<AppStore>, id: EdgeId) -> Result<(), String> {
    store.with(|s| s.unlink(id))
}

#[tauri::command]
fn update_node(
    store: State<AppStore>,
    id: NodeId,
    title: String,
    body: String,
) -> Result<Node, String> {
    store.with(|s| s.update_node(id, &title, &body))
}

#[tauri::command]
fn node_history(store: State<AppStore>, id: NodeId) -> Result<Vec<LedgerEntry>, String> {
    store.with(|s| s.history(id))
}

#[tauri::command]
fn list_edges(store: State<AppStore>) -> Result<Vec<Edge>, String> {
    store.with(|s| s.edges())
}

#[tauri::command]
fn link_nodes(
    store: State<AppStore>,
    from: NodeId,
    to: NodeId,
    kind: EdgeKind,
) -> Result<Edge, String> {
    store.with(|s| s.link(from, to, kind))
}

/// The AI the desktop asks: `SYMBIOT_AI_*` from the environment, e.g. LiteLLM on the home server via Tailscale.
struct Ai(Option<Arc<dyn Advisor>>);

#[tauri::command]
fn ai_info(ai: State<Ai>) -> Info {
    ai.0.as_ref().map(|a| a.info()).unwrap_or_default()
}

/// F4: asks the AI for the next step. Writes nothing; runs off the UI thread.
#[tauri::command]
async fn propose_next(
    store: State<'_, AppStore>,
    ai: State<'_, Ai>,
    goal: NodeId,
    remote: bool,
) -> Result<Proposal, String> {
    let not_configured = || symbiot_ai::Error::NotConfigured.to_string();
    let advisor = ai.0.clone().ok_or_else(not_configured)?;
    let (nodes, edges, home_only) =
        store.with(|s| Ok((s.nodes(None)?, s.edges()?, s.home_only_nodes()?)))?;
    let ctx =
        symbiot_ai::context(goal, &nodes, &edges, &home_only, remote).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || advisor.propose(&ctx))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// E16: the human's request becomes a suggestion of actions. Nothing is written.
#[tauri::command]
async fn assist(
    store: State<'_, AppStore>,
    ai: State<'_, Ai>,
    instruction: String,
    remote: bool,
) -> Result<symbiot_core::action::Suggestion, String> {
    let advisor =
        ai.0.clone()
            .ok_or_else(|| symbiot_ai::Error::NotConfigured.to_string())?;
    let (nodes, edges, rules, home_only) =
        store.with(|s| Ok((s.nodes(None)?, s.edges()?, s.rules()?, s.home_only_nodes()?)))?;
    let ctx = symbiot_ai::assist_context(&instruction, &nodes, &edges, &rules, &home_only, remote);
    tauri::async_runtime::spawn_blocking(move || advisor.assist(&ctx))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// E16: the human decides on a suggestion.
#[tauri::command]
fn decide_suggestion(
    store: State<AppStore>,
    suggestion: symbiot_core::action::Suggestion,
    accept: bool,
    chosen: Vec<usize>,
) -> Result<symbiot_store::Outcome, String> {
    store.with(|s| s.decide_suggestion(suggestion, accept, &chosen))
}

#[derive(serde::Serialize)]
struct Accepted {
    node: Node,
    edge: Edge,
}

#[tauri::command]
fn accept_proposal(store: State<AppStore>, proposal: Proposal) -> Result<Accepted, String> {
    store
        .with(|s| s.accept_proposal(proposal))
        .map(|(node, edge)| Accepted { node, edge })
}

#[tauri::command]
fn reject_proposal(store: State<AppStore>, proposal: Proposal) -> Result<(), String> {
    store.with(|s| s.reject_proposal(proposal))
}

#[tauri::command]
fn list_sensitive(store: State<AppStore>) -> Result<Vec<NodeId>, String> {
    store.with(|s| s.sensitive_nodes())
}

#[tauri::command]
fn set_sensitive(store: State<AppStore>, id: NodeId, sensitive: bool) -> Result<(), String> {
    store.with(|s| s.set_sensitive(id, sensitive))
}

#[tauri::command]
fn verify_ledger(store: State<AppStore>) -> Result<VerifyReport, String> {
    store.with(|s| s.verify())
}

#[tauri::command]
fn verify_all(store: State<AppStore>) -> Result<Vec<VerifyReport>, String> {
    store.with(|s| s.verify_all())
}

/// E4: why a node is sensitive or not.
#[tauri::command]
fn node_sensitivity(store: State<AppStore>, id: NodeId) -> Result<Sensitivity, String> {
    store.with(|s| s.sensitivity(id))
}

/// #30: the same filter as `GET /api/evidence`, as one argument.
#[derive(serde::Deserialize)]
struct EvidenceFilter {
    person: Option<String>,
    q: Option<String>,
    origin: Option<String>,
    kind: Option<String>,
    from: Option<u64>,
    to: Option<u64>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[tauri::command]
fn list_evidence(
    store: State<AppStore>,
    filter: EvidenceFilter,
) -> Result<Vec<StoredEvidence>, String> {
    let blank = |v: Option<String>| v.filter(|x| !x.trim().is_empty());
    let q = EvidenceQuery {
        text: blank(filter.q),
        origin: blank(filter.origin),
        kind: blank(filter.kind),
        from_ms: filter.from,
        to_ms: filter.to,
        limit: Some(filter.limit.unwrap_or(50).min(200)),
        offset: filter.offset,
        actors: None,
    };
    store.with(|s| {
        let q = match blank(filter.person) {
            None => q,
            Some(id) => EvidenceQuery {
                actors: Some(match s.person_of(&id)? {
                    Some(p) => p.identifiers.into_iter().map(|i| i.id).collect(),
                    None => vec![id],
                }),
                ..q
            },
        };
        s.evidence(&q)
    })
}

/// #31: persons and the human's decisions about them.
#[tauri::command]
fn list_persons(store: State<AppStore>) -> Result<Vec<Person>, String> {
    store.with(|s| s.persons())
}

#[tauri::command]
fn person_suggestions(store: State<AppStore>) -> Result<Vec<Suggestion>, String> {
    store.with(|s| s.person_suggestions())
}

#[tauri::command]
fn merge_identities(store: State<AppStore>, ids: Vec<String>, name: String) -> Result<(), String> {
    store.with(|s| s.merge_identities(&ids, &name))
}

#[tauri::command]
fn split_identity(store: State<AppStore>, id: String) -> Result<(), String> {
    store.with(|s| s.split_identity(&id))
}

#[tauri::command]
fn keep_apart(store: State<AppStore>, a: String, b: String) -> Result<(), String> {
    store.with(|s| s.keep_apart(&a, &b))
}

#[tauri::command]
fn list_sources(store: State<AppStore>) -> Result<Vec<symbiot_store::SourceInfo>, String> {
    store.with(|s| s.sources())
}

#[tauri::command]
fn grant_source(
    store: State<AppStore>,
    source: String,
    grant: symbiot_core::source::Grant,
) -> Result<(), String> {
    store.with(|s| s.grant_source(&source, grant))
}

#[tauri::command]
fn revoke_source(store: State<AppStore>, source: String) -> Result<usize, String> {
    store.with(|s| s.revoke_source(&source))
}

/// Pictures up to this size are shown in the app (#28); larger ones only by name.
const MAX_PICTURE_BYTES: u64 = 8 * 1024 * 1024;

/// #28: a picture of the raw data as `data:` URL. `None` if it is no picture, not here (yet)
/// or too large.
#[tauri::command]
fn file_data_url(store: State<AppStore>, hash: Hash) -> Result<Option<String>, String> {
    use base64::Engine as _;
    store.with(|s| {
        let Some(info) = s.file_info(&hash)? else {
            return Ok(None);
        };
        let picture = matches!(
            info.mime.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        );
        if !picture || info.len > MAX_PICTURE_BYTES {
            return Ok(None);
        }
        Ok(s.file(&hash)?.map(|bytes| {
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            format!("data:{};base64,{b64}", info.mime)
        }))
    })
}

#[tauri::command]
fn evidence_origins(store: State<AppStore>) -> Result<Vec<Origin>, String> {
    store.with(|s| s.evidence_origins())
}

#[tauri::command]
fn list_rules(store: State<AppStore>) -> Result<Vec<Rule>, String> {
    store.with(|s| s.rules())
}

#[tauri::command]
fn add_rule(
    store: State<AppStore>,
    label: String,
    keywords: Vec<String>,
    model: Option<String>,
) -> Result<Rule, String> {
    store.with(|s| s.add_rule(&label, &keywords, model))
}

#[tauri::command]
fn remove_rule(store: State<AppStore>, id: RuleId) -> Result<(), String> {
    store.with(|s| s.remove_rule(id))
}

/// E4: the local model suggests rules. Writes nothing; runs off the UI thread.
#[tauri::command]
async fn suggest_rules(
    store: State<'_, AppStore>,
    ai: State<'_, Ai>,
) -> Result<Vec<symbiot_ai::RuleProposal>, String> {
    let advisor =
        ai.0.clone()
            .ok_or_else(|| symbiot_ai::Error::NotConfigured.to_string())?;
    let ctx = store.with(|s| {
        Ok(symbiot_ai::rule_context(
            &s.nodes(None)?,
            &s.rules()?,
            &s.sensitive_nodes()?,
        ))
    })?;
    tauri::async_runtime::spawn_blocking(move || advisor.suggest_rules(&ctx))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct PersonInfo {
    device: PubKey,
    person: Option<PubKey>,
    devices: Vec<DeviceInfo>,
}

fn person_info(s: &mut Store) -> symbiot_store::Result<PersonInfo> {
    Ok(PersonInfo {
        device: s.device(),
        person: s.person()?,
        devices: s.devices()?,
    })
}

#[tauri::command]
fn person(store: State<AppStore>) -> Result<PersonInfo, String> {
    store.with(person_info)
}

/// E5: 24 new recovery words, shown once and never stored.
#[derive(serde::Serialize)]
struct Words {
    words: String,
}

#[tauri::command]
fn new_words() -> Result<Words, String> {
    symbiot_store::new_words()
        .map(|words| Words { words })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn certify(store: State<AppStore>, words: String, name: String) -> Result<PersonInfo, String> {
    store.with(|s| {
        s.certify_self(&words, &name)?;
        person_info(s)
    })
}

#[tauri::command]
fn revoke(store: State<AppStore>, words: String, device: PubKey) -> Result<PersonInfo, String> {
    store.with(|s| {
        s.revoke_device(&words, device)?;
        person_info(s)
    })
}

/// M4: where syncing with the home server stands. Shown under „Geräte“.
#[derive(Clone, Default, serde::Serialize)]
struct SyncStatus {
    /// The address of the home server, if set (in the app, or `SYMBIOT_SYNC_URL` until then).
    url: Option<String>,
    last_ok_ms: Option<u64>,
    last_error: Option<String>,
    last: Option<symbiot_sync::Report>,
}

struct Sync {
    remote: Mutex<Option<Arc<symbiot_sync::Remote>>>,
    status: Mutex<SyncStatus>,
    /// Where the address set in the app is kept: `<app data>/sync-url.txt`.
    file: std::path::PathBuf,
}

/// `https://…` or `http://…`, without a trailing slash; empty means „no sync“.
fn clean_url(url: &str) -> Result<Option<String>, String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return Ok(None);
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Die Adresse beginnt mit https:// (oder http://)".into());
    }
    Ok(Some(url.to_string()))
}

impl Sync {
    /// The address saved in the app wins; until one is saved, `SYMBIOT_SYNC_URL` counts.
    fn load(file: std::path::PathBuf) -> Self {
        let url = match std::fs::read_to_string(&file) {
            Ok(saved) => clean_url(&saved).ok().flatten(),
            Err(_) => std::env::var("SYMBIOT_SYNC_URL")
                .ok()
                .and_then(|u| clean_url(&u).ok().flatten()),
        };
        Sync {
            remote: Mutex::new(
                url.as_deref()
                    .map(|u| Arc::new(symbiot_sync::Remote::new(u))),
            ),
            status: Mutex::new(SyncStatus {
                url,
                ..SyncStatus::default()
            }),
            file,
        }
    }

    fn set_url(&self, url: &str) -> Result<SyncStatus, String> {
        let url = clean_url(url)?;
        std::fs::write(&self.file, url.as_deref().unwrap_or(""))
            .map_err(|e| format!("Adresse nicht speicherbar: {e}"))?;
        *self.remote.lock().unwrap_or_else(|p| p.into_inner()) = url
            .as_deref()
            .map(|u| Arc::new(symbiot_sync::Remote::new(u)));
        let mut status = self.status.lock().unwrap_or_else(|p| p.into_inner());
        *status = SyncStatus {
            url,
            ..SyncStatus::default()
        };
        Ok(status.clone())
    }

    /// One round; offline is not a failure of the app, only a status.
    fn run(&self, store: &Mutex<Store>) -> SyncStatus {
        let remote = self
            .remote
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let Some(remote) = remote else {
            return self
                .status
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
        };
        let result = symbiot_sync::sync(store, &remote);
        let mut status = self.status.lock().unwrap_or_else(|p| p.into_inner());
        match result {
            Ok(report) => {
                status.last_ok_ms = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                );
                status.last_error = None;
                status.last = Some(report);
            }
            Err(e) => status.last_error = Some(e.to_string()),
        }
        status.clone()
    }
}

#[tauri::command]
async fn sync_now(
    store: State<'_, AppStore>,
    sync: State<'_, Arc<Sync>>,
) -> Result<SyncStatus, String> {
    let (store, sync) = (store.0.clone(), sync.inner().clone());
    tauri::async_runtime::spawn_blocking(move || sync.run(&store))
        .await
        .map_err(|e| e.to_string())
}

/// Saves the address of the home server and syncs right away. Empty switches syncing off.
#[tauri::command]
async fn set_sync_url(
    store: State<'_, AppStore>,
    sync: State<'_, Arc<Sync>>,
    url: String,
) -> Result<SyncStatus, String> {
    let status = sync.set_url(&url)?;
    if status.url.is_none() {
        return Ok(status);
    }
    let (store, sync) = (store.0.clone(), sync.inner().clone());
    tauri::async_runtime::spawn_blocking(move || sync.run(&store))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn sync_status(sync: State<Arc<Sync>>) -> SyncStatus {
    sync.status
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let store = Arc::new(Mutex::new(Store::open_dir(&dir)?));
            app.manage(AppStore(store.clone()));
            // M4: sync with the home server every minute, once an address is set.
            let sync = Arc::new(Sync::load(dir.join("sync-url.txt")));
            app.manage(sync.clone());
            std::thread::spawn(move || {
                loop {
                    sync.run(&store);
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            });
            let ai = symbiot_ai::Config::from_env()
                .map(|c| Arc::new(symbiot_ai::LiteLlm::new(c)) as Arc<dyn Advisor>);
            app.manage(Ai(ai));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_nodes,
            create_node,
            update_node,
            node_history,
            list_edges,
            link_nodes,
            verify_ledger,
            ai_info,
            propose_next,
            accept_proposal,
            reject_proposal,
            list_sensitive,
            set_sensitive,
            delete_node,
            node_policy,
            set_policy,
            unlink_edge,
            person,
            new_words,
            certify,
            revoke,
            sync_now,
            set_sync_url,
            sync_status,
            verify_all,
            node_sensitivity,
            list_rules,
            add_rule,
            remove_rule,
            suggest_rules,
            list_evidence,
            evidence_origins,
            file_data_url,
            list_persons,
            person_suggestions,
            merge_identities,
            split_identity,
            keep_apart,
            list_sources,
            grant_source,
            revoke_source,
            assist,
            decide_suggestion
        ])
        .run(tauri::generate_context!())
        .expect("error while running Symbiot");
}
