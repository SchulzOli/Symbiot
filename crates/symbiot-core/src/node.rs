//! Nodes of the personal graph and the projection that builds them from ledger entries.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::bytes::{EdgeId, Hash, NodeId, PubKey, RuleId};
use crate::edge::{Edge, EdgeKind};
use crate::ledger::{Decode, LedgerEntry, Op};
use crate::proposal::Proposal;
use crate::rule::{self, Rule};

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
    /// The current version was written without seeing the one before it (another device
    /// changed the node meanwhile). Cleared by the next version written on top of it.
    pub concurrent: bool,
}

/// Whether a node may be deleted for real and may leave the own devices (E3).
/// Not part of the node: a separate entry, so nodes keep the same shape everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    /// Deleting removes the content (E2). Otherwise deleting only hides the node.
    pub deletable: bool,
    /// May go to other people's devices and to a hosted AI. Otherwise it stays on the
    /// own devices — and is not in the own backup either (E5).
    pub exportable: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            deletable: true,
            exportable: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    UnknownNode(NodeId),
    UnknownEdge(EdgeId),
    UnknownRule(RuleId),
    /// A rule needs a name and at least one keyword of three or more letters (E4).
    EmptyRule,
    SelfLink,
    DuplicateEdge,
    /// A proposal must name the nodes it is based on.
    NoSources,
    EmptyTitle,
    /// The entry is validly signed but cannot be read (unknown kind or broken params).
    Undecodable(Decode),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::UnknownNode(id) => write!(f, "Knoten {id} existiert nicht"),
            ApplyError::UnknownEdge(id) => write!(f, "Verbindung {id} existiert nicht"),
            ApplyError::UnknownRule(id) => write!(f, "Regel {id} existiert nicht"),
            ApplyError::EmptyRule => f.write_str(
                "Eine Regel braucht einen Namen und Stichwörter mit mindestens drei Buchstaben",
            ),
            ApplyError::SelfLink => {
                f.write_str("Ein Knoten kann nicht mit sich selbst verbunden werden")
            }
            ApplyError::DuplicateEdge => f.write_str("Diese Verbindung gibt es schon"),
            ApplyError::NoSources => f.write_str("Ein Vorschlag ohne Quellen wird verworfen"),
            ApplyError::EmptyTitle => f.write_str("Titel darf nicht leer sein"),
            ApplyError::Undecodable(d) => write!(f, "Eintrag nicht lesbar: {d}"),
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
    /// The node and its connections disappear. `prune` lists the entries whose content
    /// every device removes — all versions if the node is deletable, none otherwise.
    Deleted {
        node: NodeId,
        prune: Vec<Hash>,
    },
    Unlinked(EdgeId),
    /// A device was certified or revoked (E5): nothing changes in the graph.
    Device,
    /// E4: a rule for what is sensitive was added or removed.
    Rule(Rule),
    RuleRemoved(RuleId),
    Policy {
        node: NodeId,
        policy: Policy,
    },
}

/// What the rules need to know about the current projection. Implemented by [`State`]
/// and by the SQLite store, so every platform applies the same rules.
pub trait Lookup {
    /// The current version of a node; `None` if unknown or deleted.
    fn node(&self, id: &NodeId) -> Option<Node>;
    fn edge(&self, id: &EdgeId) -> Option<Edge>;
    /// Whether the same connection already exists.
    fn has_edge(&self, from: &NodeId, to: &NodeId, kind: EdgeKind) -> bool;
    fn policy(&self, id: &NodeId) -> Policy;
    fn rule(&self, id: &RuleId) -> Option<Rule>;
}

