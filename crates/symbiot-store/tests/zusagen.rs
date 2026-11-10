//! Promises from the decisions as tests (E18): what E2 and E3 say must hold on every way a node
//! can come into being — by hand, through a proposal, through the assistant — and on every
//! device, in whatever order the chains arrive.

use proptest::prelude::*;
use symbiot_core::action::{Action, Suggestion, Target};
use symbiot_core::{LedgerEntry, NodeId, NodeKind, Policy, Proposal, Signer};
use symbiot_store::{Store, new_words};

fn store(seed: u8) -> Store {
    Store::open_in_memory(Signer::from_seed([seed; 32])).unwrap()
}

/// Every chain a device holds, as it would hand them on.
fn chains(s: &Store) -> Vec<LedgerEntry> {
    s.heads()
        .unwrap()
        .iter()
        .flat_map(|h| s.chain_since(&h.author, None).unwrap())
        .collect()
}

fn holds(entries: &[LedgerEntry], needle: &str) -> Vec<u16> {
    entries
        .iter()
        .filter(|e| {
            e.content.as_ref().is_some_and(|parts| {
                parts
                    .iter()
                    .any(|p| String::from_utf8_lossy(&p.data.0).contains(needle))
            })
        })
        .map(|e| e.head.kind)
        .collect()
}

fn suggestion(title: &str, actions: Vec<Action>, sources: Vec<NodeId>) -> Suggestion {
    Suggestion {
        title: title.into(),
        reason: format!("weil {title}"),
        actions,
        sources,
        model: "mock".into(),
    }
}

fn create(title: &str) -> Action {
    Action::CreateNode {
        kind: NodeKind::Goal,
        title: title.into(),
        body: format!("Text zu {title}"),
    }
}

fn update(node: NodeId, title: &str) -> Action {
    Action::UpdateNode {
        node: Target::Node(node),
        title: title.into(),
        body: String::new(),
    }
}

fn proposal(goal: NodeId, title: &str) -> Proposal {
    Proposal {
        goal,
        kind: NodeKind::Goal,
        title: title.into(),
        body: String::new(),
        sources: vec![goal],
        model: "mock".into(),
    }
}

/// Sends everything `from` knows further than `to`.
fn pass(from: &Store, to: &mut Store) {
    for head in from.heads().unwrap() {
        if head.author == to.device() {
            continue;
        }
        let here = to.tip(&head.author).unwrap().map(|t| t.seq);
        if here.is_some_and(|s| s >= head.seq) {
            continue;
        }
        let entries = from.chain_since(&head.author, here).unwrap();
        to.receive(entries).unwrap();
    }
}

/// E2 on the file: the text is gone from the database itself, whichever entry carried it.
#[test]
fn deleted_text_is_gone_from_the_file_whatever_made_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open_dir(dir.path()).unwrap();
    // Made by the assistant, changed by the assistant, a change that was rejected, and a
    // rejected next step: four entries besides the versions that hold the node's text.
    let made = s
        .decide_suggestion(
            suggestion("GEHEIM anlegen", vec![create("GEHEIM")], vec![]),
            true,
            &[0],
        )
        .unwrap();
    let id = made.executed[0].node.unwrap();
    s.decide_suggestion(
        suggestion("GEHEIM ändern", vec![update(id, "GEHEIM 2")], vec![id]),
        true,
        &[0],
    )
    .unwrap();
    s.decide_suggestion(
        suggestion("GEHEIM anders", vec![update(id, "GEHEIM 3")], vec![id]),
        false,
        &[],
    )
    .unwrap();
    s.reject_proposal(proposal(id, "GEHEIM nächster Schritt"))
        .unwrap();
    let other = s.create_node(NodeKind::Goal, "Bleibt", "").unwrap();

    assert!(s.delete_node(id).unwrap() >= 5);
    // A decision about the node that comes after the deletion is stored without its text.
    s.decide_suggestion(
        suggestion("GEHEIM zu spät", vec![update(id, "GEHEIM 4")], vec![id]),
        false,
        &[],
    )
    .unwrap();
    assert_eq!(holds(&s.entries().unwrap(), "GEHEIM"), Vec::<u16>::new());
    assert!(s.node(other.id).unwrap().is_some());
    assert!(s.verify().unwrap().ok);
    drop(s);

    let mut raw = Vec::new();
    for f in ["symbiot.db", "symbiot.db-wal"] {
        if let Ok(bytes) = std::fs::read(dir.path().join(f)) {
            raw.extend(bytes);
        }
    }
    let found = |needle: &[u8]| raw.windows(needle.len()).any(|w| w == needle);
    assert!(!found(b"GEHEIM"), "the text is still in the file");
    assert!(found(b"Bleibt"));
}

