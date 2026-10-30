//! Edges: typed, directed connections between nodes ("evidence supports goal").

use serde::{Deserialize, Serialize};

use crate::bytes::{EdgeId, Hash, NodeId, PubKey};

/// Order matters for postcard hashing: new kinds go at the END.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Supports,
    Contradicts,
    RelatesTo,
    DerivesFrom,
}

impl EdgeKind {
    pub const ALL: [EdgeKind; 4] = [
        EdgeKind::Supports,
        EdgeKind::Contradicts,
        EdgeKind::RelatesTo,
        EdgeKind::DerivesFrom,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EdgeKind::Supports => "supports",
            EdgeKind::Contradicts => "contradicts",
            EdgeKind::RelatesTo => "relates_to",
            EdgeKind::DerivesFrom => "derives_from",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        EdgeKind::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// German verb phrase for the UI: "<from> stützt <to>".
    pub fn label(self) -> &'static str {
        match self {
            EdgeKind::Supports => "stützt",
            EdgeKind::Contradicts => "widerspricht",
            EdgeKind::RelatesTo => "bezieht sich auf",
            EdgeKind::DerivesFrom => "ist abgeleitet von",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub author: PubKey,
    pub created_ms: u64,
    /// Hash of the ledger entry that created this edge — its provenance.
    pub entry: Hash,
}
