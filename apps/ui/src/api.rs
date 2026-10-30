//! Data access for the UI. Same calls, two backends:
//! inside Tauri (desktop/mobile) → `invoke` into the local store; in a browser → HTTP to `/api`.

use serde::Serialize;
use serde::de::DeserializeOwned;
use symbiot_core::{Node, NodeKind, VerifyReport};
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

pub async fn create_node(kind: NodeKind, title: String, body: String) -> ApiResult<Node> {
    #[derive(Serialize)]
    struct Args {
        kind: NodeKind,
        title: String,
        body: String,
    }
    let args = Args { kind, title, body };
    if tauri_invoke_fn().is_some() {
        return invoke("create_node", &args).await;
    }
    http_json(gloo_net::http::Request::post("/api/nodes").json(&args)).await
}

pub async fn verify() -> ApiResult<VerifyReport> {
    #[derive(Serialize)]
    struct NoArgs {}
    if tauri_invoke_fn().is_some() {
        return invoke("verify_ledger", &NoArgs {}).await;
    }
    http_json(gloo_net::http::Request::get("/api/ledger/verify").build()).await
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
    #[derive(serde::Deserialize)]
    struct ErrBody {
        error: String,
    }
    Err(match res.json::<ErrBody>().await {
        Ok(b) => b.error,
        Err(_) => format!("Fehler {}", res.status()),
    })
}
