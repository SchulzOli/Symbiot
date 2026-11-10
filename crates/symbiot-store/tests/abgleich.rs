//! H3 as tests: what a sync costs depends on what is new, not on what is stored.

use std::time::{Duration, Instant};

use symbiot_core::evidence::{Actor, role};
use symbiot_core::{Evidence, NodeKind, Signer};
use symbiot_store::{Store, new_words};

fn store(seed: u8) -> Store {
    Store::open_in_memory(Signer::from_seed([seed; 32])).unwrap()
}

fn message(n: u32) -> Evidence {
    Evidence {
        id: format!("telegram:chat{}/{n}", n % 40),
        kind: "schema:Message".into(),
        at_ms: 1_600_000_000_000 + n as u64 * 60_000,
        actors: vec![Actor {
            id: format!("telegram:user{}", n % 25),
            role: role::FROM.into(),
            name: "Anna".into(),
        }],
        text: format!("Nachricht {n} {}", "lorem ipsum dolor sit amet ".repeat(8)),
        fields: vec![("chat".into(), format!("Chat {}", n % 40))],
        files: vec![],
    }
}

/// Sends what `from` wrote itself and `to` does not have yet, in pages.
fn pass(from: &Store, to: &mut Store) -> usize {
    let me = from.device();
    let mut got = 0;
    loop {
        let after = to.tip(&me).unwrap().map(|t| t.seq);
        let page = from.chain_page(&me, after, 500).unwrap();
        if page.is_empty() {
            return got;
        }
        got += to.receive(page).unwrap();
    }
}

/// H3 acceptance: with eight times the raw data stored, taking in one new batch and one new
/// node costs about the same. Before H3 every receive verified the whole chain again, read
/// every chain with its raw data and rebuilt everything, so the cost grew with the data.
#[test]
fn receiving_does_not_get_slower_with_more_raw_data() {
    let words = new_words().unwrap();
    let (mut home, mut adapter, mut pc) = (store(1), store(2), store(3));
    home.certify_self(&words, "heim").unwrap();
    adapter.certify_self(&words, "import").unwrap();
    pc.certify_self(&words, "pc").unwrap();
    pass(&pc, &mut home);

    let mut n = 0u32;
    let mut load = |adapter: &mut Store, home: &mut Store, items: u32| {
        adapter
            .record_evidence((n..n + items).map(message).collect())
            .unwrap();
        n += items;
        pass(adapter, home);
    };
    // One round as a live adapter and a second device cause it: a batch and a node.
    let round = |adapter: &mut Store, pc: &mut Store, home: &mut Store, i: u32| {
        adapter
            .record_evidence(vec![message(9_000_000 + i)])
            .unwrap();
        pc.create_node(NodeKind::Goal, &format!("Ziel {i}"), "")
            .unwrap();
        let batch = adapter
            .chain_page(
                &adapter.device(),
                home.tip(&adapter.device()).unwrap().map(|t| t.seq),
                500,
            )
            .unwrap();
        let node = pc
            .chain_page(
                &pc.device(),
                home.tip(&pc.device()).unwrap().map(|t| t.seq),
                500,
            )
            .unwrap();
        let t = Instant::now();
        assert_eq!(home.receive(batch).unwrap(), 1);
        assert_eq!(home.receive(node).unwrap(), 1);
        t.elapsed()
    };
    let best = |adapter: &mut Store, pc: &mut Store, home: &mut Store, from: u32| {
        (from..from + 5)
            .map(|i| round(adapter, pc, home, i))
            .min()
            .unwrap()
    };

    load(&mut adapter, &mut home, 3_000);
    let small = best(&mut adapter, &mut pc, &mut home, 0);
    load(&mut adapter, &mut home, 21_000);
    let big = best(&mut adapter, &mut pc, &mut home, 100);

    assert_eq!(home.evidence_count().unwrap().0, 24_010);
    assert_eq!(home.nodes(None).unwrap().len(), 10);
    assert!(
        big < small * 3 + Duration::from_millis(5),
        "a receive with 24 000 items stored took {big:?}, with 3 000 items {small:?}"
    );
}

/// A long chain goes over in pages and arrives whole, in order.
#[test]
fn a_long_chain_arrives_in_pages() {
    let words = new_words().unwrap();
    let (mut home, mut pc) = (store(4), store(5));
    home.certify_self(&words, "heim").unwrap();
    pc.certify_self(&words, "pc").unwrap();
    for i in 0..1_200 {
        pc.create_node(NodeKind::Goal, &format!("Ziel {i}"), "")
            .unwrap();
    }
    let me = pc.device();
    assert_eq!(pc.chain_page(&me, None, 500).unwrap().len(), 500);
    assert_eq!(pc.chain_page(&me, Some(1_100), 500).unwrap().len(), 100);
    assert_eq!(pc.chain_page(&me, Some(1_200), 500).unwrap().len(), 0);
    assert_eq!(pass(&pc, &mut home), 1_201);
    assert_eq!(home.nodes(None).unwrap().len(), 1_200);
    assert_eq!(home.tip(&me).unwrap(), pc.tip(&me).unwrap());
    assert!(home.verify_all().unwrap().iter().all(|r| r.ok));
    // A page that does not attach is refused, one that is known changes nothing.
    assert_eq!(
        home.receive(pc.chain_page(&me, Some(10), 5).unwrap())
            .unwrap(),
        0
    );
    let mut gap = store(6);
    gap.certify_self(&words, "neu").unwrap();
    assert!(
        gap.receive(pc.chain_page(&me, Some(10), 5).unwrap())
            .is_err()
    );
}

/// A database written before schema 11 gets its new columns and tips from the signed heads.
#[test]
fn an_older_database_is_upgraded_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let words = new_words().unwrap();
    let mut other = store(7);
    other.certify_self(&words, "pc").unwrap();
    other.create_node(NodeKind::Goal, "Vom PC", "").unwrap();
    let (device, heads, nodes) = {
        let mut s = Store::open_dir(dir.path()).unwrap();
        s.certify_self(&words, "heim").unwrap();
        s.create_node(NodeKind::Goal, "Ziel", "Text").unwrap();
        s.record_evidence(vec![message(1), message(2)]).unwrap();
        pass(&other, &mut s);
        (s.device(), s.heads().unwrap(), s.nodes(None).unwrap())
    };
    {
        // Back to the shape of schema 10.
        let conn = rusqlite::Connection::open(dir.path().join("symbiot.db")).unwrap();
        conn.execute_batch(
            "DROP INDEX ledger_kind; DROP TABLE chain_tip;
             ALTER TABLE ledger DROP COLUMN kind; ALTER TABLE ledger DROP COLUMN ts_ms;
             PRAGMA user_version = 10;",
        )
        .unwrap();
    }
    let mut s = Store::open_dir(dir.path()).unwrap();
    assert_eq!(s.device(), device);
    assert_eq!(s.heads().unwrap(), heads);
    assert_eq!(s.nodes(None).unwrap(), nodes);
    assert_eq!(s.evidence_count().unwrap(), (2, 0));
    assert_eq!(s.devices().unwrap().len(), 2);
    assert!(s.verify_all().unwrap().iter().all(|r| r.ok));
    // And it goes on working: a new entry here, a new one from the other device.
    s.create_node(NodeKind::Goal, "Danach", "").unwrap();
    other.create_node(NodeKind::Goal, "Noch eins", "").unwrap();
    assert_eq!(pass(&other, &mut s), 1);
    assert_eq!(s.nodes(None).unwrap().len(), 4);
}
