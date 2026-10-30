use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use symbiot_core::Signer;
use symbiot_store::Store;
use tower::ServiceExt;

fn app() -> Router {
    symbiot_server::app(
        Store::open_in_memory(Signer::from_seed([5; 32])).unwrap(),
        None,
    )
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(uri);
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn health_reports_ok_and_versions() {
    let (status, json) = call(&app(), "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert_eq!(json["app"], "symbiot");
    assert_eq!(json["protocol"], symbiot_core::PROTOCOL_VERSION);
}

#[tokio::test]
async fn create_list_edit_and_verify() {
    let app = app();
    let (s, goal) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "goal", "title": "Laufen"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let id = goal["id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 32);

    call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "evidence", "title": "5 km"})),
    )
    .await;

    let (_, all) = call(&app, "GET", "/api/nodes", None).await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    let (_, goals) = call(&app, "GET", "/api/nodes?kind=goal", None).await;
    assert_eq!(goals.as_array().unwrap().len(), 1);

    let (s, edited) = call(
        &app,
        "PUT",
        &format!("/api/nodes/{id}"),
        Some(json!({"title": "Laufen 3x"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(edited["version"], 2);
    assert_eq!(edited["history"].as_array().unwrap().len(), 2);

    let (_, report) = call(&app, "GET", "/api/ledger/verify", None).await;
    assert_eq!(report["ok"], true);
    assert_eq!(report["entries"], 3);

    let (_, ledger) = call(&app, "GET", "/api/ledger", None).await;
    assert_eq!(ledger.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn bad_input_is_rejected() {
    let app = app();
    let (s, err) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "goal", "title": " "})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("Titel"));

    let (s, _) = call(&app, "GET", "/api/nodes?kind=quatsch", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let unknown = format!("/api/nodes/{}", "ab".repeat(16));
    let (s, _) = call(&app, "PUT", &unknown, Some(json!({"title": "x"}))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