impl Change {
    /// Derives the effect of `e` on the projection seen through `view`.
    pub fn of(e: &LedgerEntry, view: &impl Lookup) -> Result<Change, ApplyError> {
        let op = e.op().map_err(ApplyError::Undecodable)?;
        let (author, ts_ms) = (e.head.author, e.head.ts_ms);
        let known = |id: &NodeId| view.node(id).ok_or(ApplyError::UnknownNode(*id));
        match &op {
            Op::CreateNode { kind, title, body } => Ok(Change::Node(Node {
                id: NodeId::from_entry_hash(&e.hash),
                kind: *kind,
                title: title.clone(),
                body: body.clone(),
                author,
                version: 1,
                created_ms: ts_ms,
                updated_ms: ts_ms,
                history: alloc::vec![e.hash],
                concurrent: false,
            })),
            Op::UpdateNode {
                node: id,
                title,
                body,
                base,
            } => {
                let mut n = known(id)?;
                // Newest wins (the projection applies entries in time order); a version that
                // did not build on the current one is marked, so the human can look at it.
                n.concurrent = base.is_some_and(|b| n.history.last() != Some(&b));
                n.title = title.clone();
                n.body = body.clone();
                n.version += 1;
                n.updated_ms = ts_ms;
                n.history.push(e.hash);
                Ok(Change::Node(n))
            }
            Op::Link { from, to, kind } => {
                if from == to {
                    return Err(ApplyError::SelfLink);
                }
                known(from)?;
                known(to)?;
                if view.has_edge(from, to, *kind) {
                    return Err(ApplyError::DuplicateEdge);
                }
                Ok(Change::Edge(Edge {
                    id: EdgeId::from_entry_hash(&e.hash),
                    from: *from,
                    to: *to,
                    kind: *kind,
                    author,
                    created_ms: ts_ms,
                    entry: e.hash,
                }))
            }
            Op::AcceptProposal { proposal: p } => {
                check_proposal(p, view)?;
                let id = NodeId::from_entry_hash(&e.hash);
                Ok(Change::Accepted {
                    node: Node {
                        id,
                        kind: p.kind,
                        title: p.title.clone(),
                        body: p.body.clone(),
                        author,
                        version: 1,
                        created_ms: ts_ms,
                        updated_ms: ts_ms,
                        history: alloc::vec![e.hash],
                        concurrent: false,
                    },
                    edge: Edge {
                        id: EdgeId::from_entry_hash(&e.hash),
                        from: id,
                        to: p.goal,
                        kind: EdgeKind::DerivesFrom,
                        author,
                        created_ms: ts_ms,
                        entry: e.hash,
                    },
                })
            }
            Op::RejectProposal { proposal: p } => {
                check_proposal(p, view)?;
                Ok(Change::Rejected)
            }
            // Which devices count is decided by `Trust`, not by the graph.
            Op::CertifyDevice { .. } | Op::RevokeDevice { .. } => Ok(Change::Device),
            Op::AddSensitivityRule {
                label,
                keywords,
                model,
            } => {
                // Only clean rules are written, so every device applies exactly these words.
                let clean = rule::clean_keywords(keywords);
                if label.trim().is_empty() || clean.is_empty() || clean != *keywords {
                    return Err(ApplyError::EmptyRule);
                }
                Ok(Change::Rule(Rule {
                    id: RuleId::from_entry_hash(&e.hash),
                    label: label.trim().into(),
                    keywords: clean,
                    model: model.clone(),
                    author,
                    created_ms: ts_ms,
                }))
            }
            Op::RemoveSensitivityRule { rule } => {
                view.rule(rule).ok_or(ApplyError::UnknownRule(*rule))?;
                Ok(Change::RuleRemoved(*rule))
            }
            Op::MarkSensitive {
                node: id,
                sensitive,
            } => {
                known(id)?;
                Ok(Change::Sensitivity {
                    node: *id,
                    sensitive: *sensitive,
                })
            }
            Op::DeleteNode { node: id } => {
                let n = known(id)?;
                let prune = if view.policy(id).deletable {
                    n.history
                } else {
                    Vec::new()
                };
                Ok(Change::Deleted { node: *id, prune })
            }
            Op::Unlink { edge } => {
                view.edge(edge).ok_or(ApplyError::UnknownEdge(*edge))?;
                Ok(Change::Unlinked(*edge))
            }
            Op::SetPolicy {
                node: id,
                deletable,
                exportable,
            } => {
                known(id)?;
                Ok(Change::Policy {
                    node: *id,
                    policy: Policy {
                        deletable: *deletable,
                        exportable: *exportable,
                    },
                })
            }
        }
    }
}

/// Same rules for accepting and rejecting: a proposal names a known goal and known sources.
fn check_proposal(p: &Proposal, view: &impl Lookup) -> Result<(), ApplyError> {
    if p.title.trim().is_empty() {
        return Err(ApplyError::EmptyTitle);
    }
    if p.sources.is_empty() {
        return Err(ApplyError::NoSources);
    }
    for id in core::iter::once(&p.goal).chain(&p.sources) {
        if view.node(id).is_none() {
            return Err(ApplyError::UnknownNode(*id));
        }
    }
    Ok(())
}

/// The projection built from several chains, see [`replay`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replay {
    pub state: State,
    /// Entries whose content every device removes (deleted, deletable nodes).
    pub prune: Vec<Hash>,
    /// Validly signed entries that do not apply, e.g. an update of a node deleted elsewhere.
    pub skipped: Vec<Hash>,
}

