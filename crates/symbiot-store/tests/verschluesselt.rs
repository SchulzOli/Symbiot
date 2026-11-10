//! E6 as tests (H5a): with a key nothing lies in the data folder in the clear — not the
//! database, not the files, not the device key — and a store that was unencrypted is
//! converted without losing anything.
#![cfg(feature = "encryption")]

use std::path::Path;

use symbiot_core::evidence::{Actor, role};
use symbiot_core::{Evidence, FileRef, NodeKind, Signer};
use symbiot_store::{Store, StoreKey, new_words};

fn key(b: u8) -> StoreKey {
    StoreKey::from_bytes([b; 32])
}

/// Every byte of every file below `dir`.
fn everything(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else {
                out.push((p.display().to_string(), std::fs::read(&p).unwrap()));
            }
        }
    }
    out
}

fn found(dir: &Path, needle: &str) -> Vec<String> {
    let n = needle.as_bytes();
    everything(dir)
        .into_iter()
        .filter(|(_, bytes)| bytes.windows(n.len()).any(|w| w == n))
        .map(|(name, _)| name)
        .collect()
}

/// A node, raw data with a photo, and a second device's chain.
fn fill(s: &mut Store, words: &str) -> symbiot_core::Hash {
    s.certify_self(words, "KLARTEXT-Gerät").unwrap();
    s.create_node(NodeKind::Goal, "KLARTEXT Ziel", "KLARTEXT Text")
        .unwrap();
    let photo = s.put_file(&b"KLARTEXT Foto ".repeat(9000)).unwrap();
    s.record_evidence(vec![Evidence {
        id: "telegram:chat/1".into(),
        kind: "schema:Message".into(),
        at_ms: 1_700_000_000_000,
        actors: vec![Actor {
            id: "telegram:anna".into(),
            role: role::FROM.into(),
            name: "KLARTEXT Anna".into(),
        }],
        text: "KLARTEXT Nachricht".into(),
        fields: vec![],
        files: vec![FileRef {
            hash: photo,
            name: "foto.jpg".into(),
            mime: "image/jpeg".into(),
            len: 126_000,
        }],
    }])
    .unwrap();
    let mut other = Store::open_in_memory(Signer::from_seed([9; 32])).unwrap();
    other.certify_self(words, "PC").unwrap();
    other
        .create_node(NodeKind::Goal, "KLARTEXT vom PC", "")
        .unwrap();
    s.receive(other.chain_since(&other.device(), None).unwrap())
        .unwrap();
    photo
}

fn check(s: &Store, photo: &symbiot_core::Hash) {
    let mut titles: Vec<String> = s
        .nodes(None)
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    titles.sort();
    assert_eq!(titles, ["KLARTEXT Ziel", "KLARTEXT vom PC"]);
    assert_eq!(s.evidence_count().unwrap(), (1, 0));
    assert_eq!(s.devices().unwrap().len(), 2);
    assert_eq!(s.file(photo).unwrap().unwrap().len(), 126_000);
    assert!(s.verify_all().unwrap().iter().all(|r| r.ok));
}

#[test]
fn with_a_key_nothing_lies_in_the_clear() {
    let dir = tempfile::tempdir().unwrap();
    let words = new_words().unwrap();
    let (device, photo) = {
        let mut s = Store::open_dir_with(dir.path(), Some(&key(1))).unwrap();
        let photo = fill(&mut s, &words);
        check(&s, &photo);
        (s.device(), photo)
    };
    assert_eq!(found(dir.path(), "KLARTEXT"), Vec::<String>::new());
    assert_eq!(found(dir.path(), "SQLite format 3"), Vec::<String>::new());
    assert!(
        !dir.path().join("device.key").exists(),
        "the device key is in the database"
    );

    // The same device again with the key; nothing without it or with another one.
    let s = Store::open_dir_with(dir.path(), Some(&key(1))).unwrap();
    assert_eq!(s.device(), device);
    check(&s, &photo);
    drop(s);
    let no_key = Store::open_dir(dir.path()).err().unwrap().to_string();
    assert!(no_key.contains("verschlüsselt"), "{no_key}");
    let wrong = Store::open_dir_with(dir.path(), Some(&key(2)))
        .err()
        .unwrap()
        .to_string();
    assert!(wrong.contains("passt nicht"), "{wrong}");
    assert!(
        !dir.path().join("device.key").exists(),
        "a failed start leaves nothing behind"
    );
}

