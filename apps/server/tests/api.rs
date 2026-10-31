use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use symbiot_ai::{Advisor, Context, Info};
use symbiot_core::{NodeKind, Proposal, Signer};
use symbiot_store::Store;
use tower::ServiceExt;

fn app() -> Router {
    symbiot_server::app(
        Store::open_in_memory(Signer::from_seed([5; 32])).unwrap(),
        None,
        None,
    )
}

/// Stands in for LiteLLM: proposes „Laufschuhe kaufen“, citing every node it was shown,
/// and remembers the context so tests can see what would have left the device.
#[derive(Default)]
struct FakeAi {
    seen: Mutex<Vec<Context>>,
}

impl Advisor for FakeAi {
    fn info(&self) -> Info {
        Info {
            available: true,
            local_model: Some("fake".into()),
            remote_model: Some("fake-remote".into()),
        }
    }

    fn propose(&self, ctx: &Context) -> Result<Proposal, symbiot_ai::Error> {
        self.seen.lock().unwrap().push(ctx.clone());
        let mut sources = vec![ctx.goal.id];
        sources.extend(ctx.related.iter().map(|r| r.node.id));
        Ok(Proposal {
            goal: ctx.goal.id,
            kind: NodeKind::Goal,
            title: "Laufschuhe kaufen".into(),
            body: String::new(),
            sources,
            model: if ctx.remote { "fake-remote" } else { "fake" }.into(),
        })
    }
}

fn app_with_ai() -> (Router, Arc<FakeAi>) {
    let ai = Arc::new(FakeAi::default());
    let app = symbiot_server::app(
        Store::open_in_memory(Signer::from_seed([6; 32])).unwrap(),
        None,
        Some(ai.clone()),
    );
    (app, ai)
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

#[tokio::test]
async fn link_nodes_and_read_history() {
    let app = app();
    let node = |kind: &str, title: &str| json!({"kind": kind, "title": title});
    let (_, goal) = call(&app, "POST", "/api/nodes", Some(node("goal", "Laufen"))).await;
    let (_, ev) = call(&app, "POST", "/api/nodes", Some(node("evidence", "5 km"))).await;
    let (goal, ev) = (goal["id"].as_str().unwrap(), ev["id"].as_str().unwrap());

    let link = json!({"from": ev, "to": goal, "kind": "supports"});
    let (s, edge) = call(&app, "POST", "/api/edges", Some(link.clone())).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(edge["from"], ev);
    assert_eq!(edge["kind"], "supports");

    let (s, err) = call(&app, "POST", "/api/edges", Some(link)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("schon"));

    let (_, edges) = call(&app, "GET", "/api/edges", None).await;
    assert_eq!(edges.as_array().unwrap().len(), 1);

    call(
        &app,
        "PUT",
        &format!("/api/nodes/{goal}"),
        Some(json!({"title": "Laufen 3x"})),
    )
    .await;
    let (s, history) = call(&app, "GET", &format!("/api/nodes/{goal}/history"), None).await;
    assert_eq!(s, StatusCode::OK);
    let history = history.as_array().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1]["op"]["update_node"]["title"], "Laufen 3x");
}

async fn entries(app: &Router) -> usize {
    let (_, ledger) = call(app, "GET", "/api/ledger", None).await;
    ledger.as_array().unwrap().len()
}

