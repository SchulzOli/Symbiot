//! AI proposals (F4): the AI suggests, the human decides.
//!
//! A proposal lives outside the ledger until the human clicks. Only the decision is
//! signed into the chain — together with the full proposal, so it stays traceable
//! which model suggested what, based on which nodes.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::bytes::NodeId;
use crate::node::NodeKind;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// The goal this is the next step for. An accepted proposal derives from it.
    pub goal: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub body: String,
    /// The nodes the proposal is based on. A proposal without sources is rejected by the core.
    pub sources: Vec<NodeId>,
    /// Which model suggested it, e.g. `qwen3:8b`.
    pub model: String,
}