/// Builds the projection from the chains of several devices (M4). The result is the same on
/// every device, whatever order the chains arrived in:
/// - Each chain keeps its own order. Among the next entries of all chains, the one with the
///   smallest `(ts_ms, author)` goes first — so the newest version of a node wins.
/// - An entry whose node or edge is not known yet (written on another device whose clock is
///   behind) waits until something else changes, then is tried again.
/// - Only when every remaining entry waits, the earliest one is given up (`skipped`).
///
/// `accepts` decides which entries count at all (see [`crate::Trust::accepts`]).
pub fn replay(chains: &[&[LedgerEntry]], accepts: impl Fn(&PubKey, u64) -> bool) -> Replay {
    let mut out = Replay::default();
    let mut next = alloc::vec![0usize; chains.len()];
    let mut waiting = alloc::vec![false; chains.len()];
    let head = |next: &[usize], c: usize| chains[c].get(next[c]);
    let earliest = |next: &[usize], waiting: &[bool], skip_waiting: bool| {
        (0..chains.len())
            .filter(|&c| !(skip_waiting && waiting[c]))
            .filter_map(|c| head(next, c).map(|e| (c, (e.head.ts_ms, e.head.author))))
            .min_by_key(|(_, key)| *key)
            .map(|(c, _)| c)
    };
    loop {
        let Some(c) = earliest(&next, &waiting, true) else {
            match earliest(&next, &waiting, false) {
                Some(c) => {
                    out.skipped.push(chains[c][next[c]].hash);
                    next[c] += 1;
                    waiting.fill(false);
                    continue;
                }
                None => break,
            }
        };
        let e = &chains[c][next[c]];
        if !accepts(&e.head.author, e.head.seq) {
            next[c] += 1;
            continue;
        }
        match out.state.apply(e) {
            Ok(change) => {
                if let Change::Deleted { prune, .. } = change {
                    out.prune.extend(prune);
                }
                next[c] += 1;
                waiting.fill(false);
            }
            Err(ApplyError::UnknownNode(_) | ApplyError::UnknownEdge(_)) => waiting[c] = true,
            Err(_) => {
                out.skipped.push(e.hash);
                next[c] += 1;
            }
        }
    }
    out
}

/// In-memory projection of verified entries — used by tests and small devices.
/// Apps with storage keep the same projection in SQLite.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub nodes: BTreeMap<NodeId, Node>,
    pub edges: BTreeMap<EdgeId, Edge>,
    /// Effective: marked by hand, or matched by a rule and not unmarked by hand (E4).
    pub sensitive: BTreeSet<NodeId>,
    /// Only nodes whose policy differs from the default.
    pub policies: BTreeMap<NodeId, Policy>,
    /// Marked or unmarked by hand; wins over every rule.
    pub by_hand: BTreeMap<NodeId, bool>,
    pub rules: BTreeMap<RuleId, Rule>,
}

impl Lookup for State {
    fn node(&self, id: &NodeId) -> Option<Node> {
        self.nodes.get(id).cloned()
    }

    fn edge(&self, id: &EdgeId) -> Option<Edge> {
        self.edges.get(id).cloned()
    }

    fn has_edge(&self, from: &NodeId, to: &NodeId, kind: EdgeKind) -> bool {
        self.edges
            .values()
            .any(|x| x.from == *from && x.to == *to && x.kind == kind)
    }

    fn policy(&self, id: &NodeId) -> Policy {
        self.policies.get(id).copied().unwrap_or_default()
    }

    fn rule(&self, id: &RuleId) -> Option<Rule> {
        self.rules.get(id).cloned()
    }
}

