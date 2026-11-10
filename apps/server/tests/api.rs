use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use symbiot_ai::{Advisor, Context, Info, RuleContext, RuleProposal};
use symbiot_core::{LedgerEntry, NodeKind, Op, Proposal, Signer};
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

    /// E16: creates „Laufschuhe kaufen“ and links it to the first node it was shown.
    fn assist(
        &self,
        ctx: &symbiot_ai::AssistContext,
    ) -> Result<symbiot_core::action::Suggestion, symbiot_ai::Error> {
        let first = ctx
            .nodes
            .last()
            .map(|n| n.id)
            .ok_or(symbiot_ai::Error::NoSources)?;
        let reply = format!(
            r#"{{"title": "Schritt anlegen", "reason": "{}", "actions": [
                {{"create_node": {{"kind": "goal", "title": "Laufschuhe kaufen"}}}},
                {{"link": {{"from": "$0", "to": "{first}", "kind": "derives_from"}}}}]}}"#,
            ctx.instruction
        );
        symbiot_ai::parse_assist(&reply, ctx, "fake")
    }

    /// Suggests „Gesundheit“ for every node that mentions a doctor.
    fn suggest_rules(&self, ctx: &RuleContext) -> Result<Vec<RuleProposal>, symbiot_ai::Error> {
        let reply = r#"{"rules": [{"label": "Gesundheit", "keywords": ["arzt", "befund"]}]}"#;
        symbiot_ai::parse_rules(reply, ctx, "fake")
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
    // H4: like the interface, every call says that it comes from the interface.
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(symbiot_server::SAME_ORIGIN_HEADER, "1");
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
    let update: LedgerEntry = serde_json::from_value(history[1].clone()).unwrap();
    assert!(matches!(update.op(), Ok(Op::UpdateNode { title, .. }) if title == "Laufen 3x"));
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

/// E2/E3 acceptance over the API: policy at creation, unlink, delete with and without content.
#[tokio::test]
async fn delete_unlink_and_policy() {
    let app = app();
    let (_, goal) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "goal", "title": "Laufen"})),
    )
    .await;
    let (s, proof) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "evidence", "title": "Arztbrief", "deletable": false})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let goal = goal["id"].as_str().unwrap().to_string();
    let proof = proof["id"].as_str().unwrap().to_string();

    let (_, policy) = call(&app, "GET", &format!("/api/nodes/{goal}/policy"), None).await;
    assert_eq!(policy, json!({"deletable": true, "exportable": true}));
    let (_, policy) = call(&app, "GET", &format!("/api/nodes/{proof}/policy"), None).await;
    assert_eq!(policy, json!({"deletable": false, "exportable": true}));
    assert_eq!(entries(&app).await, 3, "node and its policy");

    let (_, edge) = call(
        &app,
        "POST",
        "/api/edges",
        Some(json!({"from": proof, "to": goal, "kind": "supports"})),
    )
    .await;
    let edge = edge["id"].as_str().unwrap();
    let (s, _) = call(&app, "DELETE", &format!("/api/edges/{edge}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&app, "DELETE", &format!("/api/edges/{edge}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, d) = call(&app, "DELETE", &format!("/api/nodes/{goal}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["pruned"], 1);
    let (s, d) = call(&app, "DELETE", &format!("/api/nodes/{proof}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["pruned"], 0, "not deletable: hidden only");
    let (_, all) = call(&app, "GET", "/api/nodes", None).await;
    assert_eq!(all, json!([]));
    let (s, _) = call(&app, "GET", &format!("/api/nodes/{goal}"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Loosening the policy of a deleted node is refused; the chain stays valid.
    let (s, _) = call(
        &app,
        "PUT",
        &format!("/api/nodes/{proof}/policy"),
        Some(json!({"deletable": true})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, report) = call(&app, "GET", "/api/ledger/verify", None).await;
    assert_eq!(report["ok"], true);
    assert_eq!(report["entries"], 7);
    assert_eq!(report["pruned"], 1);
}

/// E3: a node that is not exportable stays away from an external model, like a sensitive one.
#[tokio::test]
async fn not_exportable_nodes_never_reach_an_external_model() {
    let (app, ai) = app_with_ai();
    let (_, goal) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "goal", "title": "Umziehen"})),
    )
    .await;
    let (_, local) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "reflection", "title": "Mietvertrag", "exportable": false})),
    )
    .await;
    let goal = goal["id"].as_str().unwrap().to_string();
    let local = local["id"].as_str().unwrap().to_string();

    let propose = format!("/api/nodes/{goal}/propose");
    let (s, _) = call(&app, "POST", &propose, Some(json!({"remote": true}))).await;
    assert_eq!(s, StatusCode::OK);
    {
        let seen = ai.seen.lock().unwrap();
        assert!(seen[0].related.iter().all(|r| r.node.id.to_hex() != local));
        assert_eq!(seen[0].withheld, 1);
    }

    call(
        &app,
        "PUT",
        &format!("/api/nodes/{goal}/policy"),
        Some(json!({"exportable": false})),
    )
    .await;
    let (s, _) = call(&app, "POST", &propose, Some(json!({"remote": true}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(ai.seen.lock().unwrap().len(), 1);
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

/// E4 acceptance: the AI suggests a rule, nothing changes until the human accepts it; then
/// matching nodes are sensitive and stay away from an external model; by hand wins.
#[tokio::test]
async fn sensitivity_rules_from_the_ai() {
    let (app, ai) = app_with_ai();
    let node = |kind: &str, title: &str| json!({"kind": kind, "title": title});
    let (_, goal) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(node("goal", "Gesünder leben")),
    )
    .await;
    let (_, doctor) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(node("reflection", "Befund vom Arzt")),
    )
    .await;
    let goal = goal["id"].as_str().unwrap().to_string();
    let doctor = doctor["id"].as_str().unwrap().to_string();
    let before = entries(&app).await;

    let (s, suggested) = call(&app, "POST", "/api/rules/suggest", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(suggested[0]["label"], "Gesundheit");
    assert_eq!(suggested[0]["matches"], json!([doctor]));
    assert_eq!(entries(&app).await, before, "a suggestion writes nothing");
    let (_, sensitive) = call(&app, "GET", "/api/sensitive", None).await;
    assert_eq!(sensitive, json!([]));

    // The human accepts it.
    let (s, rule) = call(
        &app,
        "POST",
        "/api/rules",
        Some(json!({
            "label": suggested[0]["label"],
            "keywords": suggested[0]["keywords"],
            "model": suggested[0]["model"],
        })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (_, sensitive) = call(&app, "GET", "/api/sensitive", None).await;
    assert_eq!(sensitive, json!([doctor]));
    let (_, why) = call(
        &app,
        "GET",
        &format!("/api/nodes/{doctor}/sensitivity"),
        None,
    )
    .await;
    assert_eq!(why["rules"], json!(["Gesundheit"]));

    // The external model no longer sees the doctor's note.
    let propose = format!("/api/nodes/{goal}/propose");
    call(&app, "POST", &propose, Some(json!({"remote": true}))).await;
    {
        let seen = ai.seen.lock().unwrap();
        let last = seen.last().unwrap();
        assert!(last.related.iter().all(|r| r.node.id.to_hex() != doctor));
        assert_eq!(last.withheld, 1);
    }

    // By hand wins; removing the rule leaves the mark by hand.
    call(
        &app,
        "PUT",
        &format!("/api/nodes/{doctor}/sensitive"),
        Some(json!({"sensitive": false})),
    )
    .await;
    let (_, sensitive) = call(&app, "GET", "/api/sensitive", None).await;
    assert_eq!(sensitive, json!([]));
    let id = rule["id"].as_str().unwrap();
    let (s, _) = call(&app, "DELETE", &format!("/api/rules/{id}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, rules) = call(&app, "GET", "/api/rules", None).await;
    assert_eq!(rules, json!([]));
    let (s, err) = call(
        &app,
        "POST",
        "/api/rules",
        Some(json!({"label": "x", "keywords": ["ab"]})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(err["error"].as_str().unwrap().contains("Stichwörter"));
}

/// #30: raw data can be listed and searched over HTTP.
#[tokio::test]
async fn evidence_search_and_origins() {
    use symbiot_core::evidence::{Actor, Evidence, role};
    let mut store = Store::open_in_memory(Signer::from_seed([5; 32])).unwrap();
    let msg = |n: u64, text: &str| Evidence {
        id: format!("telegram:1/{n}"),
        kind: "schema:Message".into(),
        at_ms: 1_000 + n,
        actors: vec![Actor {
            id: "telegram:anna".into(),
            role: role::FROM.into(),
            name: "Anna".into(),
        }],
        text: text.into(),
        fields: vec![],
        files: vec![],
    };
    store
        .record_evidence(vec![msg(1, "Wandern am Samstag?"), msg(2, "Gerne!")])
        .unwrap();
    let app = symbiot_server::app(store, None, None);

    let (status, all) = call(&app, "GET", "/api/evidence", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(all[0]["evidence"]["text"], "Gerne!");
    assert_eq!(all[0]["evidence"]["actors"][0]["name"], "Anna");

    let (_, found) = call(&app, "GET", "/api/evidence?q=wand&origin=telegram", None).await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["evidence"]["id"], "telegram:1/1");
    let (_, none) = call(&app, "GET", "/api/evidence?origin=mail", None).await;
    assert_eq!(none.as_array().unwrap().len(), 0);
    let (_, page) = call(&app, "GET", "/api/evidence?limit=1&offset=1", None).await;
    assert_eq!(page[0]["evidence"]["text"], "Wandern am Samstag?");

    let (_, origins) = call(&app, "GET", "/api/evidence/origins", None).await;
    assert_eq!(origins, json!([{"origin": "telegram", "count": 2}]));
}

/// #28: files of raw data for the web UI; only safe types inline, anything else as download.
#[tokio::test]
async fn files_for_the_web_ui() {
    use symbiot_core::evidence::{Evidence, FileRef};
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_in_memory(Signer::from_seed([5; 32])).unwrap();
    store.set_files_dir(dir.path().to_path_buf());
    let png = store.put_file(b"png").unwrap();
    let html = store.put_file(b"<script>alert(1)</script>").unwrap();
    let file = |hash, name: &str, mime: &str| FileRef {
        hash,
        name: name.into(),
        mime: mime.into(),
        len: 3,
    };
    store
        .record_evidence(vec![Evidence {
            id: "mail:x@example.org".into(),
            kind: "schema:EmailMessage".into(),
            at_ms: 1,
            actors: vec![],
            text: String::new(),
            fields: vec![],
            files: vec![
                file(png, "see.png", "image/png"),
                file(html, "seite.html", "text/html"),
            ],
        }])
        .unwrap();
    let app = symbiot_server::app(store, None, None);
    let get = |uri: String| {
        let app = app.clone();
        async move {
            let res = app
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let h = res.headers().clone();
            let body = res.into_body().collect().await.unwrap().to_bytes();
            (h, body)
        }
    };
    let (h, body) = get(format!("/api/files/{png}")).await;
    assert_eq!(&body[..], b"png");
    assert_eq!(h["content-type"], "image/png");
    assert!(
        h["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline")
    );
    assert_eq!(h["content-security-policy"], "sandbox");

    let (h, _) = get(format!("/api/files/{html}")).await;
    assert_eq!(h["content-type"], "application/octet-stream");
    assert!(
        h["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );

    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/files/{}", "00".repeat(32)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// #31: persons over HTTP: suggestion, merge, the timeline of one person, split.
#[tokio::test]
async fn persons_merge_and_timeline() {
    use symbiot_core::evidence::{Actor, Evidence, role};
    let said = |id: &str, who: &str, name: &str| Evidence {
        id: id.into(),
        kind: "schema:Message".into(),
        at_ms: 10,
        actors: vec![Actor {
            id: who.into(),
            role: role::FROM.into(),
            name: name.into(),
        }],
        text: String::new(),
        fields: vec![],
        files: vec![],
    };
    let mut store = Store::open_in_memory(Signer::from_seed([5; 32])).unwrap();
    store
        .record_evidence(vec![
            said("whatsapp:c/1", "name:Anna", "Anna"),
            said("mail:1", "mailto:anna@example.org", "Anna"),
            said("mail:2", "mailto:ben@example.org", "Ben"),
        ])
        .unwrap();
    let app = symbiot_server::app(store, None, None);

    let (_, persons) = call(&app, "GET", "/api/persons", None).await;
    assert_eq!(persons.as_array().unwrap().len(), 3);
    let (_, sug) = call(&app, "GET", "/api/persons/suggestions", None).await;
    assert_eq!(sug[0]["reason"], "gleicher Name");

    let merge = json!({"ids": ["name:Anna", "mailto:anna@example.org"], "name": "Anna B."});
    let (status, _) = call(&app, "POST", "/api/persons/merge", Some(merge)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, persons) = call(&app, "GET", "/api/persons", None).await;
    assert_eq!(persons[0]["name"], "Anna B.");
    assert_eq!(persons[0]["identifiers"].as_array().unwrap().len(), 2);

    let (_, items) = call(&app, "GET", "/api/evidence?person=name:Anna", None).await;
    assert_eq!(
        items.as_array().unwrap().len(),
        2,
        "both sources of one person"
    );

    let bad = json!({"ids": ["name:Anna"]});
    let (status, _) = call(&app, "POST", "/api/persons/merge", Some(bad)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = call(
        &app,
        "POST",
        "/api/persons/split",
        Some(json!({"id": "name:Anna"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, items) = call(&app, "GET", "/api/evidence?person=name:Anna", None).await;
    assert_eq!(items.as_array().unwrap().len(), 1);
}

/// E16: a request in words becomes a suggestion; only the human's decision changes anything.
#[tokio::test]
async fn assist_suggests_and_the_human_decides() {
    let (app, _) = app_with_ai();
    let (_, goal) = call(
        &app,
        "POST",
        "/api/nodes",
        Some(json!({"kind": "goal", "title": "Laufen"})),
    )
    .await;
    let (status, sug) = call(
        &app,
        "POST",
        "/api/assist",
        Some(json!({"instruction": "Leg einen Schritt an"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sug["actions"].as_array().unwrap().len(), 2);
    assert_eq!(sug["sources"][0], goal["id"]);
    let (_, nodes) = call(&app, "GET", "/api/nodes", None).await;
    assert_eq!(
        nodes.as_array().unwrap().len(),
        1,
        "a suggestion changes nothing"
    );

    let (_, out) = call(
        &app,
        "POST",
        "/api/suggestions/decide",
        Some(json!({"suggestion": sug, "accept": true, "chosen": [0, 1]})),
    )
    .await;
    assert_eq!(out["executed"].as_array().unwrap().len(), 2);
    assert_eq!(out["error"], Value::Null);
    let (_, edges) = call(&app, "GET", "/api/edges", None).await;
    assert_eq!(edges[0]["kind"], "derives_from");

    let (status, _) = call(
        &app,
        "POST",
        "/api/suggestions/decide",
        Some(json!({"suggestion": sug, "accept": true, "chosen": [1]})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "the link needs the node of action 1"
    );

    let (_, out) = call(
        &app,
        "POST",
        "/api/suggestions/decide",
        Some(json!({"suggestion": sug, "accept": false})),
    )
    .await;
    assert_eq!(out["executed"].as_array().unwrap().len(), 0);
    let (_, nodes) = call(&app, "GET", "/api/nodes", None).await;
    assert_eq!(nodes.as_array().unwrap().len(), 2);

    let (status, _) = call(
        &app,
        "POST",
        "/api/assist",
        Some(json!({"instruction": "  "})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// H4: a request that changes something must come from the interface. A page of another
/// origin, open in a browser inside the tailnet, cannot write — not even an import, which
/// takes any body.
#[tokio::test]
async fn writes_from_outside_the_interface_are_refused() {
    let app = app();
    let send =
        |method: &'static str, uri: &'static str, headers: Vec<(&'static str, &'static str)>| {
            let app = app.clone();
            async move {
                let mut req = Request::builder().method(method).uri(uri);
                for (k, v) in headers {
                    req = req.header(k, v);
                }
                let body = r#"{"kind":"goal","title":"von außen"}"#;
                let res = app
                    .oneshot(req.body(Body::from(body)).unwrap())
                    .await
                    .unwrap();
                res.status()
            }
        };
    let json = ("content-type", "application/json");
    let own = (symbiot_server::SAME_ORIGIN_HEADER, "1");

    // No header: what a form or a simple fetch from another page can send.
    assert_eq!(
        send("POST", "/api/nodes", vec![json]).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            "POST",
            "/api/import?source=whatsapp&name=x.txt",
            vec![("content-type", "text/plain")]
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send("DELETE", "/api/nodes/00", vec![]).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send("POST", "/api/person/words", vec![]).await,
        StatusCode::FORBIDDEN
    );
    // The header, but the browser says the request comes from another site.
    assert_eq!(
        send(
            "POST",
            "/api/nodes",
            vec![json, own, ("sec-fetch-site", "cross-site")]
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            "POST",
            "/api/nodes",
            vec![json, own, ("sec-fetch-site", "same-site")]
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // Nothing was written.
    let (_, nodes) = call(&app, "GET", "/api/nodes", None).await;
    assert_eq!(nodes.as_array().unwrap().len(), 0);

    // The interface itself, with and without what a browser adds.
    assert_eq!(
        send("POST", "/api/nodes", vec![json, own]).await,
        StatusCode::CREATED
    );
    assert_eq!(
        send(
            "POST",
            "/api/nodes",
            vec![json, own, ("sec-fetch-site", "same-origin")]
        )
        .await,
        StatusCode::CREATED
    );
    // Reading needs nothing, and answers may not be embedded elsewhere.
    let res = app
        .clone()
        .oneshot(Request::get("/api/nodes").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cross-origin-resource-policy"], "same-origin");
}
