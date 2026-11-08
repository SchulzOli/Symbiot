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
    assert!(first.contains("3 Belege und 0 Dateien gelesen"), "{first}");
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
async fn adapter_chain_and_files_reach_the_home_server() {
    let words = symbiot_store::new_words().unwrap();
    let server_files = tempfile::tempdir().unwrap();
    let mut home = Store::open_in_memory(Signer::from_seed([50; 32])).unwrap();
    home.set_files_dir(server_files.path().to_path_buf());
    home.certify_self(&words, "Heimserver").unwrap();
    let app = symbiot_server::app(home, None, None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    // A chat exported with media: the photo lies next to the text.
    let dir = tempfile::tempdir().unwrap();
    let chat = dir.path().join("WhatsApp-Chat mit Anna.txt");
    let with_photo =
        format!("{CHAT}12.03.24, 14:08 - Anna: IMG-20240312-WA0001.jpg (Datei angehängt)\n");
    std::fs::write(&chat, with_photo).unwrap();
    std::fs::write(
        dir.path().join("IMG-20240312-WA0001.jpg"),
        b"ein Foto vom See",
    )
    .unwrap();
    let chat = chat.to_string_lossy().into_owned();
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
    assert!(text.contains("4 Belege und 1 Dateien gelesen"), "{text}");
    assert!(text.contains("gesendet: 2 Einträge, 1 Dateien"), "{text}");

    // A device of the same person sees the adapter's chain on the home server.
    let adapter = Store::open_dir(&data).unwrap().device();
    let (w, u) = (words.clone(), url.clone());
    let heads = tokio::task::spawn_blocking(move || {
        let mut me = Store::open_in_memory(Signer::from_seed([60; 32])).unwrap();
        me.certify_self(&w, "PC").unwrap();
        Remote::new(&u).heads(&Mutex::new(me)).unwrap()
    })
    .await
    .unwrap();
    let head = heads
        .iter()
        .find(|h| h.author == adapter)
        .expect("adapter chain");
    assert_eq!(head.seq, 1, "certificate and one batch");

    // #28: a PC of the same person syncs and gets the photo, checked against its hash.
    let pc_dir = tempfile::tempdir().unwrap();
    let p = pc_dir.path().to_path_buf();
    let (report, photo) = tokio::task::spawn_blocking(move || {
        let mut pc = Store::open_dir(&p).unwrap();
        pc.certify_self(&words, "PC").unwrap();
        let pc = Mutex::new(pc);
        let report = symbiot_sync::sync(&pc, &Remote::new(&url)).unwrap();
        let pc = pc.into_inner().unwrap();
        let item = pc
            .evidence(&EvidenceQuery::default())
            .unwrap()
            .into_iter()
            .find(|e| !e.evidence.files.is_empty())
            .expect("message with photo");
        let hash = item.evidence.files[0].hash;
        (report, pc.file(&hash).unwrap())
    })
    .await
    .unwrap();
    assert_eq!(report.files_received, 1);
    assert_eq!(photo.as_deref(), Some(&b"ein Foto vom See"[..]));
}