/// F4 acceptance: no proposal changes data without the human's click.
#[tokio::test]
async fn proposals_change_nothing_until_decided() {
    let (app, _) = app_with_ai();
    let node = |kind: &str, title: &str| json!({"kind": kind, "title": title});
    let (_, goal) = call(&app, "POST", "/api/nodes", Some(node("goal", "Laufen"))).await;
    let goal = goal["id"].as_str().unwrap().to_string();

    let (s, proposal) = call(
        &app,
        "POST",
        &format!("/api/nodes/{goal}/propose"),
        Some(json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(proposal["title"], "Laufschuhe kaufen");
    assert_eq!(proposal["sources"][0], goal);
    assert_eq!(entries(&app).await, 1, "proposing writes nothing");
    let (_, nodes) = call(&app, "GET", "/api/nodes", None).await;
    assert_eq!(nodes.as_array().unwrap().len(), 1);

    let (s, _) = call(
        &app,
        "POST",
        "/api/proposals/reject",
        Some(proposal.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(entries(&app).await, 2, "the rejection is recorded");
    let (_, nodes) = call(&app, "GET", "/api/nodes", None).await;
    assert_eq!(nodes.as_array().unwrap().len(), 1, "rejecting adds no node");

    let (s, accepted) = call(
        &app,
        "POST",
        "/api/proposals/accept",
        Some(proposal.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(accepted["node"]["title"], "Laufschuhe kaufen");
    assert_eq!(accepted["edge"]["to"], goal);
    assert_eq!(accepted["edge"]["kind"], "derives_from");
    assert_eq!(entries(&app).await, 3);

    // A proposal without sources is refused, also when sent directly to the API.
    let mut bare = proposal;
    bare["sources"] = json!([]);
    let (s, err) = call(&app, "POST", "/api/proposals/accept", Some(bare)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("Quellen"));
    assert_eq!(entries(&app).await, 3);

    let (_, report) = call(&app, "GET", "/api/ledger/verify", None).await;
    assert_eq!(report["ok"], true);
}

/// F4 acceptance: with sensitive nodes nothing leaves the home network.
#[tokio::test]
async fn sensitive_nodes_never_reach_an_external_model() {
    let (app, ai) = app_with_ai();
    let node = |kind: &str, title: &str| json!({"kind": kind, "title": title});
    let (_, goal) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(node("goal", "Gesünder leben")),
    )
    .await;
    let (_, private) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(node("reflection", "Befund vom Arzt")),
    )
    .await;
    let goal = goal["id"].as_str().unwrap().to_string();
    let private = private["id"].as_str().unwrap().to_string();

    let (s, _) = call(
        &app,
        "PUT",
        &format!("/api/nodes/{private}/sensitive"),
        Some(json!({"sensitive": true})),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, sensitive) = call(&app, "GET", "/api/sensitive", None).await;
    assert_eq!(sensitive, json!([private]));

    // The local model may use it …
    let propose = format!("/api/nodes/{goal}/propose");
    call(&app, "POST", &propose, Some(json!({"remote": false}))).await;
    // … the external one never sees it.
    let (s, p) = call(&app, "POST", &propose, Some(json!({"remote": true}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(p["model"], "fake-remote");
    {
        let seen = ai.seen.lock().unwrap();
        let ids =
            |c: &Context| -> Vec<String> { c.related.iter().map(|r| r.node.id.to_hex()).collect() };
        assert!(ids(&seen[0]).contains(&private));
        assert!(!ids(&seen[1]).contains(&private));
        assert_eq!(seen[1].withheld, 1);
    }

    // A sensitive goal is not sent out at all.
    call(
        &app,
        "PUT",
        &format!("/api/nodes/{goal}/sensitive"),
        Some(json!({"sensitive": true})),
    )
    .await;
    let (s, err) = call(&app, "POST", &propose, Some(json!({"remote": true}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("sensibel"));
    assert_eq!(ai.seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn without_ai_the_rest_still_works() {
    let app = app();
    let (_, info) = call(&app, "GET", "/api/ai", None).await;
    assert_eq!(info["available"], false);
    let (_, goal) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "goal", "title": "Laufen"})),
    )
    .await;
    let id = goal["id"].as_str().unwrap();
    let (s, err) = call(
        &app,
        "POST",
        &format!("/api/nodes/{id}/propose"),
        Some(json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        err["error"]
            .as_str()
            .unwrap()
            .contains("nicht eingerichtet")
    );
}
