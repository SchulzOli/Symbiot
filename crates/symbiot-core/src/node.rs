//! Nodes of the personal graph and the projection that builds them from ledger entries.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::bytes::{EdgeId, Hash, NodeId, PubKey};
use crate::edge::{Edge, EdgeKind};
use crate::ledger::{LedgerEntry, Op};
use crate::proposal::Proposal;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Goal,
    Tension,
    Reflection,
    Evidence,
    Person,
    Value,
}

impl NodeKind {
    pub const ALL: [NodeKind; 6] = [
        NodeKind::Goal,
        NodeKind::Tension,
        NodeKind::Reflection,
        NodeKind::Evidence,
        NodeKind::Person,
        NodeKind::Value,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Goal => "goal",
            NodeKind::Tension => "tension",
            NodeKind::Reflection => "reflection",
            NodeKind::Evidence => "evidence",
            NodeKind::Person => "person",
            NodeKind::Value => "value",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        NodeKind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// German label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            NodeKind::Goal => "Ziel",
            NodeKind::Tension => "Spannung",
            NodeKind::Reflection => "Reflexion",
            NodeKind::Evidence => "Beleg",
            NodeKind::Person => "Person",
            NodeKind::Value => "Wert",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub body: String,
    pub author: PubKey,
    pub version: u32,
    pub created_ms: u64,
    pub updated_ms: u64,
    /// Hashes of every entry that shaped this node, oldest first — its provenance.
    pub history: Vec<Hash>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    UnknownNode(NodeId),
    SelfLink,
    DuplicateEdge,
    /// A proposal must name the nodes it is based on.
    NoSources,
    EmptyTitle,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::UnknownNode(id) => write!(f, "Knoten {id} existiert nicht"),
            ApplyError::SelfLink => {
                f.write_str("Ein Knoten kann nicht mit sich selbst verbunden werden")
            }
            ApplyError::DuplicateEdge => f.write_str("Diese Verbindung gibt es schon"),
            ApplyError::NoSources => f.write_str("Ein Vorschlag ohne Quellen wird verworfen"),
            ApplyError::EmptyTitle => f.write_str("Titel darf nicht leer sein"),
        }
    }
}

/// What one ledger entry does to the projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A new node, or the next version of an existing one.
    Node(Node),
    Edge(Edge),
    /// An accepted proposal: the new node and its edge „derives from“ the goal.
    Accepted {
        node: Node,
        edge: Edge,
    },
    /// A rejected proposal: nothing changes in the graph.
    Rejected,
    Sensitivity {
        node: NodeId,
        sensitive: bool,
    },
}

impl Change {
    /// Derives the effect of `e`. `node` looks up the current version of a node,
    /// `has_edge` tells whether the same connection already exists. Shared by
    /// [`State`] and the SQLite store, so every platform applies the same rules.
    pub fn of(
        e: &LedgerEntry,
        node: impl Fn(&NodeId) -> Option<Node>,
        has_edge: impl Fn(&NodeId, &NodeId, EdgeKind) -> bool,
    ) -> Result<Change, ApplyError> {
        match &e.op {
            Op::CreateNode { kind, title, body } => Ok(Change::Node(Node {
                id: NodeId::from_entry_hash(&e.hash),
                kind: *kind,
                title: title.clone(),
                body: body.clone(),
                author: e.author,
                version: 1,
                created_ms: e.ts_ms,
                updated_ms: e.ts_ms,
                history: alloc::vec![e.hash],
            })),
            Op::UpdateNode {
                node: id,
                title,
                body,
            } => {
                let mut n = node(id).ok_or(ApplyError::UnknownNode(*id))?;
                n.title = title.clone();
                n.body = body.clone();
                n.version += 1;
                n.updated_ms = e.ts_ms;
                n.history.push(e.hash);
                Ok(Change::Node(n))
            }
            Op::Link { from, to, kind } => {
                if from == to {
                    return Err(ApplyError::SelfLink);
                }
                for id in [from, to] {
                    if node(id).is_none() {
                        return Err(ApplyError::UnknownNode(*id));
                    }
                }
                if has_edge(from, to, *kind) {
                    return Err(ApplyError::DuplicateEdge);
                }
                Ok(Change::Edge(Edge {
                    id: EdgeId::from_entry_hash(&e.hash),
                    from: *from,
                    to: *to,
                    kind: *kind,
                    author: e.author,
                    created_ms: e.ts_ms,
                    entry: e.hash,
                }))
            }
            Op::AcceptProposal { proposal: p } => {
                check_proposal(p, &node)?;
                let id = NodeId::from_entry_hash(&e.hash);
                Ok(Change::Accepted {
                    node: Node {
                        id,
                        kind: p.kind,
                        title: p.title.clone(),
                        body: p.body.clone(),
                        author: e.author,
                        version: 1,
                        created_ms: e.ts_ms,
                        updated_ms: e.ts_ms,
                        history: alloc::vec![e.hash],
                    },
                    edge: Edge {
                        id: EdgeId::from_entry_hash(&e.hash),
                        from: id,
                        to: p.goal,
                        kind: EdgeKind::DerivesFrom,
                        author: e.author,
                        created_ms: e.ts_ms,
                        entry: e.hash,
                    },
                })
            }
            Op::RejectProposal { proposal: p } => {
                check_proposal(p, &node)?;
                Ok(Change::Rejected)
            }
            Op::MarkSensitive {
                node: id,
                sensitive,
            } => {
                node(id).ok_or(ApplyError::UnknownNode(*id))?;
                Ok(Change::Sensitivity {
                    node: *id,
                    sensitive: *sensitive,
                })
            }
        }
    }
}

