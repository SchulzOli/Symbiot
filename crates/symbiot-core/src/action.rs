//! E16: one language of actions for the human and the AI.
//!
//! Everything a person can change in Symbiot is an [`Action`]. The AI never changes anything:
//! it writes a [`Suggestion`] — a list of actions with a reason and the nodes it is based on.
//! Only when the human accepts it (all or some of the actions) are they carried out, and the
//! decision is signed into the chain ([`crate::Op::DecideSuggestion`]).
//!
//! Left out on purpose: the person key and devices (the 24 words never reach an AI) and
//! anything that acts outside Symbiot (E10).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::bytes::{EdgeId, NodeId, RuleId};
use crate::edge::EdgeKind;
use crate::node::NodeKind;

/// A node an action is about: one that exists, or one created by an earlier action of the
/// same suggestion (`$0` is the node of the first action). Written as a string: the node id
/// in hex, or `$n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Node(NodeId),
    New(u16),
}

impl Target {
    pub fn parse(s: &str) -> Option<Target> {
        let s = s.trim();
        match s.strip_prefix('$') {
            Some(n) => n.parse().ok().map(Target::New),
            None => NodeId::from_hex(s).map(Target::Node),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Node(id) => write!(f, "{id}"),
            Target::New(n) => write!(f, "${n}"),
        }
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Target;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a node id in hex or $n")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Target, E> {
                Target::parse(v).ok_or_else(|| E::custom("weder Knoten-Id noch $n"))
            }
        }
        d.deserialize_str(V)
    }
}

/// Everything a person can change. New actions go at the END (postcard encodes the index).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    CreateNode {
        kind: NodeKind,
        title: String,
        #[serde(default)]
        body: String,
    },
    UpdateNode {
        node: Target,
        title: String,
        #[serde(default)]
        body: String,
    },
    Link {
        from: Target,
        to: Target,
        kind: EdgeKind,
    },
    Unlink {
        edge: EdgeId,
    },
    DeleteNode {
        node: Target,
    },
    SetSensitive {
        node: Target,
        sensitive: bool,
    },
    SetPolicy {
        node: Target,
        deletable: bool,
        exportable: bool,
    },
    AddRule {
        label: String,
        keywords: Vec<String>,
    },
    RemoveRule {
        rule: RuleId,
    },
    MergeIdentities {
        ids: Vec<String>,
        #[serde(default)]
        name: String,
    },
    SplitIdentity {
        id: String,
    },
    KeepApart {
        a: String,
        b: String,
    },
    /// #44: how a source's raw data is used.
    GrantSource {
        source: String,
        grant: crate::source::Grant,
    },
    /// #44: a source is no longer used, its raw data is removed.
    RevokeSource {
        source: String,
    },
}

/// What the AI may do, for its prompt and for tools (MCP): name, what it does, an example.
pub const CATALOG: &[(&str, &str, &str)] = &[
    (
        "create_node",
        "Neuen Knoten anlegen (Art: goal, tension, reflection, evidence, person, value)",
        r#"{"create_node": {"kind": "goal", "title": "…", "body": ""}}"#,
    ),
    (
        "update_node",
        "Titel und Text eines Knotens ändern (neue Version)",
        r#"{"update_node": {"node": "<id>", "title": "…", "body": "…"}}"#,
    ),
    (
        "link",
        "Zwei Knoten verbinden (supports, contradicts, relates_to, derives_from); $0 meint den Knoten der ersten Aktion",
        r#"{"link": {"from": "$0", "to": "<id>", "kind": "supports"}}"#,
    ),
    (
        "unlink",
        "Eine Verbindung lösen",
        r#"{"unlink": {"edge": "<edge-id>"}}"#,
    ),
    (
        "delete_node",
        "Einen Knoten löschen",
        r#"{"delete_node": {"node": "<id>"}}"#,
    ),
    (
        "set_sensitive",
        "Einen Knoten als sensibel markieren oder die Markierung aufheben",
        r#"{"set_sensitive": {"node": "<id>", "sensitive": true}}"#,
    ),
    (
        "set_policy",
        "Festlegen, ob ein Knoten löschbar und exportierbar ist",
        r#"{"set_policy": {"node": "<id>", "deletable": true, "exportable": false}}"#,
    ),
    (
        "add_rule",
        "Regel für Sensibles anlegen (Name und Stichwörter)",
        r#"{"add_rule": {"label": "Gesundheit", "keywords": ["arzt"]}}"#,
    ),
    (
        "remove_rule",
        "Regel für Sensibles entfernen",
        r#"{"remove_rule": {"rule": "<rule-id>"}}"#,
    ),
    (
        "merge_identities",
        "Kennungen zu einer Person zusammenführen",
        r#"{"merge_identities": {"ids": ["name:Anna", "mailto:anna@example.org"], "name": "Anna"}}"#,
    ),
    (
        "split_identity",
        "Eine Kennung wieder als eigene Person führen",
        r#"{"split_identity": {"id": "name:Anna"}}"#,
    ),
    (
        "keep_apart",
        "Zwei Kennungen sind nicht dieselbe Person",
        r#"{"keep_apart": {"a": "name:Anna", "b": "telegram:user1"}}"#,
    ),
    (
        "grant_source",
        "Festlegen, wie eine Quelle genutzt wird: aktiv oder pausiert, ausgelassene Chats/Repos, nur die letzten N Tage (0 = alle)",
        r#"{"grant_source": {"source": "whatsapp", "grant": {"active": true, "exclude": ["Arbeit"], "keep_days": 0}}}"#,
    ),
    (
        "revoke_source",
        "Eine Quelle widerrufen: wird nicht mehr genutzt, ihre Rohdaten werden gelöscht",
        r#"{"revoke_source": {"source": "whatsapp"}}"#,
    ),
];

