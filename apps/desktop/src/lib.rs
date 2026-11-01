//! Symbiot desktop/mobile shell: the shared Leptos UI plus a local store.
//! Data lives on this device (app data dir); the UI reaches it via `invoke`.

use std::sync::{Arc, Mutex};

use symbiot_ai::{Advisor, Info};
use symbiot_core::{
    Edge, EdgeId, EdgeKind, LedgerEntry, Node, NodeId, NodeKind, Policy, Proposal, PubKey,
    VerifyReport,
};
use symbiot_store::{DeviceInfo, Store};
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

/// The AI the desktop asks: `SYMBIOT_AI_*` from the environment, e.g. LiteLLM on heimserver via Tailscale.
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

/// M4: where syncing with heimserver stands. Shown in the footer.
#[derive(Clone, Default, serde::Serialize)]
struct SyncStatus {
    /// `SYMBIOT_SYNC_URL`, if set.
    url: Option<String>,
    last_ok_ms: Option<u64>,
    last_error: Option<String>,
    last: Option<symbiot_sync::Report>,
}

struct Sync {
    remote: Option<symbiot_sync::Remote>,
    status: Mutex<SyncStatus>,
}

impl Sync {
    /// One round; offline is not a failure of the app, only a status.
    fn run(&self, store: &Mutex<Store>) -> SyncStatus {
        let Some(remote) = &self.remote else {
            return SyncStatus::default();
        };
        let result = symbiot_sync::sync(store, remote);
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
            // M4: sync with heimserver every minute if `SYMBIOT_SYNC_URL` is set.
            let url = std::env::var("SYMBIOT_SYNC_URL")
                .ok()
                .filter(|u| !u.is_empty());
            let sync = Arc::new(Sync {
                remote: url.as_deref().map(symbiot_sync::Remote::new),
                status: Mutex::new(SyncStatus {
                    url,
                    ..SyncStatus::default()
                }),
            });
            app.manage(sync.clone());
            if sync.remote.is_some() {
                std::thread::spawn(move || {
                    loop {
                        sync.run(&store);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                    }
                });
            }
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
            sync_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Symbiot");
}