impl State {
    fn refresh_sensitive(&mut self, id: &NodeId) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        if rule::is_sensitive(node, self.by_hand.get(id).copied(), self.rules.values()) {
            self.sensitive.insert(*id);
        } else {
            self.sensitive.remove(id);
        }
    }

    fn refresh_all_sensitive(&mut self) {
        let ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        for id in &ids {
            self.refresh_sensitive(id);
        }
    }

    pub fn apply(&mut self, e: &LedgerEntry) -> Result<Change, ApplyError> {
        let change = Change::of(e, self)?;
        match &change {
            Change::Node(n) => {
                self.nodes.insert(n.id, n.clone());
                self.refresh_sensitive(&n.id);
            }
            Change::Edge(x) => {
                self.edges.insert(x.id, x.clone());
            }
            Change::Accepted { node, edge } => {
                self.nodes.insert(node.id, node.clone());
                self.edges.insert(edge.id, edge.clone());
                self.refresh_sensitive(&node.id);
            }
            Change::Rejected | Change::Device => {}
            Change::Sensitivity { node, sensitive } => {
                self.by_hand.insert(*node, *sensitive);
                self.refresh_sensitive(node);
            }
            Change::Rule(r) => {
                self.rules.insert(r.id, r.clone());
                self.refresh_all_sensitive();
            }
            Change::RuleRemoved(id) => {
                self.rules.remove(id);
                self.refresh_all_sensitive();
            }
            Change::Deleted { node, .. } => {
                self.nodes.remove(node);
                self.edges.retain(|_, x| x.from != *node && x.to != *node);
                self.sensitive.remove(node);
                self.policies.remove(node);
                self.by_hand.remove(node);
            }
            Change::Unlinked(edge) => {
                self.edges.remove(edge);
            }
            Change::Policy { node, policy } => {
                if *policy == Policy::default() {
                    self.policies.remove(node);
                } else {
                    self.policies.insert(*node, *policy);
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
                base: None,
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
                base: None,
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

    /// E2/E3: deleting removes node and connections; only a deletable node loses its content.
    #[test]
    fn delete_unlink_and_policy() {
        let s = Signer::from_seed([5; 32]);
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
        let node_of = |c: Result<Change, ApplyError>| match c.unwrap() {
            Change::Node(n) => n,
            other => panic!("{other:?}"),
        };
        let a = node_of(push(&mut st, create("Ziel")));
        let b = node_of(push(&mut st, create("Beleg")));
        let a2 = node_of(push(
            &mut st,
            Op::UpdateNode {
                node: a.id,
                title: "Ziel 2".into(),
                body: String::new(),
                base: None,
            },
        ));
        let Ok(Change::Edge(edge)) = push(
            &mut st,
            Op::Link {
                from: b.id,
                to: a.id,
                kind: EdgeKind::Supports,
            },
        ) else {
            panic!()
        };

        // Unlink removes only the edge; a second time it is unknown.
        let unlink = Op::Unlink { edge: edge.id };
        assert_eq!(push(&mut st, unlink.clone()), Ok(Change::Unlinked(edge.id)));
        assert_eq!(push(&mut st, unlink), Err(ApplyError::UnknownEdge(edge.id)));
        assert_eq!(st.nodes.len(), 2);

        // A node that is not deletable is only hidden: nothing to prune.
        let keep = Op::SetPolicy {
            node: b.id,
            deletable: false,
            exportable: false,
        };
        assert!(push(&mut st, keep).is_ok());
        assert!(!st.policy(&b.id).deletable && !st.policy(&b.id).exportable);
        assert_eq!(
            push(&mut st, Op::DeleteNode { node: b.id }),
            Ok(Change::Deleted {
                node: b.id,
                prune: Vec::new()
            })
        );

        // A deletable node (the default) prunes every version; its edges go with it.
        let c = node_of(push(&mut st, create("Wert")));
        push(
            &mut st,
            Op::Link {
                from: a.id,
                to: c.id,
                kind: EdgeKind::RelatesTo,
            },
        )
        .unwrap();
        assert_eq!(
            push(&mut st, Op::DeleteNode { node: a.id }),
            Ok(Change::Deleted {
                node: a.id,
                prune: a2.history.clone()
            })
        );
        assert_eq!(a2.history.len(), 2);
        assert!(st.edges.is_empty(), "edges of a deleted node disappear");
        assert_eq!(st.nodes.len(), 1);

        // A deleted node is gone for every later rule.
        assert_eq!(
            push(
                &mut st,
                Op::UpdateNode {
                    node: a.id,
                    title: "zurück".into(),
                    body: String::new(),
                    base: None,
                }
            ),
            Err(ApplyError::UnknownNode(a.id))
        );
        assert_eq!(
            push(&mut st, Op::DeleteNode { node: a.id }),
            Err(ApplyError::UnknownNode(a.id))
        );
    }

    #[test]
    fn policy_can_be_loosened_again() {
        let s = Signer::from_seed([6; 32]);
        let c = s.append(
            None,
            1,
            Op::CreateNode {
                kind: NodeKind::Evidence,
                title: "Beleg".into(),
                body: String::new(),
            },
        );
        let id = NodeId::from_entry_hash(&c.hash);
        let strict = s.append(
            Some(&c),
            2,
            Op::SetPolicy {
                node: id,
                deletable: false,
                exportable: true,
            },
        );
        let loose = s.append(
            Some(&strict),
            3,
            Op::SetPolicy {
                node: id,
                deletable: true,
                exportable: true,
            },
        );
        let mut st = State::default();
        st.apply(&c).unwrap();
        st.apply(&strict).unwrap();
        assert!(!st.policy(&id).deletable);
        st.apply(&loose).unwrap();
        assert_eq!(st.policy(&id), Policy::default());
        assert!(st.policies.is_empty());
    }

    fn update(node: NodeId, title: &str, base: &LedgerEntry) -> Op {
        Op::UpdateNode {
            node,
            title: title.into(),
            body: String::new(),
            base: Some(base.hash),
        }
    }

    /// M4: two devices change the same node while apart. Newest wins on every device, the
    /// node is marked, and a version written on top of it clears the mark.
    #[test]
    fn replay_newest_wins_and_marks_concurrent_edits() {
        let pc = Signer::from_seed([11; 32]);
        let heimserver = Signer::from_seed([12; 32]);
        let create = pc.append(
            None,
            100,
            Op::CreateNode {
                kind: NodeKind::Goal,
                title: "Laufen".into(),
                body: String::new(),
            },
        );
        let id = NodeId::from_entry_hash(&create.hash);
        // heimserver's clock is behind: its edit carries an earlier time than the node itself.
        let sz_edit = heimserver.append(None, 50, update(id, "Laufen (heimserver)", &create));
        let pc_edit = pc.append(Some(&create), 200, update(id, "Laufen (PC)", &create));
        let pc_chain = alloc::vec![create.clone(), pc_edit.clone()];
        let sz_chain = alloc::vec![sz_edit.clone()];

        let all = |_: &PubKey, _: u64| true;
        let a = replay(&[&pc_chain, &sz_chain], all);
        let b = replay(&[&sz_chain, &pc_chain], all);
        assert_eq!(a, b, "same result whatever order the chains arrive in");
        let n = &a.state.nodes[&id];
        assert_eq!(n.title, "Laufen (PC)", "newest wins");
        assert!(n.concurrent);
        assert_eq!(
            n.history,
            alloc::vec![create.hash, sz_edit.hash, pc_edit.hash]
        );
        assert!(a.skipped.is_empty());

        // „Passt so“: a version on top of the current one clears the mark.
        let ok = pc.append(Some(&pc_edit), 300, update(id, "Laufen (PC)", &pc_edit));
        let pc_chain = alloc::vec![create, pc_edit, ok];
        let c = replay(&[&pc_chain, &sz_chain], all);
        assert!(!c.state.nodes[&id].concurrent);
    }

    #[test]
    fn replay_is_independent_of_chain_order() {
        let devices = [
            Signer::from_seed([21; 32]),
            Signer::from_seed([22; 32]),
            Signer::from_seed([23; 32]),
        ];
        let create = |s: &Signer, ts: u64, title: &str| {
            s.append(
                None,
                ts,
                Op::CreateNode {
                    kind: NodeKind::Goal,
                    title: title.into(),
                    body: String::new(),
                },
            )
        };
        let a0 = create(&devices[0], 10, "A");
        let b0 = create(&devices[1], 20, "B");
        let a = NodeId::from_entry_hash(&a0.hash);
        let b = NodeId::from_entry_hash(&b0.hash);
        let a1 = devices[0].append(
            Some(&a0),
            40,
            Op::Link {
                from: a,
                to: b,
                kind: EdgeKind::RelatesTo,
            },
        );
        let b1 = devices[1].append(Some(&b0), 30, update(a, "A von B", &a0));
        let c0 = devices[2].append(None, 35, Op::DeleteNode { node: b });
        let c1 = devices[2].append(Some(&c0), 36, update(b, "zu spät", &b0));
        let chains = [
            alloc::vec![a0, a1],
            alloc::vec![b0, b1],
            alloc::vec![c0, c1.clone()],
        ];

        let all = |_: &PubKey, _: u64| true;
        let reference = replay(&[&chains[0], &chains[1], &chains[2]], all);
        for order in [[0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            let r = replay(
                &[&chains[order[0]], &chains[order[1]], &chains[order[2]]],
                all,
            );
            assert_eq!(r, reference, "{order:?}");
        }
        // B was deleted at 35: the link at 40 and the edit at 36 no longer apply.
        assert_eq!(reference.state.nodes.len(), 1);
        assert_eq!(reference.state.nodes[&a].title, "A von B");
        assert!(reference.state.edges.is_empty());
        assert_eq!(reference.skipped.len(), 2);
        assert!(reference.skipped.contains(&c1.hash));
        assert_eq!(reference.prune, alloc::vec![chains[1][0].hash]);
    }

    #[test]
    fn replay_ignores_entries_that_do_not_count() {
        let mine = Signer::from_seed([31; 32]);
        let stranger = Signer::from_seed([32; 32]);
        let op = |title: &str| Op::CreateNode {
            kind: NodeKind::Goal,
            title: title.into(),
            body: String::new(),
        };
        let my_chain = alloc::vec![mine.append(None, 1, op("meins"))];
        let their_chain = alloc::vec![stranger.append(None, 2, op("fremd"))];
        let me = mine.public();
        let r = replay(&[&my_chain, &their_chain], |author, _| *author == me);
        assert_eq!(r.state.nodes.len(), 1);
        assert!(
            r.skipped.is_empty(),
            "not counting is not the same as failing"
        );
    }

    /// E4: an accepted rule marks matching nodes, also later ones; by hand wins; removing
    /// the rule takes its marks back.
    #[test]
    fn sensitivity_rules() {
        let s = Signer::from_seed([41; 32]);
        let mut st = State::default();
        let mut chain: Vec<LedgerEntry> = Vec::new();
        let mut push = |st: &mut State, op: Op| {
            let e = s.append(chain.last(), chain.len() as u64, op);
            chain.push(e.clone());
            st.apply(&e)
        };
        let create = |title: &str| Op::CreateNode {
            kind: NodeKind::Reflection,
            title: title.into(),
            body: String::new(),
        };
        let id_of = |c: Result<Change, ApplyError>| match c.unwrap() {
            Change::Node(n) => n.id,
            other => panic!("{other:?}"),
        };
        let doctor = id_of(push(&mut st, create("Arzttermin Dienstag")));
        let run = id_of(push(&mut st, create("Laufen")));

        // Rules must be clean: lower case, three letters or more.
        let dirty = Op::AddSensitivityRule {
            label: "Gesundheit".into(),
            keywords: alloc::vec!["Arzt".into()],
            model: None,
        };
        assert_eq!(push(&mut st, dirty), Err(ApplyError::EmptyRule));
        let empty = Op::AddSensitivityRule {
            label: " ".into(),
            keywords: alloc::vec!["arzt".into()],
            model: None,
        };
        assert_eq!(push(&mut st, empty), Err(ApplyError::EmptyRule));

        let Ok(Change::Rule(r)) = push(
            &mut st,
            Op::AddSensitivityRule {
                label: "Gesundheit".into(),
                keywords: alloc::vec!["arzt".into(), "befund".into()],
                model: Some("qwen3:8b".into()),
            },
        ) else {
            panic!()
        };
        assert!(st.sensitive.contains(&doctor), "„arzt“ finds „Arzttermin“");
        assert!(!st.sensitive.contains(&run));

        // Later nodes and edits are checked too.
        let report = id_of(push(&mut st, create("Befund abholen")));
        assert!(st.sensitive.contains(&report));
        push(
            &mut st,
            Op::UpdateNode {
                node: run,
                title: "Laufen, Arzt sagt ok".into(),
                body: String::new(),
                base: None,
            },
        )
        .unwrap();
        assert!(st.sensitive.contains(&run));

        // By hand wins over the rule.
        push(
            &mut st,
            Op::MarkSensitive {
                node: run,
                sensitive: false,
            },
        )
        .unwrap();
        assert!(!st.sensitive.contains(&run));

        // Removing the rule takes its marks back; marks by hand stay.
        push(
            &mut st,
            Op::MarkSensitive {
                node: report,
                sensitive: true,
            },
        )
        .unwrap();
        assert_eq!(
            push(&mut st, Op::RemoveSensitivityRule { rule: r.id }),
            Ok(Change::RuleRemoved(r.id))
        );
        assert_eq!(st.sensitive.iter().copied().collect::<Vec<_>>(), [report]);
        assert_eq!(
            push(&mut st, Op::RemoveSensitivityRule { rule: r.id }),
            Err(ApplyError::UnknownRule(r.id))
        );
    }

    #[test]
    fn keywords_are_cleaned() {
        assert_eq!(
            crate::rule::clean_keywords([" Arzt ", "arzt", "ei", "Befund"]),
            ["arzt", "befund"]
        );
    }
}
