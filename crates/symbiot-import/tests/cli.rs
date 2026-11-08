//! #29 acceptance: the command line tool imports an export, a second run adds nothing, and the
//! adapter's chain reaches a real home server.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::Mutex;

use symbiot_core::Signer;
use symbiot_store::{EvidenceQuery, Store};
use symbiot_sync::Remote;

const CHAT: &str = "\
12.03.24, 14:05 - Anna: Hallo
12.03.24, 14:06 - Ben: Wandern am Samstag?
12.03.24, 14:07 - Anna: Gerne!
";

fn import(args: &[&str], words: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_symbiot-import"));
    cmd.args(args).env_remove("SYMBIOT_WORDS");
    if let Some(w) = words {
        cmd.env("SYMBIOT_WORDS", w);
    }
    cmd.output().unwrap()
}

fn stdout(o: &Output) -> String {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn write_chat(dir: &Path) -> String {
    let path = dir.join("WhatsApp-Chat mit Anna.txt");
    std::fs::write(&path, CHAT).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn import_twice_adds_nothing_the_second_time() {
    let dir = tempfile::tempdir().unwrap();
    let chat = write_chat(dir.path());
    let data = dir.path().join("adapter");
    let data = data.to_str().unwrap();
    let args = ["whatsapp", &chat, "--daten", data, "--utc-offset", "60"];

    let first = stdout(&import(&args, None));
    assert!(first.contains("3 Belege gelesen"), "{first}");
    assert!(
        first.contains("neu: 3, schon bekannt: 0, Bündel: 1"),
        "{first}"
    );
    assert!(first.contains("nicht bestätigt"), "{first}");
    let second = stdout(&import(&args, None));
    assert!(
        second.contains("neu: 0, schon bekannt: 3, Bündel: 0"),
        "{second}"
    );

    let store = Store::open_dir(Path::new(data)).unwrap();
    let items = store.evidence(&EvidenceQuery::default()).unwrap();
    assert_eq!(items[0].evidence.text, "Gerne!");
    assert!(
        items[0]
            .evidence
            .fields
            .contains(&("chat".into(), "WhatsApp-Chat mit Anna".into()))
    );
    assert!(store.verify().unwrap().ok);
}

#[test]
fn bad_input_and_usage() {
    let dir = tempfile::tempdir().unwrap();
    let not_json = dir.path().join("result.json");
    std::fs::write(&not_json, "kein json").unwrap();
    let o = import(&["telegram", not_json.to_str().unwrap()], None);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("kein gültiges JSON"));

    let o = import(&["fax", "x"], None);
    assert!(String::from_utf8_lossy(&o.stderr).contains("unbekannte Quelle"));

    // Sync without words is refused before anything is written.
    let chat = write_chat(dir.path());
    let data = dir.path().join("a");
    let o = import(
        &[
            "whatsapp",
            &chat,
            "--daten",
            data.to_str().unwrap(),
            "--abgleich",
            "http://127.0.0.1:9",
        ],
        None,
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("SYMBIOT_WORDS"));
}

#[tokio::test(flavor = "multi_thread")]
async fn adapter_chain_reaches_the_home_server() {
    let words = symbiot_store::new_words().unwrap();
    let mut home = Store::open_in_memory(Signer::from_seed([50; 32])).unwrap();
    home.certify_self(&words, "Heimserver").unwrap();
    let app = symbiot_server::app(home, None, None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let dir = tempfile::tempdir().unwrap();
    let chat = write_chat(dir.path());
    let data = dir.path().join("adapter");
    let (w, u, c, d) = (words.clone(), url.clone(), chat.clone(), data.clone());
    let out = tokio::task::spawn_blocking(move || {
        import(
            &[
                "whatsapp",
                &c,
                "--daten",
                d.to_str().unwrap(),
                "--abgleich",
                &u,
            ],
            Some(&w),
        )
    })
    .await
    .unwrap();
    let text = stdout(&out);
    assert!(text.contains("bestätigt"), "{text}");
    assert!(text.contains("gesendet: 2 Einträge"), "{text}");

    // A device of the same person sees the adapter's chain on the home server.
    let adapter = Store::open_dir(&data).unwrap().device();
    let heads = tokio::task::spawn_blocking(move || {
        let mut me = Store::open_in_memory(Signer::from_seed([60; 32])).unwrap();
        me.certify_self(&words, "PC").unwrap();
        Remote::new(&url).heads(&Mutex::new(me)).unwrap()
    })
    .await
    .unwrap();
    let head = heads
        .iter()
        .find(|h| h.author == adapter)
        .expect("adapter chain");
    assert_eq!(head.seq, 1, "certificate and one batch");
}