/// Same rules for accepting and rejecting: a proposal names a known goal and known sources.
fn check_proposal(p: &Proposal, node: &impl Fn(&NodeId) -> Option<Node>) -> Result<(), ApplyError> {
    if p.title.trim().is_empty() {
        return Err(ApplyError::EmptyTitle);
    }
    if p.sources.is_empty() {
        return Err(ApplyError::NoSources);
    }
    for id in core::iter::once(&p.goal).chain(&p.sources) {
        if node(id).is_none() {
            return Err(ApplyError::UnknownNode(*id));
        }
    }
    Ok(())
}

/// In-memory projection of verified entries — used by tests and small devices.
/// Apps with storage keep the same projection in SQLite.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub nodes: BTreeMap<NodeId, Node>,
    pub edges: BTreeMap<EdgeId, Edge>,
    pub sensitive: BTreeSet<NodeId>,
}

impl State {
    pub fn apply(&mut self, e: &LedgerEntry) -> Result<Change, ApplyError> {
        let change = Change::of(
            e,
            |id| self.nodes.get(id).cloned(),
            |from, to, kind| {
                self.edges
                    .values()
                    .any(|x| x.from == *from && x.to == *to && x.kind == kind)
            },
        )?;
        match &change {
            Change::Node(n) => {
                self.nodes.insert(n.id, n.clone());
            }
            Change::Edge(x) => {
                self.edges.insert(x.id, x.clone());
            }
            Change::Accepted { node, edge } => {
                self.nodes.insert(node.id, node.clone());
                self.edges.insert(edge.id, edge.clone());
            }
            Change::Rejected => {}
            Change::Sensitivity { node, sensitive } => {
                if *sensitive {
                    self.sensitive.insert(*node);
                } else {
                    self.sensitive.remove(node);
                }
            }
        }
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Signer;

    #[test]
    fn update_appends_version_and_keeps_history() {
        let s = Signer::from_seed([1; 32]);
        let c = s.append(
            None,
            10,
            Op::CreateNode {
                kind: NodeKind::Goal,
                title: "Laufen".into(),
                body: String::new(),
            },
        );
        let id = NodeId::from_entry_hash(&c.hash);
        let u = s.append(
            Some(&c),
            20,
            Op::UpdateNode {
                node: id,
                title: "Laufen, 3x pro Woche".into(),
                body: String::new(),
            },
        );

        let mut st = State::default();
        st.apply(&c).unwrap();
        st.apply(&u).unwrap();
        let n = &st.nodes[&id];
        assert_eq!(n.version, 2);
        assert_eq!(n.title, "Laufen, 3x pro Woche");
        assert_eq!(n.history, alloc::vec![c.hash, u.hash]);
        assert_eq!((n.created_ms, n.updated_ms), (10, 20));
    }

    #[test]
    fn update_of_unknown_node_fails() {
        let s = Signer::from_seed([1; 32]);
        let u = s.append(
            None,
            1,
            Op::UpdateNode {
                node: NodeId([0; 16]),
                title: String::new(),
                body: String::new(),
            },
        );
        assert!(State::default().apply(&u).is_err());
    }

    #[test]
    fn kind_parse_round_trips() {
        for k in NodeKind::ALL {
            assert_eq!(NodeKind::parse(k.as_str()), Some(k));
        }
        for k in EdgeKind::ALL {
            assert_eq!(EdgeKind::parse(k.as_str()), Some(k));
        }
    }

    #[test]
    fn link_rules() {
        let s = Signer::from_seed([2; 32]);
        let mut st = State::default();
        let mut chain: Vec<LedgerEntry> = Vec::new();
        let mut push = |st: &mut State, op: Op| {
            let e = s.append(chain.last(), chain.len() as u64, op);
            chain.push(e.clone());
            st.apply(&e)
        };
        let create = |title: &str| Op::CreateNode {
            kind: NodeKind::Goal,
            title: title.into(),
            body: String::new(),
        };
        let a = match push(&mut st, create("Ziel")).unwrap() {
            Change::Node(n) => n.id,
            _ => unreachable!(),
        };
        let b = match push(&mut st, create("Beleg")).unwrap() {
            Change::Node(n) => n.id,
            _ => unreachable!(),
        };
        let link = |from, to| Op::Link {
            from,
            to,
            kind: EdgeKind::Supports,
        };

        assert!(matches!(push(&mut st, link(b, a)), Ok(Change::Edge(_))));
        assert_eq!(push(&mut st, link(b, a)), Err(ApplyError::DuplicateEdge));
        assert_eq!(push(&mut st, link(a, a)), Err(ApplyError::SelfLink));
        let ghost = NodeId([9; 16]);
        assert_eq!(
            push(&mut st, link(a, ghost)),
            Err(ApplyError::UnknownNode(ghost))
        );
        // The reverse direction is a different connection.
        assert!(push(&mut st, link(a, b)).is_ok());
        assert_eq!(st.edges.len(), 2);
    }

    /// F4: only a decision changes the graph, and only a proposal with known sources is accepted.
    #[test]
    fn proposal_decisions() {
        let s = Signer::from_seed([4; 32]);
        let mut st = State::default();
        let mut chain: Vec<LedgerEntry> = Vec::new();
        let mut push = |st: &mut State, op: Op| {
            let e = s.append(chain.last(), chain.len() as u64, op);
            chain.push(e.clone());
            st.apply(&e)
        };
        let goal = match push(
            &mut st,
            Op::CreateNode {
                kind: NodeKind::Goal,
                title: "Laufen".into(),
                body: String::new(),
            },
        )
        .unwrap()
        {
            Change::Node(n) => n.id,
            _ => unreachable!(),
        };
        let proposal = |sources: Vec<NodeId>| Proposal {
            goal,
            kind: NodeKind::Goal,
            title: "Laufschuhe kaufen".into(),
            body: String::new(),
            sources,
            model: "qwen3:8b".into(),
        };

        // Rejecting keeps the graph as it is.
        let before = (st.nodes.len(), st.edges.len());
        assert_eq!(
            push(
                &mut st,
                Op::RejectProposal {
                    proposal: proposal(alloc::vec![goal])
                }
            ),
            Ok(Change::Rejected)
        );
        assert_eq!((st.nodes.len(), st.edges.len()), before);

        // Without sources or with unknown sources nothing is accepted.
        let none = Op::AcceptProposal {
            proposal: proposal(Vec::new()),
        };
        assert_eq!(push(&mut st, none), Err(ApplyError::NoSources));
        let ghost = NodeId([8; 16]);
        let unknown = Op::AcceptProposal {
            proposal: proposal(alloc::vec![ghost]),
        };
        assert_eq!(push(&mut st, unknown), Err(ApplyError::UnknownNode(ghost)));

        // Accepting creates the node and its edge „derives from“ the goal.
        let ok = push(
            &mut st,
            Op::AcceptProposal {
                proposal: proposal(alloc::vec![goal]),
            },
        );
        let Ok(Change::Accepted { node, edge }) = ok else {
            panic!("{ok:?}")
        };
        assert_eq!(node.title, "Laufschuhe kaufen");
        assert_eq!(
            (edge.from, edge.to, edge.kind),
            (node.id, goal, EdgeKind::DerivesFrom)
        );
        assert_eq!((st.nodes.len(), st.edges.len()), (2, 1));
    }

    #[test]
    fn sensitivity_is_a_separate_entry() {
        let s = Signer::from_seed([6; 32]);
        let c = s.append(
            None,
            1,
            Op::CreateNode {
                kind: NodeKind::Reflection,
                title: "Arzttermin".into(),
                body: String::new(),
            },
        );
        let id = NodeId::from_entry_hash(&c.hash);
        let on = s.append(
            Some(&c),
            2,
            Op::MarkSensitive {
                node: id,
                sensitive: true,
            },
        );
        let off = s.append(
            Some(&on),
            3,
            Op::MarkSensitive {
                node: id,
                sensitive: false,
            },
        );

        let mut st = State::default();
        st.apply(&c).unwrap();
        let node = st.nodes[&id].clone();
        st.apply(&on).unwrap();
        assert!(st.sensitive.contains(&id));
        // The node itself is unchanged: same shape on every device.
        assert_eq!(st.nodes[&id], node);
        st.apply(&off).unwrap();
        assert!(st.sensitive.is_empty());
    }
}
