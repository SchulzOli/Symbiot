//! Nodes of the personal graph and the projection that builds them from ledger entries.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::bytes::{Hash, NodeId, PubKey};
use crate::ledger::{LedgerEntry, Op};

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
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::UnknownNode(id) => write!(f, "Knoten {id} existiert nicht"),
        }
    }
}

impl Node {
    /// Applies one entry: creates the node or advances it to the next version.
    /// `current` must be the node the entry refers to (`None` for a create).
    pub fn apply(current: Option<Node>, e: &LedgerEntry) -> Result<Node, ApplyError> {
        match (&e.op, current) {
            (Op::CreateNode { kind, title, body }, _) => Ok(Node {
                id: NodeId::from_entry_hash(&e.hash),
                kind: *kind,
                title: title.clone(),
                body: body.clone(),
                author: e.author,
                version: 1,
                created_ms: e.ts_ms,
                updated_ms: e.ts_ms,
                history: alloc::vec![e.hash],
            }),
            (Op::UpdateNode { title, body, .. }, Some(mut n)) => {
                n.title = title.clone();
                n.body = body.clone();
                n.version += 1;
                n.updated_ms = e.ts_ms;
                n.history.push(e.hash);
                Ok(n)
            }
            (Op::UpdateNode { node, .. }, None) => Err(ApplyError::UnknownNode(*node)),
        }
    }

    /// The node an entry refers to, if it already exists.
    pub fn target(e: &LedgerEntry) -> NodeId {
        match &e.op {
            Op::CreateNode { .. } => NodeId::from_entry_hash(&e.hash),
            Op::UpdateNode { node, .. } => *node,
        }
    }
}

/// In-memory projection of verified entries — used by the UI, tests and small devices.
/// Apps with storage keep the same projection in SQLite.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub nodes: BTreeMap<NodeId, Node>,
}

impl State {
    pub fn apply(&mut self, e: &LedgerEntry) -> Result<&Node, ApplyError> {
        let id = Node::target(e);
        let next = Node::apply(self.nodes.remove(&id), e)?;
        Ok(self.nodes.entry(id).or_insert(next))
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
        let id = Node::target(&c);
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
        let n = st.apply(&u).unwrap();
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
    }
}
