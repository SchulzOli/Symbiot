//! Symbiot desktop/mobile shell: the shared Leptos UI plus a local store.
//! Data lives on this device (app data dir); the UI reaches it via `invoke`.

use std::sync::Mutex;

use symbiot_core::{Edge, EdgeKind, LedgerEntry, Node, NodeId, NodeKind, VerifyReport};
use symbiot_store::Store;
use tauri::{Manager, State};

struct AppStore(Mutex<Store>);

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
) -> Result<Node, String> {
    store.with(|s| s.create_node(kind, &title, &body))
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

#[tauri::command]
fn verify_ledger(store: State<AppStore>) -> Result<VerifyReport, String> {
    store.with(|s| s.verify())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let store = Store::open_dir(&dir)?;
            app.manage(AppStore(Mutex::new(store)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_nodes,
            create_node,
            update_node,
            node_history,
            list_edges,
            link_nodes,
            verify_ledger
        ])
        .run(tauri::generate_context!())
        .expect("error while running Symbiot");
}