/// E2 across devices: an edit written elsewhere after the deletion loses its text everywhere.
#[test]
fn an_edit_after_the_deletion_loses_its_text_on_every_device() {
    let words = new_words().unwrap();
    let (mut a, mut b) = (store(3), store(4));
    a.certify_self(&words, "a").unwrap();
    b.certify_self(&words, "b").unwrap();
    let n = a.create_node(NodeKind::Goal, "Ziel", "").unwrap();
    pass(&a, &mut b);
    pass(&b, &mut a);

    a.delete_node(n.id).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    b.update_node(n.id, "GEHEIM offline", "GEHEIM").unwrap();
    pass(&a, &mut b);
    pass(&b, &mut a);

    for s in [&a, &b] {
        assert!(s.node(n.id).unwrap().is_none(), "deleting wins");
        assert_eq!(holds(&chains(s), "GEHEIM"), Vec::<u16>::new());
        assert!(s.verify_all().unwrap().iter().all(|r| r.ok));
    }
}

/// E3: the text of a node that is not exportable is in nothing a device hands on.
#[test]
fn not_exportable_text_stays_home_whatever_made_it() {
    let words = new_words().unwrap();
    let mut a = store(5);
    a.certify_self(&words, "a").unwrap();
    let home = Policy {
        deletable: true,
        exportable: false,
    };
    // By hand, through the assistant (node and policy in one suggestion), and a node that
    // becomes home-only after the assistant changed it.
    a.create_node_with(NodeKind::Goal, "DAHEIM von Hand", "", home)
        .unwrap();
    let keep_home = Action::SetPolicy {
        node: Target::New(0),
        deletable: true,
        exportable: false,
    };
    a.decide_suggestion(
        suggestion(
            "DAHEIM anlegen",
            vec![create("DAHEIM Assistent"), keep_home],
            vec![],
        ),
        true,
        &[0, 1],
    )
    .unwrap();
    let later = a.create_node(NodeKind::Goal, "DAHEIM später", "").unwrap();
    a.decide_suggestion(
        suggestion(
            "DAHEIM ändern",
            vec![update(later.id, "DAHEIM später 2")],
            vec![later.id],
        ),
        true,
        &[0],
    )
    .unwrap();
    a.reject_proposal(proposal(later.id, "DAHEIM Schritt"))
        .unwrap();
    a.set_policy(later.id, home).unwrap();
    let open = a.create_node(NodeKind::Goal, "Darf hinaus", "").unwrap();

    let out = a.chain_since(&a.device(), None).unwrap();
    assert_eq!(holds(&out, "DAHEIM"), Vec::<u16>::new());
    assert!(!holds(&out, "Darf hinaus").is_empty());

    // Hidden but kept (not deletable): its text still stays home after the deletion.
    let proof = a
        .create_node_with(
            NodeKind::Evidence,
            "DAHEIM Beleg",
            "",
            Policy {
                deletable: false,
                exportable: false,
            },
        )
        .unwrap();
    a.delete_node(proof.id).unwrap();
    assert!(
        !holds(&a.entries().unwrap(), "DAHEIM Beleg").is_empty(),
        "kept here"
    );
    assert_eq!(
        holds(&a.chain_since(&a.device(), None).unwrap(), "DAHEIM"),
        Vec::<u16>::new()
    );

    // The other device shows what may leave and nothing else.
    let mut b = store(6);
    b.certify_self(&words, "b").unwrap();
    pass(&a, &mut b);
    let titles: Vec<String> = b
        .nodes(None)
        .unwrap()
        .into_iter()
        .map(|n| n.title)
        .collect();
    assert_eq!(titles, ["Darf hinaus"]);
    assert_eq!(b.node(open.id).unwrap().unwrap().title, "Darf hinaus");
}

#[derive(Clone, Debug)]
enum Step {
    /// A new node: 0 by hand, 1 through the assistant, 2 as an accepted proposal for `on`.
    Create {
        dev: usize,
        how: u8,
        on: usize,
    },
    /// A change: 0 by hand, 1 assistant accepted, 2 assistant rejected, 3 proposal rejected.
    Touch {
        dev: usize,
        node: usize,
        how: u8,
    },
    Delete {
        dev: usize,
        node: usize,
    },
    Sync {
        from: usize,
        to: usize,
    },
}