#[test]
fn an_unencrypted_store_is_converted_and_keeps_everything() {
    let dir = tempfile::tempdir().unwrap();
    let words = new_words().unwrap();
    let (device, heads, photo) = {
        let mut s = Store::open_dir(dir.path()).unwrap();
        let photo = fill(&mut s, &words);
        // Something deleted before: it must not come back.
        let gone = s.create_node(NodeKind::Goal, "GELÖSCHT", "").unwrap();
        s.delete_node(gone.id).unwrap();
        (s.device(), s.heads().unwrap(), photo)
    };
    assert!(
        !found(dir.path(), "KLARTEXT").is_empty(),
        "in the clear so far"
    );
    {
        // As an older version left it: without the columns and tables of schema 11.
        let conn = rusqlite::Connection::open(dir.path().join("symbiot.db")).unwrap();
        conn.execute_batch(
            "DROP INDEX ledger_kind; DROP TABLE chain_tip; DROP TABLE forks;
             ALTER TABLE ledger DROP COLUMN kind; ALTER TABLE ledger DROP COLUMN ts_ms;
             PRAGMA user_version = 9;",
        )
        .unwrap();
    }

    let mut s = Store::open_dir_with(dir.path(), Some(&key(1))).unwrap();
    assert_eq!(s.device(), device, "the same device, the same chain");
    assert_eq!(s.heads().unwrap(), heads);
    check(&s, &photo);
    assert!(s.person().unwrap().is_some());
    // It goes on working, and a second start changes nothing.
    s.create_node(NodeKind::Goal, "KLARTEXT danach", "")
        .unwrap();
    drop(s);
    let s = Store::open_dir_with(dir.path(), Some(&key(1))).unwrap();
    assert_eq!(s.nodes(None).unwrap().len(), 3);
    assert_eq!(s.file(&photo).unwrap().unwrap().len(), 126_000);
    drop(s);

    assert_eq!(found(dir.path(), "KLARTEXT"), Vec::<String>::new());
    assert_eq!(found(dir.path(), "GELÖSCHT"), Vec::<String>::new());
    assert_eq!(found(dir.path(), "SQLite format 3"), Vec::<String>::new());
    let left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with("symbiot.db") || n.ends_with(".plain") || n.ends_with(".enc"))
        .collect();
    assert_eq!(left, ["files"], "no old database, no device.key");
}

/// The conversion was cut off between its two renames: the next start finishes it.
#[test]
fn an_interrupted_conversion_is_finished() {
    let dir = tempfile::tempdir().unwrap();
    let words = new_words().unwrap();
    let photo = {
        let mut s = Store::open_dir_with(dir.path(), Some(&key(1))).unwrap();
        fill(&mut s, &words)
    };
    let db = dir.path().join("symbiot.db");
    std::fs::rename(&db, dir.path().join("symbiot.db.enc")).unwrap();
    std::fs::write(
        dir.path().join("symbiot.db.plain"),
        b"SQLite format 3\0 KLARTEXT rest",
    )
    .unwrap();

    let s = Store::open_dir_with(dir.path(), Some(&key(1))).unwrap();
    check(&s, &photo);
    drop(s);
    assert_eq!(found(dir.path(), "KLARTEXT"), Vec::<String>::new());
    assert!(!dir.path().join("symbiot.db.plain").exists());
}