impl Action {
    /// The nodes an action refers to.
    pub fn targets(&self) -> Vec<Target> {
        match self {
            Action::UpdateNode { node, .. }
            | Action::DeleteNode { node }
            | Action::SetSensitive { node, .. }
            | Action::SetPolicy { node, .. } => alloc::vec![*node],
            Action::Link { from, to, .. } => alloc::vec![*from, *to],
            _ => Vec::new(),
        }
    }
}

/// What the AI suggests: actions, why, and on which nodes it is based.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub title: String,
    pub reason: String,
    pub actions: Vec<Action>,
    /// Existing nodes the suggestion refers to or is based on.
    pub sources: Vec<NodeId>,
    /// Which model suggested it.
    pub model: String,
}

/// Why a suggestion cannot be carried out as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    Empty,
    /// `$n` points to an action that is not an earlier `create_node`.
    BadReference(usize),
    /// An empty title, rule or identifier.
    Incomplete(usize),
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Invalid::Empty => f.write_str("Der Vorschlag enthält keine Aktion"),
            Invalid::BadReference(i) => write!(
                f,
                "Aktion {}: $n verweist nicht auf einen zuvor angelegten Knoten",
                i + 1
            ),
            Invalid::Incomplete(i) => write!(f, "Aktion {} ist unvollständig", i + 1),
        }
    }
}

impl Suggestion {
    /// Checks the form: references to new nodes point back to a `create_node`, nothing
    /// required is empty. Whether existing nodes exist is checked when it is carried out.
    pub fn check(&self) -> Result<(), Invalid> {
        if self.actions.is_empty() {
            return Err(Invalid::Empty);
        }
        for (i, a) in self.actions.iter().enumerate() {
            for t in a.targets() {
                if let Target::New(n) = t {
                    let n = n as usize;
                    if n >= i || !matches!(self.actions[n], Action::CreateNode { .. }) {
                        return Err(Invalid::BadReference(i));
                    }
                }
            }
            let incomplete = match a {
                Action::CreateNode { title, .. } | Action::UpdateNode { title, .. } => {
                    title.trim().is_empty()
                }
                Action::AddRule { label, keywords } => {
                    label.trim().is_empty() || keywords.is_empty()
                }
                Action::MergeIdentities { ids, .. } => ids.len() < 2,
                Action::SplitIdentity { id } => id.trim().is_empty(),
                Action::KeepApart { a, b } => a.trim().is_empty() || b.trim().is_empty(),
                Action::GrantSource { source, grant } => {
                    !crate::source::valid_source(source) || !grant.valid()
                }
                Action::RevokeSource { source } => !crate::source::valid_source(source),
                _ => false,
            };
            if incomplete {
                return Err(Invalid::Incomplete(i));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(b: u8) -> NodeId {
        NodeId([b; NodeId::LEN])
    }

    #[test]
    fn json_as_an_llm_writes_it() {
        let goal = id(7);
        let json = alloc::format!(
            r#"[{{"create_node": {{"kind": "goal", "title": "Laufschuhe kaufen"}}}},
                {{"link": {{"from": "$0", "to": "{goal}", "kind": "derives_from"}}}}]"#
        );
        let actions: Vec<Action> = serde_json::from_str(&json).unwrap();
        assert_eq!(
            actions[1],
            Action::Link {
                from: Target::New(0),
                to: Target::Node(goal),
                kind: EdgeKind::DerivesFrom
            }
        );
        // And through postcard, as in the chain.
        let s = Suggestion {
            title: "Schritt".into(),
            reason: "weil".into(),
            actions,
            sources: alloc::vec![goal],
            model: "m".into(),
        };
        let bytes = postcard::to_allocvec(&s).unwrap();
        assert_eq!(postcard::from_bytes::<Suggestion>(&bytes).unwrap(), s);
        assert_eq!(s.check(), Ok(()));
    }

    #[test]
    fn references_must_point_back_to_a_new_node() {
        let mk = |actions| Suggestion {
            title: String::new(),
            reason: String::new(),
            actions,
            sources: Vec::new(),
            model: String::new(),
        };
        let link = |from| Action::Link {
            from,
            to: Target::Node(id(1)),
            kind: EdgeKind::Supports,
        };
        assert_eq!(mk(alloc::vec![]).check(), Err(Invalid::Empty));
        assert_eq!(
            mk(alloc::vec![link(Target::New(0))]).check(),
            Err(Invalid::BadReference(0))
        );
        assert_eq!(
            mk(alloc::vec![
                Action::DeleteNode {
                    node: Target::Node(id(1))
                },
                link(Target::New(0))
            ])
            .check(),
            Err(Invalid::BadReference(1))
        );
        assert_eq!(
            mk(alloc::vec![Action::CreateNode {
                kind: NodeKind::Goal,
                title: " ".into(),
                body: String::new()
            }])
            .check(),
            Err(Invalid::Incomplete(0))
        );
        assert_eq!(Target::parse("$x"), None);
        assert_eq!(Target::parse("nicht hex"), None);
    }
}