fn step() -> impl Strategy<Value = Step> {
    let dev = 0usize..3;
    let node = 0usize..8;
    prop_oneof![
        3 => (dev.clone(), 0u8..3, node.clone()).prop_map(|(dev, how, on)| Step::Create { dev, how, on }),
        4 => (dev.clone(), node.clone(), 0u8..4).prop_map(|(dev, node, how)| Step::Touch { dev, node, how }),
        2 => (dev.clone(), node).prop_map(|(dev, node)| Step::Delete { dev, node }),
        3 => (dev.clone(), dev).prop_map(|(from, to)| Step::Sync { from, to }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// E2 acceptance (H1): three devices create, change and delete nodes on every way there
    /// is and sync in any order. In the end no device holds the text of a deleted node in any
    /// chain, all devices show the same nodes, and every chain verifies.
    #[test]
    fn deleted_text_is_gone_on_every_device(steps in prop::collection::vec(step(), 1..40)) {
        let words = new_words().unwrap();
        let mut devs: Vec<Store> = (0..3u8).map(|i| store(40 + i)).collect();
        for (i, d) in devs.iter_mut().enumerate() {
            d.certify_self(&words, &format!("gerät {i}")).unwrap();
        }
        // Every text about node i carries its marker.
        let mut nodes: Vec<NodeId> = Vec::new();
        let mut deleted: Vec<usize> = Vec::new();
        let mark = |i: usize| format!("«n{i}»");

        for s in steps {
            match s {
                Step::Create { dev, how, on } => {
                    let m = mark(nodes.len());
                    let d = &mut devs[dev];
                    let id = match (how, nodes.get(on % nodes.len().max(1)).copied()) {
                        (2, Some(goal)) => d.accept_proposal(proposal(goal, &m)).ok().map(|(n, _)| n.id),
                        (1, _) => d
                            .decide_suggestion(suggestion(&m, vec![create(&m)], vec![]), true, &[0])
                            .ok()
                            .and_then(|o| o.executed.first().and_then(|e| e.node)),
                        _ => d.create_node(NodeKind::Goal, &m, &m).ok().map(|n| n.id),
                    };
                    // A proposal for a goal this device does not know fails: no node then.
                    if let Some(id) = id {
                        nodes.push(id);
                    }
                }
                Step::Touch { dev, node, how } => {
                    if nodes.is_empty() { continue; }
                    let i = node % nodes.len();
                    let (id, m) = (nodes[i], mark(i));
                    let d = &mut devs[dev];
                    // Fails where the device does not know the node (yet, or any more).
                    let _ = match how {
                        0 => d.update_node(id, &m, &m).map(|_| ()),
                        1 => d.decide_suggestion(suggestion(&m, vec![update(id, &m)], vec![id]), true, &[0]).map(|_| ()),
                        2 => d.decide_suggestion(suggestion(&m, vec![update(id, &m)], vec![id]), false, &[]).map(|_| ()),
                        _ => d.reject_proposal(proposal(id, &m)),
                    };
                }
                Step::Delete { dev, node } => {
                    if nodes.is_empty() { continue; }
                    let i = node % nodes.len();
                    if devs[dev].delete_node(nodes[i]).is_ok() {
                        deleted.push(i);
                    }
                }
                Step::Sync { from, to } => {
                    if from != to {
                        let (a, b) = if from < to {
                            let (l, r) = devs.split_at_mut(to);
                            (&l[from], &mut r[0])
                        } else {
                            let (l, r) = devs.split_at_mut(from);
                            (&r[0], &mut l[to])
                        };
                        pass(a, b);
                    }
                }
            }
        }
        // Everyone gets everything.
        for _ in 0..2 {
            for from in 0..3 {
                for to in 0..3 {
                    if from != to {
                        let (a, b) = if from < to {
                            let (l, r) = devs.split_at_mut(to);
                            (&l[from], &mut r[0])
                        } else {
                            let (l, r) = devs.split_at_mut(from);
                            (&r[0], &mut l[to])
                        };
                        pass(a, b);
                    }
                }
            }
        }

        let shown = |d: &Store| {
            let mut n: Vec<(NodeId, String, u32)> = d
                .nodes(None)
                .unwrap()
                .into_iter()
                .map(|n| (n.id, n.title, n.version))
                .collect();
            n.sort();
            n
        };
        let first = shown(&devs[0]);
        for d in &devs {
            prop_assert_eq!(&shown(d), &first, "all devices show the same nodes");
            prop_assert!(d.verify_all().unwrap().iter().all(|r| r.ok));
            let all = chains(d);
            for i in &deleted {
                prop_assert_eq!(holds(&all, &mark(*i)), Vec::<u16>::new(), "text of deleted node {} is still there", i);
                prop_assert!(d.node(nodes[*i]).unwrap().is_none());
            }
        }
    }
}
