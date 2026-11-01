//! F5 acceptance: devices sync through sz. sz runs as a real HTTP server, the devices are
//! stores on this machine — like the desktop app, which uses the same client.

use std::sync::{Arc, Mutex};

use serde_json::Value;
use symbiot_core::{EdgeKind, NodeKind, Policy, Signer};
use symbiot_store::Store;
use symbiot_sync::{Error, Remote, Report};

/// sz with its own key, set up for the person behind `words`.
async fn sz(words: &str) -> String {
    let mut store = Store::open_in_memory(Signer::from_seed([50; 32])).unwrap();
    store.certify_self(words, "sz").unwrap();
    let app = symbiot_server::app(store, None, None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

type Device = Arc<Mutex<Store>>;

fn device(seed: u8, words: Option<&str>, name: &str) -> Device {
    let mut store = Store::open_in_memory(Signer::from_seed([seed; 32])).unwrap();
    if let Some(w) = words {
        store.certify_self(w, name).unwrap();
    }
    Arc::new(Mutex::new(store))
}

async fn sync(d: &Device, url: &str) -> Result<Report, Error> {
    let (d, url) = (d.clone(), url.to_string());
    tokio::task::spawn_blocking(move || symbiot_sync::sync(&d, &Remote::new(&url)))
        .await
        .unwrap()
}

/// The titles sz shows in its web UI.
async fn titles_on_sz(url: &str) -> Vec<String> {
    let nodes: Value = http_get(&format!("{url}/api/nodes")).await;
    let mut t: Vec<String> = nodes
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["title"].as_str().unwrap().to_string())
        .collect();
    t.sort();
    t
}

async fn http_get(url: &str) -> Value {
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        ureq::get(&url)
            .call()
            .unwrap()
            .body_mut()
            .read_json::<Value>()
            .unwrap()
    })
    .await
    .unwrap()
}

fn titles(d: &Device) -> Vec<String> {
    let mut t: Vec<String> = d
        .lock()
        .unwrap()
        .nodes(None)
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    t.sort();
    t
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_nodes_reach_sz_and_the_second_device() {
    let words = symbiot_store::new_words().unwrap();
    let url = sz(&words).await;
    let pc = device(1, Some(&words), "PC");
    {
        // Offline at the PC.
        let mut s = pc.lock().unwrap();
        let goal = s.create_node(NodeKind::Goal, "Laufen", "").unwrap();
        let proof = s.create_node(NodeKind::Evidence, "5 km", "").unwrap();
        s.link(proof.id, goal.id, EdgeKind::Supports).unwrap();
    }

    let r = sync(&pc, &url).await.unwrap();
    assert_eq!(r.sent, 4, "certificate, two nodes, one edge");
    assert_eq!(titles_on_sz(&url).await, ["5 km", "Laufen"]);

    let phone = device(2, Some(&words), "Handy");
    let r = sync(&phone, &url).await.unwrap();
    assert!(r.received >= 4, "{r:?}");
    assert_eq!(titles(&phone), ["5 km", "Laufen"]);
    assert_eq!(phone.lock().unwrap().edges().unwrap().len(), 1);

    // The PC learns about the phone, and nothing moves twice.
    sync(&pc, &url).await.unwrap();
    assert_eq!(sync(&pc, &url).await.unwrap(), Report::default());
    assert_eq!(sync(&phone, &url).await.unwrap(), Report::default());
    let names: Vec<String> = pc
        .lock()
        .unwrap()
        .devices()
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    for d in [&pc, &phone] {
        let s = d.lock().unwrap();
        for chain in s.heads().unwrap() {
            assert!(chain.seq < 10);
        }
        assert!(s.verify().unwrap().ok);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn strangers_tampering_and_forks_are_rejected() {
    let words = symbiot_store::new_words().unwrap();
    let url = sz(&words).await;

    // Another person, and a device that was never set up.
    let stranger = device(3, Some(&symbiot_store::new_words().unwrap()), "Fremd");
    stranger
        .lock()
        .unwrap()
        .create_node(NodeKind::Goal, "fremd", "")
        .unwrap();
    let err = sync(&stranger, &url).await.unwrap_err().to_string();
    assert!(err.contains("Personen-Schlüssel"), "{err}");
    let unset = device(4, None, "");
    unset
        .lock()
        .unwrap()
        .create_node(NodeKind::Goal, "ohne Schlüssel", "")
        .unwrap();
    assert!(sync(&unset, &url).await.is_err());
    assert!(titles_on_sz(&url).await.is_empty());

    // A manipulated chain.
    let pc = device(1, Some(&words), "PC");
    pc.lock()
        .unwrap()
        .create_node(NodeKind::Goal, "Laufen", "")
        .unwrap();
    let mut chain = pc.lock().unwrap().entries().unwrap();
    chain[1].content.as_mut().unwrap()[0].data.0[1] ^= 1;
    let (p, u) = (pc.clone(), url.clone());
    let err = tokio::task::spawn_blocking(move || Remote::new(&u).push(&p, &chain))
        .await
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(err.contains("Kette abgelehnt"), "{err}");
    assert!(titles_on_sz(&url).await.is_empty());

    // The real chain goes through; then the same key writes a different history: a fork.
    sync(&pc, &url).await.unwrap();
    let twin = device(1, None, "");
    twin.lock()
        .unwrap()
        .create_node(NodeKind::Goal, "andere Geschichte", "")
        .unwrap();
    let forged = twin.lock().unwrap().entries().unwrap();
    let (t, u) = (twin.clone(), url.clone());
    let err = tokio::task::spawn_blocking(move || Remote::new(&u).push(&t, &forged))
        .await
        .unwrap()
        .unwrap_err()
        .to_string();
    assert!(err.contains("Verzweigung"), "{err}");
    assert_eq!(titles_on_sz(&url).await, ["Laufen"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_device_is_cut_off() {
    let words = symbiot_store::new_words().unwrap();
    let url = sz(&words).await;
    let pc = device(1, Some(&words), "PC");
    let phone = device(2, Some(&words), "Handy");
    phone
        .lock()
        .unwrap()
        .create_node(NodeKind::Goal, "vor dem Verlust", "")
        .unwrap();
    sync(&phone, &url).await.unwrap();
    sync(&pc, &url).await.unwrap();

    let lost = phone.lock().unwrap().device();
    pc.lock().unwrap().revoke_device(&words, lost).unwrap();
    sync(&pc, &url).await.unwrap();

    phone
        .lock()
        .unwrap()
        .create_node(NodeKind::Goal, "vom Dieb", "")
        .unwrap();
    let err = sync(&phone, &url).await.unwrap_err().to_string();
    assert!(err.contains("Personen-Schlüssel"), "{err}");
    assert_eq!(titles_on_sz(&url).await, ["vor dem Verlust"]);
    assert_eq!(titles(&pc), ["vor dem Verlust"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_edits_end_the_same_everywhere() {
    let words = symbiot_store::new_words().unwrap();
    let url = sz(&words).await;
    let pc = device(1, Some(&words), "PC");
    let phone = device(2, Some(&words), "Handy");
    let goal = pc
        .lock()
        .unwrap()
        .create_node(NodeKind::Goal, "Laufen", "")
        .unwrap();
    sync(&pc, &url).await.unwrap();
    sync(&phone, &url).await.unwrap();

    // Both change the goal while apart; the phone's edit is the later one.
    pc.lock()
        .unwrap()
        .update_node(goal.id, "Laufen (PC)", "")
        .unwrap();
    phone
        .lock()
        .unwrap()
        .update_node(goal.id, "Laufen (Handy)", "")
        .unwrap();
    for d in [&pc, &phone, &pc] {
        sync(d, &url).await.unwrap();
    }

    for d in [&pc, &phone] {
        let n = d.lock().unwrap().node(goal.id).unwrap().unwrap();
        assert_eq!(n.title, "Laufen (Handy)", "newest wins");
        assert!(n.concurrent, "marked for a look");
        assert_eq!(n.history.len(), 3, "both versions stay in the history");
    }
    assert_eq!(titles_on_sz(&url).await, ["Laufen (Handy)"]);

    // „Passt so“ at the PC clears the mark everywhere.
    pc.lock()
        .unwrap()
        .update_node(goal.id, "Laufen (Handy)", "")
        .unwrap();
    sync(&pc, &url).await.unwrap();
    sync(&phone, &url).await.unwrap();
    assert!(
        !phone
            .lock()
            .unwrap()
            .node(goal.id)
            .unwrap()
            .unwrap()
            .concurrent
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_and_home_only_content_stays_away() {
    let words = symbiot_store::new_words().unwrap();
    let url = sz(&words).await;
    let pc = device(1, Some(&words), "PC");
    let phone = device(2, Some(&words), "Handy");
    let gone = {
        let mut s = pc.lock().unwrap();
        let gone = s.create_node(NodeKind::Goal, "Geheim", "").unwrap();
        s.create_node_with(
            NodeKind::Reflection,
            "Nur am PC",
            "Tagebuch",
            Policy {
                deletable: true,
                exportable: false,
            },
        )
        .unwrap();
        gone
    };
    sync(&pc, &url).await.unwrap();
    sync(&phone, &url).await.unwrap();
    assert_eq!(
        titles(&phone),
        ["Geheim"],
        "not exportable: not on the phone"
    );
    assert_eq!(titles_on_sz(&url).await, ["Geheim"]);

    pc.lock().unwrap().delete_node(gone.id).unwrap();
    sync(&pc, &url).await.unwrap();
    sync(&phone, &url).await.unwrap();
    assert!(titles(&phone).is_empty());
    assert!(titles_on_sz(&url).await.is_empty());

    // What sz hands out of the PC's chain: no title of the deleted node, no diary entry.
    let pc_key = pc.lock().unwrap().device();
    let (p, u) = (phone.clone(), url.clone());
    let chain = tokio::task::spawn_blocking(move || Remote::new(&u).pull(&p, &pc_key, None))
        .await
        .unwrap()
        .unwrap();
    let text = serde_json::to_string(&chain).unwrap();
    for e in &chain {
        if let Some(parts) = &e.content {
            let data = String::from_utf8_lossy(&parts[0].data.0).to_string();
            assert!(
                !data.contains("Geheim") && !data.contains("Tagebuch"),
                "{text}"
            );
        }
    }
    assert!(phone.lock().unwrap().verify().unwrap().ok);
    assert_eq!(titles(&pc), ["Nur am PC"], "the PC keeps its own");
}
